//! Wire protocol — single source of truth for both Rust + generated TS.
//!
//! See `.plan/2026-05-24-amber-falcon/protocol.md`. JSON on the wire is
//! camelCase; envelope tags use the `type` discriminator. Each enum variant
//! sets `v=1` when serialised; the receiving side validates it on inbound.
//!
//! Only the messages needed in M1 are listed today. Extending the protocol
//! is purely additive: add a variant, regenerate `web/src/proto/generated.ts`
//! via `just proto-gen`.
//!
//! ## Test-mode TS export
//! Each public struct/enum derives `ts_rs::TS` under the `ts-gen` feature
//! and exports to `web/src/proto/generated.ts` when the dedicated test
//! `proto_export` runs.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[cfg(feature = "ts-gen")]
use ts_rs::TS;

pub const PROTOCOL_VERSION: u8 = 1;

// ───────────────────────────── shared types ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Host,
    Guest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub enum TopicStatus {
    Pending,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub ord: f64,
    pub status: TopicStatus,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub guest_id: String,
    pub display_name: String,
    pub muted: bool,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub room_id: String,
    pub author_guest_id: String,
    pub author_name: String,
    pub anonymous: bool,
    pub text: String,
    pub answered: bool,
    pub created_at: i64,
    pub vote_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub guest_id: String,
    pub display_name: String,
    pub muted: bool,
    pub joined_at: i64,
    pub client_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummary {
    pub id: String,
    pub title: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct You {
    pub client_id: String,
    pub role: Role,
    pub guest_id: String,
}

/// Welcome snapshot delivered on Hello + on every GetSnapshot. M1 carries
/// only the always-present fields; later phases populate topics/boards/etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct RoomSnapshot {
    pub room: RoomSummary,
    pub you: You,
    pub guests: Vec<Guest>,
    pub presence: Vec<Presence>,
    pub topics: Vec<Topic>,
    pub active_topic_id: Option<String>,
    pub questions: Vec<Question>,
    pub my_votes: Vec<String>,
    #[cfg_attr(feature = "ts-gen", ts(type = "unknown[]"))]
    pub boards: Vec<JsonValue>,
    pub focused_board_id: Option<String>,
    #[cfg_attr(feature = "ts-gen", ts(type = "unknown[]"))]
    pub hands: Vec<JsonValue>,
    pub seq: u64,
}

// ──────────────────────────── client → server ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(
        export,
        export_to = "../../web/src/proto/generated.ts",
        rename_all_fields = "camelCase"
    )
)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum ClientMsg {
    Hello {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: Role,
        guest_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        admin_token: Option<String>,
    },
    SetDisplayName {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
    },
    GetSnapshot {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        since: Option<i64>,
    },
    Pong {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    AddTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_id: Option<String>,
    },
    RenameTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topic_id: String,
        title: String,
    },
    MoveTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topic_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_parent_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_id: Option<String>,
    },
    DeleteTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topic_id: String,
    },
    SetActiveTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        topic_id: Option<String>,
    },
    MarkTopicDone {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topic_id: String,
        done: bool,
    },
    SubmitQuestion {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        text: String,
        anonymous: bool,
    },
    VoteQuestion {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        question_id: String,
        vote: bool,
    },
    MarkQuestionAnswered {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        question_id: String,
        answered: bool,
    },
    DeleteQuestion {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        question_id: String,
    },
}

// ──────────────────────────── server → client ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(
        export,
        export_to = "../../web/src/proto/generated.ts",
        rename_all_fields = "camelCase"
    )
)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum ServerMsg {
    Welcome {
        v: u8,
        ts: i64,
        seq: u64,
        you: You,
        snapshot: RoomSnapshot,
    },
    Error {
        v: u8,
        ts: i64,
        seq: u64,
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ref_id: Option<String>,
    },
    Ack {
        v: u8,
        ts: i64,
        seq: u64,
        ref_id: String,
    },
    PresenceUpdate {
        v: u8,
        ts: i64,
        seq: u64,
        guests: Vec<Guest>,
    },
    RoomSnapshot {
        v: u8,
        ts: i64,
        seq: u64,
        snapshot: RoomSnapshot,
    },
    Ping {
        v: u8,
        ts: i64,
        seq: u64,
    },
    TopicTreeUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        topics: Vec<Topic>,
        active_topic_id: Option<String>,
    },
    QuestionAdded {
        v: u8,
        ts: i64,
        seq: u64,
        question: Question,
    },
    QuestionUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        question: Question,
    },
    QuestionDeleted {
        v: u8,
        ts: i64,
        seq: u64,
        question_id: String,
    },
    VoteUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        question_id: String,
        vote_count: u32,
        voter_guest_id: String,
    },
}

// Documented error codes used in M1 (extensible).
pub mod error_codes {
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const ROOM_NOT_FOUND: &str = "room_not_found";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const PROTOCOL_VIOLATION: &str = "protocol_violation";
    pub const FORBIDDEN: &str = "forbidden";
    pub const RATE_LIMIT: &str = "rate_limit";
}

#[cfg(all(test, feature = "ts-gen"))]
mod proto_export_tests {
    use super::*;

    // Triggered by `just proto-gen`:
    //   cargo test --features ts-gen proto_export -- --nocapture
    // ts-rs writes each `#[ts(export)]` type to the configured path when its
    // `export()` is invoked; calling it explicitly guarantees the file is
    // (re)written even if no other test pulls the type into scope.
    #[test]
    fn proto_export() {
        Role::export().expect("export Role");
        TopicStatus::export().expect("export TopicStatus");
        Topic::export().expect("export Topic");
        You::export().expect("export You");
        Guest::export().expect("export Guest");
        Presence::export().expect("export Presence");
        Question::export().expect("export Question");
        RoomSummary::export().expect("export RoomSummary");
        RoomSnapshot::export().expect("export RoomSnapshot");
        ClientMsg::export().expect("export ClientMsg");
        ServerMsg::export().expect("export ServerMsg");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_hello_round_trips() {
        let msg = ClientMsg::Hello {
            v: 1,
            id: Some("c1".into()),
            role: Role::Guest,
            guest_id: "g1".into(),
            display_name: Some("Alice".into()),
            admin_token: None,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"Hello\""));
        assert!(s.contains("\"guestId\":\"g1\""));
        assert!(s.contains("\"displayName\":\"Alice\""));
        assert!(!s.contains("adminToken"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn unknown_client_type_is_rejected() {
        let bad = r#"{"type":"Mystery","v":1}"#;
        let r: Result<ClientMsg, _> = serde_json::from_str(bad);
        assert!(r.is_err());
    }

    #[test]
    fn server_welcome_uses_type_tag_and_camel_case() {
        let snap = RoomSnapshot {
            room: RoomSummary {
                id: "ROOMID000001".into(),
                title: "T".into(),
                created_at: 0,
            },
            you: You {
                client_id: "c".into(),
                role: Role::Host,
                guest_id: "g".into(),
            },
            guests: vec![],
            presence: vec![],
            topics: vec![],
            active_topic_id: None,
            questions: vec![],
            my_votes: vec![],
            boards: vec![],
            focused_board_id: None,
            hands: vec![],
            seq: 0,
        };
        let msg = ServerMsg::Welcome {
            v: 1,
            ts: 123,
            seq: 0,
            you: snap.you.clone(),
            snapshot: snap,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"Welcome\""));
        assert!(s.contains("\"clientId\""));
        assert!(s.contains("\"activeTopicId\":null"));
    }

    #[test]
    fn add_topic_round_trips() {
        let msg = ClientMsg::AddTopic {
            v: 1,
            id: Some("c1".into()),
            parent_id: None,
            title: "Topic 1".into(),
            after_id: None,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"AddTopic\""));
        assert!(s.contains("\"title\":\"Topic 1\""));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn topic_status_serde() {
        let pending = TopicStatus::Pending;
        let done = TopicStatus::Done;
        let ps = serde_json::to_string(&pending).unwrap();
        let ds = serde_json::to_string(&done).unwrap();
        assert_eq!(ps, "\"pending\"");
        assert_eq!(ds, "\"done\"");
        let _pp: TopicStatus = serde_json::from_str(&ps).unwrap();
        let _dp: TopicStatus = serde_json::from_str(&ds).unwrap();
    }

    #[test]
    fn topic_struct_round_trips() {
        let t = Topic {
            id: "t1".into(),
            parent_id: None,
            title: "Test".into(),
            ord: 1.0,
            status: TopicStatus::Pending,
            created_at: 12345,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"id\":\"t1\""));
        assert!(s.contains("\"status\":\"pending\""));
        let _back: Topic = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn topic_tree_updated_round_trips() {
        let msg = ServerMsg::TopicTreeUpdated {
            v: 1,
            ts: 100,
            seq: 5,
            topics: vec![Topic {
                id: "t1".into(),
                parent_id: None,
                title: "Test".into(),
                ord: 1.0,
                status: TopicStatus::Pending,
                created_at: 100,
            }],
            active_topic_id: Some("t1".into()),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"TopicTreeUpdated\""));
        assert!(s.contains("\"activeTopicId\":\"t1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn question_struct_round_trips() {
        let q = Question {
            id: "q1".into(),
            room_id: "r1".into(),
            author_guest_id: "g1".into(),
            author_name: "Alice".into(),
            anonymous: false,
            text: "What is Rust?".into(),
            answered: false,
            created_at: 12345,
            vote_count: 3,
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"id\":\"q1\""));
        assert!(s.contains("\"voteCount\":3"));
        assert!(s.contains("\"anonymous\":false"));
        let _back: Question = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn question_anonymous_outbound_shaping() {
        let q = Question {
            id: "q1".into(),
            room_id: "r1".into(),
            author_guest_id: "g1".into(),
            author_name: "Real Name".into(),
            anonymous: true,
            text: "Secret question".into(),
            answered: false,
            created_at: 12345,
            vote_count: 0,
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"authorGuestId\":\"g1\""));
        assert!(s.contains("\"authorName\":\"Real Name\""));
        assert!(s.contains("\"anonymous\":true"));
    }

    #[test]
    fn submit_question_round_trips() {
        let msg = ClientMsg::SubmitQuestion {
            v: 1,
            id: Some("c1".into()),
            text: "How does async work?".into(),
            anonymous: true,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"SubmitQuestion\""));
        assert!(s.contains("\"text\":\"How does async work?\""));
        assert!(s.contains("\"anonymous\":true"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn vote_question_round_trips() {
        let msg = ClientMsg::VoteQuestion {
            v: 1,
            id: Some("c1".into()),
            question_id: "q1".into(),
            vote: true,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"VoteQuestion\""));
        assert!(s.contains("\"questionId\":\"q1\""));
        assert!(s.contains("\"vote\":true"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn mark_question_answered_round_trips() {
        let msg = ClientMsg::MarkQuestionAnswered {
            v: 1,
            id: Some("c1".into()),
            question_id: "q1".into(),
            answered: true,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"MarkQuestionAnswered\""));
        assert!(s.contains("\"questionId\":\"q1\""));
        assert!(s.contains("\"answered\":true"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn delete_question_round_trips() {
        let msg = ClientMsg::DeleteQuestion {
            v: 1,
            id: Some("c1".into()),
            question_id: "q1".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"DeleteQuestion\""));
        assert!(s.contains("\"questionId\":\"q1\""));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn question_added_round_trips() {
        let q = Question {
            id: "q1".into(),
            room_id: "r1".into(),
            author_guest_id: "g1".into(),
            author_name: "Alice".into(),
            anonymous: false,
            text: "How does borrowing work?".into(),
            answered: false,
            created_at: 12345,
            vote_count: 0,
        };
        let msg = ServerMsg::QuestionAdded {
            v: 1,
            ts: 100,
            seq: 5,
            question: q,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"QuestionAdded\""));
        assert!(s.contains("\"id\":\"q1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn question_updated_round_trips() {
        let q = Question {
            id: "q1".into(),
            room_id: "r1".into(),
            author_guest_id: "g1".into(),
            author_name: "Alice".into(),
            anonymous: false,
            text: "How does borrowing work?".into(),
            answered: true,
            created_at: 12345,
            vote_count: 5,
        };
        let msg = ServerMsg::QuestionUpdated {
            v: 1,
            ts: 100,
            seq: 6,
            question: q,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"QuestionUpdated\""));
        assert!(s.contains("\"answered\":true"));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn question_deleted_round_trips() {
        let msg = ServerMsg::QuestionDeleted {
            v: 1,
            ts: 100,
            seq: 7,
            question_id: "q1".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"QuestionDeleted\""));
        assert!(s.contains("\"questionId\":\"q1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn vote_updated_round_trips() {
        let msg = ServerMsg::VoteUpdated {
            v: 1,
            ts: 100,
            seq: 8,
            question_id: "q1".into(),
            vote_count: 5,
            voter_guest_id: "g1".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"VoteUpdated\""));
        assert!(s.contains("\"questionId\":\"q1\""));
        assert!(s.contains("\"voteCount\":5"));
        assert!(s.contains("\"voterGuestId\":\"g1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }
}
