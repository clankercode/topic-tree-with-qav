//! F4 — rate-limit drop behaviour for Cursor frames.
//!
//! Named test from `.plan/2026-05-25-followup/testing.md` §3:
//!   - `cursor_messages_exceeding_rate_limit_are_dropped`
//!
//! Approach: send a burst of Cursor frames from a guest, count how many
//! `CursorMoved` broadcasts reach a second listener within a bounded
//! window. The Cursor quota is `Quota::per_second(30.0)`, so a burst
//! of 100 frames issued back-to-back (well under one second of
//! wall-clock at integration-test speed) must yield strictly fewer
//! than 100 broadcasts.
//!
//! This shape avoids deterministic-time injection; a proper clock
//! handle on `RateLimiter` (see `.plan/2026-05-25-followup/risks.md`
//! R30) would let us assert "exactly 30", but the present test is
//! sufficient to fail loudly if the limiter is bypassed entirely.

mod common;

use std::time::Duration;

use common::{guest_hello, host_hello, TestApp};

#[tokio::test]
async fn cursor_messages_exceeding_rate_limit_are_dropped() {
    let app = TestApp::spawn().await;
    let room = app.create_room(None).await;

    // Host creates a pen board (any board works; cursor needs board to
    // exist server-side).
    let mut host = app.connect_ws(&room.room_id).await;
    host.send_json(&host_hello("h-1", "Host", &room.admin_token))
        .await;
    let _ = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;
    host.send_json(&serde_json::json!({
        "type": "CreateBoard",
        "v": 1,
        "id": "cb-rl",
        "kind": "pen",
        "title": "Pad",
    }))
    .await;
    let created = host
        .await_msg(Duration::from_secs(2), |v| v["type"] == "BoardCreated")
        .await;
    let board_id = created["board"]["id"].as_str().unwrap().to_string();

    // Guest connects and bursts 100 Cursor frames.
    let mut guest = app.connect_ws(&room.room_id).await;
    guest.send_json(&guest_hello("g-spam", "Burst")).await;
    let _ = guest
        .await_msg(Duration::from_secs(2), |v| v["type"] == "Welcome")
        .await;

    const BURST: usize = 100;
    for i in 0..BURST {
        guest
            .send_json(&serde_json::json!({
                "type": "Cursor",
                "v": 1,
                "boardId": board_id,
                "x": i as f64,
                "y": i as f64,
            }))
            .await;
    }

    // Tally CursorMoved frames on the host side until the stream goes
    // quiet for 200 ms. Cap at BURST so a hypothetical no-op limiter
    // doesn't hang the test.
    let mut received = 0usize;
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let timed_out =
            tokio::time::timeout(Duration::from_millis(200), host.recv_json()).await;
        match timed_out {
            Ok(v) if v["type"] == "CursorMoved" => {
                received += 1;
                if received >= BURST {
                    break;
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
        if tokio::time::Instant::now() > drain_deadline {
            break;
        }
    }

    assert!(
        received < BURST,
        "rate limiter must drop at least one Cursor when sender bursts {BURST} \
         frames back-to-back; got {received}",
    );
    assert!(
        received > 0,
        "limiter must still admit *some* frames; got 0 (possible regression \
         where the limiter rejects everything)",
    );
}
