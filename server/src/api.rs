//! REST API routes (non-WebSocket).
//!
//! M1: just `POST /api/rooms` to mint a new room. Subsequent phases add
//! optional read endpoints (e.g. `GET /api/rooms/:id`) only if the
//! frontend genuinely needs an HTTP fallback for ws-driven state.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tokio::task;

use crate::auth::{hash_admin_token, is_valid_room_id, new_admin_token, new_room_id};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/rooms", post(create_room))
        .route("/api/rooms/:room_id", get(get_room))
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetRoomResp {
    pub room_id: String,
    pub title: String,
}

const DEFAULT_TITLE: &str = "Untitled";

async fn create_room(State(state): State<AppState>, body: Option<Json<CreateRoomReq>>) -> Response {
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
        Ok(Err(_)) => return server_error("hash failed"),
        Err(_) => return server_error("hash join failed"),
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
            Ok(Err(_)) => return server_error("db insert failed"),
            Err(_) => return server_error("db insert join failed"),
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

async fn get_room(State(state): State<AppState>, Path(room_id): Path<String>) -> Response {
    if !is_valid_room_id(&room_id) {
        return not_found("invalid room id");
    }

    let db = state.db.clone();
    let id = room_id.clone();
    let row = task::spawn_blocking(move || -> Result<Option<(String, String)>, String> {
        let conn = db.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, title FROM rooms WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
    .await;

    match row {
        Ok(Ok(Some((id, title)))) => {
            let resp = GetRoomResp { room_id: id, title };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Ok(Ok(None)) => not_found("room not found"),
        Ok(Err(_)) => server_error("db lookup failed"),
        Err(_) => server_error("db lookup join failed"),
    }
}

#[derive(Debug, Serialize)]
struct ErrorResp {
    error: String,
}

fn not_found(msg: &str) -> Response {
    let body = ErrorResp {
        error: msg.to_string(),
    };
    (StatusCode::NOT_FOUND, Json(body)).into_response()
}

fn server_error(msg: &str) -> Response {
    tracing::error!(error = %msg, "api request failed");
    let body = ErrorResp {
        error: format!("internal error: {}", msg),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}

/// Wall clock used by the server for every visible `ts` and
/// `created_at` value.
///
/// When `TEST_FIXED_NOW` is set in the environment at process start,
/// `now_ms()` returns the parsed integer for the entire process
/// lifetime. This is the seam visual-regression tests use to stabilise
/// timestamps in screenshots without freezing the runtime's wall
/// clock. The lookup runs *once*, lazily, behind a `OnceLock` — flipping
/// the env after startup has no effect.
///
/// See `.plan/2026-05-25-followup/testing.md` §5.
pub(crate) fn now_ms() -> i64 {
    static FIXED: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
    let fixed = FIXED.get_or_init(|| {
        std::env::var("TEST_FIXED_NOW")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
    });
    if let Some(ts) = *fixed {
        return ts;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
