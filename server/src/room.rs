//! Per-room in-memory state + registry.
//!
//! M1 keeps this deliberately small: presence (active client ids per
//! guest), a per-room monotonic `seq`, and a tokio broadcast channel for
//! fan-out. Topic/question/board state lands in later phases through the
//! same `Room` struct.
//!
//! The registry lazily creates rooms on first ws connect and keeps them
//! alive until either explicit eviction (later: idle reaper) or process
//! shutdown.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::api::now_ms;
use crate::db::{Db, DbError};
use crate::proto::{
    Board, BoardKind, ExcalidrawScene, Guest, Presence, Question, RaisedHand, RoomSnapshot,
    RoomSummary, ServerMsg, Topic, TopicStatus, You,
};
use serde_json::Value as JsonValue;

/// Broadcast channel capacity per room. If a writer lags more than this
/// many messages, its receiver receives `RecvError::Lagged` and the
/// client must resync via `GetSnapshot` (handled in the ws layer).
const BROADCAST_CAPACITY: usize = 256;

pub type ClientId = String;
pub type GuestId = String;
pub type TopicId = String;
pub type QuestionId = String;
pub type BoardId = String;
pub type StrokeId = String;
pub type TextId = String;

#[derive(Debug, Clone, PartialEq)]
pub struct PenStroke {
    pub id: StrokeId,
    pub color: String,
    pub size: f64,
    pub points: Vec<[f32; 3]>,
    pub ord: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PenBoardState {
    pub strokes: Vec<PenStroke>,
    pub texts: Vec<crate::proto::PenText>,
    pub action_log: Vec<PenAction>,
    pub next_stroke_ord: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PenActionKind {
    StrokeBegin,
    TextSet,
    TextDelete,
    Clear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenAction {
    pub id: String,
    pub kind: PenActionKind,
    pub target_id: Option<String>,
    pub ord: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct PresenceEntry {
    pub guest_id: GuestId,
    pub display_name: String,
    pub muted: bool,
    pub joined_at: i64,
    pub client_ids: Vec<ClientId>,
}

impl PresenceEntry {
    pub fn to_proto_guest(&self) -> Guest {
        Guest {
            guest_id: self.guest_id.clone(),
            display_name: self.display_name.clone(),
            muted: self.muted,
            joined_at: self.joined_at,
        }
    }

    pub fn to_proto_presence(&self) -> Presence {
        Presence {
            guest_id: self.guest_id.clone(),
            display_name: self.display_name.clone(),
            muted: self.muted,
            joined_at: self.joined_at,
            client_ids: self.client_ids.clone(),
        }
    }
}

pub struct Room {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    /// Wall-clock ms of the last meaningful activity on this room.
    /// Bumped on `get_or_create_hydrated`, every inbound/outbound ws
    /// frame, and explicit `touch` calls. Read by the idle reaper.
    last_activity_at: AtomicI64,
    inner: Mutex<RoomInner>,
    pub broadcast: broadcast::Sender<ServerMsg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcalidrawUpdateOutcome {
    Applied,
    Stale,
    BoardMissing,
}

struct RoomInner {
    seq: u64,
    presence: BTreeMap<GuestId, PresenceEntry>,
    /// Guest ids the host has kicked since the room booted. Authoritative
    /// in-memory copy that takes precedence over the (fire-and-forget)
    /// moderation DB write, so a fast reconnect cannot race the kick.
    kicked_guests: HashSet<GuestId>,
    topics: BTreeMap<TopicId, Topic>,
    active_topic_id: Option<TopicId>,
    questions: BTreeMap<QuestionId, Question>,
    vote_index: HashMap<QuestionId, HashSet<GuestId>>,
    boards: BTreeMap<BoardId, Board>,
    excalidraw_scenes: BTreeMap<BoardId, ExcalidrawScene>,
    excalidraw_last_broadcast_version: HashMap<BoardId, u64>,
    focused_board_id: Option<BoardId>,
    hands: BTreeMap<GuestId, RaisedHand>,
    pen_boards: HashMap<BoardId, PenBoardState>,
}

impl Room {
    fn new(id: String, title: String, created_at: i64) -> Self {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            id,
            title,
            created_at,
            last_activity_at: AtomicI64::new(now_ms()),
            inner: Mutex::new(RoomInner {
                seq: 0,
                presence: BTreeMap::new(),
                kicked_guests: HashSet::new(),
                topics: BTreeMap::new(),
                active_topic_id: None,
                questions: BTreeMap::new(),
                vote_index: HashMap::new(),
                boards: BTreeMap::new(),
                excalidraw_scenes: BTreeMap::new(),
                excalidraw_last_broadcast_version: HashMap::new(),
                focused_board_id: None,
                hands: BTreeMap::new(),
                pen_boards: HashMap::new(),
            }),
            broadcast: tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.broadcast.subscribe()
    }

    /// Update the activity timestamp. Cheap (single relaxed atomic
    /// store). Call on every inbound ws frame and on outbound broadcast
    /// emissions.
    pub fn touch(&self, now_ms: i64) {
        self.last_activity_at.store(now_ms, Ordering::Relaxed);
    }

    pub fn last_activity_at(&self) -> i64 {
        self.last_activity_at.load(Ordering::Relaxed)
    }

    /// Number of currently connected clients across all guests. Used by
    /// the idle reaper.
    pub fn connected_client_count(&self) -> usize {
        let g = self.inner.lock().expect("room inner");
        g.presence.values().map(|p| p.client_ids.len()).sum()
    }

    /// Allocate the next per-room sequence number. Always non-zero on
    /// first call (we start at 1 so Welcome's high-water can be zero
    /// "nothing emitted yet" when desired).
    pub fn next_seq(&self) -> u64 {
        let mut g = self.inner.lock().expect("room inner");
        g.seq += 1;
        g.seq
    }

    pub fn current_seq(&self) -> u64 {
        self.inner.lock().expect("room inner").seq
    }

    /// Register a client as connected under a guest id. Returns true if
    /// this is the guest's *first* active client (i.e. presence list grew).
    pub fn add_client(
        &self,
        guest_id: GuestId,
        client_id: ClientId,
        display_name: String,
        joined_at: i64,
        muted: bool,
    ) -> bool {
        self.touch(now_ms());
        let mut g = self.inner.lock().expect("room inner");
        if let Some(p) = g.presence.get_mut(&guest_id) {
            if !p.client_ids.contains(&client_id) {
                p.client_ids.push(client_id);
            }
            // Update display name if it changed (handles
            // disconnect-then-reconnect-with-new-name).
            if !display_name.is_empty() {
                p.display_name = display_name;
            }
            return false;
        }
        g.presence.insert(
            guest_id.clone(),
            PresenceEntry {
                guest_id,
                display_name,
                muted,
                joined_at,
                client_ids: vec![client_id],
            },
        );
        true
    }

    /// Returns true if the guest is now fully disconnected (no remaining
    /// clients).
    pub fn remove_client(&self, guest_id: &str, client_id: &str) -> bool {
        self.touch(now_ms());
        let mut g = self.inner.lock().expect("room inner");
        let Some(p) = g.presence.get_mut(guest_id) else {
            return false;
        };
        p.client_ids.retain(|c| c != client_id);
        if p.client_ids.is_empty() {
            g.presence.remove(guest_id);
            true
        } else {
            false
        }
    }

    /// Returns true if the name changed.
    pub fn set_display_name(&self, guest_id: &str, name: String) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let Some(p) = g.presence.get_mut(guest_id) else {
            return false;
        };
        if p.display_name == name {
            return false;
        }
        p.display_name = name;
        true
    }

    pub fn set_muted(&self, guest_id: &str, muted: bool) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let Some(p) = g.presence.get_mut(guest_id) else {
            return false;
        };
        p.muted = muted;
        true
    }

    pub fn is_muted(&self, guest_id: &str) -> bool {
        let g = self.inner.lock().expect("room inner");
        g.presence.get(guest_id).map(|p| p.muted).unwrap_or(false)
    }

    /// Removes a guest from presence (for kick). Returns client_ids that were removed.
    pub fn kick_guest(&self, guest_id: &str) -> Vec<ClientId> {
        let mut g = self.inner.lock().expect("room inner");
        g.kicked_guests.insert(guest_id.to_string());
        if let Some(p) = g.presence.remove(guest_id) {
            p.client_ids
        } else {
            vec![]
        }
    }

    /// True iff the host has kicked this guest in the current process
    /// lifetime. Used as defense-in-depth alongside the moderation DB
    /// row so a fast reconnect cannot race the persisted kick.
    pub fn is_kicked(&self, guest_id: &str) -> bool {
        let g = self.inner.lock().expect("room inner");
        g.kicked_guests.contains(guest_id)
    }

    pub fn guests(&self) -> Vec<Guest> {
        let g = self.inner.lock().expect("room inner");
        g.presence.values().map(|p| p.to_proto_guest()).collect()
    }

    pub fn presence(&self) -> Vec<Presence> {
        let g = self.inner.lock().expect("room inner");
        g.presence.values().map(|p| p.to_proto_presence()).collect()
    }

    /// True iff the guest currently has at least one live connection
    /// (i.e. they have not been kicked since connecting).
    pub fn has_presence(&self, guest_id: &str) -> bool {
        let g = self.inner.lock().expect("room inner");
        g.presence.contains_key(guest_id)
    }

    pub fn topics(&self) -> Vec<Topic> {
        let g = self.inner.lock().expect("room inner");
        g.topics.values().cloned().collect()
    }

    pub fn active_topic_id(&self) -> Option<TopicId> {
        let g = self.inner.lock().expect("room inner");
        g.active_topic_id.clone()
    }

    pub fn add_topic(&self, topic: Topic) {
        let mut g = self.inner.lock().expect("room inner");
        g.topics.insert(topic.id.clone(), topic);
    }

    pub fn rename_topic(&self, topic_id: &str, title: String) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let Some(t) = g.topics.get_mut(topic_id) else {
            return false;
        };
        t.title = title;
        true
    }

    pub fn move_topic(&self, topic_id: &str, new_parent_id: Option<String>, new_ord: f64) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        if !g.topics.contains_key(topic_id) {
            return false;
        }
        if let Some(ref parent) = new_parent_id {
            if parent == topic_id {
                return false;
            }
            let mut cursor: Option<&str> = Some(parent.as_str());
            while let Some(p) = cursor {
                if p == topic_id {
                    return false;
                }
                cursor = g.topics.get(p).and_then(|t| t.parent_id.as_deref());
            }
        }
        if let Some(t) = g.topics.get_mut(topic_id) {
            t.parent_id = new_parent_id;
            t.ord = new_ord;
            true
        } else {
            false
        }
    }

    pub fn delete_topic(&self, topic_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        if !g.topics.contains_key(topic_id) {
            return false;
        }
        let mut to_remove = vec![topic_id.to_string()];
        let mut idx = 0;
        while idx < to_remove.len() {
            let current = to_remove[idx].clone();
            for (id, t) in g.topics.iter() {
                if t.parent_id.as_deref() == Some(current.as_str()) {
                    to_remove.push(id.clone());
                }
            }
            idx += 1;
        }
        for id in &to_remove {
            g.topics.remove(id);
        }
        if g.active_topic_id
            .as_ref()
            .map(|id| to_remove.contains(id))
            .unwrap_or(false)
        {
            g.active_topic_id = None;
        }
        true
    }

    pub fn set_active_topic(&self, topic_id: Option<String>) {
        let mut g = self.inner.lock().expect("room inner");
        let prev_id = g.active_topic_id.clone();
        if let Some(prev) = prev_id {
            let should_mark_done = match &topic_id {
                Some(new_id) => prev != *new_id,
                None => true,
            };
            if should_mark_done {
                if let Some(t) = g.topics.get_mut(&prev) {
                    t.status = TopicStatus::Done;
                }
            }
        }
        g.active_topic_id = topic_id;
    }

    pub fn mark_topic_done(&self, topic_id: &str, done: bool) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let Some(t) = g.topics.get_mut(topic_id) else {
            return false;
        };
        t.status = if done {
            TopicStatus::Done
        } else {
            TopicStatus::Pending
        };
        true
    }

    pub fn load_topics(&self, topics: Vec<Topic>, active_topic_id: Option<String>) {
        let mut g = self.inner.lock().expect("room inner");
        g.topics.clear();
        for t in topics {
            g.topics.insert(t.id.clone(), t);
        }
        g.active_topic_id = active_topic_id;
    }

    pub fn questions(&self) -> Vec<Question> {
        let g = self.inner.lock().expect("room inner");
        g.questions.values().cloned().collect()
    }

    pub fn my_votes(&self, my_guest_id: &str) -> Vec<String> {
        let g = self.inner.lock().expect("room inner");
        let mut voted = Vec::new();
        for (qid, voters) in &g.vote_index {
            if voters.contains(my_guest_id) {
                voted.push(qid.clone());
            }
        }
        voted
    }

    pub fn add_question(&self, question: Question) {
        let mut g = self.inner.lock().expect("room inner");
        g.questions.insert(question.id.clone(), question.clone());
        g.vote_index.entry(question.id.clone()).or_default();
    }

    pub fn get_question(&self, question_id: &str) -> Option<Question> {
        let g = self.inner.lock().expect("room inner");
        g.questions.get(question_id).cloned()
    }

    pub fn update_question(&self, question: Question) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        if !g.questions.contains_key(&question.id) {
            return false;
        }
        g.questions.insert(question.id.clone(), question);
        true
    }

    pub fn delete_question(&self, question_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let removed = g.questions.remove(question_id).is_some();
        if removed {
            g.vote_index.remove(question_id);
        }
        removed
    }

    pub fn vote_question(
        &self,
        question_id: &str,
        guest_id: &str,
        vote: bool,
    ) -> Option<(u32, bool)> {
        let mut g = self.inner.lock().expect("room inner");
        let was_voted = g
            .vote_index
            .get(question_id)
            .map(|voters| voters.contains(guest_id))
            .unwrap_or(false);
        let changed = vote != was_voted;
        if !changed {
            return Some((g.questions.get(question_id)?.vote_count, false));
        }
        let new_count = if vote {
            let voters = g.vote_index.entry(question_id.to_string()).or_default();
            voters.insert(guest_id.to_string());
            voters.len() as u32
        } else {
            let voters = g.vote_index.entry(question_id.to_string()).or_default();
            voters.remove(guest_id);
            voters.len() as u32
        };
        if let Some(q) = g.questions.get_mut(question_id) {
            q.vote_count = new_count;
        }
        Some((new_count, true))
    }

    pub fn load_questions(
        &self,
        questions: Vec<Question>,
        votes: HashMap<QuestionId, Vec<GuestId>>,
    ) {
        let mut g = self.inner.lock().expect("room inner");
        g.questions.clear();
        g.vote_index.clear();
        for q in questions {
            g.questions.insert(q.id.clone(), q);
        }
        for (qid, voters) in votes {
            g.vote_index.insert(qid, voters.into_iter().collect());
        }
    }

    pub fn hands_list(&self) -> Vec<RaisedHand> {
        let g = self.inner.lock().expect("room inner");
        g.hands.values().cloned().collect()
    }

    pub fn raise_hand(&self, guest_id: &str, display_name: String, topic: String, raised_at: i64) {
        let mut g = self.inner.lock().expect("room inner");
        g.hands.insert(
            guest_id.to_string(),
            RaisedHand {
                guest_id: guest_id.to_string(),
                display_name,
                topic,
                raised_at,
            },
        );
    }

    pub fn lower_hand(&self, guest_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        g.hands.remove(guest_id).is_some()
    }

    pub fn call_on_hand(&self, guest_id: &str) -> Option<RaisedHand> {
        let mut g = self.inner.lock().expect("room inner");
        g.hands.remove(guest_id)
    }

    pub fn dismiss_hand(&self, guest_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        g.hands.remove(guest_id).is_some()
    }

    pub fn promote_question_to_topic(
        &self,
        question_id: &str,
        parent_topic_id: Option<String>,
        after_topic_id: Option<String>,
    ) -> Option<(Question, Topic)> {
        let mut g = self.inner.lock().expect("room inner");
        let question = g.questions.remove(question_id)?;
        g.vote_index.remove(question_id);
        let title = question.text.chars().take(80).collect::<String>();
        let now = now_ms();
        let new_ord = if let Some(after_id) = &after_topic_id {
            g.topics.get(after_id).map(|t| t.ord + 0.5).unwrap_or(1.0)
        } else {
            g.topics
                .values()
                .map(|t| t.ord)
                .fold(0.0, |max, o| if o > max { o } else { max })
                + 1.0
        };
        let topic_id = uuid::Uuid::new_v4().to_string();
        let topic = Topic {
            id: topic_id.clone(),
            parent_id: parent_topic_id,
            title,
            ord: new_ord,
            status: TopicStatus::Pending,
            created_at: now,
        };
        g.topics.insert(topic_id, topic.clone());
        Some((question, topic))
    }

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

    pub fn pen_end_stroke(&self, board_id: &str, stroke_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let state = match g.pen_boards.get_mut(board_id) {
            Some(s) => s,
            None => return false,
        };
        if let Some(idx) = state.strokes.iter().position(|s| s.id == stroke_id) {
            state.strokes[idx].ord = state.next_stroke_ord;
            state.next_stroke_ord += 1;
            true
        } else {
            false
        }
    }

    pub fn pen_text_upsert(&self, board_id: &str, text: crate::proto::PenText, now: i64) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let state = match g.pen_boards.get_mut(board_id) {
            Some(s) => s,
            None => return false,
        };
        let text_id = text.id.clone();
        if let Some(idx) = state.texts.iter().position(|t| t.id == text.id) {
            state.texts[idx] = text;
        } else {
            state.texts.push(text);
        }
        let action = PenAction {
            id: uuid::Uuid::new_v4().to_string(),
            kind: PenActionKind::TextSet,
            target_id: Some(text_id),
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        true
    }

    pub fn pen_text_delete(&self, board_id: &str, text_id: &str, now: i64) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let state = match g.pen_boards.get_mut(board_id) {
            Some(s) => s,
            None => return false,
        };
        let pos = state.texts.iter().position(|t| t.id == text_id);
        if pos.is_none() {
            return false;
        }
        let removed = state.texts.remove(pos.unwrap());
        let action = PenAction {
            id: uuid::Uuid::new_v4().to_string(),
            kind: PenActionKind::TextDelete,
            target_id: Some(removed.id),
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        true
    }

    pub fn pen_clear(&self, board_id: &str, now: i64) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let state = match g.pen_boards.get_mut(board_id) {
            Some(s) => s,
            None => return false,
        };
        state.strokes.clear();
        state.texts.clear();
        let action = PenAction {
            id: uuid::Uuid::new_v4().to_string(),
            kind: PenActionKind::Clear,
            target_id: None,
            ord: state.action_log.len() as u32 + 1,
            created_at: now,
        };
        state.action_log.push(action);
        true
    }

    pub fn pen_undo(&self, board_id: &str) -> Option<(Option<StrokeId>, Option<TextId>)> {
        let mut g = self.inner.lock().expect("room inner");
        let state = g.pen_boards.get_mut(board_id)?;
        let action = state.action_log.pop()?;
        match action.kind {
            PenActionKind::StrokeBegin => {
                let stroke_id = action.target_id?;
                state.strokes.retain(|s| s.id != stroke_id);
                Some((Some(stroke_id), None))
            }
            PenActionKind::TextSet => {
                let text_id = action.target_id?;
                state.texts.retain(|t| t.id != text_id);
                Some((None, Some(text_id)))
            }
            PenActionKind::TextDelete | PenActionKind::Clear => None,
        }
    }

    pub fn load_pen_board_state(
        &self,
        board_id: &str,
        strokes: Vec<PenStroke>,
        texts: Vec<crate::proto::PenText>,
    ) {
        let mut g = self.inner.lock().expect("room inner");
        let max_stroke_ord = strokes.iter().map(|s| s.ord).max().unwrap_or(0);
        let state = PenBoardState {
            strokes,
            texts,
            action_log: Vec::new(),
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

    /// Build the Welcome snapshot for a given client.
    pub fn snapshot_for(&self, you: You, my_guest_id: &str) -> RoomSnapshot {
        let (
            guests,
            presence,
            topics,
            active_topic_id,
            questions,
            boards,
            focused_board_id,
            hands,
            seq,
        ) = {
            let g = self.inner.lock().expect("room inner");
            let questions: Vec<Question> = g.questions.values().map(|q| q.to_outbound()).collect();
            let my_votes: Vec<String> = g
                .vote_index
                .iter()
                .filter(|(_, voters)| voters.contains(my_guest_id))
                .map(|(qid, _)| qid.clone())
                .collect();
            let boards: Vec<JsonValue> = g
                .boards
                .values()
                .map(|b| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("id".to_string(), serde_json::Value::String(b.id.clone()));
                    obj.insert(
                        "kind".to_string(),
                        match b.kind {
                            BoardKind::Pen => serde_json::Value::String("pen".to_string()),
                            BoardKind::Excalidraw => {
                                serde_json::Value::String("excalidraw".to_string())
                            }
                        },
                    );
                    obj.insert(
                        "title".to_string(),
                        serde_json::Value::String(b.title.clone()),
                    );
                    obj.insert(
                        "createdAt".to_string(),
                        serde_json::Value::Number(b.created_at.into()),
                    );
                    obj.insert("ord".to_string(), serde_json::json!(b.ord));
                    if let Some(scene) = g.excalidraw_scenes.get(&b.id) {
                        obj.insert(
                            "sceneVersion".to_string(),
                            serde_json::Value::Number(scene.scene_version.into()),
                        );
                        obj.insert("elements".to_string(), scene.elements.clone());
                        obj.insert("appState".to_string(), scene.app_state.clone());
                    }
                    if b.kind == BoardKind::Pen {
                        if let Some(pen_state) = g.pen_boards.get(&b.id) {
                            let strokes: Vec<serde_json::Value> = pen_state
                                .strokes
                                .iter()
                                .map(|s| {
                                    serde_json::json!({
                                        "id": s.id,
                                        "color": s.color,
                                        "size": s.size,
                                        "points": s.points,
                                        "createdAt": s.created_at,
                                        "ord": s.ord,
                                    })
                                })
                                .collect();
                            let texts: Vec<serde_json::Value> = pen_state
                                .texts
                                .iter()
                                .map(|t| {
                                    serde_json::json!({
                                        "id": t.id,
                                        "x": t.x,
                                        "y": t.y,
                                        "text": t.text,
                                        "fontSize": t.font_size,
                                        "color": t.color,
                                        "updatedAt": t.updated_at,
                                    })
                                })
                                .collect();
                            obj.insert(
                                "content".to_string(),
                                serde_json::json!({ "strokes": strokes, "texts": texts }),
                            );
                        }
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            let hands: Vec<RaisedHand> = g.hands.values().cloned().collect();
            (
                g.presence
                    .values()
                    .map(|p| p.to_proto_guest())
                    .collect::<Vec<_>>(),
                g.presence
                    .values()
                    .map(|p| p.to_proto_presence())
                    .collect::<Vec<_>>(),
                g.topics.values().cloned().collect::<Vec<_>>(),
                g.active_topic_id.clone(),
                (questions, my_votes),
                boards,
                g.focused_board_id.clone(),
                hands,
                g.seq,
            )
        };
        let (questions, my_votes) = questions;
        RoomSnapshot {
            room: RoomSummary {
                id: self.id.clone(),
                title: self.title.clone(),
                created_at: self.created_at,
            },
            you,
            guests,
            presence,
            topics,
            active_topic_id,
            questions,
            my_votes,
            boards,
            focused_board_id,
            hands,
            seq,
        }
    }
}

#[derive(Default)]
pub struct RoomRegistry {
    rooms: DashMap<String, Arc<Room>>,
}

impl RoomRegistry {
    /// Get-or-insert a hub for a known-persisted room. Caller is
    /// responsible for verifying the room exists in the database first.
    /// On DashMap miss, the new `Room` is created **but not hydrated**;
    /// use `get_or_create_hydrated` for that.
    pub fn get_or_create(&self, id: &str, title: &str, created_at: i64) -> Arc<Room> {
        self.rooms
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Room::new(id.to_string(), title.to_string(), created_at)))
            .clone()
    }

    /// Get-or-insert a hub, hydrating from the database on first access.
    /// On DashMap miss the DB is read inside a single transaction and
    /// the resulting bundle is fed to the existing `load_*` setters
    /// **before** the entry is inserted, so concurrent first-access
    /// callers never see an empty room.
    ///
    /// On hydration error (DB unreachable, malformed row), the empty
    /// room is still returned and the error logged. The trade-off:
    /// fail-safe-empty is better than refusing the connection.
    pub fn get_or_create_hydrated(
        &self,
        db: &Db,
        id: &str,
        title: &str,
        created_at: i64,
    ) -> Arc<Room> {
        self.rooms
            .entry(id.to_string())
            .or_insert_with(|| {
                let room = Arc::new(Room::new(id.to_string(), title.to_string(), created_at));
                let span = tracing::info_span!("hydrate", room_id = %id);
                let _enter = span.enter();
                if let Err(e) = hydrate_room_from_db(&room, db, id) {
                    tracing::error!(error = %e, "hydration failed; serving empty room");
                }
                room
            })
            .clone()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Room>> {
        self.rooms.get(id).map(|r| r.clone())
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Arc<Room>> + '_ {
        self.rooms.iter().map(|entry| entry.value().clone())
    }

    /// Remove rooms with no connected clients and no activity for at
    /// least `idle_threshold_ms`. Returns the reaped handles so the
    /// caller can perform any side-effects (none today) before drop.
    ///
    /// Each candidate is verified under the DashMap shard lock to avoid
    /// races with concurrent `get_or_create*` first-access (see
    /// risks.md R28).
    pub fn reap_idle(&self, now_ms: i64, idle_threshold_ms: i64) -> Vec<Arc<Room>> {
        // Collect candidate keys first to avoid holding DashMap iterator
        // while mutating; remove_if is then used per candidate to
        // re-check under the shard lock.
        let candidates: Vec<String> = self
            .rooms
            .iter()
            .filter(|e| {
                let room = e.value();
                room.connected_client_count() == 0
                    && now_ms.saturating_sub(room.last_activity_at()) > idle_threshold_ms
            })
            .map(|e| e.key().clone())
            .collect();
        let mut reaped = Vec::new();
        for id in candidates {
            if let Some((_, room)) =
                self.rooms
                    .remove_if(&id, |_, room| {
                        room.connected_client_count() == 0
                            && now_ms.saturating_sub(room.last_activity_at()) > idle_threshold_ms
                    })
            {
                reaped.push(room);
            }
        }
        reaped
    }
}

/// Read every persisted row for `room_id` inside one read transaction
/// and feed the result into the room's `load_*` setters. Called from
/// `RoomRegistry::get_or_create_hydrated` on first access to a room.
fn hydrate_room_from_db(room: &Room, db: &Db, room_id: &str) -> Result<(), DbError> {
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
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

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
    let mut pen_loads: Vec<(String, Vec<PenStroke>, Vec<crate::proto::PenText>)> = Vec::new();
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
        pen_loads.push((board_id.clone(), strokes, texts));
    }

    tx.commit()?;
    drop(conn);

    // Push into the room's in-memory model. load_* setters are safe to
    // call in any order; load_boards must precede load_pen_board_state
    // because load_pen_board_state expects the board entry to exist.
    room.load_topics(topics, active_topic_id);
    room.load_questions(questions, votes);
    room.load_boards(boards, scenes, focused_board_id);
    for (board_id, strokes, texts) in pen_loads {
        room.load_pen_board_state(&board_id, strokes, texts);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Role;

    fn you(client: &str, guest: &str, role: Role) -> You {
        You {
            client_id: client.to_string(),
            role,
            guest_id: guest.to_string(),
        }
    }

    #[test]
    fn reap_idle_drops_truly_idle_rooms() {
        let reg = RoomRegistry::default();
        let now = 11 * 60 * 1000; // 11 min in ms.
        // Room A: no clients, last activity 0 (11 min idle) → reaped.
        let a = reg.get_or_create("a", "A", 0);
        a.touch(0);
        // Room B: no clients, last activity 6 min ago → kept.
        let b = reg.get_or_create("b", "B", 0);
        b.touch(now - 6 * 60 * 1000);
        // Room C: one connected client, regardless of activity → kept.
        let c = reg.get_or_create("c", "C", 0);
        c.touch(0);
        c.add_client("g".into(), "cli".into(), "n".into(), 0, false);

        let reaped = reg.reap_idle(now, 10 * 60 * 1000);
        let reaped_ids: Vec<String> = reaped.iter().map(|r| r.id.clone()).collect();
        assert_eq!(reaped_ids, vec!["a".to_string()]);
        assert!(reg.get("a").is_none());
        assert!(reg.get("b").is_some());
        assert!(reg.get("c").is_some());
    }

    #[test]
    fn touch_updates_last_activity_monotonically() {
        let r = Room::new("R".into(), "T".into(), 0);
        let initial = r.last_activity_at();
        r.touch(initial + 1_000);
        assert_eq!(r.last_activity_at(), initial + 1_000);
    }

    #[test]
    fn add_remove_client_drives_presence_correctly() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false));
        assert!(!r.add_client("g1".into(), "c2".into(), "Alice".into(), 100, false));
        assert_eq!(r.presence().len(), 1);
        assert_eq!(r.presence()[0].client_ids.len(), 2);

        assert!(!r.remove_client("g1", "c1"));
        assert_eq!(r.presence().len(), 1);
        assert!(r.remove_client("g1", "c2"));
        assert_eq!(r.presence().len(), 0);
    }

    #[test]
    fn set_display_name_returns_changed_flag() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 0, false);
        assert!(!r.set_display_name("g1", "Alice".into()));
        assert!(r.set_display_name("g1", "Alicia".into()));
        assert_eq!(r.guests()[0].display_name, "Alicia");
    }

    #[test]
    fn snapshot_is_empty_for_m1_aside_from_presence() {
        let r = Room::new("ROOMID000001".into(), "T".into(), 7);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 7, false);
        let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap.room.id, "ROOMID000001");
        assert_eq!(snap.guests.len(), 1);
        assert!(snap.topics.is_empty());
        assert!(snap.questions.is_empty());
        assert!(snap.boards.is_empty());
        assert!(snap.hands.is_empty());
        assert!(snap.my_votes.is_empty());
        assert!(snap.active_topic_id.is_none());
        assert!(snap.focused_board_id.is_none());
    }

    #[test]
    fn seq_is_monotonic() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert_eq!(r.next_seq(), 1);
        assert_eq!(r.next_seq(), 2);
        assert_eq!(r.current_seq(), 2);
    }

    #[test]
    fn registry_get_or_create_is_idempotent() {
        let reg = RoomRegistry::default();
        let a = reg.get_or_create("R", "T", 0);
        let b = reg.get_or_create("R", "T", 0);
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn topic_add_list_get() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic 1".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        let topics = r.topics();
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].title, "Topic 1");
        assert!(r.active_topic_id().is_none());
    }

    #[test]
    fn topic_rename() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Original".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        assert!(r.rename_topic("t1", "Renamed".into()));
        assert_eq!(r.topics()[0].title, "Renamed");
        assert!(!r.rename_topic("nonexistent", "X".into()));
    }

    #[test]
    fn topic_move() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        assert!(r.move_topic("t1", Some("parent1".into()), 2.5));
        let t = r.topics().pop().unwrap();
        assert_eq!(t.parent_id, Some("parent1".into()));
        assert_eq!(t.ord, 2.5);
    }

    #[test]
    fn topic_delete() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        assert_eq!(r.topics().len(), 1);
        r.delete_topic("t1");
        assert!(r.topics().is_empty());
    }

    #[test]
    fn topic_set_active() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        assert!(r.active_topic_id().is_none());
        r.set_active_topic(Some("t1".into()));
        assert_eq!(r.active_topic_id(), Some("t1".into()));
        r.set_active_topic(None);
        assert!(r.active_topic_id().is_none());
    }

    #[test]
    fn topic_mark_done() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        assert_eq!(r.topics()[0].status, TopicStatus::Pending);
        assert!(r.mark_topic_done("t1", true));
        assert_eq!(r.topics()[0].status, TopicStatus::Done);
        assert!(r.mark_topic_done("t1", false));
        assert_eq!(r.topics()[0].status, TopicStatus::Pending);
    }

    #[test]
    fn at_most_one_active() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Topic 1".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        r.add_topic(Topic {
            id: "t2".into(),
            parent_id: None,
            title: "Topic 2".into(),
            ord: 2.0,
            status: TopicStatus::Pending,
            created_at: 101,
        });
        r.set_active_topic(Some("t1".into()));
        r.set_active_topic(Some("t2".into()));
        assert_eq!(r.active_topic_id(), Some("t2".into()));
    }

    #[test]
    fn load_topics_replaces_existing() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_topic(Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Old".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 100,
        });
        r.load_topics(
            vec![Topic {
                id: "t2".into(),
                parent_id: None,
                title: "New".into(),
                ord: 1.0,
                status: TopicStatus::Done,
                created_at: 200,
            }],
            Some("t2".into()),
        );
        assert_eq!(r.topics().len(), 1);
        assert_eq!(r.topics()[0].title, "New");
        assert_eq!(r.active_topic_id(), Some("t2".into()));
    }

    fn make_question(id: &str, text: &str, vote_count: u32) -> Question {
        Question {
            id: id.into(),
            room_id: "R".into(),
            author_guest_id: "g1".into(),
            author_name: "Alice".into(),
            anonymous: false,
            text: text.into(),
            answered: false,
            created_at: 100,
            vote_count,
        }
    }

    #[test]
    fn question_add_list_get() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        let qs = r.questions();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].text, "What is Rust?");
    }

    #[test]
    fn question_update() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        let mut q = make_question("q1", "What is Rust?", 0);
        q.answered = true;
        assert!(r.update_question(q));
        assert!(r.questions()[0].answered);
        assert!(!r.update_question(make_question("nonexistent", "?", 0)));
    }

    #[test]
    fn question_delete() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        assert_eq!(r.questions().len(), 1);
        r.delete_question("q1");
        assert!(r.questions().is_empty());
    }

    #[test]
    fn question_vote_adds_count() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        let (count, changed) = r.vote_question("q1", "g1", true).unwrap();
        assert_eq!(count, 1);
        assert!(changed);
    }

    #[test]
    fn question_vote_retracts_count() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        r.vote_question("q1", "g1", true).unwrap();
        let (count, changed) = r.vote_question("q1", "g1", false).unwrap();
        assert_eq!(count, 0);
        assert!(changed);
    }

    #[test]
    fn question_vote_double_vote_no_change() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        r.vote_question("q1", "g1", true).unwrap();
        let (count, changed) = r.vote_question("q1", "g1", true).unwrap();
        assert_eq!(count, 1);
        assert!(!changed);
    }

    #[test]
    fn question_vote_multiple_guests() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 0));
        r.vote_question("q1", "g1", true).unwrap();
        r.vote_question("q1", "g2", true).unwrap();
        r.vote_question("q1", "g3", true).unwrap();
        assert_eq!(r.questions()[0].vote_count, 3);
        r.vote_question("q1", "g2", false).unwrap();
        assert_eq!(r.questions()[0].vote_count, 2);
    }

    #[test]
    fn my_votes_tracks_correctly() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "Q1", 0));
        r.add_question(make_question("q2", "Q2", 0));
        r.add_question(make_question("q3", "Q3", 0));
        r.vote_question("q1", "g1", true).unwrap();
        r.vote_question("q3", "g1", true).unwrap();
        let votes = r.my_votes("g1");
        assert!(votes.contains(&"q1".into()));
        assert!(!votes.contains(&"q2".into()));
        assert!(votes.contains(&"q3".into()));
    }

    #[test]
    fn snapshot_includes_questions_and_my_votes() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "Q1", 2));
        r.vote_question("q1", "g1", true).unwrap();
        let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap.questions.len(), 1);
        assert_eq!(snap.questions[0].vote_count, 1);
        assert!(snap.my_votes.contains(&"q1".into()));
    }

    #[test]
    fn snapshot_questions_are_cloned() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "Q1", 0));
        let snap1 = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        r.add_question(make_question("q2", "Q2", 0));
        let snap2 = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap1.questions.len(), 1);
        assert_eq!(snap2.questions.len(), 2);
    }

    #[test]
    fn raise_hand_adds_to_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "What is Rust?".into(), 1000);
        let hands = r.hands_list();
        assert_eq!(hands.len(), 1);
        assert_eq!(hands[0].guest_id, "g1");
        assert_eq!(hands[0].display_name, "Alice");
        assert_eq!(hands[0].topic, "What is Rust?");
        assert_eq!(hands[0].raised_at, 1000);
    }

    #[test]
    fn raise_hand_replaces_existing() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "First topic".into(), 1000);
        r.raise_hand("g1", "Alice".into(), "Second topic".into(), 2000);
        let hands = r.hands_list();
        assert_eq!(hands.len(), 1);
        assert_eq!(hands[0].topic, "Second topic");
    }

    #[test]
    fn lower_hand_removes_from_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        assert!(r.lower_hand("g1"));
        assert!(r.hands_list().is_empty());
    }

    #[test]
    fn lower_hand_returns_false_when_not_in_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(!r.lower_hand("nonexistent"));
    }

    #[test]
    fn call_on_hand_removes_and_returns() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        let hand = r.call_on_hand("g1");
        assert!(hand.is_some());
        assert_eq!(hand.unwrap().guest_id, "g1");
        assert!(r.hands_list().is_empty());
    }

    #[test]
    fn dismiss_hand_removes_from_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        assert!(r.dismiss_hand("g1"));
        assert!(r.hands_list().is_empty());
    }

    #[test]
    fn promote_question_to_topic_creates_topic_and_deletes_question() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_question(make_question("q1", "What is Rust?", 3));
        let result = r.promote_question_to_topic("q1", None, None);
        assert!(result.is_some());
        let (question, topic) = result.unwrap();
        assert_eq!(question.id, "q1");
        assert_eq!(topic.title, "What is Rust?");
        assert!(r.questions().is_empty());
        assert_eq!(r.topics().len(), 1);
        assert_eq!(r.topics()[0].title, "What is Rust?");
    }

    #[test]
    fn promote_question_to_topic_truncates_to_80_chars() {
        let r = Room::new("R".into(), "T".into(), 0);
        let long_text = "A".repeat(100);
        r.add_question(make_question("q1", &long_text, 0));
        let result = r.promote_question_to_topic("q1", None, None);
        assert!(result.is_some());
        let (_, topic) = result.unwrap();
        assert_eq!(topic.title.len(), 80);
    }

    #[test]
    fn promote_question_to_topic_nonexistent_returns_none() {
        let r = Room::new("R".into(), "T".into(), 0);
        let result = r.promote_question_to_topic("nonexistent", None, None);
        assert!(result.is_none());
    }

    #[test]
    fn snapshot_includes_hands() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap.hands.len(), 1);
        assert_eq!(snap.hands[0].guest_id, "g1");
    }

    #[test]
    fn create_board_pen_initializes_pen_state() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen Board".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        assert!(r.board_exists("b1"));
        let state = r.get_pen_board_state("b1").unwrap();
        assert!(state.strokes.is_empty());
        assert!(state.texts.is_empty());
    }

    #[test]
    fn create_board_excalidraw_initializes_scene() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "e1".into(),
            kind: BoardKind::Excalidraw,
            title: "Excalidraw Board".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        assert!(r.board_exists("e1"));
        let scene = r.get_excalidraw_scene("e1").unwrap();
        assert_eq!(scene.scene_version, 0);
    }

    #[test]
    fn rename_board_updates_title() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Old Title".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let renamed = r.rename_board("b1", "New Title".into());
        assert!(renamed.is_some());
        assert_eq!(renamed.unwrap().title, "New Title");
    }

    #[test]
    fn rename_board_nonexistent_returns_none() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(r.rename_board("nonexistent", "X".into()).is_none());
    }

    #[test]
    fn delete_board_removes_board_and_clears_focus() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.set_focused_board("b1".into());
        assert!(r.delete_board("b1"));
        assert!(!r.board_exists("b1"));
        assert!(r.focused_board_id().is_none());
    }

    #[test]
    fn delete_board_nonexistent_returns_false() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(!r.delete_board("nonexistent"));
    }

    #[test]
    fn set_focused_board_tracks_current() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        assert!(r.focused_board_id().is_none());
        r.set_focused_board("b1".into());
        assert_eq!(r.focused_board_id(), Some("b1".into()));
    }

    #[test]
    fn excalidraw_scene_update_increments_version() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "e1".into(),
            kind: BoardKind::Excalidraw,
            title: "Excal".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let elements: JsonValue = serde_json::json!([{"id": "el1"}]);
        let app_state: JsonValue = serde_json::json!({});
        r.update_excalidraw_scene("e1", 1, elements.clone(), app_state.clone(), 200);
        let scene = r.get_excalidraw_scene("e1").unwrap();
        assert_eq!(scene.scene_version, 1);
    }

    #[test]
    fn excalidraw_scene_update_nonexistent_returns_board_missing() {
        let r = Room::new("R".into(), "T".into(), 0);
        let elements: JsonValue = serde_json::json!([]);
        let app_state: JsonValue = serde_json::json!({});
        assert_eq!(
            r.update_excalidraw_scene("nonexistent", 1, elements, app_state, 200),
            ExcalidrawUpdateOutcome::BoardMissing
        );
    }

    #[test]
    fn excalidraw_scene_update_wrong_kind_returns_board_missing() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let elements: JsonValue = serde_json::json!([]);
        let app_state: JsonValue = serde_json::json!({});
        assert_eq!(
            r.update_excalidraw_scene("b1", 1, elements, app_state, 200),
            ExcalidrawUpdateOutcome::BoardMissing
        );
    }

    #[test]
    fn excalidraw_scene_update_rejects_stale_version() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "e1".into(),
            kind: BoardKind::Excalidraw,
            title: "Excal".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let elements: JsonValue = serde_json::json!([{"id": "el1"}]);
        let app_state: JsonValue = serde_json::json!({});
        assert_eq!(
            r.update_excalidraw_scene("e1", 5, elements.clone(), app_state.clone(), 200),
            ExcalidrawUpdateOutcome::Applied
        );
        assert_eq!(
            r.update_excalidraw_scene("e1", 5, elements.clone(), app_state.clone(), 300),
            ExcalidrawUpdateOutcome::Stale
        );
        assert_eq!(
            r.update_excalidraw_scene("e1", 3, elements.clone(), app_state.clone(), 400),
            ExcalidrawUpdateOutcome::Stale
        );
        let scene = r.get_excalidraw_scene("e1").unwrap();
        assert_eq!(scene.scene_version, 5);
    }

    #[test]
    fn get_excalidraw_scenes_needing_reset_returns_pending_scenes() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "e1".into(),
            kind: BoardKind::Excalidraw,
            title: "Excal".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let elements: JsonValue = serde_json::json!([]);
        let app_state: JsonValue = serde_json::json!({});
        r.update_excalidraw_scene("e1", 3, elements.clone(), app_state.clone(), 200);
        let pending = r.get_excalidraw_scenes_needing_reset();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].scene_version, 3);
    }

    #[test]
    fn mark_excalidraw_scene_broadcast_updates_version_tracker() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "e1".into(),
            kind: BoardKind::Excalidraw,
            title: "Excal".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.mark_excalidraw_scene_broadcast("e1", 5);
        let pending = r.get_excalidraw_scenes_needing_reset();
        assert!(pending.is_empty());
    }

    #[test]
    fn pen_begin_stroke_creates_stroke() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let stroke = r
            .pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000)
            .unwrap();
        assert_eq!(stroke.id, "s1");
        assert_eq!(stroke.color, "#000");
        assert!(stroke.points.is_empty());
    }

    #[test]
    fn pen_begin_stroke_nonexistent_board_returns_none() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(r
            .pen_begin_stroke("nonexistent", "s1".into(), "#000".into(), 4.0, 1000)
            .is_none());
    }

    #[test]
    fn pen_append_points_extends_stroke() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
        r.pen_append_points("b1", "s1", vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]]);
        let state = r.get_pen_board_state("b1").unwrap();
        assert_eq!(state.strokes[0].points.len(), 2);
    }

    #[test]
    fn pen_end_stroke_finalizes_ord() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
        r.pen_end_stroke("b1", "s1");
        let state = r.get_pen_board_state("b1").unwrap();
        assert_eq!(state.strokes[0].ord, 1);
        assert_eq!(state.next_stroke_ord, 2);
    }

    #[test]
    fn pen_text_upsert_adds_text() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let text = crate::proto::PenText {
            id: "t1".into(),
            x: 10.0,
            y: 20.0,
            text: "Hello".into(),
            font_size: 16.0,
            color: "#000".into(),
            updated_at: 1000,
        };
        r.pen_text_upsert("b1", text, 1000);
        let state = r.get_pen_board_state("b1").unwrap();
        assert_eq!(state.texts.len(), 1);
        assert_eq!(state.texts[0].text, "Hello");
    }

    #[test]
    fn pen_text_delete_removes_text() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        let text = crate::proto::PenText {
            id: "t1".into(),
            x: 10.0,
            y: 20.0,
            text: "Hello".into(),
            font_size: 16.0,
            color: "#000".into(),
            updated_at: 1000,
        };
        r.pen_text_upsert("b1", text, 1000);
        r.pen_text_delete("b1", "t1", 2000);
        let state = r.get_pen_board_state("b1").unwrap();
        assert!(state.texts.is_empty());
    }

    #[test]
    fn pen_clear_removes_all_strokes_and_texts() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
        let text = crate::proto::PenText {
            id: "t1".into(),
            x: 10.0,
            y: 20.0,
            text: "Hello".into(),
            font_size: 16.0,
            color: "#000".into(),
            updated_at: 1000,
        };
        r.pen_text_upsert("b1", text, 1000);
        r.pen_clear("b1", 2000);
        let state = r.get_pen_board_state("b1").unwrap();
        assert!(state.strokes.is_empty());
        assert!(state.texts.is_empty());
    }

    #[test]
    fn pen_undo_removes_last_action() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
        let result = r.pen_undo("b1");
        assert!(result.is_some());
        let (stroke_id, _) = result.unwrap();
        assert_eq!(stroke_id, Some("s1".into()));
        let state = r.get_pen_board_state("b1").unwrap();
        assert!(state.strokes.is_empty());
    }

    #[test]
    fn set_muted_updates_guest_muted_state() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        assert!(!r.is_muted("g1"));
        r.set_muted("g1", true);
        assert!(r.is_muted("g1"));
        r.set_muted("g1", false);
        assert!(!r.is_muted("g1"));
    }

    #[test]
    fn set_muted_nonexistent_returns_false() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(!r.set_muted("nonexistent", true));
    }

    #[test]
    fn is_muted_nonexistent_returns_false() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(!r.is_muted("nonexistent"));
    }

    #[test]
    fn kick_guest_removes_presence() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        assert_eq!(r.presence().len(), 1);
        let removed = r.kick_guest("g1");
        assert!(!removed.is_empty());
        assert_eq!(r.presence().len(), 0);
    }

    #[test]
    fn kick_guest_nonexistent_returns_empty() {
        let r = Room::new("R".into(), "T".into(), 0);
        assert!(r.kick_guest("nonexistent").is_empty());
    }

    #[test]
    fn kick_guest_remembers_kicked_state_for_reconnect_race() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
        assert!(!r.is_kicked("g1"));
        r.kick_guest("g1");
        assert!(r.is_kicked("g1"));
        assert!(!r.is_kicked("g2"));
    }

    #[test]
    fn snapshot_includes_boards() {
        let r = Room::new("R".into(), "T".into(), 0);
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen".into(),
            created_at: 100,
            ord: 1.0,
        };
        r.create_board(board, 100);
        r.set_focused_board("b1".into());
        let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap.boards.len(), 1);
        assert_eq!(snap.focused_board_id, Some("b1".into()));
    }
}
