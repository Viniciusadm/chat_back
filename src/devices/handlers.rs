use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, require_adult},
    error::{AppError, AppResult},
    realtime::emit_tx,
    state::AppState,
    utils::json_rows,
};

use super::dto::DeviceRequest;

pub(crate) async fn activate_device(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    tenant_id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    approved: bool,
    push_token: Option<String>,
    public_key: Option<String>,
) -> AppResult<()> {
    enforce_public_key_immutability(tx, device_id, public_key.as_deref()).await?;

    if approved {
        sqlx::query(
            "UPDATE devices SET active = false, deactivation_reason = 'new_active_device' WHERE user_id = ? AND id <> ?",
        )
        .bind(user_id)
        .bind(device_id)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO devices (id, tenant_id, user_id, approved, active, push_token, public_key, session_at, last_active_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, now(), now())
        ON DUPLICATE KEY UPDATE
            push_token = COALESCE(VALUES(push_token), devices.push_token),
            public_key = COALESCE(devices.public_key, VALUES(public_key)),
            session_at = now(),
            last_active_at = now(),
            approved = devices.approved OR VALUES(approved),
            active = CASE WHEN devices.approved OR VALUES(approved) THEN TRUE ELSE devices.active END,
            deactivation_reason = NULL
        "#,
    )
    .bind(device_id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(approved)
    .bind(push_token)
    .bind(public_key)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) async fn enforce_public_key_immutability(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    device_id: Uuid,
    incoming: Option<&str>,
) -> AppResult<()> {
    let Some(incoming) = incoming else {
        return Ok(());
    };
    let existing: Option<String> =
        sqlx::query_scalar("SELECT public_key FROM devices WHERE id = ?")
            .bind(device_id)
            .fetch_optional(&mut **tx)
            .await?
            .flatten();
    match existing {
        Some(current) if current != incoming => Err(AppError::Conflict(
            "device public_key is immutable once set".into(),
        )),
        _ => Ok(()),
    }
}

pub(super) async fn create_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<DeviceRequest>,
) -> AppResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    enforce_public_key_immutability(&mut tx, req.device_id, req.public_key.as_deref()).await?;
    sqlx::query(
        r#"
        INSERT INTO devices (id, tenant_id, user_id, approved, active, push_token, public_key)
        VALUES (?, ?, ?, FALSE, FALSE, ?, ?)
        ON DUPLICATE KEY UPDATE
            push_token = COALESCE(VALUES(push_token), devices.push_token),
            public_key = COALESCE(devices.public_key, VALUES(public_key)),
            last_active_at = now()
        "#,
    )
    .bind(req.device_id)
    .bind(auth.tenant_id)
    .bind(auth.user_id)
    .bind(req.push_token)
    .bind(req.public_key)
    .execute(&mut *tx)
    .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "device.updated",
        None,
        Some(req.device_id),
        json!({"user_id": auth.user_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": req.device_id })))
}

pub(super) async fn update_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
    Json(req): Json<DeviceRequest>,
) -> AppResult<Json<Value>> {
    if auth.device_id != Some(device_id) {
        return Err(AppError::Forbidden);
    }

    let mut tx = state.pool.begin().await?;
    enforce_public_key_immutability(&mut tx, device_id, req.public_key.as_deref()).await?;
    sqlx::query(
        r#"
        UPDATE devices SET
            push_token = COALESCE(?, push_token),
            public_key = COALESCE(public_key, ?),
            last_active_at = now(),
            session_at = now()
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(req.push_token)
    .bind(req.public_key)
    .bind(device_id)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "device.updated",
        None,
        Some(device_id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn heartbeat_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    if auth.device_id != Some(device_id) {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE devices SET last_active_at = now(), session_at = now() WHERE id = ? AND user_id = ?")
        .bind(device_id)
        .bind(auth.user_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn pending_devices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let rows = sqlx::query(
        r#"
        SELECT d.id, d.user_id, d.created_at, d.public_key, u.name, u.member_id
        FROM devices d
        JOIN users u ON u.id = d.user_id
        WHERE d.tenant_id = ? AND d.approved = false
        ORDER BY d.created_at DESC
        "#,
    )
    .bind(auth.tenant_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json_rows(rows)))
}

pub(super) async fn approve_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let row = sqlx::query("SELECT user_id FROM devices WHERE id = ? AND tenant_id = ?")
        .bind(device_id)
        .bind(auth.tenant_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound("device not found".into()))?;
    let user_id: Uuid = row.try_get("user_id")?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE devices SET active = false, deactivation_reason = 'new_active_device' WHERE user_id = ? AND id <> ?")
        .bind(user_id)
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE devices SET approved = true, active = true, deactivation_reason = NULL, session_at = now() WHERE id = ?")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "device.updated",
        None,
        Some(device_id),
        json!({"approved": true, "active": true}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn delete_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    require_adult(&auth)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE devices SET active = false, deactivation_reason = 'admin_removed' WHERE id = ? AND tenant_id = ?")
        .bind(device_id)
        .bind(auth.tenant_id)
        .execute(&mut *tx)
        .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        auth.tenant_id,
        "device.updated",
        None,
        Some(device_id),
        json!({"active": false, "reason": "admin_removed"}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
