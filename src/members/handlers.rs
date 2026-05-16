use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, Role, require_adult},
    error::{AppError, AppResult},
    realtime::{emit, emit_tx},
    state::AppState,
    utils::json_rows,
};

use super::dto::MemberRequest;

pub(super) async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT id, name, role AS role, login_code, photo_url, photo_path, created_at FROM members WHERE tenant_id = ? ORDER BY created_at",
    )
    .bind(auth.tenant_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn create_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<MemberRequest>,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let name = req.name.as_deref().unwrap_or("").trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let role = req.role.unwrap_or(Role::Child);
    let login_code = if role == Role::Child {
        Some(Uuid::new_v4().to_string()[..8].to_uppercase())
    } else {
        None
    };

    let mut tx = state.pool.begin().await?;
    let member_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO members (id, tenant_id, name, role, login_code) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(member_id)
    .bind(auth.tenant_id)
    .bind(name)
    .bind(role.as_db())
    .bind(&login_code)
    .execute(&mut *tx)
    .await?;

    if let Some(code) = &login_code {
        sqlx::query(
            "INSERT INTO login_codes (code, member_id, tenant_id, name_snapshot, role_snapshot) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(code)
        .bind(member_id)
        .bind(auth.tenant_id)
        .bind(name)
        .bind(role.as_db())
        .execute(&mut *tx)
        .await?;
    }

    let new_chat_ids = create_direct_chats_for_member(&mut tx, auth.tenant_id, member_id).await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "member.updated",
        None,
        Some(member_id),
        json!({"created": true}),
    )
    .await?;
    for chat_id in &new_chat_ids {
        emit_tx(
            &mut tx,
            &state.hub,
            auth.tenant_id,
            "chat.created",
            Some(*chat_id),
            Some(*chat_id),
            json!({}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "id": member_id, "login_code": login_code })))
}

async fn create_direct_chats_for_member(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    tenant_id: Uuid,
    member_id: Uuid,
) -> AppResult<Vec<Uuid>> {
    let rows = sqlx::query("SELECT id FROM members WHERE tenant_id = ? AND id <> ?")
        .bind(tenant_id)
        .bind(member_id)
        .fetch_all(&mut **tx)
        .await?;

    let mut new_chat_ids = Vec::new();
    for row in rows {
        let other_member_id: Uuid = row.try_get("id")?;
        let chat_id = Uuid::new_v4();
        sqlx::query("INSERT INTO chats (id, tenant_id, is_group, name) VALUES (?, ?, FALSE, '')")
            .bind(chat_id)
            .bind(tenant_id)
            .execute(&mut **tx)
            .await?;
        for participant in [member_id, other_member_id] {
            sqlx::query("INSERT INTO chat_participants (chat_id, member_id) VALUES (?, ?)")
                .bind(chat_id)
                .bind(participant)
                .execute(&mut **tx)
                .await?;
            sqlx::query("INSERT INTO chat_read_state (chat_id, member_id) VALUES (?, ?)")
                .bind(chat_id)
                .bind(participant)
                .execute(&mut **tx)
                .await?;
        }
        new_chat_ids.push(chat_id);
    }
    Ok(new_chat_ids)
}

pub(super) async fn update_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(member_id): Path<Uuid>,
    Json(req): Json<MemberRequest>,
) -> AppResult<Json<Value>> {
    if auth.member_id != member_id {
        require_adult(&auth)?;
    }
    sqlx::query("UPDATE members SET name = COALESCE(?, name) WHERE id = ? AND tenant_id = ?")
        .bind(req.name)
        .bind(member_id)
        .bind(auth.tenant_id)
        .execute(&state.pool)
        .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "member.updated",
        None,
        Some(member_id),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(member_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let mut tx = state.pool.begin().await?;
    let user_ids = sqlx::query("SELECT id FROM users WHERE member_id = ? AND tenant_id = ?")
        .bind(member_id)
        .bind(auth.tenant_id)
        .fetch_all(&mut *tx)
        .await?;
    for row in user_ids {
        let user_id: Uuid = row.try_get("id")?;
        sqlx::query("UPDATE devices SET active = false, deactivation_reason = 'account_deleted' WHERE user_id = ?")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE users SET deleted_at = now() WHERE member_id = ? AND tenant_id = ?")
        .bind(member_id)
        .bind(auth.tenant_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE login_codes SET revoked_at = now() WHERE member_id = ?")
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "member.updated",
        None,
        Some(member_id),
        json!({"deleted": true}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn update_member_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(member_id): Path<Uuid>,
    Json(req): Json<MemberRequest>,
) -> AppResult<Json<Value>> {
    if auth.member_id != member_id {
        require_adult(&auth)?;
    }
    sqlx::query("UPDATE members SET photo_url = ?, photo_path = ? WHERE id = ? AND tenant_id = ?")
        .bind(req.photo_url)
        .bind(req.photo_path)
        .bind(member_id)
        .bind(auth.tenant_id)
        .execute(&state.pool)
        .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "member.updated",
        None,
        Some(member_id),
        json!({"photo": true}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_member_photo(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(member_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    if auth.member_id != member_id {
        require_adult(&auth)?;
    }
    sqlx::query(
        "UPDATE members SET photo_url = NULL, photo_path = NULL WHERE id = ? AND tenant_id = ?",
    )
    .bind(member_id)
    .bind(auth.tenant_id)
    .execute(&state.pool)
    .await?;
    emit(
        &state.pool,
        &state.hub,
        auth.tenant_id,
        "member.updated",
        None,
        Some(member_id),
        json!({"photo": false}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}
