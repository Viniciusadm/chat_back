use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, assert_chat_participant},
    error::{AppError, AppResult},
    realtime::{emit, emit_tx},
    state::AppState,
    utils::row_to_json,
};

use super::dto::{ChatRequest, ReadRequest};

pub(super) async fn list_chats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query(
        r#"
        SELECT c.*
        FROM chats c
        JOIN chat_participants cp ON cp.chat_id = c.id
        WHERE c.tenant_id = ? AND cp.member_id = ?
        ORDER BY c.updated_at DESC
        "#,
    )
    .bind(auth.tenant_id)
    .bind(auth.member_id)
    .fetch_all(&state.pool)
    .await?;
    let mut chats = Vec::with_capacity(rows.len());
    for row in rows {
        let chat_id: Uuid = row.try_get("id")?;
        chats.push(enrich_chat(&state, row, chat_id).await?);
    }
    Ok(Json(Value::Array(chats)))
}

pub(super) async fn get_chat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let row = sqlx::query("SELECT * FROM chats WHERE id = ? AND tenant_id = ?")
        .bind(chat_id)
        .bind(auth.tenant_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(enrich_chat(&state, row, chat_id).await?))
}

async fn enrich_chat(
    state: &AppState,
    row: sqlx::mysql::MySqlRow,
    chat_id: Uuid,
) -> AppResult<Value> {
    let mut value = match row_to_json(row) {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };

    let participants = sqlx::query("SELECT member_id FROM chat_participants WHERE chat_id = ?")
        .bind(chat_id)
        .fetch_all(&state.pool)
        .await?;
    value.insert(
        "participant_ids".into(),
        Value::Array(
            participants
                .into_iter()
                .filter_map(|row| row.try_get::<Uuid, _>("member_id").ok())
                .map(|id| json!(id))
                .collect(),
        ),
    );

    let read_rows = sqlx::query(
        "SELECT member_id, read_up_to, unread_count FROM chat_read_state WHERE chat_id = ?",
    )
    .bind(chat_id)
    .fetch_all(&state.pool)
    .await?;
    let mut read_up_to = serde_json::Map::new();
    let mut unread_by = serde_json::Map::new();
    for row in read_rows {
        let member_id: Uuid = row.try_get("member_id")?;
        if let Ok(Some(read_at)) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("read_up_to") {
            read_up_to.insert(member_id.to_string(), json!(read_at));
        }
        let unread_count: i32 = row.try_get("unread_count")?;
        unread_by.insert(member_id.to_string(), json!(unread_count));
    }
    value.insert("read_up_to".into(), Value::Object(read_up_to));
    value.insert("unread_by".into(), Value::Object(unread_by));

    Ok(Value::Object(value))
}

pub(super) async fn create_chat(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ChatRequest>,
) -> AppResult<Json<Value>> {
    let is_group = req.is_group.unwrap_or(true);
    let mut participants = req.participant_ids.unwrap_or_default();
    if !participants.contains(&auth.member_id) {
        participants.push(auth.member_id);
    }
    participants.sort_unstable();
    participants.dedup();

    if is_group && (req.name.as_deref().unwrap_or("").trim().is_empty() || participants.len() < 3) {
        return Err(AppError::BadRequest(
            "group chats require name and at least 3 participants".into(),
        ));
    }

    let mut tx = state.pool.begin().await?;
    let chat_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chats (id, tenant_id, is_group, name) VALUES (?, ?, ?, ?)")
        .bind(chat_id)
        .bind(auth.tenant_id)
        .bind(is_group)
        .bind(req.name.unwrap_or_default())
        .execute(&mut *tx)
        .await?;

    for participant in participants {
        let belongs = sqlx::query("SELECT 1 FROM members WHERE id = ? AND tenant_id = ?")
            .bind(participant)
            .bind(auth.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if !belongs {
            return Err(AppError::BadRequest(
                "participant does not belong to tenant".into(),
            ));
        }
        sqlx::query("INSERT INTO chat_participants (chat_id, member_id) VALUES (?, ?)")
            .bind(chat_id)
            .bind(participant)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO chat_read_state (chat_id, member_id) VALUES (?, ?)")
            .bind(chat_id)
            .bind(participant)
            .execute(&mut *tx)
            .await?;
    }
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "chat.created",
        Some(chat_id),
        Some(chat_id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": chat_id })))
}

pub(super) async fn update_chat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE chats SET name = COALESCE(?, name), updated_at = now() WHERE id = ? AND tenant_id = ?")
        .bind(req.name)
        .bind(chat_id)
        .bind(auth.tenant_id)
        .execute(&mut *tx)
        .await?;
    if let Some(mut participants) = req.participant_ids {
        if !participants.contains(&auth.member_id) {
            participants.push(auth.member_id);
        }
        participants.sort_unstable();
        participants.dedup();
        if participants.len() < 3 {
            return Err(AppError::BadRequest(
                "group chats require at least 3 participants".into(),
            ));
        }
        sqlx::query("DELETE FROM chat_participants WHERE chat_id = ?")
            .bind(chat_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM chat_read_state WHERE chat_id = ?")
            .bind(chat_id)
            .execute(&mut *tx)
            .await?;
        for participant in participants {
            let belongs = sqlx::query("SELECT 1 FROM members WHERE id = ? AND tenant_id = ?")
                .bind(participant)
                .bind(auth.tenant_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
            if !belongs {
                return Err(AppError::BadRequest(
                    "participant does not belong to tenant".into(),
                ));
            }
            sqlx::query("INSERT INTO chat_participants (chat_id, member_id) VALUES (?, ?)")
                .bind(chat_id)
                .bind(participant)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO chat_read_state (chat_id, member_id) VALUES (?, ?)")
                .bind(chat_id)
                .bind(participant)
                .execute(&mut *tx)
                .await?;
        }
    }
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "chat.updated",
        Some(chat_id),
        Some(chat_id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_chat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let mut tx = state.pool.begin().await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "chat.deleted",
        Some(chat_id),
        Some(chat_id),
        json!({}),
    )
    .await?;
    sqlx::query("DELETE FROM chats WHERE id = ? AND tenant_id = ?")
        .bind(chat_id)
        .bind(auth.tenant_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn update_chat_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    sqlx::query("UPDATE chats SET photo_url = ?, photo_path = ?, updated_at = now() WHERE id = ? AND tenant_id = ?")
        .bind(req.photo_url)
        .bind(req.photo_path)
        .bind(chat_id)
        .bind(auth.tenant_id)
        .execute(&state.pool)
        .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "chat.updated",
        Some(chat_id),
        Some(chat_id),
        json!({"photo": true}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_chat_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    let previous: Option<String> =
        sqlx::query_scalar("SELECT photo_path FROM chats WHERE id = ? AND tenant_id = ?")
            .bind(chat_id)
            .bind(auth.tenant_id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    sqlx::query("UPDATE chats SET photo_url = NULL, photo_path = NULL, updated_at = now() WHERE id = ? AND tenant_id = ?")
        .bind(chat_id)
        .bind(auth.tenant_id)
        .execute(&state.pool)
        .await?;
    if let Some(path) = previous {
        state.storage.delete_relative(&path).await;
    }
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "chat.updated",
        Some(chat_id),
        Some(chat_id),
        json!({"photo": false}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn mark_chat_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Json(req): Json<ReadRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    sqlx::query("UPDATE chat_read_state SET read_up_to = COALESCE(?, now()), unread_count = 0 WHERE chat_id = ? AND member_id = ?")
        .bind(req.read_up_to)
        .bind(chat_id)
        .bind(auth.member_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
