//! Shared application state passed to every route handler.
//!
//! Holds the database handle, the room registry, metrics, and the
//! single-writer task's mpsc sender (see `crate::writer`).

use std::sync::Arc;
use std::sync::LazyLock;

use crate::db::Db;
use crate::metrics::SharedMetrics;
use crate::rate_limit::RateLimiter;
use crate::room::RoomRegistry;
use crate::writer::{spawn_writer, WriteSender};

static GLOBAL_RATE_LIMITER: LazyLock<RateLimiter> = LazyLock::new(RateLimiter::with_system_clock);

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub rooms: Arc<RoomRegistry>,
    pub metrics: SharedMetrics,
    /// Sender side of the single-writer SQLite task's mpsc channel.
    /// Cheap to clone; ws/http handlers clone it freely.
    pub writer_tx: WriteSender,
}

impl AppState {
    /// Build a new `AppState`, spawning the single writer task. The
    /// returned `JoinHandle` should be held by `main.rs` so shutdown
    /// can drain pending writes; tests that don't care about drain
    /// can let it drop (the writer detaches and exits at process end).
    pub fn new(db: Db, metrics: SharedMetrics) -> (Self, tokio::task::JoinHandle<()>) {
        let handle = spawn_writer(db.clone());
        let state = Self {
            db,
            rooms: Arc::new(RoomRegistry::default()),
            metrics,
            writer_tx: handle.tx,
        };
        (state, handle.join)
    }

    /// Convenience for tests that don't need the writer join handle.
    /// Same as `new(..)` but drops the returned handle on the floor;
    /// the writer becomes detached and exits at process teardown.
    pub fn new_detached(db: Db, metrics: SharedMetrics) -> Self {
        let (s, join) = Self::new(db, metrics);
        // The join handle is intentionally abandoned: tests that need
        // graceful shutdown drain own the join via `Self::new` directly.
        drop(join);
        s
    }
}

pub fn global_rate_limiter() -> &'static RateLimiter {
    &GLOBAL_RATE_LIMITER
}
