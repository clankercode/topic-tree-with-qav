//! F4 — Q&A intents over ws.
//!
//! Named tests from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `submit_question_broadcasts_to_all_clients_in_room`
//!   - `vote_question_dedups_by_guest_id`

mod common;

use std::time::Duration;

use common::{
    await_until, guest_hello, host_hello, read_question_votes_for_test, TestApp,
};

#[tokio::test]
async fn submit_question_broadcasts_to_all_clients_in_room() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Broadcast")).await;

    // Two distinct ws connections joined to the same room.
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("host-1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    let mut guest = app.connect_ws(&room.room_id).await;
    guest
        .send_json(&guest_hello("g-broadcast", "Alice"))
        .await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    // Drain any presence frames already queued for the host.
    guest
        .send_json(&serde_json::json!({
            "type": "SubmitQuestion",
            "v": 1,
            "id": "q-bcast-1",
            "text": "Will every client see this?",
            "anonymous": false,
        }))
        .await;

    // Both sockets must observe a matching `QuestionAdded` frame whose
    // question.text equals the submitted text.
    let host_frame = host
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "QuestionAdded"
                && v["question"]["text"] == "Will every client see this?"
        })
        .await;
    let guest_frame = guest
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "QuestionAdded"
                && v["question"]["text"] == "Will every client see this?"
        })
        .await;

    let qid_host = host_frame["question"]["id"]
        .as_str()
        .expect("question id");
    let qid_guest = guest_frame["question"]["id"]
        .as_str()
        .expect("question id");
    assert_eq!(
        qid_host, qid_guest,
        "all clients observe the same question id"
    );
    assert_eq!(host_frame["question"]["voteCount"], 0);
    assert_eq!(host_frame["question"]["authorGuestId"], "g-broadcast");
}

#[tokio::test]
async fn vote_question_dedups_by_guest_id() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;

    let mut author = app.connect_ws(&room.room_id).await;
    author.send_json(&guest_hello("g-author", "Author")).await;
    let _ = author
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    author
        .send_json(&serde_json::json!({
            "type": "SubmitQuestion",
            "v": 1,
            "id": "q-vote-1",
            "text": "Dedup my vote",
            "anonymous": false,
        }))
        .await;
    let added = author
        .await_msg(Duration::from_secs(2), |v| v["type"] == "QuestionAdded")
        .await;
    let qid = added["question"]["id"]
        .as_str()
        .expect("question id")
        .to_string();

    let mut voter = app.connect_ws(&room.room_id).await;
    voter.send_json(&guest_hello("g-voter", "Voter")).await;
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    // Vote twice in quick succession — the second is a duplicate.
    voter
        .send_json(&serde_json::json!({
            "type": "VoteQuestion",
            "v": 1,
            "id": "vq-1",
            "questionId": qid,
            "vote": true,
        }))
        .await;
    voter
        .send_json(&serde_json::json!({
            "type": "VoteQuestion",
            "v": 1,
            "id": "vq-2",
            "questionId": qid,
            "vote": true,
        }))
        .await;

    // Drain ws activity to give the server a moment to process both.
    let _ = voter
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "vq-2"
        })
        .await;

    // Wait for the writer to flush; question_votes is keyed
    // (question_id, guest_id) PRIMARY KEY, so dup INSERTs must collapse.
    let db_for_poll = app.db.clone();
    let qid_for_poll = qid.clone();
    await_until("vote to commit", Duration::from_secs(2), || {
        !read_question_votes_for_test(&db_for_poll, &qid_for_poll).is_empty()
    })
    .await;

    let rows = read_question_votes_for_test(&app.db, &qid);
    assert_eq!(rows.len(), 1, "duplicate votes from one guest collapse");
    assert_eq!(rows[0].1, "g-voter");

    // Independent voter adds a second row.
    let mut voter2 = app.connect_ws(&room.room_id).await;
    voter2.send_json(&guest_hello("g-voter-2", "Voter2")).await;
    let _ = voter2
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
    voter2
        .send_json(&serde_json::json!({
            "type": "VoteQuestion",
            "v": 1,
            "id": "vq-3",
            "questionId": qid,
            "vote": true,
        }))
        .await;
    let _ = voter2
        .await_msg(Duration::from_secs(2), |v| {
            v["type"] == "Ack" && v["refId"] == "vq-3"
        })
        .await;

    let db_for_poll = app.db.clone();
    let qid_for_poll = qid.clone();
    await_until("second vote to commit", Duration::from_secs(2), || {
        read_question_votes_for_test(&db_for_poll, &qid_for_poll).len() == 2
    })
    .await;
}
