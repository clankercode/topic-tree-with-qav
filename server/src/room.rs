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
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::api::now_ms;
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
    inner: Mutex<RoomInner>,
    pub broadcast: broadcast::Sender<ServerMsg>,
}

struct RoomInner {
    seq: u64,
    presence: BTreeMap<GuestId, PresenceEntry>,
    topics: BTreeMap<TopicId, Topic>,
    active_topic_id: Option<TopicId>,
    questions: BTreeMap<QuestionId, Question>,
    vote_index: HashMap<QuestionId, HashSet<GuestId>>,
    boards: BTreeMap<BoardId, Board>,
    excalidraw_scenes: BTreeMap<BoardId, ExcalidrawScene>,
    focused_board_id: Option<BoardId>,
    hands: BTreeMap<GuestId, RaisedHand>,
}

impl Room {
    fn new(id: String, title: String, created_at: i64) -> Self {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            id,
            title,
            created_at,
            inner: Mutex::new(RoomInner {
                seq: 0,
                presence: BTreeMap::new(),
                topics: BTreeMap::new(),
                active_topic_id: None,
                questions: BTreeMap::new(),
                vote_index: HashMap::new(),
                boards: BTreeMap::new(),
                excalidraw_scenes: BTreeMap::new(),
                focused_board_id: None,
                hands: BTreeMap::new(),
            }),
            broadcast: tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.broadcast.subscribe()
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
        if let Some(p) = g.presence.remove(guest_id) {
            p.client_ids
        } else {
            vec![]
        }
    }

    pub fn guests(&self) -> Vec<Guest> {
        let g = self.inner.lock().expect("room inner");
        g.presence.values().map(|p| p.to_proto_guest()).collect()
    }

    pub fn presence(&self) -> Vec<Presence> {
        let g = self.inner.lock().expect("room inner");
        g.presence.values().map(|p| p.to_proto_presence()).collect()
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
        let Some(t) = g.topics.get_mut(topic_id) else {
            return false;
        };
        t.parent_id = new_parent_id;
        t.ord = new_ord;
        true
    }

    pub fn delete_topic(&self, topic_id: &str) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        g.topics.retain(|id, _| id != topic_id);
        if g.active_topic_id.as_deref() == Some(topic_id) {
            g.active_topic_id = None;
        }
        true
    }

    pub fn set_active_topic(&self, topic_id: Option<String>) {
        let mut g = self.inner.lock().expect("room inner");
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
        g.questions.retain(|id, _| id != question_id);
        g.vote_index.remove(question_id);
        true
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
    ) -> bool {
        let mut g = self.inner.lock().expect("room inner");
        let board = g.boards.get(board_id);
        if board.is_none() || board.as_ref().map(|b| &b.kind) != Some(&BoardKind::Excalidraw) {
            return false;
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
        scene.scene_version = scene_version;
        scene.elements = elements;
        scene.app_state = app_state;
        true
    }

    pub fn get_excalidraw_scene(&self, board_id: &str) -> Option<ExcalidrawScene> {
        let g = self.inner.lock().expect("room inner");
        g.excalidraw_scenes.get(board_id).cloned()
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
            let questions: Vec<Question> = g.questions.values().cloned().collect();
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
    pub fn get_or_create(&self, id: &str, title: &str, created_at: i64) -> Arc<Room> {
        self.rooms
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Room::new(id.to_string(), title.to_string(), created_at)))
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
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
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
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
        r.raise_hand("g1", "Alice".into(), "First topic".into(), 1000);
        r.raise_hand("g1", "Alice".into(), "Second topic".into(), 2000);
        let hands = r.hands_list();
        assert_eq!(hands.len(), 1);
        assert_eq!(hands[0].topic, "Second topic");
    }

    #[test]
    fn lower_hand_removes_from_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
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
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        let hand = r.call_on_hand("g1");
        assert!(hand.is_some());
        assert_eq!(hand.unwrap().guest_id, "g1");
        assert!(r.hands_list().is_empty());
    }

    #[test]
    fn dismiss_hand_removes_from_queue() {
        let r = Room::new("R".into(), "T".into(), 0);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
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
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 100);
        r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
        let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
        assert_eq!(snap.hands.len(), 1);
        assert_eq!(snap.hands[0].guest_id, "g1");
    }
}
