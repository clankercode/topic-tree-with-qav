//! F4 — topic-tree intents over ws.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `set_active_topic_marks_previous_active_as_done`

mod common;

use std::time::Duration;

use common::{await_until, host_hello, read_topics_for_test, TestApp};

const ROOM_TOPIC_LIMIT: usize = 5000;

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

/// Task #13: bulk import of a nested topic tree is atomic + assigns
/// parent_id correctly. Root + 2 children + 1 grandchild should
/// produce 4 topic rows where child.parent_id == root.id.
#[tokio::test]
async fn import_topic_tree_creates_nested_structure_in_one_shot() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "ImportTopicTree",
        "v": 1,
        "id": "imp-1",
        "topics": [
            {
                "title": "Plenary",
                "status": "pending",
                "children": [
                    {
                        "title": "Intro",
                        "status": "done",
                        "children": [
                            { "title": "Goals", "status": "pending", "children": [] }
                        ]
                    },
                    { "title": "Deep dive", "status": "pending", "children": [] }
                ]
            }
        ],
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "imp-1"
        })
        .await;

    // Wait for the writer to land all 4 rows.
    let db_for_poll = app.db.clone();
    let room_id_for_poll = room.room_id.clone();
    await_until(
        "4 imported topics to commit",
        Duration::from_secs(2),
        || read_topics_for_test(&db_for_poll, &room_id_for_poll).len() == 4,
    )
    .await;
    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert_eq!(rows.len(), 4);
    let plenary = rows.iter().find(|(_, _, t, _, _)| t == "Plenary").unwrap();
    let intro = rows.iter().find(|(_, _, t, _, _)| t == "Intro").unwrap();
    let goals = rows.iter().find(|(_, _, t, _, _)| t == "Goals").unwrap();
    assert_eq!(plenary.1, None, "root topic has no parent");
    assert_eq!(
        intro.1.as_ref(),
        Some(&plenary.0),
        "Intro's parent must be Plenary's id"
    );
    assert_eq!(
        goals.1.as_ref(),
        Some(&intro.0),
        "Goals' parent must be Intro's id"
    );
    assert_eq!(intro.4, "done", "imported status preserved");
}

/// Task #13: invalid imports are rejected before mutation. An empty
/// `topics` payload should error out with no rows written.
#[tokio::test]
async fn import_topic_tree_rejects_empty_payload() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "ImportTopicTree",
        "v": 1,
        "id": "imp-empty",
        "topics": [],
    }))
    .await;
    let err = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["refId"], "imp-empty");
    assert_eq!(err["code"], "bad_request");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert!(
        rows.is_empty(),
        "no topics should have landed; got {rows:?}"
    );
}

#[tokio::test]
async fn import_topic_tree_rejects_when_room_topic_cap_would_be_exceeded() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    seed_room_topics(&app.db, &room.room_id, ROOM_TOPIC_LIMIT);

    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "ImportTopicTree",
        "v": 1,
        "id": "imp-over-room-cap",
        "topics": [{ "title": "One too many", "children": [] }],
    }))
    .await;

    let err = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["refId"], "imp-over-room-cap");
    assert_eq!(err["code"], "bad_request");
    assert!(
        err["message"].as_str().unwrap_or("").contains("5000"),
        "error message should call out the room cap; got {err}"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert_eq!(
        rows.len(),
        ROOM_TOPIC_LIMIT,
        "rejected import must not persist any additional topics"
    );
}

#[tokio::test]
async fn import_topic_tree_rate_limits_rapid_replays() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "ImportTopicTree",
        "v": 1,
        "id": "imp-rate-1",
        "topics": [{ "title": "First import", "children": [] }],
    }))
    .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "imp-rate-1"
        })
        .await;

    host.send_json(&serde_json::json!({
        "type": "ImportTopicTree",
        "v": 1,
        "id": "imp-rate-2",
        "topics": [{ "title": "Second import", "children": [] }],
    }))
    .await;
    let err = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["refId"], "imp-rate-2");
    assert_eq!(err["code"], "rate_limit");

    let db_for_poll = app.db.clone();
    let room_id_for_poll = room.room_id.clone();
    await_until(
        "first imported topic to commit",
        Duration::from_secs(2),
        || read_topics_for_test(&db_for_poll, &room_id_for_poll).len() == 1,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert_eq!(rows.len(), 1, "rate-limited import must not be persisted");
}

/// Regression: a host attempting `AddTopic` with a parent_id that
/// points at no known topic must be rejected with `bad_request`
/// *before* the in-memory mutation or writer enqueue. Without this,
/// the writer's FK enforcement aborted the entire batch, losing any
/// other writes piggybacking on it.
#[tokio::test]
async fn add_topic_with_unknown_parent_rejects_before_mutate() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "AddTopic",
        "v": 1,
        "id": "bad-parent",
        "title": "should be rejected",
        "parentId": "this-topic-does-not-exist",
    }))
    .await;
    let err = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["refId"], "bad-parent");
    assert_eq!(err["code"], "bad_request");
    assert!(
        err["message"].as_str().unwrap_or("").contains("parent_id"),
        "error message should call out parent_id; got {err}"
    );

    // Give the writer a beat — DB must still have zero topics.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let rows = read_topics_for_test(&app.db, &room.room_id);
    assert!(
        rows.is_empty(),
        "no topic should have been persisted; got {rows:?}"
    );
}

fn seed_room_topics(db: &server::Db, room_id: &str, count: usize) {
    let mut conn = db.get().expect("checkout");
    let tx = conn.transaction().expect("begin seed topics");
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO topics (id, room_id, parent_id, title, ord, status, created_at) \
                 VALUES (?1, ?2, NULL, ?3, ?4, 'pending', 0)",
            )
            .expect("prepare seed topics");
        for i in 0..count {
            stmt.execute(rusqlite::params![
                format!("seed-topic-{i}"),
                room_id,
                format!("Seed topic {i}"),
                i as f64,
            ])
            .expect("insert seed topic");
        }
    }
    tx.commit().expect("commit seed topics");
}
