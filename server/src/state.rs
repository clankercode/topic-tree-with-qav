//! Shared application state passed to every route handler.
//!
//! Today: just the database handle, the room registry, and metrics.

use std::sync::Arc;
use std::sync::LazyLock;

use crate::db::Db;
use crate::metrics::SharedMetrics;
use crate::rate_limit::RateLimiter;
use crate::room::RoomRegistry;

static GLOBAL_RATE_LIMITER: LazyLock<RateLimiter> = LazyLock::new(RateLimiter::with_system_clock);

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub rooms: Arc<RoomRegistry>,
    pub metrics: SharedMetrics,
}

impl AppState {
    pub fn new(db: Db, metrics: SharedMetrics) -> Self {
        Self {
            db,
            rooms: Arc::new(RoomRegistry::default()),
            metrics,
        }
    }
}

pub fn global_rate_limiter() -> &'static RateLimiter {
    &GLOBAL_RATE_LIMITER
}
