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

impl Question {
    pub fn to_outbound(&self) -> Question {
        if self.anonymous {
            Question {
                author_guest_id: String::new(),
                author_name: "Anonymous".to_string(),
                ..self.clone()
            }
        } else {
            self.clone()
        }
    }
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
pub enum BoardKind {
    Pen,
    Excalidraw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct RaisedHand {
    pub guest_id: String,
    pub display_name: String,
    pub topic: String,
    pub raised_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub id: String,
    pub kind: BoardKind,
    pub title: String,
    pub created_at: i64,
    pub ord: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct PenText {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub font_size: f64,
    pub color: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-gen", derive(TS))]
#[cfg_attr(
    feature = "ts-gen",
    ts(export, export_to = "../../web/src/proto/generated.ts")
)]
#[serde(rename_all = "camelCase")]
pub struct PenStrokeSummary {
    pub id: String,
    pub color: String,
    pub size: f64,
    pub points: Vec<[f32; 3]>,
    pub created_at: i64,
    pub ord: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExcalidrawScene {
    pub board_id: String,
    pub scene_version: u64,
    pub elements: JsonValue,
    pub app_state: JsonValue,
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
    pub hands: Vec<RaisedHand>,
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
    KickGuest {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        guest_id: String,
    },
    MuteGuest {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        guest_id: String,
        muted: bool,
    },
    CreateBoard {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        kind: BoardKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    RenameBoard {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        title: String,
    },
    DeleteBoard {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
    },
    SetFocusedBoard {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
    },
    ExcalidrawUpdate {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        scene_version: u64,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown[]"))]
        elements: JsonValue,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown"))]
        app_state: JsonValue,
    },
    RaiseHand {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        topic: String,
    },
    LowerHand {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    CallOnHand {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        guest_id: String,
    },
    DismissHand {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        guest_id: String,
    },
    PromoteQuestionToTopic {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        question_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_topic_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_topic_id: Option<String>,
    },
    PenStrokeBegin {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        stroke_id: String,
        color: String,
        size: f64,
    },
    PenStrokeAppend {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        stroke_id: String,
        points: Vec<[f32; 3]>,
    },
    PenStrokeEnd {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        stroke_id: String,
    },
    PenTextSet {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        text_id: String,
        x: f64,
        y: f64,
        text: String,
        font_size: f64,
        color: String,
    },
    PenTextDelete {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        text_id: String,
    },
    PenClear {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
    },
    PenUndo {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
    },
    Cursor {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        x: f64,
        y: f64,
    },
    Click {
        v: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        board_id: String,
        x: f64,
        y: f64,
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
    KickNotice {
        v: u8,
        ts: i64,
        seq: u64,
        /// Target guest id. Clients ignore this notice unless it
        /// matches their own guest id.
        guest_id: String,
    },
    VoteUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        question_id: String,
        vote_count: u32,
        voter_guest_id: String,
    },
    BoardCreated {
        v: u8,
        ts: i64,
        seq: u64,
        board: Board,
    },
    BoardUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        board: Board,
    },
    BoardDeleted {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
    },
    FocusedBoardChanged {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
    },
    ExcalidrawDelta {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        scene_version: u64,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown[]"))]
        elements: JsonValue,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown"))]
        app_state: JsonValue,
    },
    ExcalidrawSceneReset {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        scene_version: u64,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown[]"))]
        elements: JsonValue,
        #[cfg_attr(feature = "ts-gen", ts(type = "unknown"))]
        app_state: JsonValue,
    },
    HandsUpdated {
        v: u8,
        ts: i64,
        seq: u64,
        hands: Vec<RaisedHand>,
    },
    QuestionPromotedToTopic {
        v: u8,
        ts: i64,
        seq: u64,
        question_id: String,
        topic: Topic,
    },
    PenStrokeBegun {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        stroke_id: String,
        color: String,
        size: f64,
        author_client_id: String,
    },
    PenStrokeAppended {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        stroke_id: String,
        points: Vec<[f32; 3]>,
    },
    PenStrokeEnded {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        stroke_id: String,
    },
    PenTextUpserted {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        text: PenText,
    },
    PenTextDeleted {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        text_id: String,
    },
    PenCleared {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
    },
    PenUndone {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        removed_stroke_id: Option<String>,
        removed_text_id: Option<String>,
    },
    CursorMoved {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        client_id: String,
        guest_id: String,
        display_name: String,
        x: f64,
        y: f64,
    },
    Clicked {
        v: u8,
        ts: i64,
        seq: u64,
        board_id: String,
        client_id: String,
        guest_id: String,
        display_name: String,
        x: f64,
        y: f64,
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
    pub const MUTED: &str = "muted";
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
        RaisedHand::export().expect("export RaisedHand");
        RoomSummary::export().expect("export RoomSummary");
        BoardKind::export().expect("export BoardKind");
        Board::export().expect("export Board");
        PenText::export().expect("export PenText");
        PenStrokeSummary::export().expect("export PenStrokeSummary");
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

    #[test]
    fn board_kind_round_trips() {
        let pen = BoardKind::Pen;
        let exc = BoardKind::Excalidraw;
        let ps = serde_json::to_string(&pen).unwrap();
        let es = serde_json::to_string(&exc).unwrap();
        assert_eq!(ps, "\"pen\"");
        assert_eq!(es, "\"excalidraw\"");
        let _pp: BoardKind = serde_json::from_str(&ps).unwrap();
        let _ep: BoardKind = serde_json::from_str(&es).unwrap();
    }

    #[test]
    fn board_struct_round_trips() {
        let b = Board {
            id: "b1".into(),
            kind: BoardKind::Excalidraw,
            title: "My Board".into(),
            created_at: 12345,
            ord: 1.0,
        };
        let s = serde_json::to_string(&b).unwrap();
        assert!(s.contains("\"id\":\"b1\""));
        assert!(s.contains("\"kind\":\"excalidraw\""));
        assert!(s.contains("\"title\":\"My Board\""));
        let _back: Board = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn create_board_round_trips() {
        let msg = ClientMsg::CreateBoard {
            v: 1,
            id: Some("c1".into()),
            kind: BoardKind::Excalidraw,
            title: Some("Test".into()),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"CreateBoard\""));
        assert!(s.contains("\"kind\":\"excalidraw\""));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn excalidraw_update_round_trips() {
        use serde_json::json;
        let msg = ClientMsg::ExcalidrawUpdate {
            v: 1,
            id: Some("c1".into()),
            board_id: "b1".into(),
            scene_version: 5,
            elements: json!([]),
            app_state: json!({}),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"ExcalidrawUpdate\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        assert!(s.contains("\"sceneVersion\":5"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn excalidraw_delta_round_trips() {
        use serde_json::json;
        let msg = ServerMsg::ExcalidrawDelta {
            v: 1,
            ts: 100,
            seq: 9,
            board_id: "b1".into(),
            scene_version: 5,
            elements: json!([]),
            app_state: json!({}),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"ExcalidrawDelta\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn excalidraw_scene_reset_round_trips() {
        use serde_json::json;
        let msg = ServerMsg::ExcalidrawSceneReset {
            v: 1,
            ts: 100,
            seq: 10,
            board_id: "b1".into(),
            scene_version: 7,
            elements: json!([]),
            app_state: json!({}),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"ExcalidrawSceneReset\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn board_created_round_trips() {
        let board = Board {
            id: "b1".into(),
            kind: BoardKind::Pen,
            title: "Pen Board".into(),
            created_at: 12345,
            ord: 1.0,
        };
        let msg = ServerMsg::BoardCreated {
            v: 1,
            ts: 100,
            seq: 11,
            board,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"BoardCreated\""));
        assert!(s.contains("\"id\":\"b1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn focused_board_changed_round_trips() {
        let msg = ServerMsg::FocusedBoardChanged {
            v: 1,
            ts: 100,
            seq: 12,
            board_id: "b1".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"FocusedBoardChanged\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn cursor_intent_round_trips() {
        let msg = ClientMsg::Cursor {
            v: 1,
            id: Some("c1".into()),
            board_id: "b1".into(),
            x: 100.5,
            y: 200.75,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"Cursor\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        assert!(s.contains("\"x\":100.5"));
        assert!(s.contains("\"y\":200.75"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn click_intent_round_trips() {
        let msg = ClientMsg::Click {
            v: 1,
            id: Some("c1".into()),
            board_id: "b1".into(),
            x: 150.0,
            y: 250.0,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"Click\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        assert!(s.contains("\"x\":150"));
        assert!(s.contains("\"y\":250"));
        let _back: ClientMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn cursor_moved_round_trips() {
        let msg = ServerMsg::CursorMoved {
            v: 1,
            ts: 100,
            seq: 13,
            board_id: "b1".into(),
            client_id: "c1".into(),
            guest_id: "g1".into(),
            display_name: "Alice".into(),
            x: 100.5,
            y: 200.75,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"CursorMoved\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        assert!(s.contains("\"clientId\":\"c1\""));
        assert!(s.contains("\"guestId\":\"g1\""));
        assert!(s.contains("\"displayName\":\"Alice\""));
        assert!(s.contains("\"x\":100.5"));
        assert!(s.contains("\"y\":200.75"));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn clicked_round_trips() {
        let msg = ServerMsg::Clicked {
            v: 1,
            ts: 100,
            seq: 14,
            board_id: "b1".into(),
            client_id: "c1".into(),
            guest_id: "g1".into(),
            display_name: "Alice".into(),
            x: 150.0,
            y: 250.0,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"type\":\"Clicked\""));
        assert!(s.contains("\"boardId\":\"b1\""));
        assert!(s.contains("\"clientId\":\"c1\""));
        assert!(s.contains("\"displayName\":\"Alice\""));
        assert!(s.contains("\"x\":150"));
        assert!(s.contains("\"y\":250"));
        let _back: ServerMsg = serde_json::from_str(&s).unwrap();
    }
}
