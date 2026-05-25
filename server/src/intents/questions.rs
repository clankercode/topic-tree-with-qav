use uuid::Uuid;

use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_question_added, broadcast_question_deleted,
    broadcast_question_promoted_to_topic, broadcast_question_updated, broadcast_topic_tree,
    broadcast_vote_updated, enqueue_write, ensure_host, ensure_not_muted, IntentError, SessionCtx,
};
use crate::proto::{error_codes, ClientMsg, Question, Role};
use crate::rate_limit::Quota;
use crate::state::global_rate_limiter;

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::SubmitQuestion {
            id,
            text,
            anonymous,
            ..
        } => submit_question(ctx, id, text, anonymous).await,
        ClientMsg::VoteQuestion {
            id,
            question_id,
            vote,
            ..
        } => vote_question(ctx, id, question_id, vote).await,
        ClientMsg::MarkQuestionAnswered {
            id,
            question_id,
            answered,
            ..
        } => mark_question_answered(ctx, id, question_id, answered).await,
        ClientMsg::DeleteQuestion {
            id, question_id, ..
        } => delete_question(ctx, id, question_id).await,
        ClientMsg::PromoteQuestionToTopic {
            id,
            question_id,
            parent_topic_id,
            after_topic_id,
            ..
        } => promote_question_to_topic(ctx, id, question_id, parent_topic_id, after_topic_id).await,
        _ => unreachable!("non-question intent routed to questions handler"),
    }
}

async fn submit_question(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    text: String,
    anonymous: bool,
) -> Result<(), IntentError> {
    if ctx.role != Role::Guest {
        return Err(client_error(
            ctx,
            error_codes::FORBIDDEN,
            "only guests can submit questions",
            id.as_deref(),
        ));
    }
    ensure_not_muted(
        ctx,
        id.as_deref(),
        error_codes::MUTED,
        "you are muted and cannot submit questions",
    )?;
    if !global_rate_limiter().check(ctx.client_id, "SubmitQuestion", Quota::per_minute(6.0)) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "too many questions, slow down",
            id.as_deref(),
        ));
    }
    let text = text.trim().to_string();
    if text.is_empty() || text.len() > 500 {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "question text must be 1..=500 chars",
            id.as_deref(),
        ));
    }
    let question_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let presence = ctx.room.presence();
    let author_name = presence
        .iter()
        .find(|p| p.guest_id == ctx.guest_id)
        .map(|p| p.display_name.clone())
        .unwrap_or_else(|| "Anonymous".to_string());
    let question = Question {
        id: question_id.clone(),
        room_id: ctx.room.id.clone(),
        author_guest_id: ctx.guest_id.to_string(),
        author_name,
        anonymous,
        text,
        answered: false,
        created_at: now,
        vote_count: 0,
    };
    ctx.room.add_question(question.clone());
    broadcast_question_added(ctx.room, &question);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::UpsertQuestion {
            question: question.clone(),
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn vote_question(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    question_id: String,
    vote: bool,
) -> Result<(), IntentError> {
    if ctx.role != Role::Guest {
        return Err(client_error(
            ctx,
            error_codes::FORBIDDEN,
            "only guests can vote",
            id.as_deref(),
        ));
    }
    ensure_not_muted(
        ctx,
        id.as_deref(),
        error_codes::MUTED,
        "you are muted and cannot vote",
    )?;
    if !global_rate_limiter().check(ctx.client_id, "VoteQuestion", Quota::per_minute(30.0)) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "too many votes, slow down",
            id.as_deref(),
        ));
    }
    let (count, changed) = match ctx.room.vote_question(&question_id, ctx.guest_id, vote) {
        Some(count_and_change) => count_and_change,
        None => {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "question not found",
                id.as_deref(),
            ));
        }
    };
    if changed {
        broadcast_vote_updated(ctx.room, &question_id, count, ctx.guest_id);
        if vote {
            enqueue_write(
                ctx.state,
                ctx.room,
                WriteOpKind::AddVote {
                    question_id: question_id.clone(),
                    guest_id: ctx.guest_id.to_string(),
                    created_at: now_ms(),
                },
            );
        } else {
            enqueue_write(
                ctx.state,
                ctx.room,
                WriteOpKind::RemoveVote {
                    question_id: question_id.clone(),
                    guest_id: ctx.guest_id.to_string(),
                },
            );
        }
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn mark_question_answered(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    question_id: String,
    answered: bool,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let mut question = match ctx.room.get_question(&question_id) {
        Some(question) => question,
        None => {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "question not found",
                id.as_deref(),
            ));
        }
    };
    question.answered = answered;
    if !ctx.room.update_question(question.clone()) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "question not found",
            id.as_deref(),
        ));
    }
    broadcast_question_updated(ctx.room, &question);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetQuestionAnswered {
            question_id: question_id.clone(),
            answered,
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn delete_question(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    question_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let removed = ctx.room.delete_question(&question_id);
    if removed {
        broadcast_question_deleted(ctx.room, &question_id);
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::DeleteQuestion {
                question_id: question_id.clone(),
            },
        );
    }
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn promote_question_to_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    question_id: String,
    parent_topic_id: Option<String>,
    after_topic_id: Option<String>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let Some((_question, topic)) =
        ctx.room
            .promote_question_to_topic(&question_id, parent_topic_id, after_topic_id)
    else {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "question not found",
            id.as_deref(),
        ));
    };
    broadcast_question_promoted_to_topic(ctx.room, &question_id, &topic);
    broadcast_topic_tree(ctx.room);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::PromoteQuestionToTopic {
            question_id: question_id.clone(),
            topic: topic.clone(),
        },
    );
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
