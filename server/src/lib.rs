//! Library facade for the server binary so integration tests can build the
//! same `Router` the binary serves.

pub mod auth;
pub mod db;
pub mod http;
pub mod proto;
pub mod rate_limit;

use axum::Router;

pub fn app() -> Router {
    http::router()
}
