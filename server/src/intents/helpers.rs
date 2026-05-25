use std::fmt;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::SinkExt;

use crate::api::now_ms;
use crate::db::{WriteOp, WriteOpKind};
use crate::proto::{
    error_codes, Board, PenText, Question, Role, ServerMsg, Topic, PROTOCOL_VERSION,
};
use crate::room::Room;
use crate::state::AppState;

pub(crate) type WsSink = futures_util::stream::SplitSink<WebSocket, Message>;

pub(crate) struct SessionCtx<'a> {
    pub sink: &'a mut WsSink,
    pub room: &'a Arc<Room>,
    pub state: &'a AppState,
    pub client_id: &'a str,
    pub guest_id: &'a str,
    pub role: Role,
}

#[derive(Debug)]
pub(crate) struct IntentError {
    msg: Box<ServerMsg>,
    close: bool,
}

impl IntentError {
    pub(crate) fn client(
        code: &str,
        message: impl Into<String>,
        ref_id: Option<&str>,
        seq: u64,
    ) -> Self {
        Self {
            msg: Box::new(error_frame(
                code,
                &message.into(),
                ref_id.map(str::to_string),
                seq,
            )),
            close: false,
        }
    }

    pub(crate) fn should_close(&self) -> bool {
        self.close
    }

    pub(crate) fn into_server_msg(self) -> ServerMsg {
        *self.msg
    }
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.msg {
            ServerMsg::Error { code, message, .. } => write!(f, "{code}: {message}"),
            _ => write!(f, "intent error"),
        }
    }
}

impl std::error::Error for IntentError {}

pub(crate) fn ensure_host(ctx: &SessionCtx<'_>, ref_id: Option<&str>) -> Result<(), IntentError> {
    if ctx.role == Role::Host {
        Ok(())
    } else {
        Err(IntentError::client(
            error_codes::FORBIDDEN,
            "admin only",
            ref_id,
            ctx.room.current_seq(),
        ))
    }
}

pub(crate) fn ensure_not_muted(
    ctx: &SessionCtx<'_>,
    ref_id: Option<&str>,
    code: &str,
    message: &str,
) -> Result<(), IntentError> {
    if ctx.role == Role::Guest && ctx.room.is_muted(ctx.guest_id) {
        Err(IntentError::client(
            code,
            message,
            ref_id,
            ctx.room.current_seq(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) async fn ack_if_id(ctx: &mut SessionCtx<'_>, intent_ref_id: Option<&str>) {
    if let Some(ref_id) = intent_ref_id {
        let ack = ServerMsg::Ack {
            v: PROTOCOL_VERSION,
            ts: now_ms(),
            seq: ctx.room.current_seq(),
            ref_id: ref_id.to_string(),
        };
        let _ = send(ctx.sink, &ack).await;
    }
}

/// Enqueue a WriteOp on the single-writer task. Errors (channel closed
/// at shutdown) are silenced — the in-memory broadcast has already
/// occurred and the next process boot will hydrate from the prior
/// committed state.
pub(crate) fn enqueue_write(state: &AppState, room: &Arc<Room>, kind: WriteOpKind) {
    let _ = state.writer_tx.send(WriteOp {
        room_id: room.id.clone(),
        kind,
    });
}

pub(crate) fn broadcast_presence(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::PresenceUpdate {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        guests: room.guests(),
    };
    // A send error here just means there are no subscribers right now;
    // safe to ignore — the broadcast lives on the room and new subscribers
    // will pick up presence in their next snapshot.
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_topic_tree(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::TopicTreeUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        topics: room.topics(),
        active_topic_id: room.active_topic_id(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_question_added(room: &Arc<Room>, question: &Question) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionAdded {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question: question.to_outbound(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_question_updated(room: &Arc<Room>, question: &Question) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question: question.to_outbound(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_question_deleted(room: &Arc<Room>, question_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_vote_updated(
    room: &Arc<Room>,
    question_id: &str,
    vote_count: u32,
    voter_guest_id: &str,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::VoteUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
        vote_count,
        voter_guest_id: voter_guest_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_board_created(room: &Arc<Room>, board: &Board) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardCreated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board: board.clone(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_board_updated(room: &Arc<Room>, board: &Board) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board: board.clone(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_board_deleted(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::BoardDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_focused_board_changed(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::FocusedBoardChanged {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_excalidraw_delta(
    room: &Arc<Room>,
    board_id: &str,
    scene_version: u64,
    elements: &serde_json::Value,
    app_state: &serde_json::Value,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::ExcalidrawDelta {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        scene_version,
        elements: elements.clone(),
        app_state: app_state.clone(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(dead_code)]
pub(crate) fn broadcast_excalidraw_scene_reset(
    room: &Arc<Room>,
    board_id: &str,
    scene_version: u64,
    elements: &serde_json::Value,
    app_state: &serde_json::Value,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::ExcalidrawSceneReset {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        scene_version,
        elements: elements.clone(),
        app_state: app_state.clone(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(clippy::needless_borrow)]
pub(crate) fn broadcast_hands_updated(room: &Arc<Room>) {
    let seq = room.next_seq();
    let msg = ServerMsg::HandsUpdated {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        hands: room.hands_list(),
    };
    let _ = room.broadcast.send(msg);
}

#[allow(clippy::needless_borrow)]
pub(crate) fn broadcast_question_promoted_to_topic(
    room: &Arc<Room>,
    question_id: &str,
    topic: &Topic,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::QuestionPromotedToTopic {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        question_id: question_id.to_string(),
        topic: topic.clone(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_stroke_begun(
    room: &Arc<Room>,
    board_id: &str,
    stroke_id: &str,
    color: &str,
    size: f64,
    author_client_id: &str,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeBegun {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
        color: color.to_string(),
        size,
        author_client_id: author_client_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_stroke_appended(
    room: &Arc<Room>,
    board_id: &str,
    stroke_id: &str,
    points: Vec<[f32; 3]>,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeAppended {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
        points,
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_stroke_ended(room: &Arc<Room>, board_id: &str, stroke_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenStrokeEnded {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        stroke_id: stroke_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_text_upserted(room: &Arc<Room>, board_id: &str, text: &PenText) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenTextUpserted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        text: text.clone(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_text_deleted(room: &Arc<Room>, board_id: &str, text_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenTextDeleted {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        text_id: text_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_cleared(room: &Arc<Room>, board_id: &str) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenCleared {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_pen_undone(
    room: &Arc<Room>,
    board_id: &str,
    removed_stroke_id: Option<String>,
    removed_text_id: Option<String>,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::PenUndone {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        removed_stroke_id,
        removed_text_id,
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_cursor_moved(
    room: &Arc<Room>,
    board_id: &str,
    client_id: &str,
    guest_id: &str,
    display_name: &str,
    x: f64,
    y: f64,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::CursorMoved {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        client_id: client_id.to_string(),
        guest_id: guest_id.to_string(),
        display_name: display_name.to_string(),
        x,
        y,
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn broadcast_clicked(
    room: &Arc<Room>,
    board_id: &str,
    client_id: &str,
    guest_id: &str,
    display_name: &str,
    x: f64,
    y: f64,
) {
    let seq = room.next_seq();
    let msg = ServerMsg::Clicked {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        board_id: board_id.to_string(),
        client_id: client_id.to_string(),
        guest_id: guest_id.to_string(),
        display_name: display_name.to_string(),
        x,
        y,
    };
    let _ = room.broadcast.send(msg);
}

pub(crate) fn error_frame(
    code: &str,
    message: &str,
    ref_id: Option<String>,
    seq: u64,
) -> ServerMsg {
    ServerMsg::Error {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        code: code.to_string(),
        message: message.to_string(),
        ref_id,
    }
}

pub(crate) async fn send(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMsg,
) -> Result<(), String> {
    let s = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    sink.send(Message::Text(s)).await.map_err(|e| e.to_string())
}
