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

use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

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

/// Opaque database handle: pool + the path it was opened from (for logs).
#[derive(Clone)]
pub struct Db {
    pool: DbPool,
}

impl Db {
    /// Open (or create) a file-backed SQLite database, configure
    /// connection-level PRAGMAs, and run all pending migrations.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::file(path.as_ref()).with_init(configure_connection);
        let pool = Pool::builder().build(manager)?;
        run_migrations(&pool)?;
        Ok(Self { pool })
    }

    /// Open an in-memory database. The pool is sized to 1 so every checkout
    /// returns the same connection (in-memory DBs are not sharable across
    /// SQLite connections by default).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::memory().with_init(configure_connection);
        let pool = Pool::builder().max_size(1).build(manager)?;
        run_migrations(&pool)?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    pub fn get(&self) -> Result<DbConn, DbError> {
        Ok(self.pool.get()?)
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
