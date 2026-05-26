use crate::proto::{Board, BoardKind, ExcalidrawScene};
use serde_json::Value as JsonValue;

use super::{
    BoardId, ExcalidrawUpdateOutcome, PenAction, PenActionKind, PenBoardState, PenStroke,
    PenUndoOutcome, Room, StrokeId,
};

impl Room {
    pub fn boards(&self) -> Vec<Board> {
        let g = self.inner.lock().expect("room inner");
        g.boards.values().cloned().collect()
    }

    pub fn focused_board_id(&self) -> Option<BoardId> {
        let g = self.inner.lock().expect("room inner");
        g.focused_board_id.clone()
    }

    pub fn board_exists(&self, board_id: &str) -> bool {
        let g = self.inner.lock().expect("room inner");
        g.boards.contains_key(board_id)
    }

    pub fn create_board(&self, board: Board, _created_at: i64) {
        let mut g = self.inner.lock().expect("room inner");
        if board.kind == BoardKind::Excalidraw {
            g.excalidraw_scenes.insert(
                board.id.clone(),
                ExcalidrawScene {
                    board_id: board.id.clone(),
                    scene_version: 0,
                    elements: JsonValue::Array(vec![]),
                    app_state: JsonValue::Object(serde_json::Map::new()),
                },
            );
        } else if board.kind == BoardKind::Pen {
            g.pen_boards.entry(board.id.clone()).or_default();
        }
        g.boards.insert(board.id.clone(), board);
    }

    pub fn rename_board(&self, board_id: &str, title: String) -> Option<Board> {
        let mut g = self.inner.lock().expect("room inner");
        let board = g.boards.get_mut(board_id)?;
        board.title = title;
        Some(board.clone())
    }

    pub fn delete_board(&self, board_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        if g.boards.remove(board_id).is_none() {
            return false;
        }
        g.excalidraw_scenes.remove(board_id);
        g.pen_boards.remove(board_id);
        if g.focused_board_id.as_deref() == Some(board_id) {
            g.focused_board_id = None;
        }
        true
    }

    pub fn set_focused_board(&self, board_id: String) {
        let mut g = self.inner.lock().expect("room inner");
        g.focused_board_id = Some(board_id);
    }

    pub fn update_excalidraw_scene(
        &self,
        board_id: &str,
        scene_version: u64,
        elements: JsonValue,
        app_state: JsonValue,
        _updated_at: i64,
    ) -> ExcalidrawUpdateOutcome {
        let mut g = self.inner.lock().expect("room inner");
        let board = g.boards.get(board_id);
        if board.is_none() || board.as_ref().map(|b| &b.kind) != Some(&BoardKind::Excalidraw) {
            return ExcalidrawUpdateOutcome::BoardMissing;
        }
        let scene = g
            .excalidraw_scenes
            .entry(board_id.to_string())
            .or_insert_with(|| ExcalidrawScene {
                board_id: board_id.to_string(),
                scene_version: 0,
                elements: JsonValue::Array(vec![]),
                app_state: JsonValue::Object(serde_json::Map::new()),
            });
        if scene_version <= scene.scene_version {
            return ExcalidrawUpdateOutcome::Stale;
        }
        scene.scene_version = scene_version;
        scene.elements = elements;
        scene.app_state = app_state;
        ExcalidrawUpdateOutcome::Applied
    }

    pub fn get_excalidraw_scene(&self, board_id: &str) -> Option<ExcalidrawScene> {
        let g = self.inner.lock().expect("room inner");
        g.excalidraw_scenes.get(board_id).cloned()
    }

    pub fn get_excalidraw_scenes_needing_reset(&self) -> Vec<ExcalidrawScene> {
        let g = self.inner.lock().expect("room inner");
        g.excalidraw_scenes
            .iter()
            .filter(|(board_id, scene)| {
                let last_version = g
                    .excalidraw_last_broadcast_version
                    .get(board_id as &str)
                    .copied()
                    .unwrap_or(0);
                scene.scene_version > last_version
            })
            .map(|(_, scene)| scene.clone())
            .collect()
    }

    pub fn mark_excalidraw_scene_broadcast(&self, board_id: &str, scene_version: u64) {
        let mut g = self.inner.lock().expect("room inner");
        g.excalidraw_last_broadcast_version
            .insert(board_id.to_string(), scene_version);
    }

    pub fn get_pen_board_state(&self, board_id: &str) -> Option<PenBoardState> {
        let g = self.inner.lock().expect("room inner");
        g.pen_boards.get(board_id).cloned()
    }

    pub fn pen_begin_stroke(
        &self,
        board_id: &str,
        stroke_id: StrokeId,
        color: String,
        size: f64,
        now: i64,
    ) -> Option<PenStroke> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let ord = state.next_stroke_ord;
        state.next_stroke_ord += 1;
        let stroke = PenStroke {
            id: stroke_id.clone(),
            color: color.clone(),
            size,
            points: Vec::new(),
            ord,
            created_at: now,
        };
        state.strokes.push(stroke.clone());
        let action = PenAction {
            id: uuid::Uuid::new_v4().to_string(),
            kind: PenActionKind::StrokeBegin,
            target_id: Some(stroke_id),
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        Some(stroke)
    }

    pub fn pen_append_points(
        &self,
        board_id: &str,
        stroke_id: &str,
        points: Vec<[f32; 3]>,
    ) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let state = match g.pen_boards.get_mut(board_id) {
            Some(s) => s,
            None => return false,
        };
        let stroke = match state.strokes.iter_mut().find(|s| s.id == stroke_id) {
            Some(s) => s,
            None => return false,
        };
        stroke.points.extend(points);
        true
    }

    /// Finalize a stroke and return the snapshot the persistence layer
    /// needs (summary + the matching StrokeBegin action_id, which the
    /// writer will record on the `pen_actions` row and reuse for
    /// PenUndo lookups).
    pub fn pen_end_stroke(
        &self,
        board_id: &str,
        stroke_id: &str,
    ) -> Option<(crate::proto::PenStrokeSummary, String)> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let idx = state.strokes.iter().position(|s| s.id == stroke_id)?;
        state.strokes[idx].ord = state.next_stroke_ord;
        state.next_stroke_ord += 1;
        let s = &state.strokes[idx];
        let summary = crate::proto::PenStrokeSummary {
            id: s.id.clone(),
            color: s.color.clone(),
            size: s.size,
            points: s.points.clone(),
            created_at: s.created_at,
            ord: s.ord,
        };
        let action_id = state
            .action_log
            .iter()
            .rev()
            .find(|a| {
                a.kind == PenActionKind::StrokeBegin && a.target_id.as_deref() == Some(stroke_id)
            })
            .map(|a| a.id.clone())?;
        Some((summary, action_id))
    }

    /// Upsert a pen text. Returns `(action_id, prior)` so the writer
    /// can persist the new row and record the prior state in
    /// `pen_actions.payload_json` for durable undo.
    pub fn pen_text_upsert(
        &self,
        board_id: &str,
        text: crate::proto::PenText,
        now: i64,
    ) -> Option<(String, Option<crate::proto::PenText>)> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let text_id = text.id.clone();
        let prior = state.texts.iter().find(|t| t.id == text.id).cloned();
        if let Some(idx) = state.texts.iter().position(|t| t.id == text.id) {
            state.texts[idx] = text;
        } else {
            state.texts.push(text);
        }
        let action_id = uuid::Uuid::new_v4().to_string();
        let action = PenAction {
            id: action_id.clone(),
            kind: PenActionKind::TextSet,
            target_id: Some(text_id),
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        Some((action_id, prior))
    }

    /// Remove a pen text. Returns `(action_id, removed)` so the writer
    /// can stash the deleted text in `payload_json` for undo.
    pub fn pen_text_delete(
        &self,
        board_id: &str,
        text_id: &str,
        now: i64,
    ) -> Option<(String, crate::proto::PenText)> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let pos = state.texts.iter().position(|t| t.id == text_id)?;
        let removed = state.texts.remove(pos);
        let action_id = uuid::Uuid::new_v4().to_string();
        let action = PenAction {
            id: action_id.clone(),
            kind: PenActionKind::TextDelete,
            target_id: Some(removed.id.clone()),
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        Some((action_id, removed))
    }

    /// Clear all strokes + texts on a board. Returns
    /// `(action_id, prior_strokes, prior_texts)` so the writer can
    /// pack them into `payload_json` for undo.
    pub fn pen_clear(
        &self,
        board_id: &str,
        now: i64,
    ) -> Option<(String, Vec<PenStroke>, Vec<crate::proto::PenText>)> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let strokes = std::mem::take(&mut state.strokes);
        let texts = std::mem::take(&mut state.texts);
        let action_id = uuid::Uuid::new_v4().to_string();
        let action = PenAction {
            id: action_id.clone(),
            kind: PenActionKind::Clear,
            target_id: None,
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        Some((action_id, strokes, texts))
    }

    /// Pop the most recent action. Returns `Some(outcome)` carrying
    /// the popped action_id (so the writer can locate the
    /// `pen_actions` row and reverse it) and, for stroke_begin /
    /// text_set actions, the id of the in-memory stroke/text the
    /// caller should broadcast as removed. For text_delete / clear
    /// undos the writer rehydrates the in-memory state from
    /// `payload_json` on next process boot — see persistence.md
    /// §PenUndo limitations.
    pub fn pen_undo(&self, board_id: &str) -> Option<PenUndoOutcome> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let action = state.action_log.pop()?;
        let mut removed_stroke = None;
        let mut removed_text = None;
        match action.kind {
            PenActionKind::StrokeBegin => {
                if let Some(sid) = action.target_id.as_ref() {
                    state.strokes.retain(|s| &s.id != sid);
                    removed_stroke = action.target_id.clone();
                }
            }
            PenActionKind::TextSet => {
                if let Some(tid) = action.target_id.as_ref() {
                    state.texts.retain(|t| &t.id != tid);
                    removed_text = action.target_id.clone();
                }
            }
            PenActionKind::TextDelete | PenActionKind::Clear => {
                // In-memory state does not restore; writer's
                // apply_pen_undo rehydrates from payload_json.
            }
        }
        Some(PenUndoOutcome {
            action_id: action.id,
            removed_stroke,
            removed_text,
        })
    }

    pub fn load_pen_board_state(
        &self,
        board_id: &str,
        strokes: Vec<PenStroke>,
        texts: Vec<crate::proto::PenText>,
        action_log: Vec<PenAction>,
    ) {
        let mut g = self.inner.lock().expect("room inner");
        let max_stroke_ord = strokes.iter().map(|s| s.ord).max().unwrap_or(0);
        let state = PenBoardState {
            strokes,
            texts,
            action_log,
            next_stroke_ord: max_stroke_ord + 1,
        };
        g.pen_boards.insert(board_id.to_string(), state);
    }

    pub fn load_boards(
        &self,
        boards: Vec<Board>,
        excalidraw_scenes: Vec<ExcalidrawScene>,
        focused_board_id: Option<String>,
    ) {
        let mut g = self.inner.lock().expect("room inner");
        g.boards.clear();
        g.excalidraw_scenes.clear();
        g.pen_boards.clear();
        for b in boards {
            g.boards.insert(b.id.clone(), b);
        }
        for s in excalidraw_scenes {
            g.excalidraw_scenes.insert(s.board_id.clone(), s);
        }
        g.focused_board_id = focused_board_id;
    }
}
