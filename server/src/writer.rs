//! Single-writer SQLite task.
//!
//! One tokio task per process drains a `tokio::sync::mpsc::UnboundedReceiver<WriteOp>`,
//! batches whatever is immediately available, and commits the batch in
//! a single `rusqlite::Transaction`. This amortises WAL fsync across
//! bursts and keeps WAL writes serialised — SQLite's preferred shape.
//!
//! See `.plan/2026-05-25-followup/persistence.md` for the full design.

use std::time::Duration;

use rusqlite::Transaction;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::db::{Db, DbError, WriteOp, WriteOpKind};
use crate::proto::{Topic, TopicStatus};

pub type WriteSender = UnboundedSender<WriteOp>;
pub type WriteReceiver = UnboundedReceiver<WriteOp>;

/// Soft ceiling on the number of ops in a single transaction. Keeps a
/// pathological flood from blocking readers behind one giant fsync.
const MAX_BATCH_SIZE: usize = 256;

/// Bounded timeout for the writer to finish in-flight work on shutdown.
pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Handle returned by `spawn_writer`. The `tx` is cheap to clone and
/// the `join` resolves when the channel is closed and the loop exits.
pub struct WriterHandle {
    pub tx: WriteSender,
    pub join: JoinHandle<()>,
}

impl WriterHandle {
    /// Close the sender (drops the held clone) and await the loop.
    /// Returns whether the join finished within `SHUTDOWN_DRAIN_TIMEOUT`.
    pub async fn shutdown(self) -> bool {
        drop(self.tx);
        tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT, self.join)
            .await
            .is_ok()
    }
}

/// Spawn the writer task and return its sender + join handle.
pub fn spawn_writer(db: Db) -> WriterHandle {
    let (tx, rx) = unbounded_channel();
    let join = tokio::spawn(writer_loop(db, rx));
    WriterHandle { tx, join }
}

async fn writer_loop(db: Db, mut rx: WriteReceiver) {
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        while let Ok(op) = rx.try_recv() {
            batch.push(op);
            if batch.len() >= MAX_BATCH_SIZE {
                break;
            }
        }
        let batch_len = batch.len();
        let db_for_blocking = db.clone();
        let join_result = tokio::task::spawn_blocking(move || {
            apply_batch_sync(&db_for_blocking, &batch)
        })
        .await;
        match join_result {
            Ok(Ok(())) => {
                tracing::trace!(batch_len, "writer batch committed");
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, batch_len, "writer batch failed");
            }
            Err(e) => {
                tracing::error!(error = %e, batch_len, "writer batch panicked");
            }
        }
    }
    tracing::info!("writer task: channel closed, exiting");
}

fn apply_batch_sync(db: &Db, ops: &[WriteOp]) -> Result<(), DbError> {
    let mut writer = db.acquire_writer_conn()?;
    let tx = writer.transaction()?;
    for op in ops {
        apply_op_in_tx(&tx, op)?;
    }
    tx.commit()?;
    Ok(())
}

/// Apply one op inside an existing transaction. Public for unit tests
/// that want to verify a single op without spinning a writer task.
pub(crate) fn apply_op_in_tx(tx: &Transaction<'_>, op: &WriteOp) -> Result<(), DbError> {
    match &op.kind {
        WriteOpKind::UpsertTopic { topic } => apply_upsert_topic(tx, &op.room_id, topic),
        WriteOpKind::RenameTopic { topic_id, title } => apply_rename_topic(tx, topic_id, title),
        WriteOpKind::MoveTopic {
            topic_id,
            parent_id,
            ord,
        } => apply_move_topic(tx, topic_id, parent_id.as_deref(), *ord),
        WriteOpKind::SetTopicStatus { topic_id, status } => {
            apply_set_topic_status(tx, topic_id, *status)
        }
        WriteOpKind::DeleteTopic { topic_id } => apply_delete_topic(tx, topic_id),
        WriteOpKind::SetActiveTopic { topic_id } => {
            apply_set_active_topic(tx, &op.room_id, topic_id.as_deref())
        }
    }
}

fn topic_status_str(s: TopicStatus) -> &'static str {
    match s {
        TopicStatus::Pending => "pending",
        TopicStatus::Done => "done",
    }
}

fn apply_upsert_topic(
    tx: &Transaction<'_>,
    room_id: &str,
    topic: &Topic,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO topics (id, room_id, parent_id, title, ord, status, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(id) DO UPDATE SET \
           parent_id  = excluded.parent_id, \
           title      = excluded.title, \
           ord        = excluded.ord, \
           status     = excluded.status",
        rusqlite::params![
            topic.id,
            room_id,
            topic.parent_id,
            topic.title,
            topic.ord,
            topic_status_str(topic.status),
            topic.created_at,
        ],
    )?;
    Ok(())
}

fn apply_rename_topic(
    tx: &Transaction<'_>,
    topic_id: &str,
    title: &str,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE topics SET title = ?1 WHERE id = ?2",
        rusqlite::params![title, topic_id],
    )?;
    Ok(())
}

fn apply_move_topic(
    tx: &Transaction<'_>,
    topic_id: &str,
    parent_id: Option<&str>,
    ord: f64,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE topics SET parent_id = ?1, ord = ?2 WHERE id = ?3",
        rusqlite::params![parent_id, ord, topic_id],
    )?;
    Ok(())
}

fn apply_set_topic_status(
    tx: &Transaction<'_>,
    topic_id: &str,
    status: TopicStatus,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE topics SET status = ?1 WHERE id = ?2",
        rusqlite::params![topic_status_str(status), topic_id],
    )?;
    Ok(())
}

fn apply_delete_topic(tx: &Transaction<'_>, topic_id: &str) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM topics WHERE id = ?1",
        rusqlite::params![topic_id],
    )?;
    Ok(())
}

fn apply_set_active_topic(
    tx: &Transaction<'_>,
    room_id: &str,
    topic_id: Option<&str>,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE rooms SET active_topic_id = ?1 WHERE id = ?2",
        rusqlite::params![topic_id, room_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_room(db: &Db, room_id: &str) {
        let conn = db.get().unwrap();
        conn.execute(
            "INSERT INTO rooms (id, title, admin_token_hash, created_at, last_active_at) \
             VALUES (?1, 't', 'h', 0, 0)",
            [room_id],
        )
        .unwrap();
    }

    fn topic(id: &str, ord: f64) -> Topic {
        Topic {
            id: id.into(),
            parent_id: None,
            title: format!("Topic {id}"),
            ord,
            status: TopicStatus::Pending,
            created_at: 0,
        }
    }

    #[tokio::test]
    async fn writer_drains_and_commits_batch() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMWRITER01");

        let handle = spawn_writer(db.clone());
        for i in 0..3 {
            handle
                .tx
                .send(WriteOp {
                    room_id: "ROOMWRITER01".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic(&format!("topic-{i}"), i as f64),
                    },
                })
                .unwrap();
        }
        assert!(handle.shutdown().await, "writer should finish within timeout");

        let conn = db.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM topics WHERE room_id='ROOMWRITER01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3, "expected three topic rows after the batch");
    }

    fn drive_ops(db: &Db, ops: Vec<WriteOp>) {
        let mut writer = db.acquire_writer_conn().unwrap();
        let tx = writer.transaction().unwrap();
        for op in &ops {
            apply_op_in_tx(&tx, op).unwrap();
        }
        tx.commit().unwrap();
    }

    #[test]
    fn apply_rename_topic_updates_only_title() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMRENAME01");
        drive_ops(
            &db,
            vec![
                WriteOp {
                    room_id: "ROOMRENAME01".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("t-1", 1.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMRENAME01".into(),
                    kind: WriteOpKind::RenameTopic {
                        topic_id: "t-1".into(),
                        title: "new-name".into(),
                    },
                },
            ],
        );
        let conn = db.get().unwrap();
        let (title, ord): (String, f64) = conn
            .query_row(
                "SELECT title, ord FROM topics WHERE id='t-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "new-name");
        assert_eq!(ord, 1.0, "rename must not touch ord");
    }

    #[test]
    fn apply_move_topic_updates_parent_and_ord() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMMOVE0001");
        drive_ops(
            &db,
            vec![
                WriteOp {
                    room_id: "ROOMMOVE0001".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("p", 0.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMMOVE0001".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("c", 1.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMMOVE0001".into(),
                    kind: WriteOpKind::MoveTopic {
                        topic_id: "c".into(),
                        parent_id: Some("p".into()),
                        ord: 5.0,
                    },
                },
            ],
        );
        let conn = db.get().unwrap();
        let (parent, ord): (Option<String>, f64) = conn
            .query_row(
                "SELECT parent_id, ord FROM topics WHERE id='c'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(parent.as_deref(), Some("p"));
        assert_eq!(ord, 5.0);
    }

    #[test]
    fn apply_set_topic_status_toggles_done() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMSTATUS01");
        drive_ops(
            &db,
            vec![
                WriteOp {
                    room_id: "ROOMSTATUS01".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("t", 0.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMSTATUS01".into(),
                    kind: WriteOpKind::SetTopicStatus {
                        topic_id: "t".into(),
                        status: TopicStatus::Done,
                    },
                },
            ],
        );
        let conn = db.get().unwrap();
        let status: String = conn
            .query_row("SELECT status FROM topics WHERE id='t'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "done");
    }

    #[test]
    fn apply_delete_topic_removes_row_and_cascades_children() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMDELET0001");
        drive_ops(
            &db,
            vec![
                WriteOp {
                    room_id: "ROOMDELET0001".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("p", 0.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMDELET0001".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: Topic {
                            parent_id: Some("p".into()),
                            ..topic("c", 1.0)
                        },
                    },
                },
                WriteOp {
                    room_id: "ROOMDELET0001".into(),
                    kind: WriteOpKind::DeleteTopic {
                        topic_id: "p".into(),
                    },
                },
            ],
        );
        let conn = db.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM topics WHERE room_id='ROOMDELET0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "parent + child should cascade");
    }

    #[test]
    fn apply_set_active_topic_writes_room_column() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMACTIVE001");
        drive_ops(
            &db,
            vec![
                WriteOp {
                    room_id: "ROOMACTIVE001".into(),
                    kind: WriteOpKind::UpsertTopic {
                        topic: topic("t", 0.0),
                    },
                },
                WriteOp {
                    room_id: "ROOMACTIVE001".into(),
                    kind: WriteOpKind::SetActiveTopic {
                        topic_id: Some("t".into()),
                    },
                },
            ],
        );
        {
            let conn = db.get().unwrap();
            let active: Option<String> = conn
                .query_row(
                    "SELECT active_topic_id FROM rooms WHERE id='ROOMACTIVE001'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(active.as_deref(), Some("t"));
        } // drop the pool checkout before driving more ops in :memory: mode

        drive_ops(
            &db,
            vec![WriteOp {
                room_id: "ROOMACTIVE001".into(),
                kind: WriteOpKind::SetActiveTopic { topic_id: None },
            }],
        );
        let conn = db.get().unwrap();
        let active: Option<String> = conn
            .query_row(
                "SELECT active_topic_id FROM rooms WHERE id='ROOMACTIVE001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(active.is_none(), "set_active_topic(None) should clear");
    }

    #[tokio::test]
    async fn writer_upsert_topic_replaces_existing_row() {
        let db = Db::open_in_memory().unwrap();
        seed_room(&db, "ROOMWRITER02");

        let handle = spawn_writer(db.clone());
        handle
            .tx
            .send(WriteOp {
                room_id: "ROOMWRITER02".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("t-1", 1.0),
                },
            })
            .unwrap();
        // Same id, different title + ord.
        let mut t = topic("t-1", 9.0);
        t.title = "renamed".into();
        handle
            .tx
            .send(WriteOp {
                room_id: "ROOMWRITER02".into(),
                kind: WriteOpKind::UpsertTopic { topic: t },
            })
            .unwrap();
        assert!(handle.shutdown().await);

        let conn = db.get().unwrap();
        let (title, ord): (String, f64) = conn
            .query_row(
                "SELECT title, ord FROM topics WHERE id='t-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "renamed");
        assert_eq!(ord, 9.0);
    }
}
