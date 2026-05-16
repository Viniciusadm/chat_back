use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, assert_chat_participant},
    error::{AppError, AppResult},
    realtime::{emit, emit_tx},
    state::AppState,
    utils::json_rows,
};

use super::dto::{MessageQuery, MessageRequest};

pub(super) async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Query(query): Query<MessageQuery>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let rows = sqlx::query(
        r#"
        SELECT *, type AS kind, reply_to_type AS reply_kind
        FROM messages
        WHERE chat_id = ? AND tenant_id = ? AND (? IS NULL OR created_at > ?)
        ORDER BY created_at ASC
        LIMIT ?
        "#,
    )
    .bind(chat_id)
    .bind(auth.tenant_id)
    .bind(query.after)
    .bind(query.after)
    .bind(query.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn create_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Json(req): Json<MessageRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    validate_message_payload(&req)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        r#"
        INSERT IGNORE INTO messages (
            id, chat_id, tenant_id, sender_member_id, type,
            ciphertext, iv, enc_version, audio_url, audio_duration,
            image_url, thumbnail_url, image_width, image_height, image_file_size,
            reply_to_message_id, reply_to_sender_id, reply_to_sender_name, reply_to_type, reply_to_preview
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(req.id)
    .bind(chat_id)
    .bind(auth.tenant_id)
    .bind(auth.member_id)
    .bind(&req.kind)
    .bind(&req.ciphertext)
    .bind(&req.iv)
    .bind(req.enc_version)
    .bind(&req.audio_url)
    .bind(req.audio_duration)
    .bind(&req.image_url)
    .bind(&req.thumbnail_url)
    .bind(req.image_width)
    .bind(req.image_height)
    .bind(req.image_file_size)
    .bind(req.reply_to_message_id)
    .bind(req.reply_to_sender_id)
    .bind(&req.reply_to_sender_name)
    .bind(&req.reply_to_type)
    .bind(&req.reply_to_preview)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE chats SET last_message_ciphertext = ?, last_message_iv = ?, last_message_type = ?, last_message_at = now(), updated_at = now() WHERE id = ?",
    )
    .bind(&req.ciphertext)
    .bind(&req.iv)
    .bind(&req.kind)
    .bind(chat_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE chat_read_state SET unread_count = unread_count + 1 WHERE chat_id = ? AND member_id <> ?",
    )
    .bind(chat_id)
    .bind(auth.member_id)
    .execute(&mut *tx)
    .await?;

    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "message.created",
        Some(chat_id),
        Some(req.id),
        json!({"sender_member_id": auth.member_id, "type": req.kind}),
    )
    .await?;
    tx.commit().await?;

    let push = Arc::clone(&state.push);
    let pool = state.pool.clone();
    let kind = req.kind.clone();
    let ciphertext = req.ciphertext.clone();
    let iv = req.iv.clone();
    let sender_member_id = auth.member_id;
    let tenant_id = auth.tenant_id;
    let message_id = req.id;
    tokio::spawn(async move {
        push.notify_message_created(
            pool,
            tenant_id,
            chat_id,
            message_id,
            sender_member_id,
            kind,
            ciphertext,
            iv,
        )
        .await;
    });

    Ok(Json(json!({ "id": req.id })))
}

fn validate_message_payload(req: &MessageRequest) -> AppResult<()> {
    match req.kind.as_str() {
        "text" if req.ciphertext.is_some() && req.iv.is_some() && req.enc_version.is_some() => {
            Ok(())
        }
        "audio" if req.audio_url.is_some() && req.audio_duration.is_some() => Ok(()),
        "image"
            if req.image_url.is_some()
                && req.image_width.is_some()
                && req.image_height.is_some() =>
        {
            Ok(())
        }
        "text" | "audio" | "image" => Err(AppError::BadRequest("invalid message payload".into())),
        _ => Err(AppError::BadRequest("invalid message type".into())),
    }
}

pub(super) async fn update_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((chat_id, message_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<MessageRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    validate_message_payload(&req)?;
    let affected = sqlx::query(
        r#"
        UPDATE messages
        SET ciphertext = ?, iv = ?, enc_version = ?, edited_at = now()
        WHERE id = ? AND chat_id = ? AND tenant_id = ? AND sender_member_id = ?
          AND created_at >= now() - interval '1 hour' AND is_deleted = false AND type = 'text'
        "#,
    )
    .bind(req.ciphertext)
    .bind(req.iv)
    .bind(req.enc_version)
    .bind(message_id)
    .bind(chat_id)
    .bind(auth.tenant_id)
    .bind(auth.member_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::Forbidden);
    }
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "message.updated",
        Some(chat_id),
        Some(message_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((chat_id, message_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let affected = sqlx::query(
        r#"
        UPDATE messages
        SET is_deleted = true, deleted_at = now(), ciphertext = NULL, iv = NULL
        WHERE id = ? AND chat_id = ? AND tenant_id = ? AND sender_member_id = ?
          AND created_at >= now() - interval '1 hour'
        "#,
    )
    .bind(message_id)
    .bind(chat_id)
    .bind(auth.tenant_id)
    .bind(auth.member_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::Forbidden);
    }
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "message.deleted",
        Some(chat_id),
        Some(message_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
