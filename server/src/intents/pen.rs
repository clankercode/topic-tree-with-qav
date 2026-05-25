use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_pen_cleared, broadcast_pen_stroke_appended, broadcast_pen_stroke_begun,
    broadcast_pen_stroke_ended, broadcast_pen_text_deleted, broadcast_pen_text_upserted,
    broadcast_pen_undone, enqueue_write, ensure_host, IntentError, SessionCtx,
};
use crate::proto::{ClientMsg, PenStrokeSummary, PenText};
use crate::rate_limit::Quota;
use crate::state::global_rate_limiter;

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::PenStrokeBegin {
            id,
            board_id,
            stroke_id,
            color,
            size,
            ..
        } => pen_stroke_begin(ctx, id, board_id, stroke_id, color, size).await,
        ClientMsg::PenStrokeAppend {
            id,
            board_id,
            stroke_id,
            points,
            ..
        } => pen_stroke_append(ctx, id, board_id, stroke_id, points).await,
        ClientMsg::PenStrokeEnd {
            id,
            board_id,
            stroke_id,
            ..
        } => pen_stroke_end(ctx, id, board_id, stroke_id).await,
        ClientMsg::PenTextSet {
            id,
            board_id,
            text_id,
            x,
            y,
            text,
            font_size,
            color,
            ..
        } => {
            let input = PenTextSetInput {
                id,
                board_id,
                text_id,
                x,
                y,
                text,
                font_size,
                color,
            };
            pen_text_set(ctx, input).await
        }
        ClientMsg::PenTextDelete {
            id,
            board_id,
            text_id,
            ..
        } => pen_text_delete(ctx, id, board_id, text_id).await,
        ClientMsg::PenClear { id, board_id, .. } => pen_clear(ctx, id, board_id).await,
        ClientMsg::PenUndo { id, board_id, .. } => pen_undo(ctx, id, board_id).await,
        _ => unreachable!("non-pen intent routed to pen handler"),
    }
}

async fn pen_stroke_begin(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    stroke_id: String,
    color: String,
    size: f64,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let now = now_ms();
    if ctx
        .room
        .pen_begin_stroke(&board_id, stroke_id.clone(), color.clone(), size, now)
        .is_some()
    {
        broadcast_pen_stroke_begun(ctx.room, &board_id, &stroke_id, &color, size, ctx.client_id);
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn pen_stroke_append(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    stroke_id: String,
    points: Vec<[f32; 3]>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if !global_rate_limiter().check(ctx.client_id, "PenStrokeAppend", Quota::per_second(60.0)) {
        return Ok(());
    }
    if ctx
        .room
        .pen_append_points(&board_id, &stroke_id, points.clone())
    {
        broadcast_pen_stroke_appended(ctx.room, &board_id, &stroke_id, points);
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn pen_stroke_end(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    stroke_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if let Some((summary, action_id)) = ctx.room.pen_end_stroke(&board_id, &stroke_id) {
        broadcast_pen_stroke_ended(ctx.room, &board_id, &stroke_id);
        let created_at = summary.created_at;
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::InsertCompletedPenStroke {
                board_id: board_id.clone(),
                stroke: summary,
                action_id,
                created_at,
            },
        );
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

struct PenTextSetInput {
    id: Option<String>,
    board_id: String,
    text_id: String,
    x: f64,
    y: f64,
    text: String,
    font_size: f64,
    color: String,
}

async fn pen_text_set(ctx: &mut SessionCtx<'_>, input: PenTextSetInput) -> Result<(), IntentError> {
    ensure_host(ctx, input.id.as_deref())?;
    let now = now_ms();
    let pen_text = PenText {
        id: input.text_id.clone(),
        x: input.x,
        y: input.y,
        text: input.text.clone(),
        font_size: input.font_size,
        color: input.color.clone(),
        updated_at: now,
    };
    if let Some((action_id, prior)) =
        ctx.room
            .pen_text_upsert(&input.board_id, pen_text.clone(), now)
    {
        broadcast_pen_text_upserted(ctx.room, &input.board_id, &pen_text);
        let before_json = prior.as_ref().and_then(|p| serde_json::to_string(p).ok());
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::UpsertPenText {
                board_id: input.board_id.clone(),
                text: pen_text,
                action_id,
                before_json,
                created_at: now,
            },
        );
    }
    ack_if_id(ctx, input.id.as_deref()).await;
    Ok(())
}

async fn pen_text_delete(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    text_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let now = now_ms();
    if let Some((action_id, removed)) = ctx.room.pen_text_delete(&board_id, &text_id, now) {
        broadcast_pen_text_deleted(ctx.room, &board_id, &text_id);
        let before_json = serde_json::to_string(&removed).unwrap_or_else(|_| "null".to_string());
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::DeletePenText {
                board_id: board_id.clone(),
                text_id: text_id.clone(),
                action_id,
                before_json,
                created_at: now,
            },
        );
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn pen_clear(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let now = now_ms();
    if let Some((action_id, prior_strokes, prior_texts)) = ctx.room.pen_clear(&board_id, now) {
        broadcast_pen_cleared(ctx.room, &board_id);
        let prior_stroke_summaries: Vec<PenStrokeSummary> = prior_strokes
            .into_iter()
            .map(|stroke| PenStrokeSummary {
                id: stroke.id,
                color: stroke.color,
                size: stroke.size,
                points: stroke.points,
                created_at: stroke.created_at,
                ord: stroke.ord,
            })
            .collect();
        let before_strokes_json =
            serde_json::to_string(&prior_stroke_summaries).unwrap_or_else(|_| "[]".to_string());
        let before_texts_json =
            serde_json::to_string(&prior_texts).unwrap_or_else(|_| "[]".to_string());
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::PenClear {
                board_id: board_id.clone(),
                action_id,
                before_strokes_json,
                before_texts_json,
                created_at: now,
            },
        );
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn pen_undo(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if let Some(outcome) = ctx.room.pen_undo(&board_id) {
        broadcast_pen_undone(
            ctx.room,
            &board_id,
            outcome.removed_stroke.clone(),
            outcome.removed_text.clone(),
        );
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::PenUndo {
                board_id: board_id.clone(),
                target_action_id: outcome.action_id,
            },
        );
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}
