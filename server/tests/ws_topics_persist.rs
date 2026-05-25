//! End-to-end persistence test for the topic intents wired in F1.3b.
//! Submits intents over ws against a real Axum app and asserts that
//! the writer task commits the matching rows to SQLite within 2 s.

mod common;

use std::time::Duration;

use common::{
    await_until, host_hello, read_topics_for_test, TestApp,
};

#[tokio::test]
async fn add_topic_over_ws_persists_to_db() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Persist Topics")).await;

    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello(
        "host-1",
        "Host",
        &room.admin_token,
    ))
    .await;
    // Wait for Welcome before sending AddTopic.
    let _welcome = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "AddTopic",
        "v": 1,
        "id": "client-add-1",
        "title": "First topic",
    }))
    .await;
    // Wait for the Ack so the in-memory side is settled.
    let _ack = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "client-add-1"
        })
        .await;

    // Poll the DB until the row lands or the writer batch commits.
    let room_id = room.room_id.clone();
    let db = app.db.clone();
    await_until("topic row to land", Duration::from_secs(2), || {
        let rows = read_topics_for_test(&db, &room_id);
        rows.iter().any(|t| t.2 == "First topic")
    })
    .await;

    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert_eq!(rows.len(), 1, "exactly one topic should persist");
    let (_id, parent, title, _ord, status) = &rows[0];
    assert_eq!(title, "First topic");
    assert!(parent.is_none());
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn rename_topic_over_ws_persists_to_db() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "H", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "AddTopic", "v": 1, "id": "a1", "title": "orig"
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Ack" && v["refId"] == "a1")
        .await;

    // Pick up the topic id from the broadcast snapshot.
    let updated = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let topic_id = updated["topics"][0]["id"].as_str().unwrap().to_string();

    host.send_json(&serde_json::json!({
        "type": "RenameTopic", "v": 1, "id": "r1", "topicId": topic_id, "title": "renamed"
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Ack" && v["refId"] == "r1")
        .await;

    let room_id = room.room_id.clone();
    let db = app.db.clone();
    await_until("rename to commit", Duration::from_secs(2), || {
        let rows = read_topics_for_test(&db, &room_id);
        rows.iter().any(|t| t.2 == "renamed")
    })
    .await;
}

#[tokio::test]
async fn delete_topic_over_ws_removes_from_db() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "H", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "AddTopic", "v": 1, "id": "a", "title": "to-delete"
    }))
    .await;
    let updated = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let topic_id = updated["topics"][0]["id"].as_str().unwrap().to_string();

    // Make sure the insert lands first.
    let db = app.db.clone();
    let room_id = room.room_id.clone();
    await_until("insert to land", Duration::from_secs(2), || {
        !read_topics_for_test(&db, &room_id).is_empty()
    })
    .await;

    host.send_json(&serde_json::json!({
        "type": "DeleteTopic", "v": 1, "id": "d", "topicId": topic_id
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Ack" && v["refId"] == "d")
        .await;

    await_until("delete to land", Duration::from_secs(2), || {
        read_topics_for_test(&db, &room_id).is_empty()
    })
    .await;
}
