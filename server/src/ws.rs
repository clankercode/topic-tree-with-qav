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
use crate::auth::{is_valid_display_name, is_valid_guest_id, verify_admin_token};
use crate::metrics::SharedMetrics;
use crate::proto::{
    error_codes, ClientMsg, Question, Role, ServerMsg, Topic, TopicStatus, You, PROTOCOL_VERSION,
};
use crate::rate_limit::Quota;
use crate::room::Room;
use crate::state::{global_rate_limiter, AppState};

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
    // Reject pathologically large frames before the handler sees them.
    ws.max_message_size(2 * 1024 * 1024)
        .max_frame_size(1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state, q.room))
}

async fn handle_socket(socket: WebSocket, state: AppState, room_id: String) {
    if !crate::auth::is_valid_room_id(&room_id) {
        tracing::debug!(room_id = %room_id, "rejecting malformed room id");
        return;
    }
    if let Err(e) = run_connection(socket, state, room_id.clone()).await {
        tracing::debug!(room_id = %room_id, error = %e, "ws connection ended");
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
    let metrics = state.metrics.clone();

    // ── 1. Await Hello (with a deadline so dead sockets don't pile up). ──
    let hello_frame = match timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(msg))) => msg,
        Ok(Some(Err(e))) => return Err(ConnError::Io(e.to_string())),
        Ok(None) => return Err(ConnError::NoHello),
        Err(_) => {
            let _ = send(
                &mut sink,
                &error_frame(error_codes::PROTOCOL_VIOLATION, "hello timeout", None, 0),
            )
            .await;
            return Err(ConnError::NoHello);
        }
    };
    let hello_text = match hello_frame {
        Message::Text(t) => t,
        Message::Close(_) => return Err(ConnError::NoHello),
        _ => {
            let _ = send(
                &mut sink,
                &error_frame(
                    error_codes::PROTOCOL_VIOLATION,
                    "expected text Hello",
                    None,
                    0,
                ),
            )
            .await;
            return Err(ConnError::Protocol("non-text hello".into()));
        }
    };

    let parsed: ClientMsg = match serde_json::from_str(&hello_text) {
        Ok(m) => m,
        Err(e) => {
            let _ = send(
                &mut sink,
                &error_frame(
                    error_codes::BAD_REQUEST,
                    &format!("bad hello: {e}"),
                    None,
                    0,
                ),
            )
            .await;
            return Err(ConnError::Protocol(e.to_string()));
        }
    };

    let (role_req, guest_id, display_name, admin_token, hello_id) = match parsed {
        ClientMsg::Hello {
            v,
            id,
            role,
            guest_id,
            display_name,
            admin_token,
        } => {
            if v != PROTOCOL_VERSION {
                let _ = send(
                    &mut sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "unsupported protocol version",
                        id,
                        0,
                    ),
                )
                .await;
                return Err(ConnError::Protocol("bad v".into()));
            }
            (
                role,
                guest_id,
                display_name.unwrap_or_default(),
                admin_token,
                id,
            )
        }
        _ => {
            let _ = send(
                &mut sink,
                &error_frame(
                    error_codes::PROTOCOL_VIOLATION,
                    "first message must be Hello",
                    None,
                    0,
                ),
            )
            .await;
            return Err(ConnError::Protocol("first not hello".into()));
        }
    };

    if !is_valid_guest_id(&guest_id) {
        let _ = send(
            &mut sink,
            &error_frame(
                error_codes::BAD_REQUEST,
                "guestId must be 1..=64 visible chars",
                hello_id,
                0,
            ),
        )
        .await;
        return Err(ConnError::Protocol("invalid guest id".into()));
    }
    if !display_name.is_empty() && !is_valid_display_name(&display_name) {
        let _ = send(
            &mut sink,
            &error_frame(
                error_codes::BAD_REQUEST,
                "displayName must be 1..=64 chars",
                hello_id,
                0,
            ),
        )
        .await;
        return Err(ConnError::Protocol("invalid display name".into()));
    }

    // ── 2. Verify the room exists and load it. ──
    let row = {
        let db = state.db.clone();
        let id = room_id.clone();
        task::spawn_blocking(move || -> rusqlite::Result<Option<(String, String, i64)>> {
            let conn = db
                .get()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            conn.query_row(
                "SELECT id, title, created_at FROM rooms WHERE id = ?1",
                rusqlite::params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .map(Some)
            .or_else(|e| {
                if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })
        })
        .await
        .map_err(|e| ConnError::Io(e.to_string()))?
    };
    let (rid, title, created_at) = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = send(
                &mut sink,
                &error_frame(error_codes::ROOM_NOT_FOUND, "no such room", hello_id, 0),
            )
            .await;
            return Err(ConnError::Protocol("room missing".into()));
        }
        Err(e) => {
            tracing::error!(error = %e, room = %room_id, "room lookup failed");
            let _ = send(
                &mut sink,
                &error_frame(error_codes::BAD_REQUEST, "lookup failed", hello_id, 0),
            )
            .await;
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
                    let _ = send(
                        &mut sink,
                        &error_frame(
                            error_codes::UNAUTHORIZED,
                            "adminToken required for host",
                            hello_id,
                            0,
                        ),
                    )
                    .await;
                    return Err(ConnError::Protocol("no admin token".into()));
                }
            };
            let stored_hash: Option<String> = {
                let db = state.db.clone();
                let id = rid.clone();
                task::spawn_blocking(move || -> rusqlite::Result<String> {
                    let conn = db
                        .get()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
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
                let _ = send(
                    &mut sink,
                    &error_frame(
                        error_codes::UNAUTHORIZED,
                        "no admin token configured",
                        hello_id,
                        0,
                    ),
                )
                .await;
                return Err(ConnError::Protocol("no hash".into()));
            };
            let ok = task::spawn_blocking(move || verify_admin_token(&token, &hash))
                .await
                .map_err(|e| ConnError::Io(e.to_string()))?
                .unwrap_or(false);
            if !ok {
                let _ = send(
                    &mut sink,
                    &error_frame(
                        error_codes::UNAUTHORIZED,
                        "invalid admin token",
                        hello_id,
                        0,
                    ),
                )
                .await;
                return Err(ConnError::Protocol("bad admin token".into()));
            }
            Role::Host
        }
    };

    // ── 4. Check moderation: kicked guests are rejected at Hello. ──
    if role == Role::Guest {
        let kicked: bool = {
            let db = state.db.clone();
            let rid = rid.clone();
            let gid = guest_id.clone();
            let inner = task::spawn_blocking(move || {
                let conn = db
                    .get()
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                conn.query_row(
                    "SELECT kicked FROM moderation WHERE room_id = ?1 AND guest_id = ?2",
                    rusqlite::params![rid, gid],
                    |r| r.get::<_, i32>(0),
                )
                .map(|k| k != 0)
                .or_else(|e| {
                    if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                        Ok(false)
                    } else {
                        Err(e)
                    }
                })
            })
            .await
            .map_err(|e| ConnError::Io(e.to_string()))?;
            match inner {
                Ok(k) => k,
                Err(e) => return Err(ConnError::Io(e.to_string())),
            }
        };
        if kicked {
            let kick_notice = ServerMsg::KickNotice {
                v: PROTOCOL_VERSION,
                ts: now_ms(),
                seq: 0,
            };
            let _ = send(&mut sink, &kick_notice).await;
            let _ = send(
                &mut sink,
                &error_frame(error_codes::UNAUTHORIZED, "removed by host", hello_id, 0),
            )
            .await;
            return Err(ConnError::Protocol("kicked".into()));
        }
    }

    // ── 5. Load muted state from DB for guests. ──
    let initial_muted: bool = if role == Role::Guest {
        let db = state.db.clone();
        let rid = rid.clone();
        let gid = guest_id.clone();
        let inner = task::spawn_blocking(move || {
            let conn = db
                .get()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            conn.query_row(
                "SELECT muted FROM moderation WHERE room_id = ?1 AND guest_id = ?2",
                rusqlite::params![rid, gid],
                |r| r.get::<_, i32>(0),
            )
            .map(|m| m != 0)
            .or_else(|e| {
                if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                    Ok(false)
                } else {
                    Err(e)
                }
            })
        })
        .await
        .map_err(|e| ConnError::Io(e.to_string()))?;
        match inner {
            Ok(m) => m,
            Err(e) => return Err(ConnError::Io(e.to_string())),
        }
    } else {
        false
    };

    // ── 6. Register with the room hub + send Welcome + broadcast presence. ──
    let room = state.rooms.get_or_create(&rid, &title, created_at);
    let client_id = Uuid::new_v4().to_string();
    let effective_name = if display_name.is_empty() {
        match role {
            Role::Host => "Host".to_string(),
            Role::Guest => "Guest".to_string(),
        }
    } else {
        display_name
    };
    let presence_changed = room.add_client(
        guest_id.clone(),
        client_id.clone(),
        effective_name.clone(),
        now_ms(),
        initial_muted,
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
    {
        let m = metrics.read().await;
        m.ws_connections_opened.inc();
    }
    tracing::info!(room_id = %room_id, client_id = %client_id, "ws connection opened");
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

    // ── 7. Main loop. ──
    let result = main_loop(
        &mut sink,
        &mut stream,
        &mut rx,
        &room,
        &client_id,
        &guest_id,
        role,
        &state,
        &metrics,
    )
    .await;

    // ── 8. Cleanup. ──
    let now_disconnected = room.remove_client(&guest_id, &client_id);
    if now_disconnected {
        broadcast_presence(&room);
    }
    global_rate_limiter().forget_client(&client_id);
    {
        let m = metrics.read().await;
        m.ws_connections_closed.inc();
    }
    tracing::info!(room_id = %room_id, client_id = %client_id, "ws connection closed");
    result
}

#[allow(clippy::too_many_arguments)]
async fn main_loop(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    rx: &mut broadcast::Receiver<ServerMsg>,
    room: &Arc<Room>,
    client_id: &str,
    guest_id: &str,
    role: Role,
    state: &AppState,
    metrics: &SharedMetrics,
) -> Result<(), ConnError> {
    let mut hb = interval(HEARTBEAT_INTERVAL);
    hb.tick().await; // first tick is immediate; skip it
    let room_id = room.id.clone();
    loop {
        tokio::select! {
            biased;
            inbound = timeout(IDLE_TIMEOUT, stream.next()) => {
                let frame = match inbound {
                    Ok(Some(Ok(m))) => m,
                    Ok(Some(Err(e))) => return Err(ConnError::Io(e.to_string())),
                    Ok(None) => return Ok(()),
                    Err(_) => {
                        tracing::debug!(room_id = %room_id, client_id = %client_id, "ws idle timeout");
                        return Ok(());
                    }
                };
                match frame {
                    Message::Text(t) => {
                        if let Err(e) = handle_text(sink, t, room, client_id, guest_id, role, state).await {
                            tracing::debug!(room_id = %room_id, client_id = %client_id, error = %e, "handle_text failed");
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
                        {
                            let m = metrics.read().await;
                            m.ws_messages_sent.inc();
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
    state: &AppState,
) -> Result<(), String> {
    let msg: ClientMsg = match serde_json::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            let _ = send(
                sink,
                &error_frame(
                    error_codes::BAD_REQUEST,
                    &format!("parse: {e}"),
                    None,
                    room.current_seq(),
                ),
            )
            .await;
            return Err(e.to_string());
        }
    };
    #[allow(clippy::needless_borrow)]
    match msg {
        ClientMsg::Hello { id, .. } => {
            let _ = send(
                sink,
                &error_frame(
                    error_codes::PROTOCOL_VIOLATION,
                    "duplicate Hello on established connection",
                    id,
                    room.current_seq(),
                ),
            )
            .await;
            Ok(())
        }
        ClientMsg::SetDisplayName { id, name, .. } => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() || trimmed.len() > 64 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "name must be 1..=64 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if room.set_display_name(guest_id, trimmed) {
                broadcast_presence(room);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::GetSnapshot { id, .. } => {
            let snap = room.snapshot_for(
                You {
                    client_id: client_id.to_string(),
                    role,
                    guest_id: guest_id.to_string(),
                },
                guest_id,
            );
            let msg = ServerMsg::RoomSnapshot {
                v: PROTOCOL_VERSION,
                ts: now_ms(),
                seq: room.current_seq(),
                snapshot: snap,
            };
            send(sink, &msg).await?;
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::Pong { .. } => Ok(()),
        ClientMsg::AddTopic {
            id,
            parent_id,
            title,
            after_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let title = title.trim().to_string();
            if title.is_empty() || title.len() > 200 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "title must be 1..=200 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let topic_id = Uuid::new_v4().to_string();
            let now = now_ms();
            let ord = if let Some(after) = after_id {
                room.topics()
                    .iter()
                    .find(|t| t.id == after)
                    .map(|t| t.ord + 0.5)
                    .unwrap_or(1.0)
            } else {
                room.topics().iter().map(|t| t.ord).fold(0.0, f64::max) + 1.0
            };
            let topic = Topic {
                id: topic_id,
                parent_id,
                title,
                ord,
                status: TopicStatus::Pending,
                created_at: now,
            };
            room.add_topic(topic);
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::RenameTopic {
            id,
            topic_id,
            title,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let title = title.trim().to_string();
            if title.is_empty() || title.len() > 200 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "title must be 1..=200 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if !room.rename_topic(&topic_id, title) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "topic not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::MoveTopic {
            id,
            topic_id,
            new_parent_id,
            after_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let ord = if let Some(after) = after_id {
                room.topics()
                    .iter()
                    .find(|t| t.id == *after)
                    .map(|t| t.ord + 0.001)
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            if !room.move_topic(&topic_id, new_parent_id, ord) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "topic not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::DeleteTopic { id, topic_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            room.delete_topic(&topic_id);
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::SetActiveTopic { id, topic_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            room.set_active_topic(topic_id.clone());
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::MarkTopicDone {
            id, topic_id, done, ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if !room.mark_topic_done(&topic_id, done) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "topic not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            broadcast_topic_tree(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::SubmitQuestion {
            id,
            text,
            anonymous,
            ..
        } => {
            if role != Role::Guest {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::FORBIDDEN,
                        "only guests can submit questions",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if room.is_muted(guest_id) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::MUTED,
                        "you are muted and cannot submit questions",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if !global_rate_limiter().check(client_id, "SubmitQuestion", Quota::per_minute(6.0)) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::RATE_LIMIT,
                        "too many questions, slow down",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let text = text.trim().to_string();
            if text.is_empty() || text.len() > 500 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "question text must be 1..=500 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let question_id = Uuid::new_v4().to_string();
            let now = now_ms();
            let presence = room.presence();
            let author_name = presence
                .iter()
                .find(|p| p.guest_id == guest_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| "Anonymous".to_string());
            let question = Question {
                id: question_id.clone(),
                room_id: room.id.clone(),
                author_guest_id: guest_id.to_string(),
                author_name,
                anonymous,
                text,
                answered: false,
                created_at: now,
                vote_count: 0,
            };
            room.add_question(question.clone());
            broadcast_question_added(room, &question);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::VoteQuestion {
            id,
            question_id,
            vote,
            ..
        } => {
            if role != Role::Guest {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::FORBIDDEN,
                        "only guests can vote",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if room.is_muted(guest_id) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::MUTED,
                        "you are muted and cannot vote",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if !global_rate_limiter().check(client_id, "VoteQuestion", Quota::per_minute(30.0)) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::RATE_LIMIT,
                        "too many votes, slow down",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let (count, _) = match room.vote_question(&question_id, guest_id, vote) {
                Some(c) => c,
                None => {
                    let _ = send(
                        sink,
                        &error_frame(
                            error_codes::BAD_REQUEST,
                            "question not found",
                            id,
                            room.current_seq(),
                        ),
                    )
                    .await;
                    return Ok(());
                }
            };
            broadcast_vote_updated(room, &question_id, count, guest_id);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::MarkQuestionAnswered {
            id,
            question_id,
            answered,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let mut question = match room.get_question(&question_id) {
                Some(q) => q,
                None => {
                    let _ = send(
                        sink,
                        &error_frame(
                            error_codes::BAD_REQUEST,
                            "question not found",
                            id,
                            room.current_seq(),
                        ),
                    )
                    .await;
                    return Ok(());
                }
            };
            question.answered = answered;
            if !room.update_question(question.clone()) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "question not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            broadcast_question_updated(room, &question);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::DeleteQuestion {
            id, question_id, ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let removed = room.delete_question(&question_id);
            if removed {
                broadcast_question_deleted(room, &question_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::KickGuest {
            id,
            guest_id: target_guest_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let room_id = room.id.clone();
            let target = target_guest_id.clone();
            let db = state.db.clone();
            task::spawn_blocking(move || {
                if let Err(e) = db.set_kicked(&room_id, &target, true) {
                    tracing::error!(error = %e, room_id = %room_id, "failed to persist kick");
                }
            });
            room.kick_guest(&target_guest_id);
            broadcast_presence(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::MuteGuest {
            id,
            guest_id: target_guest_id,
            muted,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let room_id = room.id.clone();
            let target = target_guest_id.clone();
            let db = state.db.clone();
            let muted_flag = muted;
            task::spawn_blocking(move || {
                if let Err(e) = db.set_muted(&room_id, &target, muted_flag) {
                    tracing::error!(error = %e, room_id = %room_id, "failed to persist mute");
                }
            });
            room.set_muted(&target_guest_id, muted);
            broadcast_presence(room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::CreateBoard {
            id, kind, title, ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let title = title.unwrap_or_else(|| "Untitled".into());
            if title.is_empty() || title.len() > 200 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "title must be 1..=200 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let board_id = Uuid::new_v4().to_string();
            let now = now_ms();
            let ord = room.boards().iter().map(|b| b.ord).fold(0.0, f64::max) + 1.0;
            let board = crate::proto::Board {
                id: board_id.clone(),
                kind,
                title,
                created_at: now,
                ord,
            };
            room.create_board(board.clone(), now);
            broadcast_board_created(room, &board);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::RenameBoard {
            id,
            board_id,
            title,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let title = title.trim().to_string();
            if title.is_empty() || title.len() > 200 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "title must be 1..=200 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            match room.rename_board(&board_id, title) {
                Some(board) => {
                    broadcast_board_updated(room, &board);
                }
                None => {
                    let _ = send(
                        sink,
                        &error_frame(
                            error_codes::BAD_REQUEST,
                            "board not found",
                            id,
                            room.current_seq(),
                        ),
                    )
                    .await;
                    return Ok(());
                }
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::DeleteBoard { id, board_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if !room.delete_board(&board_id) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "board not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            broadcast_board_deleted(room, &board_id);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::SetFocusedBoard { id, board_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if !room.board_exists(&board_id) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "board not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            room.set_focused_board(board_id.clone());
            broadcast_focused_board_changed(room, &board_id);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::ExcalidrawUpdate {
            id,
            board_id,
            scene_version,
            elements,
            app_state,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let now = now_ms();
            match room.update_excalidraw_scene(
                &board_id,
                scene_version,
                elements.clone(),
                app_state.clone(),
                now,
            ) {
                crate::room::ExcalidrawUpdateOutcome::Applied => {
                    broadcast_excalidraw_delta(
                        room,
                        &board_id,
                        scene_version,
                        &elements,
                        &app_state,
                    );
                }
                crate::room::ExcalidrawUpdateOutcome::Stale => {
                    // Silently drop — newer state is already authoritative.
                }
                crate::room::ExcalidrawUpdateOutcome::BoardMissing => {
                    let _ = send(
                        sink,
                        &error_frame(
                            error_codes::BAD_REQUEST,
                            "board not found or not an excalidraw board",
                            id,
                            room.current_seq(),
                        ),
                    )
                    .await;
                    return Ok(());
                }
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::RaiseHand { id, topic, .. } => {
            if role != Role::Guest {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::FORBIDDEN,
                        "raise hand is guest-only",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if room.is_muted(guest_id) {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "muted", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let topic = topic.trim().to_string();
            if topic.is_empty() || topic.len() > 80 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "topic must be 1..=80 chars",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let word_count = topic.split_whitespace().count();
            if word_count > 10 {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "topic must be 10 words or fewer",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if !global_rate_limiter().check(client_id, "RaiseHand", Quota::per_minute(2.0)) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::RATE_LIMIT,
                        "too many raised hands, slow down",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            let presence = room.presence();
            let display_name = presence
                .iter()
                .find(|p| p.guest_id == guest_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| "Guest".to_string());
            let now = now_ms();
            room.raise_hand(guest_id, display_name, topic, now);
            broadcast_hands_updated(&room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::LowerHand { id, .. } => {
            room.lower_hand(guest_id);
            broadcast_hands_updated(&room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::CallOnHand {
            id,
            guest_id: target_guest_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let _ = room.call_on_hand(&target_guest_id);
            broadcast_hands_updated(&room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::DismissHand {
            id,
            guest_id: target_guest_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            room.dismiss_hand(&target_guest_id);
            broadcast_hands_updated(&room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PromoteQuestionToTopic {
            id,
            question_id,
            parent_topic_id,
            after_topic_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let Some((_question, topic)) =
                room.promote_question_to_topic(&question_id, parent_topic_id, after_topic_id)
            else {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::BAD_REQUEST,
                        "question not found",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            };
            broadcast_question_promoted_to_topic(&room, &question_id, &topic);
            broadcast_topic_tree(&room);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenStrokeBegin {
            id,
            board_id,
            stroke_id,
            color,
            size,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let now = now_ms();
            if room
                .pen_begin_stroke(&board_id, stroke_id.clone(), color.clone(), size, now)
                .is_some()
            {
                broadcast_pen_stroke_begun(room, &board_id, &stroke_id, &color, size, client_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenStrokeAppend {
            id,
            board_id,
            stroke_id,
            points,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if !global_rate_limiter().check(client_id, "PenStrokeAppend", Quota::per_second(60.0)) {
                return Ok(());
            }
            if room.pen_append_points(&board_id, &stroke_id, points.clone()) {
                broadcast_pen_stroke_appended(room, &board_id, &stroke_id, points);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenStrokeEnd {
            id,
            board_id,
            stroke_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if room.pen_end_stroke(&board_id, &stroke_id) {
                broadcast_pen_stroke_ended(room, &board_id, &stroke_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenTextSet {
            id,
            board_id,
            text_id,
            x,
            y,
            text,
            font_size,
            color,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let now = now_ms();
            let pt = crate::proto::PenText {
                id: text_id.clone(),
                x,
                y,
                text: text.clone(),
                font_size,
                color: color.clone(),
                updated_at: now,
            };
            if room.pen_text_upsert(&board_id, pt.clone(), now) {
                broadcast_pen_text_upserted(room, &board_id, &pt);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenTextDelete {
            id,
            board_id,
            text_id,
            ..
        } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let now = now_ms();
            if room.pen_text_delete(&board_id, &text_id, now) {
                broadcast_pen_text_deleted(room, &board_id, &text_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenClear { id, board_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            let now = now_ms();
            if room.pen_clear(&board_id, now) {
                broadcast_pen_cleared(room, &board_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::PenUndo { id, board_id, .. } => {
            if role != Role::Host {
                let _ = send(
                    sink,
                    &error_frame(error_codes::FORBIDDEN, "admin only", id, room.current_seq()),
                )
                .await;
                return Ok(());
            }
            if let Some((removed_stroke_id, removed_text_id)) = room.pen_undo(&board_id) {
                broadcast_pen_undone(room, &board_id, removed_stroke_id, removed_text_id);
            }
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::Cursor {
            id, board_id, x, y, ..
        } => {
            if !global_rate_limiter().check(client_id, "Cursor", Quota::per_second(30.0)) {
                return Ok(());
            }
            if !room.board_exists(&board_id) {
                return Ok(());
            }
            let display_name = room
                .presence()
                .iter()
                .find(|p| p.guest_id == guest_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_default();
            broadcast_cursor_moved(room, &board_id, client_id, guest_id, &display_name, x, y);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
        ClientMsg::Click {
            id, board_id, x, y, ..
        } => {
            if !global_rate_limiter().check(client_id, "Click", Quota::per_second(5.0)) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::RATE_LIMIT,
                        "click rate exceeded",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
            if !room.board_exists(&board_id) {
                return Ok(());
            }
            let display_name = room
                .presence()
                .iter()
                .find(|p| p.guest_id == guest_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_default();
            broadcast_clicked(room, &board_id, client_id, guest_id, &display_name, x, y);
            if let Some(rid) = id {
                let ack = ServerMsg::Ack {
                    v: PROTOCOL_VERSION,
                    ts: now_ms(),
                    seq: room.current_seq(),
                    ref_id: rid,
                };
                let _ = send(sink, &ack).await;
            }
            Ok(())
        }
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

fn broadcast_topic_tree(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::TopicTreeUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        topics: room.topics(),
        active_topic_id: room.active_topic_id(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_question_added(room: &Arc<Room>, question: &Question) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionAdded {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question: question.to_outbound(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_question_updated(room: &Arc<Room>, question: &Question) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question: question.to_outbound(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_question_deleted(room: &Arc<Room>, question_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_vote_updated(
    room: &Arc<Room>,
    question_id: &str,
    vote_count: u32,
    voter_guest_id: &str,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::VoteUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
        vote_count,
        voter_guest_id: voter_guest_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_board_created(room: &Arc<Room>, board: &crate::proto::Board) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardCreated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board: board.clone(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_board_updated(room: &Arc<Room>, board: &crate::proto::Board) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board: board.clone(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_board_deleted(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_focused_board_changed(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::FocusedBoardChanged {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_excalidraw_delta(
    room: &Arc<Room>,
    board_id: &str,
    scene_version: u64,
    elements: &serde_json::Value,
    app_state: &serde_json::Value,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::ExcalidrawDelta {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        scene_version,
        elements: elements.clone(),
        app_state: app_state.clone(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(dead_code)]
fn broadcast_excalidraw_scene_reset(
    room: &Arc<Room>,
    board_id: &str,
    scene_version: u64,
    elements: &serde_json::Value,
    app_state: &serde_json::Value,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::ExcalidrawSceneReset {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        scene_version,
        elements: elements.clone(),
        app_state: app_state.clone(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(clippy::needless_borrow)]
fn broadcast_hands_updated(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::HandsUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        hands: room.hands_list(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(clippy::needless_borrow)]
fn broadcast_question_promoted_to_topic(room: &Arc<Room>, question_id: &str, topic: &Topic) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionPromotedToTopic {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
        topic: topic.clone(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_stroke_begun(
    room: &Arc<Room>,
    board_id: &str,
    stroke_id: &str,
    color: &str,
    size: f64,
    author_client_id: &str,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeBegun {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
        color: color.to_string(),
        size,
        author_client_id: author_client_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_stroke_appended(
    room: &Arc<Room>,
    board_id: &str,
    stroke_id: &str,
    points: Vec<[f32; 3]>,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeAppended {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
        points,
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_stroke_ended(room: &Arc<Room>, board_id: &str, stroke_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeEnded {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_text_upserted(room: &Arc<Room>, board_id: &str, text: &crate::proto::PenText) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenTextUpserted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        text: text.clone(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_text_deleted(room: &Arc<Room>, board_id: &str, text_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenTextDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        text_id: text_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_cleared(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenCleared {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_pen_undone(
    room: &Arc<Room>,
    board_id: &str,
    removed_stroke_id: Option<String>,
    removed_text_id: Option<String>,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenUndone {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        removed_stroke_id,
        removed_text_id,
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_cursor_moved(
    room: &Arc<Room>,
    board_id: &str,
    client_id: &str,
    guest_id: &str,
    display_name: &str,
    x: f64,
    y: f64,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::CursorMoved {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        client_id: client_id.to_string(),
        guest_id: guest_id.to_string(),
        display_name: display_name.to_string(),
        x,
        y,
    };
    let _ = room.broadcast.send(msg);
}

fn broadcast_clicked(
    room: &Arc<Room>,
    board_id: &str,
    client_id: &str,
    guest_id: &str,
    display_name: &str,
    x: f64,
    y: f64,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::Clicked {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        client_id: client_id.to_string(),
        guest_id: guest_id.to_string(),
        display_name: display_name.to_string(),
        x,
        y,
    };
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

const EXCALIDRAW_SCENE_RESET_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_excalidraw_scene_reset_task(state: AppState) -> task::JoinHandle<()> {
    task::spawn(async move {
        let mut tick_interval = tokio::time::interval(EXCALIDRAW_SCENE_RESET_INTERVAL);
        tick_interval.tick().await;
        loop {
            tick_interval.tick().await;
            let rooms: Vec<Arc<Room>> = state.rooms.iter().collect();
            for room in rooms {
                let scenes_to_reset = room.get_excalidraw_scenes_needing_reset();
                for scene in scenes_to_reset {
                    broadcast_excalidraw_scene_reset(
                        &room,
                        &scene.board_id,
                        scene.scene_version,
                        &scene.elements,
                        &scene.app_state,
                    );
                    room.mark_excalidraw_scene_broadcast(&scene.board_id, scene.scene_version);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    // The full ws lifecycle is exercised by integration tests in
    // server/tests/ws_handshake.rs against a live `axum::serve` instance.
    // This module is intentionally empty so unit-level isolation of the
    // socket loop doesn't fight Axum's ws extractor.
}
