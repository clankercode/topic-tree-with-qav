use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_presence, enqueue_write, ensure_host, IntentError, SessionCtx,
};
use crate::proto::{ClientMsg, ServerMsg, PROTOCOL_VERSION};

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::KickGuest {
            id,
            guest_id: target_guest_id,
            ..
        } => kick_guest(ctx, id, target_guest_id).await,
        ClientMsg::MuteGuest {
            id,
            guest_id: target_guest_id,
            muted,
            ..
        } => mute_guest(ctx, id, target_guest_id, muted).await,
        _ => unreachable!("non-moderation intent routed to moderation handler"),
    }
}

async fn kick_guest(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    target_guest_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetKicked {
            guest_id: target_guest_id.clone(),
            kicked: true,
            updated_at: now_ms(),
        },
    );
    ctx.room.kick_guest(&target_guest_id);
    let seq = ctx.room.next_seq();
    let kick_notice = ServerMsg::KickNotice {
        v: PROTOCOL_VERSION,
        ts: now_ms(),
        seq,
        guest_id: target_guest_id.clone(),
    };
    let _ = ctx.room.broadcast.send(kick_notice);
    broadcast_presence(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn mute_guest(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    target_guest_id: String,
    muted: bool,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetMuted {
            guest_id: target_guest_id.clone(),
            muted,
            updated_at: now_ms(),
        },
    );
    ctx.room.set_muted(&target_guest_id, muted);
    broadcast_presence(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}
