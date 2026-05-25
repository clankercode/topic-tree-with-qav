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
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    broadcast_board_created, broadcast_board_deleted, broadcast_board_updated, broadcast_clicked,
    broadcast_cursor_moved, broadcast_excalidraw_delta, broadcast_excalidraw_scene_reset,
    broadcast_focused_board_changed, broadcast_pen_cleared, broadcast_pen_stroke_appended,
    broadcast_pen_stroke_begun, broadcast_pen_stroke_ended, broadcast_pen_text_deleted,
    broadcast_pen_text_upserted, broadcast_pen_undone, broadcast_presence, enqueue_write,
    error_frame, send, IntentError, SessionCtx,
};
use crate::metrics::SharedMetrics;
use crate::proto::{error_codes, ClientMsg, Role, ServerMsg, You, PROTOCOL_VERSION};
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
        let kicked_in_memory = state
            .rooms
            .get(&rid)
            .map(|r| r.is_kicked(&guest_id))
            .unwrap_or(false);
        let kicked_in_db: bool = {
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
        let kicked = kicked_in_memory || kicked_in_db;
        if kicked {
            let kick_notice = ServerMsg::KickNotice {
                v: PROTOCOL_VERSION,
                ts: now_ms(),
                seq: 0,
                guest_id: guest_id.clone(),
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
    let room = state
        .rooms
        .get_or_create_hydrated(&state.db, &rid, &title, created_at);
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
                        let kick_me = matches!(
                            &msg,
                            ServerMsg::KickNotice { guest_id: target, .. } if target == guest_id
                        );
                        if let Err(e) = send(sink, &msg).await {
                            return Err(ConnError::Io(e));
                        }
                        {
                            let m = metrics.read().await;
                            m.ws_messages_sent.inc();
                        }
                        if kick_me {
                            return Ok(());
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
    if role == Role::Guest
        && !matches!(&msg, ClientMsg::Hello { .. } | ClientMsg::Pong { .. })
        && !room.has_presence(guest_id)
    {
        let _ = send(
            sink,
            &error_frame(
                error_codes::UNAUTHORIZED,
                "removed from room",
                None,
                room.current_seq(),
            ),
        )
        .await;
        return Err("kicked".into());
    }
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
            // Without this gate a guest can spam GetSnapshot and force
            // the server to render the room's full state on each call.
            // The 5/sec cap mirrors the "all others" 20 msg/s catch-all
            // budget in protocol.md §rate-limits but is tighter because
            // each snapshot is much more expensive than a typical intent.
            if !global_rate_limiter().check(client_id, "GetSnapshot", Quota::per_second(5.0)) {
                let _ = send(
                    sink,
                    &error_frame(
                        error_codes::RATE_LIMIT,
                        "snapshot rate exceeded",
                        id,
                        room.current_seq(),
                    ),
                )
                .await;
                return Ok(());
            }
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
        ClientMsg::AddTopic { .. }
        | ClientMsg::RenameTopic { .. }
        | ClientMsg::MoveTopic { .. }
        | ClientMsg::DeleteTopic { .. }
        | ClientMsg::SetActiveTopic { .. }
        | ClientMsg::MarkTopicDone { .. }
        | ClientMsg::ImportTopicTree { .. } => {
            let result = {
                let mut ctx = SessionCtx {
                    sink,
                    room,
                    state,
                    client_id,
                    guest_id,
                    role,
                };
                crate::intents::topics::handle(&mut ctx, msg).await
            };
            handle_intent_result(sink, result).await
        }
        ClientMsg::SubmitQuestion { .. }
        | ClientMsg::VoteQuestion { .. }
        | ClientMsg::MarkQuestionAnswered { .. }
        | ClientMsg::DeleteQuestion { .. }
        | ClientMsg::PromoteQuestionToTopic { .. } => {
            let result = {
                let mut ctx = SessionCtx {
                    sink,
                    room,
                    state,
                    client_id,
                    guest_id,
                    role,
                };
                crate::intents::questions::handle(&mut ctx, msg).await
            };
            handle_intent_result(sink, result).await
        }
        ClientMsg::KickGuest { .. } | ClientMsg::MuteGuest { .. } => {
            let result = {
                let mut ctx = SessionCtx {
                    sink,
                    room,
                    state,
                    client_id,
                    guest_id,
                    role,
                };
                crate::intents::moderation::handle(&mut ctx, msg).await
            };
            handle_intent_result(sink, result).await
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
            enqueue_write(
                state,
                room,
                WriteOpKind::UpsertBoard {
                    board: board.clone(),
                },
            );
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
            match room.rename_board(&board_id, title.clone()) {
                Some(board) => {
                    broadcast_board_updated(room, &board);
                    enqueue_write(
                        state,
                        room,
                        WriteOpKind::RenameBoard {
                            board_id: board_id.clone(),
                            title,
                        },
                    );
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
            enqueue_write(
                state,
                room,
                WriteOpKind::DeleteBoard {
                    board_id: board_id.clone(),
                },
            );
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
            enqueue_write(
                state,
                room,
                WriteOpKind::SetFocusedBoard {
                    board_id: Some(board_id.clone()),
                },
            );
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
                    enqueue_write(
                        state,
                        room,
                        WriteOpKind::UpsertExcalidrawScene {
                            board_id: board_id.clone(),
                            scene_version,
                            elements_json: serde_json::to_string(&elements)
                                .unwrap_or_else(|_| "[]".into()),
                            app_state_json: serde_json::to_string(&app_state)
                                .unwrap_or_else(|_| "{}".into()),
                            updated_at: now,
                        },
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
        ClientMsg::RaiseHand { .. }
        | ClientMsg::LowerHand { .. }
        | ClientMsg::CallOnHand { .. }
        | ClientMsg::DismissHand { .. } => {
            let result = {
                let mut ctx = SessionCtx {
                    sink,
                    room,
                    state,
                    client_id,
                    guest_id,
                    role,
                };
                crate::intents::raise_hand::handle(&mut ctx, msg).await
            };
            handle_intent_result(sink, result).await
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
            if let Some((summary, action_id)) = room.pen_end_stroke(&board_id, &stroke_id) {
                broadcast_pen_stroke_ended(room, &board_id, &stroke_id);
                let created_at = summary.created_at;
                enqueue_write(
                    state,
                    room,
                    WriteOpKind::InsertCompletedPenStroke {
                        board_id: board_id.clone(),
                        stroke: summary,
                        action_id,
                        created_at,
                    },
                );
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
            if let Some((action_id, prior)) = room.pen_text_upsert(&board_id, pt.clone(), now) {
                broadcast_pen_text_upserted(room, &board_id, &pt);
                let before_json = prior.as_ref().and_then(|p| serde_json::to_string(p).ok());
                enqueue_write(
                    state,
                    room,
                    WriteOpKind::UpsertPenText {
                        board_id: board_id.clone(),
                        text: pt,
                        action_id,
                        before_json,
                        created_at: now,
                    },
                );
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
            if let Some((action_id, removed)) = room.pen_text_delete(&board_id, &text_id, now) {
                broadcast_pen_text_deleted(room, &board_id, &text_id);
                // before_json captures the row we just removed so
                // PenUndo can restore it. Fall back to "null" only on
                // serializer error — apply_pen_undo treats that as "no
                // prior state" and skips the restore, matching the
                // semantics of an undo whose row never persisted.
                let before_json =
                    serde_json::to_string(&removed).unwrap_or_else(|_| "null".to_string());
                enqueue_write(
                    state,
                    room,
                    WriteOpKind::DeletePenText {
                        board_id: board_id.clone(),
                        text_id: text_id.clone(),
                        action_id,
                        before_json,
                        created_at: now,
                    },
                );
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
            if let Some((action_id, prior_strokes, prior_texts)) = room.pen_clear(&board_id, now) {
                broadcast_pen_cleared(room, &board_id);
                let prior_stroke_summaries: Vec<crate::proto::PenStrokeSummary> = prior_strokes
                    .into_iter()
                    .map(|s| crate::proto::PenStrokeSummary {
                        id: s.id,
                        color: s.color,
                        size: s.size,
                        points: s.points,
                        created_at: s.created_at,
                        ord: s.ord,
                    })
                    .collect();
                let before_strokes_json = serde_json::to_string(&prior_stroke_summaries)
                    .unwrap_or_else(|_| "[]".to_string());
                let before_texts_json =
                    serde_json::to_string(&prior_texts).unwrap_or_else(|_| "[]".to_string());
                enqueue_write(
                    state,
                    room,
                    WriteOpKind::PenClear {
                        board_id: board_id.clone(),
                        action_id,
                        before_strokes_json,
                        before_texts_json,
                        created_at: now,
                    },
                );
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
            if let Some(outcome) = room.pen_undo(&board_id) {
                broadcast_pen_undone(
                    room,
                    &board_id,
                    outcome.removed_stroke.clone(),
                    outcome.removed_text.clone(),
                );
                enqueue_write(
                    state,
                    room,
                    WriteOpKind::PenUndo {
                        board_id: board_id.clone(),
                        target_action_id: outcome.action_id,
                    },
                );
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

async fn handle_intent_result(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    result: Result<(), IntentError>,
) -> Result<(), String> {
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            let should_close = err.should_close();
            let err_text = err.to_string();
            let msg = err.into_server_msg();
            send(sink, &msg).await?;
            if should_close {
                Err(err_text)
            } else {
                Ok(())
            }
        }
    }
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
