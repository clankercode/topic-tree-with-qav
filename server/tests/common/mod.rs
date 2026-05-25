//! Integration test harness shared across `tests/ws_*.rs`.
//!
//! See `.plan/2026-05-25-followup/testing.md` §1 for the contract.
//!
//! `TestApp::spawn()` boots an Axum server on `127.0.0.1:0` with an
//! in-memory database. HTTP requests are dispatched in-process via
//! `tower::ServiceExt::oneshot` (no socket round-trip needed) while
//! WebSocket connections use the real bound listener — both share one
//! `AppState`, so state mutations made via either path are visible to
//! the other.
//!
//! `spawn_with_db` accepts an existing `Db`. F2 reuses it to simulate
//! a server restart by dropping a `TestApp` and spawning another over
//! the same `Db` clone (`r2d2` keeps the in-memory connection alive
//! while any clone is held).

#![allow(dead_code)] // Tests can pick which helpers they use.

use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Instant};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomResp {
    pub room_id: String,
    pub admin_token: String,
    pub admin_url: String,
    pub join_url: String,
    pub title: String,
    pub created_at: i64,
}

pub struct TestApp {
    pub addr: SocketAddr,
    pub db: server::Db,
    pub state: server::AppState,
    /// Cloned router used for in-process HTTP dispatch via `oneshot`.
    pub http_router: Router,
    pub server_handle: JoinHandle<()>,
}

impl TestApp {
    /// Boot with a fresh in-memory database.
    pub async fn spawn() -> Self {
        let db = server::Db::open_in_memory().expect("open in-memory db");
        Self::spawn_with_db(db).await
    }

    /// Boot reusing a caller-owned `Db`. Useful for restart tests: drop
    /// the first `TestApp`, then call `spawn_with_db(db.clone())` to get
    /// a second server over the same data.
    pub async fn spawn_with_db(db: server::Db) -> Self {
        let metrics = server::create_metrics();
        let state = server::AppState::new(db.clone(), metrics);

        let http_router = server::app_with_state(state.clone());
        let ws_router = server::app_with_state(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind 0");
        let addr = listener.local_addr().expect("local_addr");
        let server_handle = tokio::spawn(async move {
            // We swallow the result: when the test drops TestApp, the
            // join handle is aborted; the resulting error is expected.
            let _ = axum::serve(listener, ws_router).await;
        });

        Self {
            addr,
            db,
            state,
            http_router,
            server_handle,
        }
    }

    pub fn ws_url(&self, room_id: &str) -> String {
        format!("ws://{}/ws?room={}", self.addr, room_id)
    }

    /// POST /api/rooms via the in-process router. Returns the decoded
    /// response struct. Panics on non-201.
    pub async fn create_room(&self, title: Option<&str>) -> CreateRoomResp {
        let body = match title {
            Some(t) => serde_json::json!({ "title": t }).to_string(),
            None => "{}".to_string(),
        };
        let req = Request::builder()
            .method("POST")
            .uri("/api/rooms")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = self
            .http_router
            .clone()
            .oneshot(req)
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED, "create_room non-201");
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("decode CreateRoomResp")
    }

    /// Open a raw WebSocket connection to `/ws?room=<room_id>`. The caller
    /// is responsible for sending `Hello` and awaiting `Welcome`.
    pub async fn connect_ws(&self, room_id: &str) -> WsClient {
        let url = self.ws_url(room_id);
        let (stream, _resp) = connect_async(url).await.expect("ws connect");
        WsClient { stream }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.server_handle.abort();
    }
}

pub struct WsClient {
    pub stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WsClient {
    pub async fn send_json(&mut self, msg: &serde_json::Value) {
        let text = serde_json::to_string(msg).expect("serialize ws frame");
        self.stream
            .send(Message::Text(text))
            .await
            .expect("ws send");
    }

    /// Receive the next text frame as JSON. Non-text frames (ping/pong/
    /// binary) are ignored and the next text frame is awaited. Panics on
    /// close or timeout.
    pub async fn recv_json(&mut self) -> serde_json::Value {
        self.recv_json_within(Duration::from_secs(2)).await
    }

    pub async fn recv_json_within(&mut self, max_wait: Duration) -> serde_json::Value {
        let deadline = Instant::now() + max_wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!("recv_json timed out after {:?}", max_wait);
            }
            let frame = match timeout(remaining, self.stream.next()).await {
                Ok(opt) => opt,
                Err(_) => panic!("recv_json timed out after {:?}", max_wait),
            };
            match frame {
                Some(Ok(Message::Text(t))) => {
                    return serde_json::from_str(&t).expect("decode ws frame as json")
                }
                Some(Ok(Message::Binary(_)))
                | Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Frame(_))) => continue,
                Some(Ok(Message::Close(c))) => panic!("ws closed: {:?}", c),
                Some(Err(e)) => panic!("ws error: {}", e),
                None => panic!("ws stream ended unexpectedly"),
            }
        }
    }

    /// Receive frames until `matcher` returns `true` or `max_wait`
    /// elapses. Useful when intermediate frames (e.g. `PresenceUpdate`)
    /// are expected before the frame under test.
    pub async fn await_msg<F>(&mut self, max_wait: Duration, mut matcher: F) -> serde_json::Value
    where
        F: FnMut(&serde_json::Value) -> bool,
    {
        let deadline = Instant::now() + max_wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!("await_msg timed out after {:?}", max_wait);
            }
            let v = self.recv_json_within(remaining).await;
            if matcher(&v) {
                return v;
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.stream.send(Message::Close(None)).await;
    }
}

/// Convenience: build a Hello frame for a guest with the given
/// `guest_id` and `display_name`. The room's host uses a separate
/// `host_hello` helper because the admin token is required.
pub fn guest_hello(guest_id: &str, display_name: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "Hello",
        "v": 1,
        "role": "guest",
        "guestId": guest_id,
        "displayName": display_name,
    })
}

pub fn host_hello(guest_id: &str, display_name: &str, admin_token: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "Hello",
        "v": 1,
        "role": "host",
        "guestId": guest_id,
        "displayName": display_name,
        "adminToken": admin_token,
    })
}
