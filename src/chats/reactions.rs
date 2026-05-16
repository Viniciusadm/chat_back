use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, assert_chat_participant},
    error::AppResult,
    realtime::emit,
    state::AppState,
    utils::json_rows,
};

use super::dto::{MessageQuery, ReactionRequest};

pub(super) async fn list_reactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Query(query): Query<MessageQuery>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let rows = sqlx::query(
        "SELECT * FROM message_reactions WHERE chat_id = ? AND (? IS NULL OR updated_at > ?) ORDER BY updated_at DESC LIMIT ?",
    )
    .bind(chat_id)
    .bind(query.after)
    .bind(query.after)
    .bind(query.limit.unwrap_or(500).clamp(1, 1000))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn upsert_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((chat_id, message_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReactionRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    sqlx::query(
        r#"
        INSERT INTO message_reactions (message_id, member_id, chat_id, emoji)
        VALUES (?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE emoji = VALUES(emoji), updated_at = now()
        "#,
    )
    .bind(message_id)
    .bind(auth.member_id)
    .bind(chat_id)
    .bind(&req.emoji)
    .execute(&state.pool)
    .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "reaction.updated",
        Some(chat_id),
        Some(message_id),
        json!({"member_id": auth.member_id, "emoji": req.emoji}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((chat_id, message_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    sqlx::query("DELETE FROM message_reactions WHERE message_id = ? AND member_id = ?")
        .bind(message_id)
        .bind(auth.member_id)
        .execute(&state.pool)
        .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "reaction.deleted",
        Some(chat_id),
        Some(message_id),
        json!({"member_id": auth.member_id}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
