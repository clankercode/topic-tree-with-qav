use super::*;
use crate::proto::Role;
use std::sync::Arc;

fn you(client: &str, guest: &str, role: Role) -> You {
    You {
        client_id: client.to_string(),
        role,
        guest_id: guest.to_string(),
    }
}

#[test]
fn reap_idle_drops_truly_idle_rooms() {
    let reg = RoomRegistry::default();
    let now = 11 * 60 * 1000; // 11 min in ms.
                              // Room A: no clients, last activity 0 (11 min idle) → reaped.
    let a = reg.get_or_create("a", "A", 0);
    a.touch(0);
    // Room B: no clients, last activity 6 min ago → kept.
    let b = reg.get_or_create("b", "B", 0);
    b.touch(now - 6 * 60 * 1000);
    // Room C: one connected client, regardless of activity → kept.
    let c = reg.get_or_create("c", "C", 0);
    c.touch(0);
    c.add_client("g".into(), "cli".into(), "n".into(), 0, false);

    let reaped = reg.reap_idle(now, 10 * 60 * 1000);
    let reaped_ids: Vec<String> = reaped.iter().map(|r| r.id.clone()).collect();
    assert_eq!(reaped_ids, vec!["a".to_string()]);
    assert!(reg.get("a").is_none());
    assert!(reg.get("b").is_some());
    assert!(reg.get("c").is_some());
}

#[test]
fn touch_updates_last_activity_monotonically() {
    let r = Room::new("R".into(), "T".into(), 0);
    let initial = r.last_activity_at();
    r.touch(initial + 1_000);
    assert_eq!(r.last_activity_at(), initial + 1_000);
}

#[test]
fn add_remove_client_drives_presence_correctly() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false));
    assert!(!r.add_client("g1".into(), "c2".into(), "Alice".into(), 100, false));
    assert_eq!(r.presence().len(), 1);
    assert_eq!(r.presence()[0].client_ids.len(), 2);

    assert!(!r.remove_client("g1", "c1"));
    assert_eq!(r.presence().len(), 1);
    assert!(r.remove_client("g1", "c2"));
    assert_eq!(r.presence().len(), 0);
}

#[test]
fn set_display_name_returns_changed_flag() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 0, false);
    assert!(!r.set_display_name("g1", "Alice".into()));
    assert!(r.set_display_name("g1", "Alicia".into()));
    assert_eq!(r.guests()[0].display_name, "Alicia");
}

#[test]
fn snapshot_is_empty_for_m1_aside_from_presence() {
    let r = Room::new("ROOMID000001".into(), "T".into(), 7);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 7, false);
    let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    assert_eq!(snap.room.id, "ROOMID000001");
    assert_eq!(snap.guests.len(), 1);
    assert!(snap.topics.is_empty());
    assert!(snap.questions.is_empty());
    assert!(snap.boards.is_empty());
    assert!(snap.hands.is_empty());
    assert!(snap.my_votes.is_empty());
    assert!(snap.active_topic_id.is_none());
    assert!(snap.focused_board_id.is_none());
}

#[test]
fn seq_is_monotonic() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert_eq!(r.next_seq(), 1);
    assert_eq!(r.next_seq(), 2);
    assert_eq!(r.current_seq(), 2);
}

#[test]
fn registry_get_or_create_is_idempotent() {
    let reg = RoomRegistry::default();
    let a = reg.get_or_create("R", "T", 0);
    let b = reg.get_or_create("R", "T", 0);
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(reg.len(), 1);
}

#[test]
fn topic_add_list_get() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic 1".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    let topics = r.topics();
    assert_eq!(topics.len(), 1);
    assert_eq!(topics[0].title, "Topic 1");
    assert!(r.active_topic_id().is_none());
}

#[test]
fn topic_rename() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Original".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    assert!(r.rename_topic("t1", "Renamed".into()));
    assert_eq!(r.topics()[0].title, "Renamed");
    assert!(!r.rename_topic("nonexistent", "X".into()));
}

#[test]
fn topic_move() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    assert!(r.move_topic("t1", Some("parent1".into()), 2.5));
    let t = r.topics().pop().unwrap();
    assert_eq!(t.parent_id, Some("parent1".into()));
    assert_eq!(t.ord, 2.5);
}

#[test]
fn topic_delete() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    assert_eq!(r.topics().len(), 1);
    r.delete_topic("t1");
    assert!(r.topics().is_empty());
}

#[test]
fn topic_set_active() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    assert!(r.active_topic_id().is_none());
    r.set_active_topic(Some("t1".into()));
    assert_eq!(r.active_topic_id(), Some("t1".into()));
    r.set_active_topic(None);
    assert!(r.active_topic_id().is_none());
}

#[test]
fn topic_mark_done() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    assert_eq!(r.topics()[0].status, TopicStatus::Pending);
    assert!(r.mark_topic_done("t1", true));
    assert_eq!(r.topics()[0].status, TopicStatus::Done);
    assert!(r.mark_topic_done("t1", false));
    assert_eq!(r.topics()[0].status, TopicStatus::Pending);
}

#[test]
fn at_most_one_active() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Topic 1".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    r.add_topic(Topic {
        id: "t2".into(),
        parent_id: None,
        title: "Topic 2".into(),
        ord: 2.0,
        status: TopicStatus::Pending,
        created_at: 101,
        vote_count: 0,
    });
    r.set_active_topic(Some("t1".into()));
    r.set_active_topic(Some("t2".into()));
    assert_eq!(r.active_topic_id(), Some("t2".into()));
}

#[test]
fn load_topics_replaces_existing() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_topic(Topic {
        id: "t1".into(),
        parent_id: None,
        title: "Old".into(),
        ord: 1.0,
        status: TopicStatus::Pending,
        created_at: 100,
        vote_count: 0,
    });
    r.load_topics(
        vec![Topic {
            id: "t2".into(),
            parent_id: None,
            title: "New".into(),
            ord: 1.0,
            status: TopicStatus::Done,
            created_at: 200,
            vote_count: 0,
        }],
        HashMap::new(),
        Some("t2".into()),
    );
    assert_eq!(r.topics().len(), 1);
    assert_eq!(r.topics()[0].title, "New");
    assert_eq!(r.active_topic_id(), Some("t2".into()));
}

fn make_question(id: &str, text: &str, vote_count: u32) -> Question {
    Question {
        id: id.into(),
        room_id: "R".into(),
        author_guest_id: "g1".into(),
        author_name: "Alice".into(),
        anonymous: false,
        text: text.into(),
        answered: false,
        created_at: 100,
        vote_count,
    }
}

#[test]
fn question_add_list_get() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    let qs = r.questions();
    assert_eq!(qs.len(), 1);
    assert_eq!(qs[0].text, "What is Rust?");
}

#[test]
fn question_update() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    let mut q = make_question("q1", "What is Rust?", 0);
    q.answered = true;
    assert!(r.update_question(q));
    assert!(r.questions()[0].answered);
    assert!(!r.update_question(make_question("nonexistent", "?", 0)));
}

#[test]
fn question_delete() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    assert_eq!(r.questions().len(), 1);
    r.delete_question("q1");
    assert!(r.questions().is_empty());
}

#[test]
fn question_vote_adds_count() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    let (count, changed) = r.vote_question("q1", "g1", true).unwrap();
    assert_eq!(count, 1);
    assert!(changed);
}

#[test]
fn question_vote_retracts_count() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    r.vote_question("q1", "g1", true).unwrap();
    let (count, changed) = r.vote_question("q1", "g1", false).unwrap();
    assert_eq!(count, 0);
    assert!(changed);
}

#[test]
fn question_vote_double_vote_no_change() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    r.vote_question("q1", "g1", true).unwrap();
    let (count, changed) = r.vote_question("q1", "g1", true).unwrap();
    assert_eq!(count, 1);
    assert!(!changed);
}

#[test]
fn question_vote_multiple_guests() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 0));
    r.vote_question("q1", "g1", true).unwrap();
    r.vote_question("q1", "g2", true).unwrap();
    r.vote_question("q1", "g3", true).unwrap();
    assert_eq!(r.questions()[0].vote_count, 3);
    r.vote_question("q1", "g2", false).unwrap();
    assert_eq!(r.questions()[0].vote_count, 2);
}

#[test]
fn my_votes_tracks_correctly() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "Q1", 0));
    r.add_question(make_question("q2", "Q2", 0));
    r.add_question(make_question("q3", "Q3", 0));
    r.vote_question("q1", "g1", true).unwrap();
    r.vote_question("q3", "g1", true).unwrap();
    let votes = r.my_votes("g1");
    assert!(votes.contains(&"q1".into()));
    assert!(!votes.contains(&"q2".into()));
    assert!(votes.contains(&"q3".into()));
}

#[test]
fn snapshot_includes_questions_and_my_votes() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "Q1", 2));
    r.vote_question("q1", "g1", true).unwrap();
    let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    assert_eq!(snap.questions.len(), 1);
    assert_eq!(snap.questions[0].vote_count, 1);
    assert!(snap.my_votes.contains(&"q1".into()));
}

#[test]
fn snapshot_questions_are_cloned() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "Q1", 0));
    let snap1 = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    r.add_question(make_question("q2", "Q2", 0));
    let snap2 = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    assert_eq!(snap1.questions.len(), 1);
    assert_eq!(snap2.questions.len(), 2);
}

#[test]
fn raise_hand_adds_to_queue() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "What is Rust?".into(), 1000);
    let hands = r.hands_list();
    assert_eq!(hands.len(), 1);
    assert_eq!(hands[0].guest_id, "g1");
    assert_eq!(hands[0].display_name, "Alice");
    assert_eq!(hands[0].topic, "What is Rust?");
    assert_eq!(hands[0].raised_at, 1000);
}

#[test]
fn raise_hand_replaces_existing() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "First topic".into(), 1000);
    r.raise_hand("g1", "Alice".into(), "Second topic".into(), 2000);
    let hands = r.hands_list();
    assert_eq!(hands.len(), 1);
    assert_eq!(hands[0].topic, "Second topic");
}

#[test]
fn lower_hand_removes_from_queue() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
    assert!(r.lower_hand("g1"));
    assert!(r.hands_list().is_empty());
}

#[test]
fn lower_hand_returns_false_when_not_in_queue() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(!r.lower_hand("nonexistent"));
}

#[test]
fn call_on_hand_removes_and_returns() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
    let hand = r.call_on_hand("g1");
    assert!(hand.is_some());
    assert_eq!(hand.unwrap().guest_id, "g1");
    assert!(r.hands_list().is_empty());
}

#[test]
fn dismiss_hand_removes_from_queue() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
    assert!(r.dismiss_hand("g1"));
    assert!(r.hands_list().is_empty());
}

#[test]
fn promote_question_to_topic_creates_topic_and_deletes_question() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_question(make_question("q1", "What is Rust?", 3));
    let result = r.promote_question_to_topic("q1", None, None);
    assert!(result.is_some());
    let (question, topic) = result.unwrap();
    assert_eq!(question.id, "q1");
    assert_eq!(topic.title, "What is Rust?");
    assert!(r.questions().is_empty());
    assert_eq!(r.topics().len(), 1);
    assert_eq!(r.topics()[0].title, "What is Rust?");
}

#[test]
fn promote_question_to_topic_truncates_to_80_chars() {
    let r = Room::new("R".into(), "T".into(), 0);
    let long_text = "A".repeat(100);
    r.add_question(make_question("q1", &long_text, 0));
    let result = r.promote_question_to_topic("q1", None, None);
    assert!(result.is_some());
    let (_, topic) = result.unwrap();
    assert_eq!(topic.title.len(), 80);
}

#[test]
fn promote_question_to_topic_nonexistent_returns_none() {
    let r = Room::new("R".into(), "T".into(), 0);
    let result = r.promote_question_to_topic("nonexistent", None, None);
    assert!(result.is_none());
}

#[test]
fn snapshot_includes_hands() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    r.raise_hand("g1", "Alice".into(), "Topic".into(), 1000);
    let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    assert_eq!(snap.hands.len(), 1);
    assert_eq!(snap.hands[0].guest_id, "g1");
}

#[test]
fn create_board_pen_initializes_pen_state() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen Board".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    assert!(r.board_exists("b1"));
    let state = r.get_pen_board_state("b1").unwrap();
    assert!(state.strokes.is_empty());
    assert!(state.texts.is_empty());
}

#[test]
fn create_board_excalidraw_initializes_scene() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "e1".into(),
        kind: BoardKind::Excalidraw,
        title: "Excalidraw Board".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    assert!(r.board_exists("e1"));
    let scene = r.get_excalidraw_scene("e1").unwrap();
    assert_eq!(scene.scene_version, 0);
}

#[test]
fn rename_board_updates_title() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Old Title".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let renamed = r.rename_board("b1", "New Title".into());
    assert!(renamed.is_some());
    assert_eq!(renamed.unwrap().title, "New Title");
}

#[test]
fn rename_board_nonexistent_returns_none() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(r.rename_board("nonexistent", "X".into()).is_none());
}

#[test]
fn delete_board_removes_board_and_clears_focus() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.set_focused_board("b1".into());
    assert!(r.delete_board("b1"));
    assert!(!r.board_exists("b1"));
    assert!(r.focused_board_id().is_none());
}

#[test]
fn delete_board_nonexistent_returns_false() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(!r.delete_board("nonexistent"));
}

#[test]
fn set_focused_board_tracks_current() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    assert!(r.focused_board_id().is_none());
    r.set_focused_board("b1".into());
    assert_eq!(r.focused_board_id(), Some("b1".into()));
}

#[test]
fn excalidraw_scene_update_increments_version() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "e1".into(),
        kind: BoardKind::Excalidraw,
        title: "Excal".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let elements: JsonValue = serde_json::json!([{"id": "el1"}]);
    let app_state: JsonValue = serde_json::json!({});
    r.update_excalidraw_scene("e1", 1, elements.clone(), app_state.clone(), 200);
    let scene = r.get_excalidraw_scene("e1").unwrap();
    assert_eq!(scene.scene_version, 1);
}

#[test]
fn excalidraw_scene_update_nonexistent_returns_board_missing() {
    let r = Room::new("R".into(), "T".into(), 0);
    let elements: JsonValue = serde_json::json!([]);
    let app_state: JsonValue = serde_json::json!({});
    assert_eq!(
        r.update_excalidraw_scene("nonexistent", 1, elements, app_state, 200),
        ExcalidrawUpdateOutcome::BoardMissing
    );
}

#[test]
fn excalidraw_scene_update_wrong_kind_returns_board_missing() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let elements: JsonValue = serde_json::json!([]);
    let app_state: JsonValue = serde_json::json!({});
    assert_eq!(
        r.update_excalidraw_scene("b1", 1, elements, app_state, 200),
        ExcalidrawUpdateOutcome::BoardMissing
    );
}

#[test]
fn excalidraw_scene_update_rejects_stale_version() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "e1".into(),
        kind: BoardKind::Excalidraw,
        title: "Excal".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let elements: JsonValue = serde_json::json!([{"id": "el1"}]);
    let app_state: JsonValue = serde_json::json!({});
    assert_eq!(
        r.update_excalidraw_scene("e1", 5, elements.clone(), app_state.clone(), 200),
        ExcalidrawUpdateOutcome::Applied
    );
    assert_eq!(
        r.update_excalidraw_scene("e1", 5, elements.clone(), app_state.clone(), 300),
        ExcalidrawUpdateOutcome::Stale
    );
    assert_eq!(
        r.update_excalidraw_scene("e1", 3, elements.clone(), app_state.clone(), 400),
        ExcalidrawUpdateOutcome::Stale
    );
    let scene = r.get_excalidraw_scene("e1").unwrap();
    assert_eq!(scene.scene_version, 5);
}

#[test]
fn get_excalidraw_scenes_needing_reset_returns_pending_scenes() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "e1".into(),
        kind: BoardKind::Excalidraw,
        title: "Excal".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let elements: JsonValue = serde_json::json!([]);
    let app_state: JsonValue = serde_json::json!({});
    r.update_excalidraw_scene("e1", 3, elements.clone(), app_state.clone(), 200);
    let pending = r.get_excalidraw_scenes_needing_reset();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].scene_version, 3);
}

#[test]
fn mark_excalidraw_scene_broadcast_updates_version_tracker() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "e1".into(),
        kind: BoardKind::Excalidraw,
        title: "Excal".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.mark_excalidraw_scene_broadcast("e1", 5);
    let pending = r.get_excalidraw_scenes_needing_reset();
    assert!(pending.is_empty());
}

#[test]
fn pen_begin_stroke_creates_stroke() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let stroke = r
        .pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000)
        .unwrap();
    assert_eq!(stroke.id, "s1");
    assert_eq!(stroke.color, "#000");
    assert!(stroke.points.is_empty());
}

#[test]
fn pen_begin_stroke_nonexistent_board_returns_none() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(r
        .pen_begin_stroke("nonexistent", "s1".into(), "#000".into(), 4.0, 1000)
        .is_none());
}

#[test]
fn pen_append_points_extends_stroke() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    r.pen_append_points("b1", "s1", vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]]);
    let state = r.get_pen_board_state("b1").unwrap();
    assert_eq!(state.strokes[0].points.len(), 2);
}

#[test]
fn pen_end_stroke_finalizes_ord() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    r.pen_end_stroke("b1", "s1");
    let state = r.get_pen_board_state("b1").unwrap();
    assert_eq!(state.strokes[0].ord, 1);
    assert_eq!(state.next_stroke_ord, 2);
}

#[test]
fn pen_text_upsert_adds_text() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let text = crate::proto::PenText {
        id: "t1".into(),
        x: 10.0,
        y: 20.0,
        text: "Hello".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    r.pen_text_upsert("b1", text, 1000);
    let state = r.get_pen_board_state("b1").unwrap();
    assert_eq!(state.texts.len(), 1);
    assert_eq!(state.texts[0].text, "Hello");
}

#[test]
fn pen_text_delete_removes_text() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let text = crate::proto::PenText {
        id: "t1".into(),
        x: 10.0,
        y: 20.0,
        text: "Hello".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    r.pen_text_upsert("b1", text, 1000);
    r.pen_text_delete("b1", "t1", 2000);
    let state = r.get_pen_board_state("b1").unwrap();
    assert!(state.texts.is_empty());
}

#[test]
fn pen_clear_removes_all_strokes_and_texts() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    let text = crate::proto::PenText {
        id: "t1".into(),
        x: 10.0,
        y: 20.0,
        text: "Hello".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    r.pen_text_upsert("b1", text, 1000);
    r.pen_clear("b1", 2000);
    let state = r.get_pen_board_state("b1").unwrap();
    assert!(state.strokes.is_empty());
    assert!(state.texts.is_empty());
}

#[test]
fn pen_undo_removes_last_action() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    let outcome = r.pen_undo("b1").expect("pen_undo returns Some");
    assert_eq!(outcome.removed_stroke, Some("s1".into()));
    assert_eq!(outcome.removed_text, None);
    assert!(!outcome.action_id.is_empty());
    let state = r.get_pen_board_state("b1").unwrap();
    assert!(state.strokes.is_empty());
}

#[test]
fn pen_end_stroke_returns_summary_and_matching_begin_action_id() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    r.pen_append_points("b1", "s1", vec![[0.0, 1.0, 2.0]]);
    let (summary, action_id) = r.pen_end_stroke("b1", "s1").expect("end returns Some");
    assert_eq!(summary.id, "s1");
    assert_eq!(summary.points.len(), 1);
    assert_eq!(summary.ord, 1);
    // The returned action_id must match the StrokeBegin action that
    // sits in the in-memory log for this stroke.
    let state = r.get_pen_board_state("b1").unwrap();
    let begin = state
        .action_log
        .iter()
        .find(|a| a.kind == PenActionKind::StrokeBegin && a.target_id.as_deref() == Some("s1"))
        .expect("StrokeBegin action present");
    assert_eq!(begin.id, action_id);
}

#[test]
fn pen_text_upsert_returns_prior_state_when_overwriting() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let first = crate::proto::PenText {
        id: "t1".into(),
        x: 1.0,
        y: 2.0,
        text: "first".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    let (_id1, prior_first) = r
        .pen_text_upsert("b1", first.clone(), 1000)
        .expect("insert");
    assert!(prior_first.is_none(), "first insert has no prior");
    let second = crate::proto::PenText {
        id: "t1".into(),
        x: 3.0,
        y: 4.0,
        text: "second".into(),
        font_size: 18.0,
        color: "#111".into(),
        updated_at: 2000,
    };
    let (_id2, prior_second) = r.pen_text_upsert("b1", second, 2000).expect("overwrite");
    let prior = prior_second.expect("overwrite returns prior");
    assert_eq!(prior, first, "prior must be exactly the first text");
}

#[test]
fn pen_text_delete_returns_removed_text() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    let text = crate::proto::PenText {
        id: "t1".into(),
        x: 1.0,
        y: 2.0,
        text: "doomed".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    r.pen_text_upsert("b1", text.clone(), 1000);
    let (_, removed) = r
        .pen_text_delete("b1", "t1", 2000)
        .expect("delete returns Some");
    assert_eq!(removed, text);
}

#[test]
fn pen_clear_returns_snapshot_of_prior_strokes_and_texts() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.pen_begin_stroke("b1", "s1".into(), "#000".into(), 4.0, 1000);
    let text = crate::proto::PenText {
        id: "t1".into(),
        x: 1.0,
        y: 2.0,
        text: "hi".into(),
        font_size: 16.0,
        color: "#000".into(),
        updated_at: 1000,
    };
    r.pen_text_upsert("b1", text, 1000);
    let (_, strokes, texts) = r.pen_clear("b1", 2000).expect("clear");
    assert_eq!(strokes.len(), 1, "snapshot must contain prior stroke");
    assert_eq!(texts.len(), 1, "snapshot must contain prior text");
}

#[test]
fn set_muted_updates_guest_muted_state() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    assert!(!r.is_muted("g1"));
    r.set_muted("g1", true);
    assert!(r.is_muted("g1"));
    r.set_muted("g1", false);
    assert!(!r.is_muted("g1"));
}

#[test]
fn set_muted_nonexistent_returns_false() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(!r.set_muted("nonexistent", true));
}

#[test]
fn is_muted_nonexistent_returns_false() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(!r.is_muted("nonexistent"));
}

#[test]
fn kick_guest_removes_presence() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    assert_eq!(r.presence().len(), 1);
    let removed = r.kick_guest("g1");
    assert!(!removed.is_empty());
    assert_eq!(r.presence().len(), 0);
}

#[test]
fn kick_guest_nonexistent_returns_empty() {
    let r = Room::new("R".into(), "T".into(), 0);
    assert!(r.kick_guest("nonexistent").is_empty());
}

#[test]
fn kick_guest_remembers_kicked_state_for_reconnect_race() {
    let r = Room::new("R".into(), "T".into(), 0);
    r.add_client("g1".into(), "c1".into(), "Alice".into(), 100, false);
    assert!(!r.is_kicked("g1"));
    r.kick_guest("g1");
    assert!(r.is_kicked("g1"));
    assert!(!r.is_kicked("g2"));
}

#[test]
fn snapshot_includes_boards() {
    let r = Room::new("R".into(), "T".into(), 0);
    let board = Board {
        id: "b1".into(),
        kind: BoardKind::Pen,
        title: "Pen".into(),
        created_at: 100,
        ord: 1.0,
    };
    r.create_board(board, 100);
    r.set_focused_board("b1".into());
    let snap = r.snapshot_for(you("c1", "g1", Role::Guest), "g1");
    assert_eq!(snap.boards.len(), 1);
    assert_eq!(snap.focused_board_id, Some("b1".into()));
}
