//! F4 — moderation enforcement.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `kicked_guest_cannot_reconnect_until_room_unblocks`

mod common;

use std::time::Duration;

use common::{guest_hello, host_hello, TestApp};

#[tokio::test]
async fn kicked_guest_cannot_reconnect_until_room_unblocks() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;

    // Host + guest both Hello-handshake-complete.
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h-1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    let mut guest = app.connect_ws(&room.room_id).await;
    guest.send_json(&guest_hello("g-kickable", "Alice")).await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    // Host kicks the guest. The first connection should see a
    // KickNotice and the next reconnect attempt must be rejected.
    host.send_json(&serde_json::json!({
        "type": "KickGuest",
        "v": 1,
        "id": "kg-1",
        "guestId": "g-kickable",
    }))
    .await;
    let notice = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "KickNotice")
        .await;
    assert_eq!(notice["guestId"], "g-kickable");

    // Give the server a beat to finish closing the socket.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Reconnect attempt as the same guest_id: must be rejected with
    // KickNotice + an Error frame, then closed.
    let mut reconnect = app.connect_ws(&room.room_id).await;
    reconnect
        .send_json(&guest_hello("g-kickable", "Alice"))
        .await;
    let kicked_again = reconnect
        .await_msg(Duration::from_secs(2), |v| v["type"] == "KickNotice")
        .await;
    assert_eq!(kicked_again["guestId"], "g-kickable");
    let err = reconnect
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    let code = err["code"].as_str().unwrap_or("");
    assert!(
        matches!(code, "unauthorized" | "forbidden"),
        "expected unauthorized for kicked guest reconnect; got code={code}"
    );

    // A different guest_id, however, can still join — kick is per-guest.
    let mut stranger = app.connect_ws(&room.room_id).await;
    stranger.send_json(&guest_hello("g-newcomer", "Bob")).await;
    let _ = stranger
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
}
