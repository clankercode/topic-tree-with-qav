//! F4 — room lifecycle integration tests.
//! See `.plan/2026-05-25-followup/testing.md` §2.

mod common;

use std::time::Duration;

use common::{guest_hello, host_hello, TestApp};

#[tokio::test]
async fn create_room_returns_admin_token_and_room_id() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Plenary")).await;

    assert_eq!(room.room_id.len(), 12, "room id must be 12 b32 chars");
    assert!(
        room.room_id
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7')),
        "room id must be base32 (A-Z, 2-7); got '{}'",
        room.room_id
    );
    assert!(room.admin_token.len() >= 16, "admin token too short");
    assert_eq!(
        room.admin_url,
        format!("/r/{}?admin={}", room.room_id, room.admin_token)
    );
    assert_eq!(room.join_url, format!("/r/{}", room.room_id));
    assert_eq!(room.title, "Plenary");
    assert!(
        room.created_at > 0,
        "created_at should be a positive epoch ms"
    );
}

#[tokio::test]
async fn hello_with_invalid_admin_token_returns_error() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;

    // Forge a token of the right shape but wrong content.
    let bogus = "A".repeat(room.admin_token.len());
    assert_ne!(bogus, room.admin_token);

    let mut ws = app.connect_ws(&room.room_id).await;
    ws.send_json(&host_hello("attacker", "Bad", &bogus)).await;

    let err = ws
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    let code = err["code"].as_str().unwrap_or("");
    // Existing server uses "unauthorized" for this case; accept any of
    // the auth-related codes so the test stays robust to renames.
    assert!(
        matches!(code, "unauthorized" | "auth_failed" | "forbidden"),
        "expected an auth error code, got code={code}; err={err}"
    );
}

#[tokio::test]
async fn hello_on_missing_room_returns_room_not_found() {
    let app = TestApp::spawn().await;
    let missing = "ABCDEFGH2JKL";

    let mut ws = app.connect_ws(missing).await;
    ws.send_json(&guest_hello("guest-1", "Alice")).await;

    let err = ws
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["code"].as_str(), Some("room_not_found"));
}

#[tokio::test]
async fn get_room_returns_404_for_missing_room() {
    let app = TestApp::spawn().await;
    let resp = app.get_room("ABCDEFGH2JKL").await;
    assert_eq!(resp.status, axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_room_returns_title_for_existing_room() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Plenary")).await;
    let resp = app.get_room(&room.room_id).await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);
    assert_eq!(resp.body["roomId"].as_str(), Some(room.room_id.as_str()));
    assert_eq!(resp.body["title"].as_str(), Some("Plenary"));
}
