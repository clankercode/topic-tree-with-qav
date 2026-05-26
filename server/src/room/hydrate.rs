use std::collections::HashMap;

use crate::db::{Db, DbError};
use crate::proto::{Board, BoardKind, ExcalidrawScene, Question, Topic, TopicStatus};

use super::{GuestId, PenAction, PenActionKind, PenStroke, QuestionId, Room, TopicId};

/// Read every persisted row for `room_id` inside one read transaction
/// and feed the result into the room's `load_*` setters. Called from
/// `RoomRegistry::get_or_create_hydrated` on first access to a room.
pub(super) fn hydrate_room_from_db(room: &Room, db: &Db, room_id: &str) -> Result<(), DbError> {
    let mut conn = db.get()?;
    let tx = conn.transaction()?;

    // 1. Room-level columns (active_topic_id, focused_board_id).
    let (active_topic_id, focused_board_id): (Option<String>, Option<String>) = tx.query_row(
        "SELECT active_topic_id, focused_board_id FROM rooms WHERE id = ?1",
        rusqlite::params![room_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    // 2. Topics, ordered by ord (load_topics later picks them up by id).
    let topics: Vec<Topic> = {
        let mut stmt = tx.prepare(
            "SELECT id, parent_id, title, ord, status, created_at FROM topics \
             WHERE room_id = ?1 ORDER BY ord",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            let status_str: String = r.get(4)?;
            Ok(Topic {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                title: r.get(2)?,
                ord: r.get(3)?,
                status: match status_str.as_str() {
                    "done" => TopicStatus::Done,
                    _ => TopicStatus::Pending,
                },
                created_at: r.get(5)?,
                vote_count: 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut topic_votes: HashMap<TopicId, Vec<GuestId>> = HashMap::new();
    {
        let mut stmt = tx.prepare(
            "SELECT v.topic_id, v.guest_id FROM topic_votes v \
             JOIN topics t ON t.id = v.topic_id WHERE t.room_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (tid, gid) = row?;
            topic_votes.entry(tid).or_default().push(gid);
        }
    }
    let topics: Vec<Topic> = topics
        .into_iter()
        .map(|mut t| {
            t.vote_count = topic_votes.get(&t.id).map(|v| v.len() as u32).unwrap_or(0);
            t
        })
        .collect();

    // 3. Questions + votes.
    let questions: Vec<Question> = {
        let mut stmt = tx.prepare(
            "SELECT id, author_guest_id, author_name, anonymous, text, answered, created_at \
             FROM questions WHERE room_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            Ok(Question {
                id: r.get(0)?,
                room_id: room_id.to_string(),
                author_guest_id: r.get(1)?,
                author_name: r.get(2)?,
                anonymous: r.get::<_, i32>(3)? != 0,
                text: r.get(4)?,
                answered: r.get::<_, i32>(5)? != 0,
                created_at: r.get(6)?,
                vote_count: 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut votes: HashMap<QuestionId, Vec<GuestId>> = HashMap::new();
    {
        let mut stmt = tx.prepare(
            "SELECT v.question_id, v.guest_id FROM question_votes v \
             JOIN questions q ON q.id = v.question_id WHERE q.room_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut counts: HashMap<QuestionId, u32> = HashMap::new();
        for row in rows {
            let (qid, gid) = row?;
            votes.entry(qid.clone()).or_default().push(gid);
            *counts.entry(qid).or_insert(0) += 1;
        }
        // vote_count is derived; merge in.
        // (We already collected questions above; mutate after the fact.)
        // No-op here since we directly build votes; load_questions
        // re-derives presence from vote_index.
        let _ = counts;
    }
    // Merge vote_count into questions before loading.
    let questions: Vec<Question> = questions
        .into_iter()
        .map(|mut q| {
            q.vote_count = votes.get(&q.id).map(|v| v.len() as u32).unwrap_or(0);
            q
        })
        .collect();

    // 4. Boards + excalidraw scenes.
    let boards: Vec<Board> = {
        let mut stmt = tx.prepare(
            "SELECT id, kind, title, ord, created_at FROM boards \
             WHERE room_id = ?1 ORDER BY ord",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            let kind_str: String = r.get(1)?;
            Ok(Board {
                id: r.get(0)?,
                kind: match kind_str.as_str() {
                    "excalidraw" => BoardKind::Excalidraw,
                    _ => BoardKind::Pen,
                },
                title: r.get(2)?,
                ord: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let scenes: Vec<ExcalidrawScene> = {
        let mut stmt = tx.prepare(
            "SELECT s.board_id, s.scene_version, s.elements_json, s.app_state_json \
             FROM excalidraw_scenes s \
             JOIN boards b ON b.id = s.board_id WHERE b.room_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![room_id], |r| {
            let board_id: String = r.get(0)?;
            let scene_version: i64 = r.get(1)?;
            let elements_json: String = r.get(2)?;
            let app_state_json: String = r.get(3)?;
            Ok(ExcalidrawScene {
                board_id,
                scene_version: scene_version as u64,
                elements: serde_json::from_str(&elements_json)
                    .unwrap_or(serde_json::Value::Array(vec![])),
                app_state: serde_json::from_str(&app_state_json)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // 5. Pen state per pen board.
    let pen_board_ids: Vec<String> = boards
        .iter()
        .filter(|b| matches!(b.kind, BoardKind::Pen))
        .map(|b| b.id.clone())
        .collect();
    struct PenLoad {
        board_id: String,
        strokes: Vec<PenStroke>,
        texts: Vec<crate::proto::PenText>,
        action_log: Vec<PenAction>,
    }
    let mut pen_loads: Vec<PenLoad> = Vec::new();
    for board_id in &pen_board_ids {
        let strokes: Vec<PenStroke> = {
            let mut stmt = tx.prepare(
                "SELECT id, color, size, points_json, ord, created_at FROM pen_strokes \
                 WHERE board_id = ?1 ORDER BY ord",
            )?;
            let rows = stmt.query_map(rusqlite::params![board_id], |r| {
                let pts_json: String = r.get(3)?;
                let points: Vec<[f32; 3]> = serde_json::from_str(&pts_json).unwrap_or_default();
                Ok(PenStroke {
                    id: r.get(0)?,
                    color: r.get(1)?,
                    size: r.get(2)?,
                    points,
                    ord: r.get::<_, i64>(4)? as u32,
                    created_at: r.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let texts: Vec<crate::proto::PenText> = {
            let mut stmt = tx.prepare(
                "SELECT id, x, y, text, font_size, color, updated_at FROM pen_texts \
                 WHERE board_id = ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![board_id], |r| {
                Ok(crate::proto::PenText {
                    id: r.get(0)?,
                    x: r.get(1)?,
                    y: r.get(2)?,
                    text: r.get(3)?,
                    font_size: r.get(4)?,
                    color: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        // Hydrate the action log so that PenUndo works across
        // restarts. Without this, undo-after-restart no-ops because
        // the in-memory log is empty even though pen_actions rows
        // exist on disk.
        let action_log: Vec<PenAction> = {
            let mut stmt = tx.prepare(
                "SELECT id, kind, target_id, ord, created_at FROM pen_actions \
                 WHERE board_id = ?1 ORDER BY ord",
            )?;
            let rows = stmt.query_map(rusqlite::params![board_id], |r| {
                let kind_s: String = r.get(1)?;
                let kind = match kind_s.as_str() {
                    "stroke_begin" => PenActionKind::StrokeBegin,
                    "text_set" => PenActionKind::TextSet,
                    "text_delete" => PenActionKind::TextDelete,
                    "clear" => PenActionKind::Clear,
                    _ => PenActionKind::StrokeBegin,
                };
                Ok(PenAction {
                    id: r.get(0)?,
                    kind,
                    target_id: r.get(2)?,
                    ord: r.get::<_, i64>(3)? as u32,
                    created_at: r.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        pen_loads.push(PenLoad {
            board_id: board_id.clone(),
            strokes,
            texts,
            action_log,
        });
    }

    tx.commit()?;
    drop(conn);

    // Push into the room's in-memory model. load_* setters are safe to
    // call in any order; load_boards must precede load_pen_board_state
    // because load_pen_board_state expects the board entry to exist.
    room.load_topics(topics, topic_votes, active_topic_id);
    room.load_questions(questions, votes);
    room.load_boards(boards, scenes, focused_board_id);
    for load in pen_loads {
        room.load_pen_board_state(&load.board_id, load.strokes, load.texts, load.action_log);
    }
    Ok(())
}
