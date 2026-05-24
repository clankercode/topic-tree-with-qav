//! HTTP surface: health endpoint, API routes, WebSocket upgrade, and the
//! SPA static-asset fallback served from the embedded `web/dist/` bundle.

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use rust_embed::RustEmbed;

use crate::api;
use crate::state::AppState;
use crate::ws::ws_handler;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../web/dist/"]
struct WebAssets;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/ws", any(ws_handler))
        .merge(api::router())
        .route("/api/*rest", any(api_placeholder))
        .route("/", get(serve_index))
        .route("/*path", get(serve_asset))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn api_placeholder(uri: Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        format!("no api route: {}", uri.path()),
    )
}

async fn serve_index() -> Response {
    asset_or_fallback("index.html")
}

async fn serve_asset(Path(path): Path<String>) -> Response {
    if let Some(resp) = try_asset(&path) {
        return resp;
    }
    asset_or_fallback("index.html")
}

fn try_asset(path: &str) -> Option<Response> {
    let trimmed = path.trim_start_matches('/');
    let file = WebAssets::get(trimmed)?;
    let mime = mime_guess::from_path(trimmed).first_or_octet_stream();
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data.into_owned()))
            .expect("build asset response"),
    )
}

fn asset_or_fallback(name: &str) -> Response {
    if let Some(resp) = try_asset(name) {
        return resp;
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(
            "<!doctype html><html><body><h1>topic-tree-with-qav</h1>\
             <p>frontend bundle not embedded; run <code>pnpm -C web build</code>.</p>\
             </body></html>",
        ))
        .expect("build fallback response")
}
