//! F4 — pen board persistence + replay.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//! `pen_stroke_lifecycle_persists_and_replays_on_reconnect`.
//!
//! Flow:
//!   1. Host opens a fresh in-memory DB, creates a pen board, draws a
//!      stroke (Begin → Append → End), waits for the stroke row to
//!      land in `pen_strokes`.
//!   2. First `TestApp` is dropped. The DB clone keeps the
//!      single-connection pool alive across the gap.
//!   3. A second `TestApp` boots over the same DB and a second host
//!      connection issues `Hello`. The resulting `Welcome` snapshot
//!      must include the board, and the in-memory pen state must
//!      contain the full stroke with its three points + final ord.

mod common;

use std::time::Duration;

use common::{
    await_until, guest_hello, host_hello, read_pen_actions_for_test, read_pen_strokes_for_test,
    TestApp,
};

#[tokio::test]
async fn pen_stroke_lifecycle_persists_and_replays_on_reconnect() {
    let db = server::Db::open_in_memory().expect("open db");

    let room_id;
    let admin_token;
    let board_id;
    let stroke_id = "stroke-pen-life".to_string();

    {
        let app = TestApp::spawn_with_db(db.clone()).await;
        let room = app.create_room(Some("Pen Lifecycle")).await;
        room_id = room.room_id.clone();
        admin_token = room.admin_token.clone();

        let mut host = app.connect_ws(&room.room_id).await;
        host.send_json(&host_hello("h1", "Host", &room.admin_token))
            .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
            .await;

        // Create a pen board and capture the server-issued board id from
        // the BoardCreated broadcast.
        host.send_json(&serde_json::json!({
            "type": "CreateBoard",
            "v": 1,
            "id": "cb-1",
            "kind": "pen",
            "title": "Pad",
        }))
        .await;
        let created = host
            .await_msg(Duration::from_secs(2), |v| v["type"] == "BoardCreated")
            .await;
        board_id = created["board"]["id"]
            .as_str()
            .expect("board id in BoardCreated")
            .to_string();

        // Stroke lifecycle: Begin → Append (twice) → End.
        host.send_json(&serde_json::json!({
            "type": "PenStrokeBegin",
            "v": 1,
            "id": "psb-1",
            "boardId": board_id,
            "strokeId": stroke_id,
            "color": "#222",
            "size": 4.0,
        }))
        .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "psb-1"
            })
            .await;

        host.send_json(&serde_json::json!({
            "type": "PenStrokeAppend",
            "v": 1,
            "id": "psa-1",
            "boardId": board_id,
            "strokeId": stroke_id,
            "points": [[10.0, 20.0, 0.5], [11.0, 21.0, 0.6]],
        }))
        .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "psa-1"
            })
            .await;

        host.send_json(&serde_json::json!({
            "type": "PenStrokeAppend",
            "v": 1,
            "id": "psa-2",
            "boardId": board_id,
            "strokeId": stroke_id,
            "points": [[12.0, 22.0, 0.7]],
        }))
        .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "psa-2"
            })
            .await;

        host.send_json(&serde_json::json!({
            "type": "PenStrokeEnd",
            "v": 1,
            "id": "pse-1",
            "boardId": board_id,
            "strokeId": stroke_id,
        }))
        .await;
        let _ = host
            .await_msg(Duration::from_secs(2), |v| {
                v["type"] == "Ack" && v["refId"] == "pse-1"
            })
            .await;

        // Wait for the writer task to land the stroke + matching
        // pen_actions row.
        let db_for_poll = db.clone();
        let bid = board_id.clone();
        await_until("stroke to commit", Duration::from_secs(2), || {
            !read_pen_strokes_for_test(&db_for_poll, &bid).is_empty()
        })
        .await;

        let strokes = read_pen_strokes_for_test(&db, &board_id);
        assert_eq!(strokes.len(), 1, "exactly one stroke persisted");
        let (sid, color, size, points_json, ord) = &strokes[0];
        assert_eq!(sid, &stroke_id);
        assert_eq!(color, "#222");
        assert_eq!(*size, 4.0);
        assert_eq!(*ord, 1, "stroke ord finalized at PenStrokeEnd");
        let points: Vec<[f32; 3]> = serde_json::from_str(points_json).expect("decode points_json");
        assert_eq!(points.len(), 3, "all three appended points persisted");
        assert_eq!(points[0], [10.0, 20.0, 0.5]);
        assert_eq!(points[2], [12.0, 22.0, 0.7]);

        let actions = read_pen_actions_for_test(&db, &board_id);
        let begin = actions
            .iter()
            .find(|(_, kind, target, _)| {
                kind == "stroke_begin" && target.as_deref() == Some(stroke_id.as_str())
            })
            .expect("a stroke_begin action for this stroke");
        assert!(begin.3.is_none(), "stroke_begin carries no payload_json");
    }

    // ── Second boot: same DB. ──
    let app2 = TestApp::spawn_with_db(db.clone()).await;
    let mut host2 = app2.connect_ws(&room_id).await;
    host2
        .send_json(&host_hello("h1", "Host", &admin_token))
        .await;
    let welcome = host2
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    let boards = welcome["snapshot"]["boards"]
        .as_array()
        .expect("Welcome.snapshot.boards");
    let pen_board = boards
        .iter()
        .find(|b| b["id"].as_str() == Some(board_id.as_str()))
        .expect("pen board survives restart");
    assert_eq!(pen_board["kind"], "pen");

    // Pen state ships embedded as `boards[i].content.{strokes,texts}`.
    let strokes = pen_board["content"]["strokes"]
        .as_array()
        .expect("rehydrated pen board exposes content.strokes");
    assert_eq!(strokes.len(), 1, "exactly one rehydrated stroke");
    assert_eq!(strokes[0]["id"], stroke_id);
    let pts = strokes[0]["points"]
        .as_array()
        .expect("rehydrated stroke has points");
    assert_eq!(pts.len(), 3, "rehydrated stroke has all three points");
}

#[tokio::test]
async fn pen_stroke_multi_point_append_broadcasts_to_guest() {
    let app = TestApp::spawn().await;
    let room = app.create_room(Some("Pen Broadcast")).await;

    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    let mut guest = app.connect_ws(&room.room_id).await;
    guest.send_json(&guest_hello("g1", "Guest")).await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    host.send_json(&serde_json::json!({
        "type": "CreateBoard",
        "v": 1,
        "id": "cb-1",
        "kind": "pen",
        "title": "Pad",
    }))
    .await;
    let created = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "BoardCreated")
        .await;
    let board_id = created["board"]["id"]
        .as_str()
        .expect("board id")
        .to_string();
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "BoardCreated")
        .await;

    let stroke_id = "stroke-batch";
    host.send_json(&serde_json::json!({
        "type": "PenStrokeBegin",
        "v": 1,
        "id": "psb-1",
        "boardId": board_id,
        "strokeId": stroke_id,
        "color": "#222",
        "size": 4.0,
    }))
    .await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "PenStrokeBegun")
        .await;

    host.send_json(&serde_json::json!({
        "type": "PenStrokeAppend",
        "v": 1,
        "id": "psa-1",
        "boardId": board_id,
        "strokeId": stroke_id,
        "points": [[10.0, 20.0, 0.5], [11.0, 21.0, 0.6], [12.0, 22.0, 0.7]],
    }))
    .await;

    let appended = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "PenStrokeAppended")
        .await;
    let points = appended["points"].as_array().expect("points array");
    assert_eq!(points.len(), 3);
}
