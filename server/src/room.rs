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
use std::sync::Mutex;
use tokio::sync::broadcast;

use crate::api::now_ms;
use crate::proto::{
    Board, BoardKind, ExcalidrawScene, Guest, Presence, Question, RaisedHand, RoomSnapshot,
    RoomSummary, ServerMsg, Topic, TopicStatus, You,
};
use serde_json::Value as JsonValue;

mod boards;
mod hydrate;
mod model;
mod registry;
#[cfg(test)]
mod tests;

pub use model::*;
pub use registry::RoomRegistry;

/// Broadcast channel capacity per room. If a writer lags more than this
/// many messages, its receiver receives `RecvError::Lagged` and the
/// client must resync via `GetSnapshot` (handled in the ws layer).
const BROADCAST_CAPACITY: usize = 256;

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
    topic_vote_index: HashMap<TopicId, HashSet<GuestId>>,
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
                topic_vote_index: HashMap::new(),
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

    /// Bulk insert a pre-resolved batch of topics. Used by the import
    /// path: the caller has already generated UUIDs, set parent_id and
    /// ord, so this is a single locked map insertion.
    pub fn add_topics_bulk(&self, topics: Vec<Topic>) {
        let mut g = self.inner.lock().expect("room inner");
        for t in topics {
            g.topics.insert(t.id.clone(), t);
        }
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
            g.topic_vote_index.remove(id);
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

    pub fn load_topics(
        &self,
        topics: Vec<Topic>,
        topic_votes: HashMap<TopicId, Vec<GuestId>>,
        active_topic_id: Option<String>,
    ) {
        let mut g = self.inner.lock().expect("room inner");
        g.topics.clear();
        g.topic_vote_index.clear();
        for mut t in topics {
            t.vote_count = topic_votes.get(&t.id).map(|v| v.len() as u32).unwrap_or(0);
            g.topics.insert(t.id.clone(), t);
        }
        for (tid, voters) in topic_votes {
            g.topic_vote_index.insert(tid, voters.into_iter().collect());
        }
        g.active_topic_id = active_topic_id;
    }

    pub fn my_topic_votes(&self, my_guest_id: &str) -> Vec<String> {
        let g = self.inner.lock().expect("room inner");
        g.topic_vote_index
            .iter()
            .filter(|(_, voters)| voters.contains(my_guest_id))
            .map(|(tid, _)| tid.clone())
            .collect()
    }

    pub fn vote_topic(&self, topic_id: &str, guest_id: &str, vote: bool) -> Option<(u32, bool)> {
        let mut g = self.inner.lock().expect("room inner");
        let was_voted = g
            .topic_vote_index
            .get(topic_id)
            .map(|voters| voters.contains(guest_id))
            .unwrap_or(false);
        let changed = vote != was_voted;
        if !changed {
            return Some((g.topics.get(topic_id)?.vote_count, false));
        }
        let new_count = if vote {
            let voters = g.topic_vote_index.entry(topic_id.to_string()).or_default();
            voters.insert(guest_id.to_string());
            voters.len() as u32
        } else {
            let voters = g.topic_vote_index.entry(topic_id.to_string()).or_default();
            voters.remove(guest_id);
            voters.len() as u32
        };
        if let Some(t) = g.topics.get_mut(topic_id) {
            t.vote_count = new_count;
        }
        Some((new_count, true))
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
            vote_count: 0,
        };
        g.topics.insert(topic_id, topic.clone());
        Some((question, topic))
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
            let my_topic_votes: Vec<String> = g
                .topic_vote_index
                .iter()
                .filter(|(_, voters)| voters.contains(my_guest_id))
                .map(|(tid, _)| tid.clone())
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
                (questions, my_votes, my_topic_votes),
                boards,
                g.focused_board_id.clone(),
                hands,
                g.seq,
            )
        };
        let (questions, my_votes, my_topic_votes) = questions;
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
            my_topic_votes,
            boards,
            focused_board_id,
            hands,
            seq,
        }
    }
}
