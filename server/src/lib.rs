//! Library facade for the server binary so integration tests can build the
//! same `Router` the binary serves.

pub mod api;
pub mod auth;
pub mod db;
pub mod http;
pub mod metrics;
pub mod proto;
pub mod rate_limit;
pub mod room;
pub mod state;
pub mod ws;

use axum::Router;

pub use db::{Db, DbError};
pub use metrics::create_metrics;
pub use state::AppState;

/// Build the router with an explicit AppState (used by tests + main).
pub fn app_with_state(state: AppState) -> Router {
    http::router(state)
}

/// Convenience for tests + the binary: build an in-memory app.
pub fn app_in_memory() -> Result<Router, DbError> {
    let db = Db::open_in_memory()?;
    let metrics = create_metrics();
    Ok(app_with_state(AppState::new(db, metrics)))
}

/// Backwards-compatible facade used by the existing http smoke tests.
pub fn app() -> Router {
    app_in_memory().expect("init in-memory db")
}
