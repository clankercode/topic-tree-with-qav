//! Topic voting over ws — mirrors ws_questions vote patterns.

mod common;

use std::time::Duration;

use common::{await_until, guest_hello, host_hello, read_topic_votes_for_test, TestApp};

#[tokio::test]
async fn vote_topic_dedups_by_guest_id() {
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
        "id": "at-1",
        "title": "Vote me",
    }))
    .await;
    let tree = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let tid = tree["topics"][0]["id"]
        .as_str()
        .expect("topic id")
        .to_string();

    let mut voter = app.connect_ws(&room.room_id).await;
    voter.send_json(&guest_hello("g-voter", "Voter")).await;
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    voter
        .send_json(&serde_json::json!({
            "type": "VoteTopic",
            "v": 1,
            "id": "vt-1",
            "topicId": tid,
            "vote": true,
        }))
        .await;
    voter
        .send_json(&serde_json::json!({
            "type": "VoteTopic",
            "v": 1,
            "id": "vt-2",
            "topicId": tid,
            "vote": true,
        }))
        .await;
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "vt-2"
        })
        .await;

    let db_for_poll = app.db.clone();
    let tid_for_poll = tid.clone();
    await_until("topic vote to commit", Duration::from_secs(2), || {
        !read_topic_votes_for_test(&db_for_poll, &tid_for_poll).is_empty()
    })
    .await;

    let rows = read_topic_votes_for_test(&app.db, &tid);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "g-voter");
}

#[tokio::test]
async fn vote_topic_broadcasts_topic_vote_updated() {
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
        "title": "Broadcast vote",
    }))
    .await;
    let tree = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let tid = tree["topics"][0]["id"].as_str().unwrap().to_string();

    let mut guest = app.connect_ws(&room.room_id).await;
    guest.send_json(&guest_hello("g1", "Guest")).await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    guest
        .send_json(&serde_json::json!({
            "type": "VoteTopic",
            "v": 1,
            "topicId": tid,
            "vote": true,
        }))
        .await;

    let frame = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicVoteUpdated")
        .await;
    assert_eq!(frame["topicId"], tid);
    assert_eq!(frame["voteCount"], 1);
    assert_eq!(frame["voterGuestId"], "g1");

    let host_frame = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicVoteUpdated")
        .await;
    assert_eq!(host_frame["voteCount"], 1);
}

#[tokio::test]
async fn host_cannot_vote_on_topic() {
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
        "title": "No host votes",
    }))
    .await;
    let tree = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let tid = tree["topics"][0]["id"].as_str().unwrap();

    host.send_json(&serde_json::json!({
        "type": "VoteTopic",
        "v": 1,
        "id": "vt-host",
        "topicId": tid,
        "vote": true,
    }))
    .await;
    let err = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Error")
        .await;
    assert_eq!(err["code"], "forbidden");
}

#[tokio::test]
async fn welcome_includes_my_topic_votes() {
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
        "title": "Persist vote",
    }))
    .await;
    let tree = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicTreeUpdated")
        .await;
    let tid = tree["topics"][0]["id"].as_str().unwrap().to_string();

    let mut voter = app.connect_ws(&room.room_id).await;
    voter.send_json(&guest_hello("g-persist", "Voter")).await;
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    voter
        .send_json(&serde_json::json!({
            "type": "VoteTopic",
            "v": 1,
            "topicId": tid,
            "vote": true,
        }))
        .await;
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| v["type"] == "TopicVoteUpdated")
        .await;

    let db_for_poll = app.db.clone();
    let tid_for_poll = tid.clone();
    await_until("vote persisted", Duration::from_secs(2), || {
        !read_topic_votes_for_test(&db_for_poll, &tid_for_poll).is_empty()
    })
    .await;

    let mut voter2 = app.connect_ws(&room.room_id).await;
    voter2.send_json(&guest_hello("g-persist", "Voter")).await;
    let welcome = voter2
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
    let my = welcome["snapshot"]["myTopicVotes"]
        .as_array()
        .expect("myTopicVotes array");
    assert!(
        my.iter().any(|v| v.as_str() == Some(tid.as_str())),
        "reconnect should restore myTopicVotes; got {my:?}"
    );
}
