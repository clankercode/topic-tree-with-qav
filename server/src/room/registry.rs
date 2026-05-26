use std::sync::Arc;

use dashmap::DashMap;

use crate::db::Db;

use super::hydrate::hydrate_room_from_db;
use super::Room;

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
            if let Some((_, room)) = self.rooms.remove_if(&id, |_, room| {
                room.connected_client_count() == 0
                    && now_ms.saturating_sub(room.last_activity_at()) > idle_threshold_ms
            }) {
                reaped.push(room);
            }
        }
        reaped
    }
}
