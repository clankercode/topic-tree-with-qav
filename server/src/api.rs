//! REST API routes (non-WebSocket).
//!
//! M1: just `POST /api/rooms` to mint a new room. Subsequent phases add
//! optional read endpoints (e.g. `GET /api/rooms/:id`) only if the
//! frontend genuinely needs an HTTP fallback for ws-driven state.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::task;

use crate::auth::{hash_admin_token, new_admin_token, new_room_id};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/rooms", post(create_room))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomReq {
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomResp {
    pub room_id: String,
    pub admin_token: String,
    pub admin_url: String,
    pub join_url: String,
    pub title: String,
    pub created_at: i64,
}

const DEFAULT_TITLE: &str = "Untitled";

async fn create_room(
    State(state): State<AppState>,
    body: Option<Json<CreateRoomReq>>,
) -> Response {
    let req = body.map(|Json(r)| r).unwrap_or_default();
    let title = req
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| DEFAULT_TITLE.to_string());

    let room_id = new_room_id();
    let admin_token = new_admin_token();
    let now_ms = now_ms();

    // argon2 is CPU-bound; run on the blocking pool so we don't stall the
    // tokio reactor. Errors here are server faults, not client.
    let token_for_hash = admin_token.clone();
    let hash = match task::spawn_blocking(move || hash_admin_token(&token_for_hash)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => return server_error(format!("hash failed: {e}")),
        Err(e) => return server_error(format!("hash join failed: {e}")),
    };

    {
        let db = state.db.clone();
        let id = room_id.clone();
        let title = title.clone();
        let insert = task::spawn_blocking(move || -> Result<(), String> {
            let conn = db.get().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO rooms (id, title, admin_token_hash, created_at, last_active_at) \
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![id, title, hash, now_ms],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
        .await;
        match insert {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return server_error(format!("insert failed: {e}")),
            Err(e) => return server_error(format!("insert join failed: {e}")),
        }
    }

    let resp = CreateRoomResp {
        admin_url: format!("/r/{room_id}?admin={admin_token}"),
        join_url: format!("/r/{room_id}"),
        room_id,
        admin_token,
        title,
        created_at: now_ms,
    };
    (StatusCode::CREATED, Json(resp)).into_response()
}

fn server_error(msg: String) -> Response {
    tracing::error!(error = %msg, "create_room failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
