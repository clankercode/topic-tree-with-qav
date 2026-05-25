//! F2 acceptance: persisted state survives a process restart.
//!
//! Pattern: spawn TestApp_1 against a fresh in-memory Db, submit some
//! intents, await persistence, drop TestApp_1, spawn TestApp_2 over
//! the SAME Db clone, reconnect, assert the snapshot is intact.
//!
//! The Db clone keeps the underlying r2d2 pool's single in-memory
//! SQLite connection alive across the drop, simulating a restart.

mod common;

use std::time::Duration;

use common::{await_until, host_hello, read_topics_for_test, TestApp};

#[tokio::test]
async fn submit_topic_survives_restart_and_appears_in_welcome() {
    let db = server::Db::open_in_memory().expect("open db");

    // ── First boot: submit a topic, await persistence, drop. ──
    let room_id;
    let admin_token;
    {
        let app = TestApp::spawn_with_db(db.clone()).await;
        let room = app.create_room(Some("Rehydrate")).await;
        room_id = room.room_id.clone();
        admin_token = room.admin_token.clone();

        let mut host = app.connect_ws(&room.room_id).await;
        host.send_json(&host_hello("h1", "Host", &room.admin_token))
            .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
            .await;

        host.send_json(&serde_json::json!({
            "type": "AddTopic",
            "v": 1,
            "id": "a-rehydrate",
            "title": "survives a restart",
        }))
        .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "a-rehydrate"
            })
            .await;

        // Wait for the row to actually land in SQLite.
        let db_for_poll = db.clone();
        let rid = room_id.clone();
        await_until("topic to commit", Duration::from_secs(2), || {
            !read_topics_for_test(&db_for_poll, &rid).is_empty()
        })
        .await;
    } // TestApp_1 dropped: server + state + AppState clones gone.

    // ── Second boot: rehydrate from the same Db. ──
    let app2 = TestApp::spawn_with_db(db.clone()).await;
    let mut host2 = app2.connect_ws(&room_id).await;
    host2
        .send_json(&host_hello("h1", "Host", &admin_token))
        .await;
    let welcome = host2
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    let topics = welcome["snapshot"]["topics"].as_array().unwrap();
    assert_eq!(topics.len(), 1, "rehydrated room should expose the topic");
    assert_eq!(topics[0]["title"], "survives a restart");
}

#[tokio::test]
async fn submit_question_survives_restart() {
    let db = server::Db::open_in_memory().expect("open db");

    let room_id;
    let admin_token;
    {
        let app = TestApp::spawn_with_db(db.clone()).await;
        let room = app.create_room(None).await;
        room_id = room.room_id.clone();
        admin_token = room.admin_token.clone();

        // Connect as a guest to submit the question.
        let mut guest = app.connect_ws(&room.room_id).await;
        guest
            .send_json(&common::guest_hello("g-survive", "Alice"))
            .await;
        let _ = guest
            .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
            .await;
        guest
            .send_json(&serde_json::json!({
                "type": "SubmitQuestion",
                "v": 1,
                "id": "q-1",
                "text": "Does this persist?",
                "anonymous": false,
            }))
            .await;
        let _ = guest
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "q-1"
            })
            .await;

        let db_for_poll = db.clone();
        let rid = room_id.clone();
        await_until("question to commit", Duration::from_secs(2), || {
            !common::read_questions_for_test(&db_for_poll, &rid).is_empty()
        })
        .await;
    }

    let app2 = TestApp::spawn_with_db(db.clone()).await;
    let mut host = app2.connect_ws(&room_id).await;
    host.send_json(&host_hello("h", "H", &admin_token)).await;
    let welcome = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
    let qs = welcome["snapshot"]["questions"].as_array().unwrap();
    assert_eq!(qs.len(), 1, "rehydrated room should expose the question");
    assert_eq!(qs[0]["text"], "Does this persist?");
}
