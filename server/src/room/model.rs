use crate::proto::{Guest, Presence};

pub type ClientId = String;
pub type GuestId = String;
pub type TopicId = String;
pub type QuestionId = String;
pub type BoardId = String;
pub type StrokeId = String;
pub type TextId = String;

#[derive(Debug, Clone, PartialEq)]
pub struct PenStroke {
    pub id: StrokeId,
    pub color: String,
    pub size: f64,
    pub points: Vec<[f32; 3]>,
    pub ord: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PenBoardState {
    pub strokes: Vec<PenStroke>,
    pub texts: Vec<crate::proto::PenText>,
    pub action_log: Vec<PenAction>,
    pub next_stroke_ord: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PenActionKind {
    StrokeBegin,
    TextSet,
    TextDelete,
    Clear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PenAction {
    pub id: String,
    pub kind: PenActionKind,
    pub target_id: Option<String>,
    pub ord: u32,
    pub created_at: i64,
}

/// Outcome of a `pen_undo` call. `action_id` is the id of the popped
/// `pen_actions` row — used by the writer's `PenUndo` op to locate
/// the persisted state and apply the inverse. `removed_stroke` /
/// `removed_text` are populated only for the stroke_begin / text_set
/// undo paths where in-memory state shrinks; for text_delete / clear
/// the in-memory state stays empty until rehydration.
#[derive(Debug, Clone, PartialEq)]
pub struct PenUndoOutcome {
    pub action_id: String,
    pub removed_stroke: Option<StrokeId>,
    pub removed_text: Option<TextId>,
}

#[derive(Debug, Clone)]
pub struct PresenceEntry {
    pub guest_id: GuestId,
    pub display_name: String,
    pub muted: bool,
    pub joined_at: i64,
    pub client_ids: Vec<ClientId>,
}

impl PresenceEntry {
    pub fn to_proto_guest(&self) -> Guest {
        Guest {
            guest_id: self.guest_id.clone(),
            display_name: self.display_name.clone(),
            muted: self.muted,
            joined_at: self.joined_at,
        }
    }

    pub fn to_proto_presence(&self) -> Presence {
        Presence {
            guest_id: self.guest_id.clone(),
            display_name: self.display_name.clone(),
            muted: self.muted,
            joined_at: self.joined_at,
            client_ids: self.client_ids.clone(),
        }
    }
}
