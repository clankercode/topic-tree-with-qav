//! WebSocket upgrade + per-connection handler.
//!
//! Connection lifecycle (M1 surface; see protocol.md §lifecycle):
//!   1. Client connects to `/ws?room=<id>` and we upgrade.
//!   2. First inbound frame **must** be `Hello`. Anything else → close with
//!      `protocol_violation`.
//!   3. We verify the room exists in SQLite. Missing → `room_not_found`.
//!   4. If `role=host`, the supplied `adminToken` is argon2id-verified
//!      against the stored hash *once*. Result is cached as the connection's
//!      role for its lifetime — no per-message rehashing.
//!   5. We register the client in the room's presence map, send a Welcome
//!      with the M1 snapshot, then broadcast a `PresenceUpdate` to all
//!      subscribers if this is the guest's first active client.
//!   6. From there: route inbound messages (SetDisplayName, GetSnapshot,
//!      Pong) and forward broadcast events to the writer. Server emits a
//!      Ping every 25s; a 60s silence window closes the socket.
//!
//! Closing the socket runs cleanup: drop the client from presence and
//! emit a `PresenceUpdate` if the guest is now fully disconnected.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio::task;
use tokio::time::{interval, timeout};
use uuid::Uuid;

use crate::api::now_ms;
use crate::auth::verify_admin_token;
use crate::proto::{
    error_codes, ClientMsg, Role, ServerMsg, You, PROTOCOL_VERSION,
};
use crate::room::Room;
use crate::state::AppState;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
const HELLO_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub room: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, q.room))
}

async fn handle_socket(socket: WebSocket, state: AppState, room_id: String) {
    if let Err(e) = run_connection(socket, state, room_id).await {
        tracing::debug!(error = %e, "ws connection ended");
    }
}

#[derive(Debug, thiserror::Error)]
enum ConnError {
    #[error("socket closed before hello")]
    NoHello,
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("io: {0}")]
    Io(String),
}

async fn run_connection(
    socket: WebSocket,
    state: AppState,
    room_id: String,
) -> Result<(), ConnError> {
    let (mut sink, mut stream) = socket.split();

    // ── 1. Await Hello (with a deadline so dead sockets don't pile up). ──
    let hello_frame = match timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(msg))) => msg,
        Ok(Some(Err(e))) => return Err(ConnError::Io(e.to_string())),
        Ok(None) => return Err(ConnError::NoHello),
        Err(_) => {
            let _ = send(&mut sink, &error_frame(error_codes::PROTOCOL_VIOLATION, "hello timeout", None, 0)).await;
            return Err(ConnError::NoHello);
        }
    };
    let hello_text = match hello_frame {
        Message::Text(t) => t,
        Message::Close(_) => return Err(ConnError::NoHello),
        _ => {
            let _ = send(&mut sink, &error_frame(error_codes::PROTOCOL_VIOLATION, "expected text Hello", None, 0)).await;
            return Err(ConnError::Protocol("non-text hello".into()));
        }
    };

    let parsed: ClientMsg = match serde_json::from_str(&hello_text) {
        Ok(m) => m,
        Err(e) => {
            let _ = send(&mut sink, &error_frame(error_codes::BAD_REQUEST, &format!("bad hello: {e}"), None, 0)).await;
            return Err(ConnError::Protocol(e.to_string()));
        }
    };

    let (role_req, guest_id, display_name, admin_token, hello_id) = match parsed {
        ClientMsg::Hello {
            v, id, role, guest_id, display_name, admin_token,
        } => {
            if v != PROTOCOL_VERSION {
                let _ = send(&mut sink, &error_frame(error_codes::BAD_REQUEST, "unsupported protocol version", id, 0)).await;
                return Err(ConnError::Protocol("bad v".into()));
            }
            (role, guest_id, display_name.unwrap_or_default(), admin_token, id)
        }
        _ => {
            let _ = send(&mut sink, &error_frame(error_codes::PROTOCOL_VIOLATION, "first message must be Hello", None, 0)).await;
            return Err(ConnError::Protocol("first not hello".into()));
        }
    };

    if guest_id.trim().is_empty() {
        let _ = send(&mut sink, &error_frame(error_codes::BAD_REQUEST, "guestId required", hello_id, 0)).await;
        return Err(ConnError::Protocol("empty guest id".into()));
    }

    // ── 2. Verify the room exists and load it. ──
    let row = {
        let db = state.db.clone();
        let id = room_id.clone();
        task::spawn_blocking(move || -> rusqlite::Result<Option<(String, String, i64)>> {
            let conn = db.get().map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            conn.query_row(
                "SELECT id, title, created_at FROM rooms WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
            )
            .map(Some)
            .or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) })
        })
        .await
        .map_err(|e| ConnError::Io(e.to_string()))?
    };
    let (rid, title, created_at) = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = send(&mut sink, &error_frame(error_codes::ROOM_NOT_FOUND, "no such room", hello_id, 0)).await;
            return Err(ConnError::Protocol("room missing".into()));
        }
        Err(e) => {
            tracing::error!(error = %e, room = %room_id, "room lookup failed");
            let _ = send(&mut sink, &error_frame(error_codes::BAD_REQUEST, "lookup failed", hello_id, 0)).await;
            return Err(ConnError::Io(e.to_string()));
        }
    };

    // ── 3. Verify role. Hosts must present a valid admin token. ──
    let role = match role_req {
        Role::Guest => Role::Guest,
        Role::Host => {
            let token = match &admin_token {
                Some(t) if !t.is_empty() => t.clone(),
                _ => {
                    let _ = send(&mut sink, &error_frame(error_codes::UNAUTHORIZED, "adminToken required for host", hello_id, 0)).await;
                    return Err(ConnError::Protocol("no admin token".into()));
                }
            };
            let stored_hash: Option<String> = {
                let db = state.db.clone();
                let id = rid.clone();
                task::spawn_blocking(move || -> rusqlite::Result<String> {
                    let conn = db.get().map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    conn.query_row(
                        "SELECT admin_token_hash FROM rooms WHERE id = ?1",
                        rusqlite::params![id],
                        |r| r.get::<_, String>(0),
                    )
                })
                .await
                .map_err(|e| ConnError::Io(e.to_string()))?
                .ok()
            };
            let Some(hash) = stored_hash else {
                let _ = send(&mut sink, &error_frame(error_codes::UNAUTHORIZED, "no admin token configured", hello_id, 0)).await;
                return Err(ConnError::Protocol("no hash".into()));
            };
            let ok = task::spawn_blocking(move || verify_admin_token(&token, &hash))
                .await
                .map_err(|e| ConnError::Io(e.to_string()))?
                .unwrap_or(false);
            if !ok {
                let _ = send(&mut sink, &error_frame(error_codes::UNAUTHORIZED, "invalid admin token", hello_id, 0)).await;
                return Err(ConnError::Protocol("bad admin token".into()));
            }
            Role::Host
        }
    };

    // ── 4. Register with the room hub + send Welcome + broadcast presence. ──
    let room = state.rooms.get_or_create(&rid, &title, created_at);
    let client_id = Uuid::new_v4().to_string();
    let effective_name = if display_name.is_empty() {
        match role { Role::Host => "Host".to_string(), Role::Guest => "Guest".to_string() }
    } else {
        display_name
    };
    let presence_changed = room.add_client(
        guest_id.clone(),
        client_id.clone(),
        effective_name.clone(),
        now_ms(),
    );

    let you = You {
        client_id: client_id.clone(),
        role,
        guest_id: guest_id.clone(),
    };
    let snapshot = room.snapshot_for(you.clone(), &guest_id);
    let welcome = ServerMsg::Welcome {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq: room.current_seq(),
        you: you.clone(),
        snapshot,
    };
    if let Err(e) = send(&mut sink, &welcome).await {
        room.remove_client(&guest_id, &client_id);
        return Err(ConnError::Io(e));
    }
    if let Some(ref rid_echo) = hello_id {
        let ack = ServerMsg::Ack {
            v: PROTOCOL_VERSION,
            ts: now_ms(),
            seq: room.current_seq(),
            ref_id: rid_echo.clone(),
        };
        let _ = send(&mut sink, &ack).await;
    }

    let mut rx = room.subscribe();
    if presence_changed {
        broadcast_presence(&room);
    }

    // ── 5. Main loop. ──
    let result = main_loop(
        &mut sink,
        &mut stream,
        &mut rx,
        &room,
        &client_id,
        &guest_id,
        role,
    )
    .await;

    // ── 6. Cleanup. ──
    let now_disconnected = room.remove_client(&guest_id, &client_id);
    if now_disconnected {
        broadcast_presence(&room);
    }
    result
}

async fn main_loop(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    rx: &mut broadcast::Receiver<ServerMsg>,
    room: &Arc<Room>,
    client_id: &str,
    guest_id: &str,
    role: Role,
) -> Result<(), ConnError> {
    let mut hb = interval(HEARTBEAT_INTERVAL);
    hb.tick().await; // first tick is immediate; skip it
    loop {
        tokio::select! {
            biased;
            inbound = timeout(IDLE_TIMEOUT, stream.next()) => {
                let frame = match inbound {
                    Ok(Some(Ok(m))) => m,
                    Ok(Some(Err(e))) => return Err(ConnError::Io(e.to_string())),
                    Ok(None) => return Ok(()),
                    Err(_) => {
                        tracing::debug!(client = %client_id, "ws idle timeout");
                        return Ok(());
                    }
                };
                match frame {
                    Message::Text(t) => {
                        if let Err(e) = handle_text(sink, t, room, client_id, guest_id, role).await {
                            tracing::debug!(error = %e, "handle_text failed");
                        }
                    }
                    Message::Ping(p) => { let _ = sink.send(Message::Pong(p)).await; }
                    Message::Pong(_) => {}
                    Message::Close(_) => return Ok(()),
                    Message::Binary(_) => {
                        let _ = send(sink, &error_frame(error_codes::BAD_REQUEST, "binary frames not supported", None, room.current_seq())).await;
                    }
                }
            }
            out = rx.recv() => {
                match out {
                    Ok(msg) => {
                        if let Err(e) = send(sink, &msg).await {
                            return Err(ConnError::Io(e));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Force resync: send a fresh snapshot so the client
                        // closes the seq gap without manual GetSnapshot.
                        let snap = room.snapshot_for(
                            You { client_id: client_id.to_string(), role, guest_id: guest_id.to_string() },
                            guest_id,
                        );
                        let msg = ServerMsg::RoomSnapshot {
                            v: PROTOCOL_VERSION,
                            ts: now_ms(),
                            seq: room.current_seq(),
                            snapshot: snap,
                        };
                        let _ = send(sink, &msg).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
            _ = hb.tick() => {
                let ping = ServerMsg::Ping { v: PROTOCOL_VERSION, ts: now_ms(), seq: room.current_seq() };
                if let Err(e) = send(sink, &ping).await {
                    return Err(ConnError::Io(e));
                }
            }
        }
    }
}

async fn handle_text(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    text: String,
    room: &Arc<Room>,
    client_id: &str,
    guest_id: &str,
    role: Role,
) -> Result<(), String> {
    let msg: ClientMsg = match serde_json::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            let _ = send(sink, &error_frame(error_codes::BAD_REQUEST, &format!("parse: {e}"), None, room.current_seq())).await;
            return Err(e.to_string());
        }
    };
    match msg {
        ClientMsg::Hello { id, .. } => {
            let _ = send(sink, &error_frame(error_codes::PROTOCOL_VIOLATION, "duplicate Hello on established connection", id, room.current_seq())).await;
            Ok(())
        }
        ClientMsg::SetDisplayName { id, name, .. } => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() || trimmed.len() > 64 {
                let _ = send(sink, &error_frame(error_codes::BAD_REQUEST, "name must be 1..=64 chars", id, room.current_seq())).await;
                return Ok(());
            }
            if room.set_display_name(guest_id, trimmed) {
                broadcast_presence(room);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack { v: PROTOCOL_VERSION, ts: now_ms(), seq: room.current_seq(), ref_id: rid };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::GetSnapshot { id, .. } => {
            let snap = room.snapshot_for(
                You { client_id: client_id.to_string(), role, guest_id: guest_id.to_string() },
                guest_id,
            );
            let msg = ServerMsg::RoomSnapshot { v: PROTOCOL_VERSION, ts: now_ms(), seq: room.current_seq(), snapshot: snap };
            send(sink, &msg).await?;
            if let Some(rid) = id {
                let ack = ServerMsg::Ack { v: PROTOCOL_VERSION, ts: now_ms(), seq: room.current_seq(), ref_id: rid };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::Pong { .. } => Ok(()),
    }
}

fn broadcast_presence(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::PresenceUpdate {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        guests: room.guests(),
    };
    // A send error here just means there are no subscribers right now;
    // safe to ignore — the broadcast lives on the room and new subscribers
    // will pick up presence in their next snapshot.
    let _ = room.broadcast.send(msg);
}

fn error_frame(code: &str, message: &str, ref_id: Option<String>, seq: u64) -> ServerMsg {
    ServerMsg::Error {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        code: code.to_string(),
        message: message.to_string(),
        ref_id,
    }
}

async fn send(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMsg,
) -> Result<(), String> {
    let s = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    sink.send(Message::Text(s)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    // The full ws lifecycle is exercised by integration tests in
    // server/tests/ws_handshake.rs against a live `axum::serve` instance.
    // This module is intentionally empty so unit-level isolation of the
    // socket loop doesn't fight Axum's ws extractor.
}
