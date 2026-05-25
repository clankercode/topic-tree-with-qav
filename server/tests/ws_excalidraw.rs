//! F4 — Excalidraw admin-only enforcement.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `excalidraw_update_from_guest_is_rejected_when_view_mode`
//!
//! Background: Excalidraw boards are display-only for guests on the
//! frontend (the React component runs in viewModeEnabled). The
//! frontend gate is defense-in-depth — the server's gate is the
//! authoritative one and rejects any ExcalidrawUpdate from a non-host
//! socket. See `protocol.md` §rate-limits and CLAUDE.md §9.

mod common;

use std::time::Duration;

use common::{guest_hello, host_hello, TestApp};

#[tokio::test]
async fn excalidraw_update_from_guest_is_rejected_when_view_mode() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;

    // Host first creates an excalidraw board so we have a target.
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h-1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
    host.send_json(&serde_json::json!({
        "type": "CreateBoard",
        "v": 1,
        "id": "cb-exc-1",
        "kind": "excalidraw",
        "title": "Sketch",
    }))
    .await;
    let created = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "BoardCreated")
        .await;
    let board_id = created["board"]["id"].as_str().unwrap().to_string();

    // Guest attempts the same update — must be rejected with an Error
    // frame and no broadcast.
    let mut guest = app.connect_ws(&room.room_id).await;
    guest.send_json(&guest_hello("g-attacker", "Guest")).await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    guest
        .send_json(&serde_json::json!({
            "type": "ExcalidrawUpdate",
            "v": 1,
            "id": "eu-deny",
            "boardId": board_id,
            "sceneVersion": 7,
            "elements": [],
            "appState": {},
        }))
        .await;

    let err = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["refId"], "eu-deny");
    let code = err["code"].as_str().unwrap_or("");
    assert!(
        matches!(code, "forbidden" | "unauthorized"),
        "expected forbidden/unauthorized, got code={code}; err={err}"
    );

    // The host must not observe an ExcalidrawDelta from the guest's
    // attempt. We give the broadcast pipeline a window and assert
    // nothing matching comes through.
    let timed = tokio::time::timeout(Duration::from_millis(250), host.recv_json()).await;
    if let Ok(v) = timed {
        assert_ne!(
            v["type"], "ExcalidrawDelta",
            "guest update must not reach host as a delta; got {v}"
        );
    }
}
