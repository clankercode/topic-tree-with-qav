//! Shared application state passed to every route handler.
//!
//! Today: just the database handle and the room registry. Later phases
//! will add a metrics handle and a server-start instant.

use std::sync::Arc;

use crate::db::Db;
use crate::room::RoomRegistry;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub rooms: Arc<RoomRegistry>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            rooms: Arc::new(RoomRegistry::default()),
        }
    }
}
