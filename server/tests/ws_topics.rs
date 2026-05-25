//! F4 — topic-tree intents over ws.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `set_active_topic_marks_previous_active_as_done`

mod common;

use std::time::Duration;

use common::{await_until, host_hello, read_topics_for_test, TestApp};

#[tokio::test]
async fn set_active_topic_marks_previous_active_as_done() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Active")).await;

    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    // Two topics. T1 is added first, then activated; T2 added second.
    host.send_json(&serde_json::json!({
        "type": "AddTopic",
        "v": 1,
        "id": "at-1",
        "title": "First topic",
    }))
    .await;
    let added1 = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "TopicTreeUpdated" || v["type"] == "Ack" && v["refId"] == "at-1"
        })
        .await;
    // Re-await the Ack to drain it if the previous frame was the broadcast.
    if added1["type"] != "Ack" {
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "at-1"
            })
            .await;
    }
    host.send_json(&serde_json::json!({
        "type": "AddTopic",
        "v": 1,
        "id": "at-2",
        "title": "Second topic",
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "at-2"
        })
        .await;

    // Capture the two topic ids via the DB.
    let db_for_poll = app.db.clone();
    let room_id_for_poll = room.room_id.clone();
    await_until("topics to commit", Duration::from_secs(2), || {
        read_topics_for_test(&db_for_poll, &room_id_for_poll).len() == 2
    })
    .await;
    let topics = read_topics_for_test(&app.db, &room.room_id);
    let t1_id = topics[0].0.clone();
    let t2_id = topics[1].0.clone();
    assert_ne!(t1_id, t2_id);

    // Activate T1, then T2 — T1 should flip to status='done' in DB.
    host.send_json(&serde_json::json!({
        "type": "SetActiveTopic",
        "v": 1,
        "id": "sat-1",
        "topicId": t1_id,
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "sat-1"
        })
        .await;

    host.send_json(&serde_json::json!({
        "type": "SetActiveTopic",
        "v": 1,
        "id": "sat-2",
        "topicId": t2_id,
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "sat-2"
        })
        .await;

    // Wait for the writer to land both SetTopicStatus(t1, Done) and the
    // active-topic update.
    let db_for_poll = app.db.clone();
    let room_id_for_poll = room.room_id.clone();
    let t1_id_for_poll = t1_id.clone();
    await_until(
        "previous active topic to be marked done",
        Duration::from_secs(2),
        || {
            let rows = read_topics_for_test(&db_for_poll, &room_id_for_poll);
            rows.iter()
                .find(|(id, _, _, _, _)| id == &t1_id_for_poll)
                .is_some_and(|(_, _, _, _, status)| status == "done")
        },
    )
    .await;

    let rows = read_topics_for_test(&app.db, &room.room_id);
    let t1 = rows.iter().find(|(id, _, _, _, _)| id == &t1_id).unwrap();
    let t2 = rows.iter().find(|(id, _, _, _, _)| id == &t2_id).unwrap();
    assert_eq!(t1.4, "done", "previously active topic flipped to done");
    assert_eq!(t2.4, "pending", "newly active topic stays pending");
}
