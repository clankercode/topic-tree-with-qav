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
use crate::proto::{Board, BoardKind, PenStrokeSummary, PenText, Question, Topic, TopicStatus};

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
        let join_result =
            tokio::task::spawn_blocking(move || apply_batch_sync(&db_for_blocking, &batch)).await;
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
        WriteOpKind::UpsertQuestion { question } => {
            apply_upsert_question(tx, &op.room_id, question)
        }
        WriteOpKind::SetQuestionAnswered {
            question_id,
            answered,
        } => apply_set_question_answered(tx, question_id, *answered),
        WriteOpKind::DeleteQuestion { question_id } => apply_delete_question(tx, question_id),
        WriteOpKind::AddVote {
            question_id,
            guest_id,
            created_at,
        } => apply_add_vote(tx, question_id, guest_id, *created_at),
        WriteOpKind::RemoveVote {
            question_id,
            guest_id,
        } => apply_remove_vote(tx, question_id, guest_id),
        WriteOpKind::AddTopicVote {
            topic_id,
            guest_id,
            created_at,
        } => apply_add_topic_vote(tx, topic_id, guest_id, *created_at),
        WriteOpKind::RemoveTopicVote { topic_id, guest_id } => {
            apply_remove_topic_vote(tx, topic_id, guest_id)
        }
        WriteOpKind::PromoteQuestionToTopic { question_id, topic } => {
            // Atomicity is implied by the surrounding Transaction — both
            // rows land or neither does.
            apply_upsert_topic(tx, &op.room_id, topic)?;
            apply_delete_question(tx, question_id)
        }
        WriteOpKind::BulkUpsertTopics { topics } => {
            // Topics are pre-ordered so parents precede children — the
            // surrounding Transaction guarantees atomicity, so a FK
            // failure on row N rolls rows 0..N back together.
            for topic in topics {
                apply_upsert_topic(tx, &op.room_id, topic)?;
            }
            Ok(())
        }
        WriteOpKind::UpsertBoard { board } => apply_upsert_board(tx, &op.room_id, board),
        WriteOpKind::RenameBoard { board_id, title } => apply_rename_board(tx, board_id, title),
        WriteOpKind::DeleteBoard { board_id } => apply_delete_board(tx, board_id),
        WriteOpKind::SetFocusedBoard { board_id } => {
            apply_set_focused_board(tx, &op.room_id, board_id.as_deref())
        }
        WriteOpKind::UpsertExcalidrawScene {
            board_id,
            scene_version,
            elements_json,
            app_state_json,
            updated_at,
        } => apply_upsert_excalidraw_scene(
            tx,
            board_id,
            *scene_version,
            elements_json,
            app_state_json,
            *updated_at,
        ),
        WriteOpKind::SetKicked {
            guest_id,
            kicked,
            updated_at,
        } => apply_set_kicked(tx, &op.room_id, guest_id, *kicked, *updated_at),
        WriteOpKind::SetMuted {
            guest_id,
            muted,
            updated_at,
        } => apply_set_muted(tx, &op.room_id, guest_id, *muted, *updated_at),
        WriteOpKind::InsertCompletedPenStroke {
            board_id,
            stroke,
            action_id,
            created_at,
        } => apply_insert_completed_pen_stroke(tx, board_id, stroke, action_id, *created_at),
        WriteOpKind::UpsertPenText {
            board_id,
            text,
            action_id,
            before_json,
            created_at,
        } => apply_upsert_pen_text(
            tx,
            board_id,
            text,
            action_id,
            before_json.as_deref(),
            *created_at,
        ),
        WriteOpKind::DeletePenText {
            board_id,
            text_id,
            action_id,
            before_json,
            created_at,
        } => apply_delete_pen_text(tx, board_id, text_id, action_id, before_json, *created_at),
        WriteOpKind::PenClear {
            board_id,
            action_id,
            before_strokes_json,
            before_texts_json,
            created_at,
        } => apply_pen_clear(
            tx,
            board_id,
            action_id,
            before_strokes_json,
            before_texts_json,
            *created_at,
        ),
        WriteOpKind::PenUndo {
            board_id,
            target_action_id,
        } => apply_pen_undo(tx, board_id, target_action_id),
    }
}

fn topic_status_str(s: TopicStatus) -> &'static str {
    match s {
        TopicStatus::Pending => "pending",
        TopicStatus::Done => "done",
    }
}

fn apply_upsert_topic(tx: &Transaction<'_>, room_id: &str, topic: &Topic) -> Result<(), DbError> {
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

fn apply_rename_topic(tx: &Transaction<'_>, topic_id: &str, title: &str) -> Result<(), DbError> {
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

fn apply_upsert_question(tx: &Transaction<'_>, room_id: &str, q: &Question) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO questions (id, room_id, author_guest_id, author_name, anonymous, text, \
                                answered, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET \
           author_guest_id = excluded.author_guest_id, \
           author_name     = excluded.author_name, \
           anonymous       = excluded.anonymous, \
           text            = excluded.text, \
           answered        = excluded.answered",
        rusqlite::params![
            q.id,
            room_id,
            q.author_guest_id,
            q.author_name,
            q.anonymous as i32,
            q.text,
            q.answered as i32,
            q.created_at,
        ],
    )?;
    Ok(())
}

fn apply_set_question_answered(
    tx: &Transaction<'_>,
    question_id: &str,
    answered: bool,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE questions SET answered = ?1 WHERE id = ?2",
        rusqlite::params![answered as i32, question_id],
    )?;
    Ok(())
}

fn apply_delete_question(tx: &Transaction<'_>, question_id: &str) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM questions WHERE id = ?1",
        rusqlite::params![question_id],
    )?;
    Ok(())
}

fn apply_add_vote(
    tx: &Transaction<'_>,
    question_id: &str,
    guest_id: &str,
    created_at: i64,
) -> Result<(), DbError> {
    // dedup is enforced by the PK (question_id, guest_id).
    tx.execute(
        "INSERT OR IGNORE INTO question_votes (question_id, guest_id, created_at) \
         VALUES (?1, ?2, ?3)",
        rusqlite::params![question_id, guest_id, created_at],
    )?;
    Ok(())
}

fn apply_remove_vote(
    tx: &Transaction<'_>,
    question_id: &str,
    guest_id: &str,
) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM question_votes WHERE question_id = ?1 AND guest_id = ?2",
        rusqlite::params![question_id, guest_id],
    )?;
    Ok(())
}

fn apply_add_topic_vote(
    tx: &Transaction<'_>,
    topic_id: &str,
    guest_id: &str,
    created_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT OR IGNORE INTO topic_votes (topic_id, guest_id, created_at) \
         VALUES (?1, ?2, ?3)",
        rusqlite::params![topic_id, guest_id, created_at],
    )?;
    Ok(())
}

fn apply_remove_topic_vote(
    tx: &Transaction<'_>,
    topic_id: &str,
    guest_id: &str,
) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM topic_votes WHERE topic_id = ?1 AND guest_id = ?2",
        rusqlite::params![topic_id, guest_id],
    )?;
    Ok(())
}

fn board_kind_str(k: &BoardKind) -> &'static str {
    match k {
        BoardKind::Pen => "pen",
        BoardKind::Excalidraw => "excalidraw",
    }
}

fn apply_upsert_board(tx: &Transaction<'_>, room_id: &str, b: &Board) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO boards (id, room_id, kind, title, ord, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(id) DO UPDATE SET \
           title = excluded.title, \
           ord   = excluded.ord",
        rusqlite::params![
            b.id,
            room_id,
            board_kind_str(&b.kind),
            b.title,
            b.ord,
            b.created_at,
        ],
    )?;
    Ok(())
}

fn apply_rename_board(tx: &Transaction<'_>, board_id: &str, title: &str) -> Result<(), DbError> {
    tx.execute(
        "UPDATE boards SET title = ?1 WHERE id = ?2",
        rusqlite::params![title, board_id],
    )?;
    Ok(())
}

fn apply_delete_board(tx: &Transaction<'_>, board_id: &str) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM boards WHERE id = ?1",
        rusqlite::params![board_id],
    )?;
    Ok(())
}

fn apply_set_focused_board(
    tx: &Transaction<'_>,
    room_id: &str,
    board_id: Option<&str>,
) -> Result<(), DbError> {
    tx.execute(
        "UPDATE rooms SET focused_board_id = ?1 WHERE id = ?2",
        rusqlite::params![board_id, room_id],
    )?;
    Ok(())
}

fn apply_upsert_excalidraw_scene(
    tx: &Transaction<'_>,
    board_id: &str,
    scene_version: u64,
    elements_json: &str,
    app_state_json: &str,
    updated_at: i64,
) -> Result<(), DbError> {
    // scene_version monotonically grows on the client; the writer just
    // accepts what the handler sent. INSERT … ON CONFLICT does the
    // upsert without two round-trips.
    tx.execute(
        "INSERT INTO excalidraw_scenes (board_id, scene_version, elements_json, \
                                        app_state_json, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(board_id) DO UPDATE SET \
           scene_version  = excluded.scene_version, \
           elements_json  = excluded.elements_json, \
           app_state_json = excluded.app_state_json, \
           updated_at     = excluded.updated_at",
        rusqlite::params![
            board_id,
            scene_version as i64,
            elements_json,
            app_state_json,
            updated_at
        ],
    )?;
    Ok(())
}

fn apply_set_kicked(
    tx: &Transaction<'_>,
    room_id: &str,
    guest_id: &str,
    kicked: bool,
    updated_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO moderation (room_id, guest_id, kicked, muted, updated_at) \
         VALUES (?1, ?2, ?3, 0, ?4) \
         ON CONFLICT(room_id, guest_id) DO UPDATE SET \
           kicked     = ?3, \
           updated_at = ?4",
        rusqlite::params![room_id, guest_id, kicked as i32, updated_at],
    )?;
    Ok(())
}

fn apply_set_muted(
    tx: &Transaction<'_>,
    room_id: &str,
    guest_id: &str,
    muted: bool,
    updated_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO moderation (room_id, guest_id, kicked, muted, updated_at) \
         VALUES (?1, ?2, 0, ?3, ?4) \
         ON CONFLICT(room_id, guest_id) DO UPDATE SET \
           muted      = ?3, \
           updated_at = ?4",
        rusqlite::params![room_id, guest_id, muted as i32, updated_at],
    )?;
    Ok(())
}

// ─────────────── Pen ───────────────
//
// Same-transaction invariant: each variant writes BOTH the data row AND
// the matching pen_actions row (incl. payload_json for undo) inside the
// surrounding tx. Splitting them would corrupt undo state. See
// `.plan/2026-05-25-followup/persistence.md` §3 Pen.

fn next_pen_action_ord(tx: &Transaction<'_>, board_id: &str) -> Result<i64, DbError> {
    let max: Option<i64> = tx
        .query_row(
            "SELECT MAX(ord) FROM pen_actions WHERE board_id = ?1",
            rusqlite::params![board_id],
            |r| r.get(0),
        )
        .ok();
    Ok(max.unwrap_or(0) + 1)
}

fn write_pen_action(
    tx: &Transaction<'_>,
    action_id: &str,
    board_id: &str,
    kind: &str,
    target_id: Option<&str>,
    payload_json: Option<&str>,
    created_at: i64,
) -> Result<(), DbError> {
    let ord = next_pen_action_ord(tx, board_id)?;
    tx.execute(
        "INSERT INTO pen_actions (id, board_id, kind, target_id, ord, created_at, payload_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            action_id,
            board_id,
            kind,
            target_id,
            ord,
            created_at,
            payload_json
        ],
    )?;
    Ok(())
}

fn apply_insert_completed_pen_stroke(
    tx: &Transaction<'_>,
    board_id: &str,
    stroke: &PenStrokeSummary,
    action_id: &str,
    created_at: i64,
) -> Result<(), DbError> {
    let points_json = serde_json::to_string(&stroke.points)
        .map_err(|e| DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?;
    tx.execute(
        "INSERT INTO pen_strokes (id, board_id, color, size, points_json, ord, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            stroke.id,
            board_id,
            stroke.color,
            stroke.size,
            points_json,
            stroke.ord as i64,
            stroke.created_at,
        ],
    )?;
    write_pen_action(
        tx,
        action_id,
        board_id,
        "stroke_begin",
        Some(&stroke.id),
        None,
        created_at,
    )?;
    Ok(())
}

fn apply_upsert_pen_text(
    tx: &Transaction<'_>,
    board_id: &str,
    text: &PenText,
    action_id: &str,
    before_json: Option<&str>,
    created_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO pen_texts (id, board_id, x, y, text, font_size, color, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET \
           x          = excluded.x, \
           y          = excluded.y, \
           text       = excluded.text, \
           font_size  = excluded.font_size, \
           color      = excluded.color, \
           updated_at = excluded.updated_at",
        rusqlite::params![
            text.id,
            board_id,
            text.x,
            text.y,
            text.text,
            text.font_size,
            text.color,
            text.updated_at,
        ],
    )?;
    write_pen_action(
        tx,
        action_id,
        board_id,
        "text_set",
        Some(&text.id),
        before_json,
        created_at,
    )?;
    Ok(())
}

fn apply_delete_pen_text(
    tx: &Transaction<'_>,
    board_id: &str,
    text_id: &str,
    action_id: &str,
    before_json: &str,
    created_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM pen_texts WHERE id = ?1 AND board_id = ?2",
        rusqlite::params![text_id, board_id],
    )?;
    write_pen_action(
        tx,
        action_id,
        board_id,
        "text_delete",
        Some(text_id),
        Some(before_json),
        created_at,
    )?;
    Ok(())
}

fn apply_pen_clear(
    tx: &Transaction<'_>,
    board_id: &str,
    action_id: &str,
    before_strokes_json: &str,
    before_texts_json: &str,
    created_at: i64,
) -> Result<(), DbError> {
    tx.execute(
        "DELETE FROM pen_strokes WHERE board_id = ?1",
        rusqlite::params![board_id],
    )?;
    tx.execute(
        "DELETE FROM pen_texts WHERE board_id = ?1",
        rusqlite::params![board_id],
    )?;
    let payload = serde_json::json!({
        "strokes": serde_json::from_str::<serde_json::Value>(before_strokes_json).unwrap_or(serde_json::Value::Array(vec![])),
        "texts": serde_json::from_str::<serde_json::Value>(before_texts_json).unwrap_or(serde_json::Value::Array(vec![])),
    });
    let payload_str = payload.to_string();
    write_pen_action(
        tx,
        action_id,
        board_id,
        "clear",
        None,
        Some(&payload_str),
        created_at,
    )?;
    Ok(())
}

fn apply_pen_undo(
    tx: &Transaction<'_>,
    board_id: &str,
    target_action_id: &str,
) -> Result<(), DbError> {
    // Read the action's snapshot first; abort if the row is gone (no-op).
    let (kind, target_id, payload): (String, Option<String>, Option<String>) = match tx.query_row(
        "SELECT kind, target_id, payload_json FROM pen_actions \
             WHERE id = ?1 AND board_id = ?2",
        rusqlite::params![target_action_id, board_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ) {
        Ok(row) => row,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    match kind.as_str() {
        "stroke_begin" => {
            if let Some(stroke_id) = target_id {
                tx.execute(
                    "DELETE FROM pen_strokes WHERE id = ?1 AND board_id = ?2",
                    rusqlite::params![stroke_id, board_id],
                )?;
            }
        }
        "text_set" => match (target_id.as_deref(), payload.as_deref()) {
            (Some(text_id), Some(prev)) => {
                // Restore the prior text state.
                let prev_text: PenText = serde_json::from_str(prev).map_err(|e| {
                    DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                })?;
                tx.execute(
                    "INSERT INTO pen_texts (id, board_id, x, y, text, font_size, color, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                     ON CONFLICT(id) DO UPDATE SET \
                       x = excluded.x, y = excluded.y, text = excluded.text, \
                       font_size = excluded.font_size, color = excluded.color, \
                       updated_at = excluded.updated_at",
                    rusqlite::params![
                        prev_text.id,
                        board_id,
                        prev_text.x,
                        prev_text.y,
                        prev_text.text,
                        prev_text.font_size,
                        prev_text.color,
                        prev_text.updated_at,
                    ],
                )?;
                // Belt-and-braces: if the prev row had a different id (it shouldn't),
                // also drop the post-action row.
                if prev_text.id != text_id {
                    tx.execute(
                        "DELETE FROM pen_texts WHERE id = ?1 AND board_id = ?2",
                        rusqlite::params![text_id, board_id],
                    )?;
                }
            }
            (Some(text_id), None) => {
                // No prior state → the upsert was a new insert; delete it.
                tx.execute(
                    "DELETE FROM pen_texts WHERE id = ?1 AND board_id = ?2",
                    rusqlite::params![text_id, board_id],
                )?;
            }
            _ => {}
        },
        "text_delete" => {
            if let Some(prev_json) = payload {
                let prev: PenText = serde_json::from_str(&prev_json).map_err(|e| {
                    DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                })?;
                tx.execute(
                    "INSERT INTO pen_texts (id, board_id, x, y, text, font_size, color, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                     ON CONFLICT(id) DO UPDATE SET \
                       x = excluded.x, y = excluded.y, text = excluded.text, \
                       font_size = excluded.font_size, color = excluded.color, \
                       updated_at = excluded.updated_at",
                    rusqlite::params![
                        prev.id,
                        board_id,
                        prev.x,
                        prev.y,
                        prev.text,
                        prev.font_size,
                        prev.color,
                        prev.updated_at,
                    ],
                )?;
            }
        }
        "clear" => {
            if let Some(p) = payload {
                let v: serde_json::Value = serde_json::from_str(&p).map_err(|e| {
                    DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                })?;
                if let Some(strokes) = v.get("strokes").and_then(|x| x.as_array()) {
                    for s in strokes {
                        let stroke: PenStrokeSummary =
                            serde_json::from_value(s.clone()).map_err(|e| {
                                DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(
                                    e,
                                )))
                            })?;
                        let pts = serde_json::to_string(&stroke.points).map_err(|e| {
                            DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                        })?;
                        tx.execute(
                            "INSERT OR REPLACE INTO pen_strokes \
                               (id, board_id, color, size, points_json, ord, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            rusqlite::params![
                                stroke.id,
                                board_id,
                                stroke.color,
                                stroke.size,
                                pts,
                                stroke.ord as i64,
                                stroke.created_at,
                            ],
                        )?;
                    }
                }
                if let Some(texts) = v.get("texts").and_then(|x| x.as_array()) {
                    for t in texts {
                        let text: PenText = serde_json::from_value(t.clone()).map_err(|e| {
                            DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                        })?;
                        tx.execute(
                            "INSERT OR REPLACE INTO pen_texts \
                               (id, board_id, x, y, text, font_size, color, updated_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            rusqlite::params![
                                text.id,
                                board_id,
                                text.x,
                                text.y,
                                text.text,
                                text.font_size,
                                text.color,
                                text.updated_at,
                            ],
                        )?;
                    }
                }
            }
        }
        other => {
            tracing::warn!(kind = %other, target_action_id, "pen_undo: unknown action kind");
        }
    }

    // Finally, delete the action row itself.
    tx.execute(
        "DELETE FROM pen_actions WHERE id = ?1 AND board_id = ?2",
        rusqlite::params![target_action_id, board_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
