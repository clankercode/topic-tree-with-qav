use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_clicked, broadcast_cursor_moved, broadcast_focused_board_changed,
    broadcast_presence, enqueue_write, ensure_host, send, IntentError, SessionCtx,
};
use crate::proto::{error_codes, ClientMsg, ServerMsg, You, PROTOCOL_VERSION};
use crate::rate_limit::Quota;
use crate::state::global_rate_limiter;

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::SetDisplayName { id, name, .. } => set_display_name(ctx, id, name).await,
        ClientMsg::GetSnapshot { id, .. } => get_snapshot(ctx, id).await,
        ClientMsg::SetFocusedBoard { id, board_id, .. } => {
            set_focused_board(ctx, id, board_id).await
        }
        ClientMsg::Cursor {
            id, board_id, x, y, ..
        } => cursor(ctx, id, board_id, x, y).await,
        ClientMsg::Click {
            id, board_id, x, y, ..
        } => click(ctx, id, board_id, x, y).await,
        _ => unreachable!("non-presence intent routed to presence handler"),
    }
}

async fn set_display_name(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    name: String,
) -> Result<(), IntentError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "name must be 1..=64 chars",
            id.as_deref(),
        ));
    }
    if ctx.room.set_display_name(ctx.guest_id, trimmed) {
        broadcast_presence(ctx.room);
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn get_snapshot(ctx: &mut SessionCtx<'_>, id: Option<String>) -> Result<(), IntentError> {
    // Without this gate a guest can spam GetSnapshot and force
    // the server to render the room's full state on each call.
    // The 5/sec cap mirrors the "all others" 20 msg/s catch-all
    // budget in protocol.md §rate-limits but is tighter because
    // each snapshot is much more expensive than a typical intent.
    if !global_rate_limiter().check(ctx.client_id, "GetSnapshot", Quota::per_second(5.0)) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "snapshot rate exceeded",
            id.as_deref(),
        ));
    }
    let snap = ctx.room.snapshot_for(
        You {
            client_id: ctx.client_id.to_string(),
            role: ctx.role,
            guest_id: ctx.guest_id.to_string(),
        },
        ctx.guest_id,
    );
    let msg = ServerMsg::RoomSnapshot {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq: ctx.room.current_seq(),
        snapshot: snap,
    };
    send(ctx.sink, &msg).await.map_err(IntentError::io)?;
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn set_focused_board(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if !ctx.room.board_exists(&board_id) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "board not found",
            id.as_deref(),
        ));
    }
    ctx.room.set_focused_board(board_id.clone());
    broadcast_focused_board_changed(ctx.room, &board_id);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetFocusedBoard {
            board_id: Some(board_id.clone()),
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn cursor(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    x: f64,
    y: f64,
) -> Result<(), IntentError> {
    if !global_rate_limiter().check(ctx.client_id, "Cursor", Quota::per_second(30.0)) {
        return Ok(());
    }
    if !ctx.room.board_exists(&board_id) {
        return Ok(());
    }
    let display_name = current_display_name(ctx);
    broadcast_cursor_moved(
        ctx.room,
        &board_id,
        ctx.client_id,
        ctx.guest_id,
        &display_name,
        x,
        y,
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn click(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    x: f64,
    y: f64,
) -> Result<(), IntentError> {
    if !global_rate_limiter().check(ctx.client_id, "Click", Quota::per_second(5.0)) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "click rate exceeded",
            id.as_deref(),
        ));
    }
    if !ctx.room.board_exists(&board_id) {
        return Ok(());
    }
    let display_name = current_display_name(ctx);
    broadcast_clicked(
        ctx.room,
        &board_id,
        ctx.client_id,
        ctx.guest_id,
        &display_name,
        x,
        y,
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

fn current_display_name(ctx: &SessionCtx<'_>) -> String {
    ctx.room
        .presence()
        .iter()
        .find(|presence| presence.guest_id == ctx.guest_id)
        .map(|presence| presence.display_name.clone())
        .unwrap_or_default()
}

fn client_error(
    ctx: &SessionCtx<'_>,
    code: &str,
    message: impl Into<String>,
    ref_id: Option<&str>,
) -> IntentError {
    IntentError::client(code, message, ref_id, ctx.room.current_seq())
}
