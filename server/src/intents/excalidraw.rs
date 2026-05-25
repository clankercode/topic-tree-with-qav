use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_board_created, broadcast_board_deleted, broadcast_board_updated,
    broadcast_excalidraw_delta, enqueue_write, ensure_host, IntentError, SessionCtx,
};
use crate::proto::{error_codes, Board, BoardKind, ClientMsg};
use crate::room::ExcalidrawUpdateOutcome;

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::CreateBoard {
            id, kind, title, ..
        } => create_board(ctx, id, kind, title).await,
        ClientMsg::RenameBoard {
            id,
            board_id,
            title,
            ..
        } => rename_board(ctx, id, board_id, title).await,
        ClientMsg::DeleteBoard { id, board_id, .. } => delete_board(ctx, id, board_id).await,
        ClientMsg::ExcalidrawUpdate {
            id,
            board_id,
            scene_version,
            elements,
            app_state,
            ..
        } => {
            let input = ExcalidrawUpdateInput {
                id,
                board_id,
                scene_version,
                elements,
                app_state,
            };
            excalidraw_update(ctx, input).await
        }
        _ => unreachable!("non-excalidraw intent routed to excalidraw handler"),
    }
}

async fn create_board(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    kind: BoardKind,
    title: Option<String>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let title = title.unwrap_or_else(|| "Untitled".into());
    if title.is_empty() || title.len() > 200 {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "title must be 1..=200 chars",
            id.as_deref(),
        ));
    }
    let board_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let ord = ctx
        .room
        .boards()
        .iter()
        .map(|board| board.ord)
        .fold(0.0, f64::max)
        + 1.0;
    let board = Board {
        id: board_id.clone(),
        kind,
        title,
        created_at: now,
        ord,
    };
    ctx.room.create_board(board.clone(), now);
    broadcast_board_created(ctx.room, &board);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::UpsertBoard {
            board: board.clone(),
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn rename_board(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
    title: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let title = title.trim().to_string();
    if title.is_empty() || title.len() > 200 {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "title must be 1..=200 chars",
            id.as_deref(),
        ));
    }
    match ctx.room.rename_board(&board_id, title.clone()) {
        Some(board) => {
            broadcast_board_updated(ctx.room, &board);
            enqueue_write(
                ctx.state,
                ctx.room,
                WriteOpKind::RenameBoard {
                    board_id: board_id.clone(),
                    title,
                },
            );
        }
        None => {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "board not found",
                id.as_deref(),
            ));
        }
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn delete_board(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    board_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if !ctx.room.delete_board(&board_id) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "board not found",
            id.as_deref(),
        ));
    }
    broadcast_board_deleted(ctx.room, &board_id);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::DeleteBoard {
            board_id: board_id.clone(),
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

struct ExcalidrawUpdateInput {
    id: Option<String>,
    board_id: String,
    scene_version: u64,
    elements: JsonValue,
    app_state: JsonValue,
}

async fn excalidraw_update(
    ctx: &mut SessionCtx<'_>,
    input: ExcalidrawUpdateInput,
) -> Result<(), IntentError> {
    ensure_host(ctx, input.id.as_deref())?;
    let now = now_ms();
    match ctx.room.update_excalidraw_scene(
        &input.board_id,
        input.scene_version,
        input.elements.clone(),
        input.app_state.clone(),
        now,
    ) {
        ExcalidrawUpdateOutcome::Applied => {
            broadcast_excalidraw_delta(
                ctx.room,
                &input.board_id,
                input.scene_version,
                &input.elements,
                &input.app_state,
            );
            enqueue_write(
                ctx.state,
                ctx.room,
                WriteOpKind::UpsertExcalidrawScene {
                    board_id: input.board_id.clone(),
                    scene_version: input.scene_version,
                    elements_json: serde_json::to_string(&input.elements)
                        .unwrap_or_else(|_| "[]".into()),
                    app_state_json: serde_json::to_string(&input.app_state)
                        .unwrap_or_else(|_| "{}".into()),
                    updated_at: now,
                },
            );
        }
        ExcalidrawUpdateOutcome::Stale => {}
        ExcalidrawUpdateOutcome::BoardMissing => {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "board not found or not an excalidraw board",
                input.id.as_deref(),
            ));
        }
    }
    ack_if_id(ctx, input.id.as_deref()).await;
    Ok(())
}

fn client_error(
    ctx: &SessionCtx<'_>,
    code: &str,
    message: impl Into<String>,
    ref_id: Option<&str>,
) -> IntentError {
    IntentError::client(code, message, ref_id, ctx.room.current_seq())
}
