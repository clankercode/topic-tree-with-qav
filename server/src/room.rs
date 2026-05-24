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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::proto::{
    Guest, Presence, RoomSnapshot, RoomSummary, ServerMsg, Topic, TopicStatus, You,
};

/// Broadcast channel capacity per room. If a writer lags more than this
/// many messages, its receiver receives `RecvError::Lagged` and the
/// client must resync via `GetSnapshot` (handled in the ws layer).
const BROADCAST_CAPACITY: usize = 256;

pub type ClientId = String;
pub type GuestId = String;
pub type TopicId = String;

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
                muted: false,
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

    /// Build the M1 Welcome snapshot for a given client. Topics/boards/
    /// questions/hands all empty in M1.
    pub fn snapshot_for(&self, you: You, _my_guest_id: &str) -> RoomSnapshot {
        let (guests, presence, topics, active_topic_id, seq) = {
            let g = self.inner.lock().expect("room inner");
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
                g.seq,
            )
        };
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
            questions: vec![],
            my_votes: vec![],
            boards: vec![],
            focused_board_id: None,
            hands: vec![],
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
        assert!(r.add_client("g1".into(), "c1".into(), "Alice".into(), 100));
        assert!(!r.add_client("g1".into(), "c2".into(), "Alice".into(), 100));
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
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 0);
        assert!(!r.set_display_name("g1", "Alice".into()));
        assert!(r.set_display_name("g1", "Alicia".into()));
        assert_eq!(r.guests()[0].display_name, "Alicia");
    }

    #[test]
    fn snapshot_is_empty_for_m1_aside_from_presence() {
        let r = Room::new("ROOMID000001".into(), "T".into(), 7);
        r.add_client("g1".into(), "c1".into(), "Alice".into(), 7);
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
}
