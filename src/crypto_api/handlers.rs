use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, Role, assert_chat_participant, require_adult},
    error::{AppError, AppResult},
    state::AppState,
    utils::{json_rows, row_to_json},
};

use super::dto::{KeyBackupRequest, KeyShareRequest, PasswordSettingsRequest};

pub(super) async fn get_password_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    let row = sqlx::query("SELECT password_salt, password_verifier_ciphertext, password_verifier_iv FROM users WHERE id = ?")
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(row_to_json(row)))
}

pub(super) async fn put_password_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PasswordSettingsRequest>,
) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE users SET password_salt = ?, password_verifier_ciphertext = ?, password_verifier_iv = ? WHERE id = ?")
        .bind(req.password_salt)
        .bind(req.password_verifier_ciphertext)
        .bind(req.password_verifier_iv)
        .bind(auth.user_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_password_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE users SET password_salt = NULL, password_verifier_ciphertext = NULL, password_verifier_iv = NULL WHERE id = ?")
        .bind(auth.user_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn list_key_backups(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM key_backups WHERE user_id = ?")
        .bind(auth.user_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn put_key_backup(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(chat_id): Path<Uuid>,
    Json(req): Json<KeyBackupRequest>,
) -> AppResult<Json<Value>> {
    assert_chat_participant(&state.pool, chat_id, auth.member_id, auth.tenant_id).await?;
    sqlx::query(
        r#"
        INSERT INTO key_backups (user_id, chat_id, ciphertext, iv, enc_version)
        VALUES (?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE ciphertext = VALUES(ciphertext), iv = VALUES(iv), enc_version = VALUES(enc_version), created_at = now()
        "#,
    )
    .bind(auth.user_id)
    .bind(chat_id)
    .bind(req.ciphertext)
    .bind(req.iv)
    .bind(req.enc_version)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_key_backups(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM key_backups WHERE user_id = ?")
        .bind(auth.user_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn list_key_shares(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    if auth.device_id != Some(device_id) && auth.role != Role::Adult {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query("SELECT * FROM key_shares WHERE device_id = ? ORDER BY created_at DESC")
        .bind(device_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn put_key_share(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((device_id, chat_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<KeyShareRequest>,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let device_belongs = sqlx::query("SELECT 1 FROM devices WHERE id = ? AND tenant_id = ?")
        .bind(device_id)
        .bind(auth.tenant_id)
        .fetch_optional(&state.pool)
        .await?
        .is_some();
    if !device_belongs {
        return Err(AppError::NotFound("device not found".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO key_shares (device_id, chat_id, ephemeral_public_key, iv, ciphertext, wrapped_by_member_id)
        VALUES (?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            ephemeral_public_key = VALUES(ephemeral_public_key),
            iv = VALUES(iv),
            ciphertext = VALUES(ciphertext),
            wrapped_by_member_id = VALUES(wrapped_by_member_id),
            created_at = now()
        "#,
    )
    .bind(device_id)
    .bind(chat_id)
    .bind(req.ephemeral_public_key)
    .bind(req.iv)
    .bind(req.ciphertext)
    .bind(auth.member_id)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
