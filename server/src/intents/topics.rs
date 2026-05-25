use uuid::Uuid;

use crate::api::now_ms;
use crate::db::WriteOpKind;
use crate::intents::helpers::{
    ack_if_id, broadcast_topic_tree, enqueue_write, ensure_host, IntentError, SessionCtx,
};
use crate::proto::{error_codes, ClientMsg, ImportedTopicNode, Topic, TopicStatus};
use crate::rate_limit::Quota;
use crate::state::global_rate_limiter;

const IMPORT_TOPIC_TREE_QUOTA: Quota = Quota {
    // One immediate import, then one refill every ten seconds (6/min).
    capacity: 1.0,
    refill_per_sec: 0.1,
};

pub(crate) async fn handle(ctx: &mut SessionCtx<'_>, msg: ClientMsg) -> Result<(), IntentError> {
    match msg {
        ClientMsg::AddTopic {
            id,
            parent_id,
            title,
            after_id,
            ..
        } => add_topic(ctx, id, parent_id, title, after_id).await,
        ClientMsg::RenameTopic {
            id,
            topic_id,
            title,
            ..
        } => rename_topic(ctx, id, topic_id, title).await,
        ClientMsg::MoveTopic {
            id,
            topic_id,
            new_parent_id,
            after_id,
            ..
        } => move_topic(ctx, id, topic_id, new_parent_id, after_id).await,
        ClientMsg::DeleteTopic { id, topic_id, .. } => delete_topic(ctx, id, topic_id).await,
        ClientMsg::SetActiveTopic { id, topic_id, .. } => set_active_topic(ctx, id, topic_id).await,
        ClientMsg::MarkTopicDone {
            id, topic_id, done, ..
        } => mark_topic_done(ctx, id, topic_id, done).await,
        ClientMsg::ImportTopicTree {
            id,
            parent_topic_id,
            topics,
            ..
        } => import_topic_tree(ctx, id, parent_topic_id, topics).await,
        _ => unreachable!("non-topic intent routed to topics handler"),
    }
}

async fn add_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    parent_id: Option<String>,
    title: String,
    after_id: Option<String>,
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
    let known_topics = ctx.room.topics();
    if let Some(parent_id) = parent_id.as_ref() {
        if !topic_exists(&known_topics, parent_id) {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "parent_id refers to a topic that no longer exists",
                id.as_deref(),
            ));
        }
    }
    if let Some(after_id) = after_id.as_ref() {
        if !topic_exists(&known_topics, after_id) {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "after_id refers to a topic that no longer exists",
                id.as_deref(),
            ));
        }
    }
    let topic_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let ord = if let Some(after) = after_id {
        known_topics
            .iter()
            .find(|t| t.id == after)
            .map(|t| t.ord + 0.5)
            .unwrap_or(1.0)
    } else {
        known_topics.iter().map(|t| t.ord).fold(0.0, f64::max) + 1.0
    };
    let topic = Topic {
        id: topic_id,
        parent_id,
        title,
        ord,
        status: TopicStatus::Pending,
        created_at: now,
    };
    ctx.room.add_topic(topic.clone());
    broadcast_topic_tree(ctx.room);
    enqueue_write(ctx.state, ctx.room, WriteOpKind::UpsertTopic { topic });
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn rename_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic_id: String,
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
    if !ctx.room.rename_topic(&topic_id, title.clone()) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "topic not found",
            id.as_deref(),
        ));
    }
    broadcast_topic_tree(ctx.room);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::RenameTopic {
            topic_id: topic_id.clone(),
            title,
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn move_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic_id: String,
    new_parent_id: Option<String>,
    after_id: Option<String>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let known_topics = ctx.room.topics();
    if let Some(parent_id) = new_parent_id.as_ref() {
        if !topic_exists(&known_topics, parent_id) {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "new_parent_id refers to a topic that no longer exists",
                id.as_deref(),
            ));
        }
    }
    if let Some(after_id) = after_id.as_ref() {
        if !topic_exists(&known_topics, after_id) {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "after_id refers to a topic that no longer exists",
                id.as_deref(),
            ));
        }
    }
    let ord = if let Some(after) = after_id {
        known_topics
            .iter()
            .find(|t| t.id == *after)
            .map(|t| t.ord + 0.001)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    if !ctx.room.move_topic(&topic_id, new_parent_id.clone(), ord) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "topic not found",
            id.as_deref(),
        ));
    }
    broadcast_topic_tree(ctx.room);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::MoveTopic {
            topic_id: topic_id.clone(),
            parent_id: new_parent_id,
            ord,
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn delete_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic_id: String,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if ctx.room.delete_topic(&topic_id) {
        enqueue_write(
            ctx.state,
            ctx.room,
            WriteOpKind::DeleteTopic {
                topic_id: topic_id.clone(),
            },
        );
    }
    broadcast_topic_tree(ctx.room);
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn set_active_topic(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic_id: Option<String>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    let prev_active = ctx.room.active_topic_id();
    ctx.room.set_active_topic(topic_id.clone());
    broadcast_topic_tree(ctx.room);
    if let Some(prev) = prev_active.as_deref() {
        let should_mark_done = match &topic_id {
            Some(new) => prev != new.as_str(),
            None => true,
        };
        if should_mark_done {
            enqueue_write(
                ctx.state,
                ctx.room,
                WriteOpKind::SetTopicStatus {
                    topic_id: prev.to_string(),
                    status: TopicStatus::Done,
                },
            );
        }
    }
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetActiveTopic {
            topic_id: topic_id.clone(),
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn mark_topic_done(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    topic_id: String,
    done: bool,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if !ctx.room.mark_topic_done(&topic_id, done) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            "topic not found",
            id.as_deref(),
        ));
    }
    broadcast_topic_tree(ctx.room);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::SetTopicStatus {
            topic_id: topic_id.clone(),
            status: if done {
                TopicStatus::Done
            } else {
                TopicStatus::Pending
            },
        },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

async fn import_topic_tree(
    ctx: &mut SessionCtx<'_>,
    id: Option<String>,
    parent_topic_id: Option<String>,
    imported: Vec<ImportedTopicNode>,
) -> Result<(), IntentError> {
    ensure_host(ctx, id.as_deref())?;
    if !global_rate_limiter().check(ctx.client_id, "ImportTopicTree", IMPORT_TOPIC_TREE_QUOTA) {
        return Err(client_error(
            ctx,
            error_codes::RATE_LIMIT,
            "too many imports, slow down",
            id.as_deref(),
        ));
    }
    if let Err(e) = crate::validation::validate_imported_topics(&imported) {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            e.to_string(),
            id.as_deref(),
        ));
    }
    let known_topics = ctx.room.topics();
    if let Some(parent_id) = parent_topic_id.as_ref() {
        if !topic_exists(&known_topics, parent_id) {
            return Err(client_error(
                ctx,
                error_codes::BAD_REQUEST,
                "parent_topic_id refers to a topic that no longer exists",
                id.as_deref(),
            ));
        }
    }
    let now = now_ms();
    let base_ord = if parent_topic_id.is_some() {
        known_topics
            .iter()
            .filter(|t| t.parent_id == parent_topic_id)
            .map(|t| t.ord)
            .fold(0.0f64, f64::max)
            + 1.0
    } else {
        known_topics.iter().map(|t| t.ord).fold(0.0f64, f64::max) + 1.0
    };
    let mut flat: Vec<Topic> = Vec::new();
    flatten(&imported, parent_topic_id.clone(), base_ord, now, &mut flat);
    if known_topics.len().saturating_add(flat.len()) > crate::validation::MAX_TOPICS_PER_ROOM {
        return Err(client_error(
            ctx,
            error_codes::BAD_REQUEST,
            format!(
                "room topic limit exceeded; max is {}",
                crate::validation::MAX_TOPICS_PER_ROOM
            ),
            id.as_deref(),
        ));
    }
    ctx.room.add_topics_bulk(flat.clone());
    broadcast_topic_tree(ctx.room);
    enqueue_write(
        ctx.state,
        ctx.room,
        WriteOpKind::BulkUpsertTopics { topics: flat },
    );
    ack_if_id(ctx, id.as_deref()).await;
    Ok(())
}

fn flatten(
    nodes: &[ImportedTopicNode],
    parent_id: Option<String>,
    depth_base_ord: f64,
    now: i64,
    out: &mut Vec<Topic>,
) {
    let mut ord = depth_base_ord;
    for node in nodes {
        let id = Uuid::new_v4().to_string();
        out.push(Topic {
            id: id.clone(),
            parent_id: parent_id.clone(),
            title: node.title.trim().to_string(),
            ord,
            status: node.status,
            created_at: now,
        });
        flatten(&node.children, Some(id), 1.0, now, out);
        ord += 1.0;
    }
}

fn topic_exists(topics: &[Topic], topic_id: &str) -> bool {
    topics.iter().any(|topic| topic.id == topic_id)
}

fn client_error(
    ctx: &SessionCtx<'_>,
    code: &str,
    message: impl Into<String>,
    ref_id: Option<&str>,
) -> IntentError {
    IntentError::client(code, message, ref_id, ctx.room.current_seq())
}
