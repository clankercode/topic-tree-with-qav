//! SQLite pool, migrations, and PRAGMA setup.
//!
//! The pool is shared across all read-paths via `AppState`. Writes in the
//! room actor reuse a checkout from the same pool — single-writer behaviour
//! is guaranteed by the WAL + room actor pattern, not by the pool size.
//!
//! ## Test mode
//! `Db::open_in_memory()` returns a pool with `max_size = 1` so all tests
//! share one connection (in-memory DBs are not shared across connections in
//! SQLite). File-backed temp databases (`Db::open_path`) use the default
//! pool size and are used by integration tests via `tempfile::TempDir`.

use std::path::{Path, PathBuf};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::proto::{Question, Topic};

mod embedded {
    refinery::embed_migrations!("./migrations");
}

pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConn = r2d2::PooledConnection<SqliteConnectionManager>;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] refinery::Error),
}

/// Where this `Db` was opened from. The writer task takes different paths
/// depending on the mode (see `acquire_writer_conn`).
#[derive(Clone, Debug)]
pub enum DbMode {
    /// File-backed database. The writer owns its own freshly-opened
    /// `rusqlite::Connection` for its lifetime; the read pool is unaffected.
    File(PathBuf),
    /// In-memory database. The pool is forced to `max_size = 1` because
    /// `r2d2_sqlite::SqliteConnectionManager::memory()` creates a fresh
    /// anonymous database per `connect()` call. The writer **borrows**
    /// the single pool connection per batch and returns it on commit so
    /// readers can run between batches.
    Memory,
}

/// Opaque database handle: pool + mode (file path or in-memory marker).
#[derive(Clone)]
pub struct Db {
    pool: DbPool,
    mode: DbMode,
}

/// Connection handle used by the single-writer task. See
/// `.plan/2026-05-25-followup/persistence.md` §4.
pub enum WriterConn {
    /// File mode: owned, opened by the writer itself.
    Owned(rusqlite::Connection),
    /// `:memory:` mode: borrowed from the size-1 read pool; dropped at the
    /// end of each batch so reads can run.
    Pooled(DbConn),
}

impl std::ops::Deref for WriterConn {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        match self {
            WriterConn::Owned(c) => c,
            WriterConn::Pooled(c) => c,
        }
    }
}

impl std::ops::DerefMut for WriterConn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            WriterConn::Owned(c) => c,
            WriterConn::Pooled(c) => c,
        }
    }
}

impl Db {
    /// Open (or create) a file-backed SQLite database, configure
    /// connection-level PRAGMAs, and run all pending migrations.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        let manager = SqliteConnectionManager::file(&path).with_init(configure_connection);
        let pool = Pool::builder().build(manager)?;
        run_migrations(&pool)?;
        Ok(Self {
            pool,
            mode: DbMode::File(path),
        })
    }

    /// Open an in-memory database. The pool is sized to 1 so every checkout
    /// returns the same connection (in-memory DBs are not sharable across
    /// SQLite connections by default).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::memory().with_init(configure_connection);
        let pool = Pool::builder().max_size(1).build(manager)?;
        run_migrations(&pool)?;
        Ok(Self {
            pool,
            mode: DbMode::Memory,
        })
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    pub fn mode(&self) -> &DbMode {
        &self.mode
    }

    pub fn get(&self) -> Result<DbConn, DbError> {
        Ok(self.pool.get()?)
    }

    /// Acquire the writer task's connection for one batch.
    ///
    /// File mode: opens a fresh `rusqlite::Connection` configured with the
    /// same PRAGMAs as the pool. The writer can keep this `Owned` variant
    /// alive for its whole lifetime if it wants to avoid the per-batch
    /// open cost.
    ///
    /// `:memory:` mode: borrows the single pool connection. The caller
    /// **must** drop the returned `WriterConn` between batches; otherwise
    /// readers will block on the pool.
    pub fn acquire_writer_conn(&self) -> Result<WriterConn, DbError> {
        match &self.mode {
            DbMode::File(path) => {
                let mut conn = rusqlite::Connection::open(path)?;
                configure_connection(&mut conn)?;
                Ok(WriterConn::Owned(conn))
            }
            DbMode::Memory => Ok(WriterConn::Pooled(self.pool.get()?)),
        }
    }

    pub fn set_kicked(&self, room_id: &str, guest_id: &str, kicked: bool) -> Result<(), DbError> {
        let conn = self.get()?;
        let now = crate::api::now_ms();
        conn.execute(
            "INSERT INTO moderation (room_id, guest_id, kicked, muted, updated_at) VALUES (?1, ?2, ?3, 0, ?4)
             ON CONFLICT (room_id, guest_id) DO UPDATE SET kicked=?3, updated_at=?4",
            rusqlite::params![room_id, guest_id, kicked as i32, now],
        )?;
        Ok(())
    }

    pub fn set_muted(&self, room_id: &str, guest_id: &str, muted: bool) -> Result<(), DbError> {
        let conn = self.get()?;
        let now = crate::api::now_ms();
        conn.execute(
            "INSERT INTO moderation (room_id, guest_id, kicked, muted, updated_at) VALUES (?1, ?2, 0, ?3, ?4)
             ON CONFLICT (room_id, guest_id) DO UPDATE SET muted=?3, updated_at=?4",
            rusqlite::params![room_id, guest_id, muted as i32, now],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn upsert_moderation(
        &self,
        room_id: &str,
        guest_id: &str,
        kicked: bool,
        muted: bool,
    ) -> Result<(), DbError> {
        let conn = self.get()?;
        let now = crate::api::now_ms();
        conn.execute(
            "INSERT INTO moderation (room_id, guest_id, kicked, muted, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (room_id, guest_id) DO UPDATE SET kicked=?3, muted=?4, updated_at=?5",
            rusqlite::params![room_id, guest_id, kicked as i32, muted as i32, now],
        )?;
        Ok(())
    }

    pub fn get_moderation(
        &self,
        room_id: &str,
        guest_id: &str,
    ) -> Result<Option<(bool, bool)>, DbError> {
        let conn = self.get()?;
        let result = conn.query_row(
            "SELECT kicked, muted FROM moderation WHERE room_id = ?1 AND guest_id = ?2",
            rusqlite::params![room_id, guest_id],
            |r| Ok((r.get::<_, i32>(0)? != 0, r.get::<_, i32>(1)? != 0)),
        );
        match result {
            Ok((k, m)) => Ok(Some((k, m))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

// ──────────────────────── WriteOp envelope ────────────────────────
//
// See `.plan/2026-05-25-followup/persistence.md` §3 for the canonical
// exhaustive list. New state-changing intents add a `WriteOpKind`
// variant here **before** any writer arm or handler is wired.

/// One state-changing intent enqueued by an ws/http handler. Applied by
/// the single writer task (`server/src/writer.rs`).
#[derive(Debug, Clone)]
pub struct WriteOp {
    pub room_id: String,
    pub kind: WriteOpKind,
}

#[derive(Debug, Clone)]
pub enum WriteOpKind {
    /// Insert or update a topic row.
    UpsertTopic { topic: Topic },
    /// Rename only — leaves parent_id / ord / status untouched.
    RenameTopic { topic_id: String, title: String },
    /// Move under a (possibly new) parent at the given ord.
    MoveTopic {
        topic_id: String,
        parent_id: Option<String>,
        ord: f64,
    },
    /// Set status (pending|done).
    SetTopicStatus {
        topic_id: String,
        status: crate::proto::TopicStatus,
    },
    /// Delete a topic. SQLite FK cascades to descendants.
    DeleteTopic { topic_id: String },
    /// Update `rooms.active_topic_id`. `None` clears it.
    SetActiveTopic { topic_id: Option<String> },

    /// Insert or update a question row. The `vote_count` field is
    /// **not** persisted — it's a derived column computed from
    /// `question_votes` on hydration.
    UpsertQuestion { question: Question },
    /// Flip `questions.answered`.
    SetQuestionAnswered { question_id: String, answered: bool },
    /// Delete a question. FK cascade removes its votes.
    DeleteQuestion { question_id: String },
    /// Record a guest's vote on a question. `INSERT OR IGNORE` keeps
    /// dedup; broadcast count is the in-memory authoritative source.
    AddVote {
        question_id: String,
        guest_id: String,
        created_at: i64,
    },
    /// Remove a guest's vote.
    RemoveVote {
        question_id: String,
        guest_id: String,
    },
    /// Atomic: insert a topic AND delete a question, one transaction.
    /// See `.plan/2026-05-25-followup/persistence.md` §3 Questions.
    PromoteQuestionToTopic {
        question_id: String,
        topic: Topic,
    },
}

fn configure_connection(conn: &mut Connection) -> rusqlite::Result<()> {
    // foreign_keys + synchronous are safe to set even for :memory:.
    // journal_mode=WAL is a no-op on :memory: but returns the prior mode
    // ("memory"), so we ignore the return value rather than asserting.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn run_migrations(pool: &DbPool) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    embedded::migrations::runner().run(&mut *conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_db_runs_migrations_and_has_rooms_table() {
        let db = Db::open_in_memory().expect("open");
        let conn = db.get().unwrap();
        let rooms: i64 = conn
            .query_row("SELECT COUNT(*) FROM rooms", [], |r| r.get(0))
            .expect("rooms table exists");
        assert_eq!(rooms, 0);
        let mods: i64 = conn
            .query_row("SELECT COUNT(*) FROM moderation", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mods, 0);
    }

    #[test]
    fn acquire_writer_conn_in_memory_writes_visible_to_pool() {
        let db = Db::open_in_memory().expect("open");
        {
            let writer = db.acquire_writer_conn().expect("acquire writer");
            assert!(matches!(writer, WriterConn::Pooled(_)), "memory mode → Pooled");
            writer
                .execute(
                    "INSERT INTO rooms (id, title, admin_token_hash, created_at, last_active_at) \
                     VALUES ('WRITERMEM001','t','h',0,0)",
                    [],
                )
                .unwrap();
        } // writer dropped → pool slot returned

        // Subsequent pool checkout sees the writer's insert.
        let conn = db.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM rooms WHERE id='WRITERMEM001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "writer insert visible to pool checkout");
    }

    #[test]
    fn acquire_writer_conn_in_file_mode_returns_owned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let db = Db::open_path(&path).unwrap();
        let writer = db.acquire_writer_conn().expect("acquire writer");
        assert!(matches!(writer, WriterConn::Owned(_)), "file mode → Owned");
        writer
            .execute(
                "INSERT INTO rooms (id, title, admin_token_hash, created_at, last_active_at) \
                 VALUES ('WRITERFILE01','t','h',0,0)",
                [],
            )
            .unwrap();
        drop(writer);
        drop(db);

        // Verify the row persisted to disk by reopening with a fresh handle.
        let db2 = Db::open_path(&path).unwrap();
        let conn = db2.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM rooms WHERE id='WRITERFILE01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn db_mode_reflects_constructor() {
        let mem = Db::open_in_memory().unwrap();
        assert!(matches!(mem.mode(), DbMode::Memory));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let file = Db::open_path(&path).unwrap();
        assert!(matches!(file.mode(), DbMode::File(_)));
    }

    #[test]
    fn migrations_create_v5_excalidraw_scenes_and_v6_payload_json() {
        let db = Db::open_in_memory().expect("open");
        let conn = db.get().unwrap();

        // V0005: excalidraw_scenes table with the four required columns.
        let scenes: i64 = conn
            .query_row("SELECT COUNT(*) FROM excalidraw_scenes", [], |r| r.get(0))
            .expect("excalidraw_scenes table exists");
        assert_eq!(scenes, 0);
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(excalidraw_scenes)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for required in [
            "board_id",
            "scene_version",
            "elements_json",
            "app_state_json",
            "updated_at",
        ] {
            assert!(
                cols.iter().any(|c| c == required),
                "missing column {required} on excalidraw_scenes; have {cols:?}"
            );
        }

        // V0006: pen_actions gains a payload_json column.
        let pen_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(pen_actions)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            pen_cols.iter().any(|c| c == "payload_json"),
            "pen_actions missing payload_json; have {pen_cols:?}"
        );
    }

    #[test]
    fn foreign_keys_pragma_is_on() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.get().unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn moderation_cascade_deletes_with_room() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.get().unwrap();
        conn.execute(
            "INSERT INTO rooms (id,title,admin_token_hash,created_at,last_active_at) \
             VALUES ('ROOMID000001','t','hash',0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO moderation (room_id,guest_id,kicked,muted,updated_at) \
             VALUES ('ROOMID000001','g1',0,0,0)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM rooms WHERE id = 'ROOMID000001'", [])
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM moderation", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "moderation row should cascade");
    }

    #[test]
    fn file_backed_db_persists_across_handles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let db = Db::open_path(&path).unwrap();
            let conn = db.get().unwrap();
            conn.execute(
                "INSERT INTO rooms (id,title,admin_token_hash,created_at,last_active_at) \
                 VALUES ('FILEROOM0001','t','h',1,1)",
                [],
            )
            .unwrap();
        }
        let db = Db::open_path(&path).unwrap();
        let conn = db.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM rooms WHERE id='FILEROOM0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let _ = Db::open_path(&path).unwrap();
        let _ = Db::open_path(&path).unwrap();
        let _ = Db::open_path(&path).unwrap();
    }

    fn seed_room(db: &Db, room_id: &str) {
        let conn = db.get().unwrap();
        conn.execute(
            "INSERT INTO rooms (id,title,admin_token_hash,created_at,last_active_at) \
             VALUES (?1,'t','h',0,0)",
            [room_id],
        )
        .unwrap();
    }

    #[test]
    fn set_kicked_preserves_existing_muted_flag() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMID000001");
        db.set_muted("ROOMID000001", "g1", true).unwrap();
        db.set_kicked("ROOMID000001", "g1", true).unwrap();
        let (kicked, muted) = db.get_moderation("ROOMID000001", "g1").unwrap().unwrap();
        assert!(kicked && muted, "kick must preserve mute");
    }

    #[test]
    fn set_muted_preserves_existing_kicked_flag() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMID000001");
        db.set_kicked("ROOMID000001", "g1", true).unwrap();
        db.set_muted("ROOMID000001", "g1", true).unwrap();
        let (kicked, muted) = db.get_moderation("ROOMID000001", "g1").unwrap().unwrap();
        assert!(kicked && muted, "mute must preserve kick");
    }

    #[test]
    fn set_kicked_can_clear_kick_without_touching_muted() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMID000001");
        db.set_muted("ROOMID000001", "g1", true).unwrap();
        db.set_kicked("ROOMID000001", "g1", true).unwrap();
        db.set_kicked("ROOMID000001", "g1", false).unwrap();
        let (kicked, muted) = db.get_moderation("ROOMID000001", "g1").unwrap().unwrap();
        assert!(!kicked && muted, "unkick must preserve mute");
    }
}
