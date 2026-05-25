use crate::api::now_ms;
use crate::intents::helpers::{
    ack_if_id, broadcast_hands_updated, ensure_host, ensure_not_muted, IntentError, SessionCtx,
};
use crate::proto::{error_codes, ClientMsg, Role};
use crate::rate_limit::Quota;
use crate::state::global_rate_limiter;

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::RaiseHand { id, topic, .. } => raise_hand(ctx, id, topic).await,
        ClientMsg::LowerHand { id, .. } => lower_hand(ctx, id).await,
        ClientMsg::CallOnHand {
            id,
            guest_id: target_guest_id,
            ..
        } => call_on_hand(ctx, id, target_guest_id).await,
        ClientMsg::DismissHand {
            id,
            guest_id: target_guest_id,
            ..
        } => dismiss_hand(ctx, id, target_guest_id).await,
        _ => unreachable!("non-raise-hand intent routed to raise-hand handler"),
    }
}

async fn raise_hand(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic: String,
) -> Result<(), IntentError> {
    if ctx.role != Role::Guest {
        return Err(client_error(
            ctx,
            error_codes::FORBIDDEN,
            "raise hand is guest-only",
            id.as_deref(),
        ));
    }
    ensure_not_muted(ctx, id.as_deref(), error_codes::FORBIDDEN, "muted")?;
    let topic = topic.trim().to_string();
    if topic.is_empty() || topic.len() > crate::validation::MAX_RAISE_HAND_TOPIC_LEN {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "topic must be 1..=80 chars",
            id.as_deref(),
        ));
    }
    let word_count = crate::validation::count_topic_words(&topic);
    if word_count > crate::validation::MAX_RAISE_HAND_TOPIC_WORDS {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "topic must be 10 words or fewer",
            id.as_deref(),
        ));
    }
    if !global_rate_limiter().check(ctx.client_id, "RaiseHand", Quota::per_minute(2.0)) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "too many raised hands, slow down",
            id.as_deref(),
        ));
    }
    let presence = ctx.room.presence();
    let display_name = presence
        .iter()
        .find(|p| p.guest_id == ctx.guest_id)
        .map(|p| p.display_name.clone())
        .unwrap_or_else(|| "Guest".to_string());
    let now = now_ms();
    ctx.room.raise_hand(ctx.guest_id, display_name, topic, now);
    broadcast_hands_updated(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn lower_hand(ctx: &mut SessionCtx<'_>, id: Option<String>) -> Result<(), IntentError> {
    ctx.room.lower_hand(ctx.guest_id);
    broadcast_hands_updated(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn call_on_hand(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    target_guest_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let _ = ctx.room.call_on_hand(&target_guest_id);
    broadcast_hands_updated(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn dismiss_hand(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    target_guest_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    ctx.room.dismiss_hand(&target_guest_id);
    broadcast_hands_updated(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
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
