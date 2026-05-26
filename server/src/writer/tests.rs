use super::*;

fn seed_room(db: &Db, room_id: &str) {
    let conn = db.get().unwrap();
    conn.execute(
        "INSERT INTO rooms (id, title, admin_token_hash, created_at, last_active_at) \
             VALUES (?1, 't', 'h', 0, 0)",
        [room_id],
    )
    .unwrap();
}

fn seed_pen_board(db: &Db, room_id: &str, board_id: &str) {
    seed_room(db, room_id);
    let conn = db.get().unwrap();
    conn.execute(
        "INSERT INTO boards (id, room_id, kind, title, ord, created_at) \
             VALUES (?1, ?2, 'pen', 't', 0.0, 0)",
        [board_id, room_id],
    )
    .unwrap();
}

fn stroke(id: &str, ord: u32) -> PenStrokeSummary {
    PenStrokeSummary {
        id: id.into(),
        color: "#000000".into(),
        size: 2.0,
        points: vec![[0.0, 0.0, 0.5], [1.0, 1.0, 0.5]],
        created_at: 100,
        ord,
    }
}

fn pen_text(id: &str, txt: &str) -> PenText {
    PenText {
        id: id.into(),
        x: 10.0,
        y: 20.0,
        text: txt.into(),
        font_size: 16.0,
        color: "#111111".into(),
        updated_at: 200,
    }
}

fn board(id: &str, kind: BoardKind, title: &str, ord: f64) -> Board {
    Board {
        id: id.into(),
        kind,
        title: title.into(),
        created_at: 0,
        ord,
    }
}

fn question(id: &str, room_id: &str, text: &str, author: &str) -> Question {
    Question {
        id: id.into(),
        room_id: room_id.into(),
        author_guest_id: author.into(),
        author_name: author.into(),
        anonymous: false,
        text: text.into(),
        answered: false,
        created_at: 0,
        vote_count: 0,
    }
}

fn topic(id: &str, ord: f64) -> Topic {
    Topic {
        id: id.into(),
        parent_id: None,
        title: format!("Topic {id}"),
        ord,
        status: TopicStatus::Pending,
        created_at: 0,
        vote_count: 0,
    }
}

#[tokio::test]
async fn writer_drains_and_commits_batch() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMWRITER01");

    let handle = spawn_writer(db.clone());
    for i in 0..3 {
        handle
            .tx
            .send(WriteOp {
                room_id: "ROOMWRITER01".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic(&format!("topic-{i}"), i as f64),
                },
            })
            .unwrap();
    }
    assert!(
        handle.shutdown().await,
        "writer should finish within timeout"
    );

    let conn = db.get().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM topics WHERE room_id='ROOMWRITER01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 3, "expected three topic rows after the batch");
}

fn drive_ops(db: &Db, ops: Vec<WriteOp>) {
    let mut writer = db.acquire_writer_conn().unwrap();
    let tx = writer.transaction().unwrap();
    for op in &ops {
        apply_op_in_tx(&tx, op).unwrap();
    }
    tx.commit().unwrap();
}

#[test]
fn apply_rename_topic_updates_only_title() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMRENAME01");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMRENAME01".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("t-1", 1.0),
                },
            },
            WriteOp {
                room_id: "ROOMRENAME01".into(),
                kind: WriteOpKind::RenameTopic {
                    topic_id: "t-1".into(),
                    title: "new-name".into(),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let (title, ord): (String, f64) = conn
        .query_row("SELECT title, ord FROM topics WHERE id='t-1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(title, "new-name");
    assert_eq!(ord, 1.0, "rename must not touch ord");
}

#[test]
fn apply_move_topic_updates_parent_and_ord() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMMOVE0001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMMOVE0001".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("p", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMMOVE0001".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("c", 1.0),
                },
            },
            WriteOp {
                room_id: "ROOMMOVE0001".into(),
                kind: WriteOpKind::MoveTopic {
                    topic_id: "c".into(),
                    parent_id: Some("p".into()),
                    ord: 5.0,
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let (parent, ord): (Option<String>, f64) = conn
        .query_row("SELECT parent_id, ord FROM topics WHERE id='c'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(parent.as_deref(), Some("p"));
    assert_eq!(ord, 5.0);
}

#[test]
fn apply_set_topic_status_toggles_done() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMSTATUS01");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMSTATUS01".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("t", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMSTATUS01".into(),
                kind: WriteOpKind::SetTopicStatus {
                    topic_id: "t".into(),
                    status: TopicStatus::Done,
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let status: String = conn
        .query_row("SELECT status FROM topics WHERE id='t'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "done");
}

#[test]
fn apply_delete_topic_removes_row_and_cascades_children() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMDELET0001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMDELET0001".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("p", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMDELET0001".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: Topic {
                        parent_id: Some("p".into()),
                        ..topic("c", 1.0)
                    },
                },
            },
            WriteOp {
                room_id: "ROOMDELET0001".into(),
                kind: WriteOpKind::DeleteTopic {
                    topic_id: "p".into(),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM topics WHERE room_id='ROOMDELET0001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "parent + child should cascade");
}

#[test]
fn apply_set_active_topic_writes_room_column() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMACTIVE001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMACTIVE001".into(),
                kind: WriteOpKind::UpsertTopic {
                    topic: topic("t", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMACTIVE001".into(),
                kind: WriteOpKind::SetActiveTopic {
                    topic_id: Some("t".into()),
                },
            },
        ],
    );
    {
        let conn = db.get().unwrap();
        let active: Option<String> = conn
            .query_row(
                "SELECT active_topic_id FROM rooms WHERE id='ROOMACTIVE001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active.as_deref(), Some("t"));
    } // drop the pool checkout before driving more ops in :memory: mode

    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMACTIVE001".into(),
            kind: WriteOpKind::SetActiveTopic { topic_id: None },
        }],
    );
    let conn = db.get().unwrap();
    let active: Option<String> = conn
        .query_row(
            "SELECT active_topic_id FROM rooms WHERE id='ROOMACTIVE001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(active.is_none(), "set_active_topic(None) should clear");
}

#[test]
fn apply_upsert_question_inserts_then_updates() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMQ0000001");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMQ0000001".into(),
            kind: WriteOpKind::UpsertQuestion {
                question: question("q-1", "ROOMQ0000001", "first?", "alice"),
            },
        }],
    );
    let mut q2 = question("q-1", "ROOMQ0000001", "first (edited)", "alice");
    q2.answered = true;
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMQ0000001".into(),
            kind: WriteOpKind::UpsertQuestion { question: q2 },
        }],
    );
    let conn = db.get().unwrap();
    let (text, answered): (String, i32) = conn
        .query_row(
            "SELECT text, answered FROM questions WHERE id='q-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(text, "first (edited)");
    assert_eq!(answered, 1);
}

#[test]
fn apply_add_vote_dedups_by_pk() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMV0000001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMV0000001".into(),
                kind: WriteOpKind::UpsertQuestion {
                    question: question("q", "ROOMV0000001", "?", "alice"),
                },
            },
            WriteOp {
                room_id: "ROOMV0000001".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "bob".into(),
                    created_at: 100,
                },
            },
            WriteOp {
                room_id: "ROOMV0000001".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "bob".into(),
                    created_at: 200,
                },
            },
            WriteOp {
                room_id: "ROOMV0000001".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "carol".into(),
                    created_at: 300,
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM question_votes WHERE question_id='q'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2, "dup vote from bob must dedup; carol distinct");
    let bob_ts: i64 = conn
        .query_row(
            "SELECT created_at FROM question_votes WHERE question_id='q' AND guest_id='bob'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        bob_ts, 100,
        "first AddVote wins; INSERT OR IGNORE skips the second"
    );
}

#[test]
fn apply_remove_vote_drops_only_one_row() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMV0000002");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMV0000002".into(),
                kind: WriteOpKind::UpsertQuestion {
                    question: question("q", "ROOMV0000002", "?", "alice"),
                },
            },
            WriteOp {
                room_id: "ROOMV0000002".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "bob".into(),
                    created_at: 100,
                },
            },
            WriteOp {
                room_id: "ROOMV0000002".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "carol".into(),
                    created_at: 200,
                },
            },
            WriteOp {
                room_id: "ROOMV0000002".into(),
                kind: WriteOpKind::RemoveVote {
                    question_id: "q".into(),
                    guest_id: "bob".into(),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let voters: Vec<String> = conn
        .prepare("SELECT guest_id FROM question_votes WHERE question_id='q' ORDER BY guest_id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(voters, vec!["carol".to_string()]);
}

#[test]
fn apply_set_question_answered_toggles() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMQA000001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMQA000001".into(),
                kind: WriteOpKind::UpsertQuestion {
                    question: question("q", "ROOMQA000001", "?", "alice"),
                },
            },
            WriteOp {
                room_id: "ROOMQA000001".into(),
                kind: WriteOpKind::SetQuestionAnswered {
                    question_id: "q".into(),
                    answered: true,
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let answered: i32 = conn
        .query_row("SELECT answered FROM questions WHERE id='q'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(answered, 1);
}

#[test]
fn apply_delete_question_cascades_votes() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMQDEL0001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMQDEL0001".into(),
                kind: WriteOpKind::UpsertQuestion {
                    question: question("q", "ROOMQDEL0001", "?", "alice"),
                },
            },
            WriteOp {
                room_id: "ROOMQDEL0001".into(),
                kind: WriteOpKind::AddVote {
                    question_id: "q".into(),
                    guest_id: "bob".into(),
                    created_at: 0,
                },
            },
            WriteOp {
                room_id: "ROOMQDEL0001".into(),
                kind: WriteOpKind::DeleteQuestion {
                    question_id: "q".into(),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let qs: i64 = conn
        .query_row("SELECT COUNT(*) FROM questions", [], |r| r.get(0))
        .unwrap();
    let vs: i64 = conn
        .query_row("SELECT COUNT(*) FROM question_votes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(qs, 0);
    assert_eq!(vs, 0, "FK cascade on questions(id) deletes votes");
}

#[test]
fn apply_promote_question_to_topic_is_atomic() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMPROM0001");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPROM0001".into(),
            kind: WriteOpKind::UpsertQuestion {
                question: question("q-source", "ROOMPROM0001", "ask?", "alice"),
            },
        }],
    );

    // Now promote: one WriteOp, one transaction, both rows must land.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPROM0001".into(),
            kind: WriteOpKind::PromoteQuestionToTopic {
                question_id: "q-source".into(),
                topic: topic("t-from-q", 0.0),
            },
        }],
    );
    let conn = db.get().unwrap();
    let qs: i64 = conn
        .query_row("SELECT COUNT(*) FROM questions", [], |r| r.get(0))
        .unwrap();
    let ts: i64 = conn
        .query_row("SELECT COUNT(*) FROM topics WHERE id='t-from-q'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(qs, 0, "source question should be gone");
    assert_eq!(ts, 1, "promoted topic should be present");
}

#[test]
fn apply_promote_rolls_back_if_topic_violates_fk() {
    // Both rows must land in one tx. We force a failure on the topic
    // insert (bad parent_id) and assert the question is still present.
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMPROMERR1");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPROMERR1".into(),
            kind: WriteOpKind::UpsertQuestion {
                question: question("q-keep", "ROOMPROMERR1", "?", "alice"),
            },
        }],
    );

    // Topic insert with a parent_id that doesn't exist → FK violation.
    let mut bad_topic = topic("t-bad", 0.0);
    bad_topic.parent_id = Some("nonexistent".into());

    {
        let mut writer = db.acquire_writer_conn().unwrap();
        let tx = writer.transaction().unwrap();
        let result = apply_op_in_tx(
            &tx,
            &WriteOp {
                room_id: "ROOMPROMERR1".into(),
                kind: WriteOpKind::PromoteQuestionToTopic {
                    question_id: "q-keep".into(),
                    topic: bad_topic,
                },
            },
        );
        assert!(result.is_err(), "FK violation must surface");
        // Even though `apply_upsert_topic` runs first and errors, the
        // outer transaction is rolled back when we drop it without
        // commit (the apply order is topic → delete; if it were
        // reversed the rollback still saves us).
        drop(tx);
    } // drop writer → return :memory: pool slot before the read below

    let conn = db.get().unwrap();
    let qs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM questions WHERE id='q-keep'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ts: i64 = conn
        .query_row("SELECT COUNT(*) FROM topics WHERE id='t-bad'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(qs, 1, "tx must roll back: question survives");
    assert_eq!(ts, 0, "tx must roll back: topic never landed");
}

#[test]
fn apply_upsert_board_then_rename() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMBOARDS01");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMBOARDS01".into(),
                kind: WriteOpKind::UpsertBoard {
                    board: board("b1", BoardKind::Pen, "first", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMBOARDS01".into(),
                kind: WriteOpKind::RenameBoard {
                    board_id: "b1".into(),
                    title: "renamed".into(),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let (title, kind): (String, String) = conn
        .query_row("SELECT title, kind FROM boards WHERE id='b1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(title, "renamed");
    assert_eq!(kind, "pen", "rename must not change kind");
}

#[test]
fn apply_delete_board_cascades_to_strokes_and_actions() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMBOARDD01");
    let conn = db.get().unwrap();
    conn.execute(
        "INSERT INTO boards (id,room_id,kind,title,ord,created_at) \
             VALUES ('b','ROOMBOARDD01','pen','t',0.0,0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pen_strokes (id,board_id,color,size,points_json,ord,created_at) \
             VALUES ('s','b','#000',1.0,'[]',0,0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pen_actions (id,board_id,kind,target_id,ord,created_at) \
             VALUES ('a','b','stroke_add','s',0,0)",
        [],
    )
    .unwrap();
    drop(conn);

    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMBOARDD01".into(),
            kind: WriteOpKind::DeleteBoard {
                board_id: "b".into(),
            },
        }],
    );

    let conn = db.get().unwrap();
    let n_strokes: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_strokes", [], |r| r.get(0))
        .unwrap();
    let n_actions: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_actions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_strokes, 0);
    assert_eq!(n_actions, 0);
}

#[test]
fn apply_set_focused_board_writes_room_column() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMFOCUS001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMFOCUS001".into(),
                kind: WriteOpKind::UpsertBoard {
                    board: board("b1", BoardKind::Pen, "t", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMFOCUS001".into(),
                kind: WriteOpKind::SetFocusedBoard {
                    board_id: Some("b1".into()),
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let focused: Option<String> = conn
        .query_row(
            "SELECT focused_board_id FROM rooms WHERE id='ROOMFOCUS001'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(focused.as_deref(), Some("b1"));
}

#[test]
fn apply_upsert_excalidraw_scene_inserts_then_updates() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMEXCAL001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMEXCAL001".into(),
                kind: WriteOpKind::UpsertBoard {
                    board: board("b1", BoardKind::Excalidraw, "t", 0.0),
                },
            },
            WriteOp {
                room_id: "ROOMEXCAL001".into(),
                kind: WriteOpKind::UpsertExcalidrawScene {
                    board_id: "b1".into(),
                    scene_version: 1,
                    elements_json: "[]".into(),
                    app_state_json: "{}".into(),
                    updated_at: 100,
                },
            },
            WriteOp {
                room_id: "ROOMEXCAL001".into(),
                kind: WriteOpKind::UpsertExcalidrawScene {
                    board_id: "b1".into(),
                    scene_version: 2,
                    elements_json: "[{\"x\":1}]".into(),
                    app_state_json: "{\"k\":1}".into(),
                    updated_at: 200,
                },
            },
        ],
    );
    let conn = db.get().unwrap();
    let (v, els, st, t): (i64, String, String, i64) = conn
        .query_row(
            "SELECT scene_version, elements_json, app_state_json, updated_at \
                 FROM excalidraw_scenes WHERE board_id='b1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(v, 2);
    assert_eq!(els, "[{\"x\":1}]");
    assert_eq!(st, "{\"k\":1}");
    assert_eq!(t, 200);
}

#[test]
fn apply_set_kicked_preserves_existing_muted() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMMOD000001");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMMOD000001".into(),
                kind: WriteOpKind::SetMuted {
                    guest_id: "g".into(),
                    muted: true,
                    updated_at: 100,
                },
            },
            WriteOp {
                room_id: "ROOMMOD000001".into(),
                kind: WriteOpKind::SetKicked {
                    guest_id: "g".into(),
                    kicked: true,
                    updated_at: 200,
                },
            },
        ],
    );
    let (k, m) = db.get_moderation("ROOMMOD000001", "g").unwrap().unwrap();
    assert!(k && m, "kick must preserve existing mute");
}

#[test]
fn apply_set_muted_preserves_existing_kicked() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMMOD000002");
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMMOD000002".into(),
                kind: WriteOpKind::SetKicked {
                    guest_id: "g".into(),
                    kicked: true,
                    updated_at: 100,
                },
            },
            WriteOp {
                room_id: "ROOMMOD000002".into(),
                kind: WriteOpKind::SetMuted {
                    guest_id: "g".into(),
                    muted: true,
                    updated_at: 200,
                },
            },
        ],
    );
    let (k, m) = db.get_moderation("ROOMMOD000002", "g").unwrap().unwrap();
    assert!(k && m, "mute must preserve existing kick");
}

#[test]
fn pen_insert_completed_stroke_writes_stroke_and_action() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPEN000001", "b1");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPEN000001".into(),
            kind: WriteOpKind::InsertCompletedPenStroke {
                board_id: "b1".into(),
                stroke: stroke("s1", 1),
                action_id: "a1".into(),
                created_at: 500,
            },
        }],
    );
    let conn = db.get().unwrap();
    let strokes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pen_strokes WHERE board_id='b1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (kind, target, ord, payload): (String, Option<String>, i64, Option<String>) = conn
        .query_row(
            "SELECT kind, target_id, ord, payload_json FROM pen_actions WHERE id='a1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(strokes, 1);
    assert_eq!(kind, "stroke_begin");
    assert_eq!(target.as_deref(), Some("s1"));
    assert_eq!(ord, 1, "first action should get ord = 1");
    assert!(payload.is_none(), "stroke_begin has no payload");
}

#[test]
fn pen_text_set_then_set_carries_before_json() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPEN000002", "b1");
    // First text set — no prior state.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPEN000002".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: pen_text("t1", "hello"),
                action_id: "a1".into(),
                before_json: None,
                created_at: 100,
            },
        }],
    );
    // Second text set — capture the previous state as JSON.
    let prev = pen_text("t1", "hello");
    let prev_json = serde_json::to_string(&prev).unwrap();
    let mut new = pen_text("t1", "world");
    new.updated_at = 300;
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPEN000002".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: new,
                action_id: "a2".into(),
                before_json: Some(prev_json.clone()),
                created_at: 300,
            },
        }],
    );

    let conn = db.get().unwrap();
    let row_text: String = conn
        .query_row("SELECT text FROM pen_texts WHERE id='t1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(row_text, "world");
    let (payload, ord): (Option<String>, i64) = conn
        .query_row(
            "SELECT payload_json, ord FROM pen_actions WHERE id='a2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(ord, 2, "monotonic per board");
    assert_eq!(payload.as_deref(), Some(prev_json.as_str()));
}

#[test]
fn pen_undo_text_set_with_prev_restores_prev_text() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENUND001", "b1");
    // Set "hello", then set "world" with before_json = hello.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND001".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: pen_text("t1", "hello"),
                action_id: "a1".into(),
                before_json: None,
                created_at: 1,
            },
        }],
    );
    let prev = pen_text("t1", "hello");
    let prev_json = serde_json::to_string(&prev).unwrap();
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND001".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: pen_text("t1", "world"),
                action_id: "a2".into(),
                before_json: Some(prev_json),
                created_at: 2,
            },
        }],
    );
    // Undo a2.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND001".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "a2".into(),
            },
        }],
    );

    let conn = db.get().unwrap();
    let row_text: String = conn
        .query_row("SELECT text FROM pen_texts WHERE id='t1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(row_text, "hello", "undo should restore prior text");
    let action_present: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_actions WHERE id='a2'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(action_present, 0, "undo deletes the target action row");
}

#[test]
fn pen_undo_text_set_without_prev_removes_inserted_text() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENUND002", "b1");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND002".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: pen_text("t-new", "fresh"),
                action_id: "a1".into(),
                before_json: None,
                created_at: 1,
            },
        }],
    );
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND002".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "a1".into(),
            },
        }],
    );
    let conn = db.get().unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_texts WHERE id='t-new'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 0, "undo of a fresh upsert (no prev) removes the row");
}

#[test]
fn pen_undo_text_delete_restores_text() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENUND003", "b1");
    // Insert, then delete, then undo.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND003".into(),
            kind: WriteOpKind::UpsertPenText {
                board_id: "b1".into(),
                text: pen_text("t1", "keep-me"),
                action_id: "a1".into(),
                before_json: None,
                created_at: 1,
            },
        }],
    );
    let snapshot = pen_text("t1", "keep-me");
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND003".into(),
            kind: WriteOpKind::DeletePenText {
                board_id: "b1".into(),
                text_id: "t1".into(),
                action_id: "a2".into(),
                before_json: snapshot_json,
                created_at: 2,
            },
        }],
    );
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND003".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "a2".into(),
            },
        }],
    );

    let conn = db.get().unwrap();
    let text: String = conn
        .query_row("SELECT text FROM pen_texts WHERE id='t1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(text, "keep-me");
}

#[test]
fn pen_undo_stroke_begin_deletes_stroke() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENUND004", "b1");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND004".into(),
            kind: WriteOpKind::InsertCompletedPenStroke {
                board_id: "b1".into(),
                stroke: stroke("s1", 1),
                action_id: "a1".into(),
                created_at: 1,
            },
        }],
    );
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUND004".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "a1".into(),
            },
        }],
    );
    let conn = db.get().unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_strokes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn pen_clear_then_undo_restores_strokes_and_texts() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENCLR001", "b1");
    // Add a stroke + a text.
    drive_ops(
        &db,
        vec![
            WriteOp {
                room_id: "ROOMPENCLR001".into(),
                kind: WriteOpKind::InsertCompletedPenStroke {
                    board_id: "b1".into(),
                    stroke: stroke("s1", 1),
                    action_id: "a1".into(),
                    created_at: 1,
                },
            },
            WriteOp {
                room_id: "ROOMPENCLR001".into(),
                kind: WriteOpKind::UpsertPenText {
                    board_id: "b1".into(),
                    text: pen_text("t1", "x"),
                    action_id: "a2".into(),
                    before_json: None,
                    created_at: 2,
                },
            },
        ],
    );
    let strokes_json = serde_json::to_string(&vec![stroke("s1", 1)]).unwrap();
    let texts_json = serde_json::to_string(&vec![pen_text("t1", "x")]).unwrap();
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENCLR001".into(),
            kind: WriteOpKind::PenClear {
                board_id: "b1".into(),
                action_id: "a-clear".into(),
                before_strokes_json: strokes_json,
                before_texts_json: texts_json,
                created_at: 3,
            },
        }],
    );
    // After clear: rows are gone.
    {
        let conn = db.get().unwrap();
        let n_s: i64 = conn
            .query_row("SELECT COUNT(*) FROM pen_strokes", [], |r| r.get(0))
            .unwrap();
        let n_t: i64 = conn
            .query_row("SELECT COUNT(*) FROM pen_texts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_s, 0);
        assert_eq!(n_t, 0);
    }
    // Undo clear.
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENCLR001".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "a-clear".into(),
            },
        }],
    );
    let conn = db.get().unwrap();
    let n_s: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_strokes", [], |r| r.get(0))
        .unwrap();
    let n_t: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_texts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n_s, 1, "stroke should be restored after clear-undo");
    assert_eq!(n_t, 1, "text should be restored after clear-undo");
}

#[test]
fn pen_undo_unknown_action_is_noop() {
    let db = Db::open_in_memory().unwrap();
    seed_pen_board(&db, "ROOMPENUNK001", "b1");
    drive_ops(
        &db,
        vec![WriteOp {
            room_id: "ROOMPENUNK001".into(),
            kind: WriteOpKind::PenUndo {
                board_id: "b1".into(),
                target_action_id: "ghost".into(),
            },
        }],
    );
    // No panic, no rows.
    let conn = db.get().unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM pen_actions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn writer_upsert_topic_replaces_existing_row() {
    let db = Db::open_in_memory().unwrap();
    seed_room(&db, "ROOMWRITER02");

    let handle = spawn_writer(db.clone());
    handle
        .tx
        .send(WriteOp {
            room_id: "ROOMWRITER02".into(),
            kind: WriteOpKind::UpsertTopic {
                topic: topic("t-1", 1.0),
            },
        })
        .unwrap();
    // Same id, different title + ord.
    let mut t = topic("t-1", 9.0);
    t.title = "renamed".into();
    handle
        .tx
        .send(WriteOp {
            room_id: "ROOMWRITER02".into(),
            kind: WriteOpKind::UpsertTopic { topic: t },
        })
        .unwrap();
    assert!(handle.shutdown().await);

    let conn = db.get().unwrap();
    let (title, ord): (String, f64) = conn
        .query_row("SELECT title, ord FROM topics WHERE id='t-1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(title, "renamed");
    assert_eq!(ord, 9.0);
}
