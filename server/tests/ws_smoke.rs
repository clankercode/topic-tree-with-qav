//! F0 — single end-to-end ws smoke test against a real bound listener.
//! See `.plan/2026-05-25-followup/phases.md` §F0.

mod common;

use std::time::Duration;

use common::{guest_hello, TestApp};

#[tokio::test]
async fn client_receives_welcome_after_hello() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Smoke")).await;
    assert_eq!(room.title, "Smoke");

    let mut ws = app.connect_ws(&room.room_id).await;
    ws.send_json(&guest_hello("smoke-guest-1", "Smoker")).await;

    let welcome = ws
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    assert_eq!(welcome["v"], 1, "welcome v != 1");
    assert_eq!(welcome["you"]["role"], "guest", "role != guest");
    assert_eq!(welcome["you"]["guestId"], "smoke-guest-1");
    assert_eq!(welcome["snapshot"]["room"]["id"], room.room_id);
    assert_eq!(welcome["snapshot"]["room"]["title"], "Smoke");
}
