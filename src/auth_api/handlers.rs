use axum::{Json, extract::State};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, Role, hash_password, hash_refresh_token, rate_limit, verify_password},
    devices::activate_device,
    error::{AppError, AppResult},
    realtime::{emit, emit_tx},
    state::AppState,
};

use super::dto::{
    ChildLoginRequest, LoginRequest, RefreshRequest, RegisterRequest, TokenResponse, UserResponse,
};
use super::tokens::issue_tokens;

pub(super) async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<TokenResponse>> {
    if req.email.trim().is_empty() || req.password.len() < 8 || req.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "email, name and password >= 8 are required".into(),
        ));
    }

    let email = req.email.trim().to_lowercase();
    let email_exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM users WHERE lower(email) = lower(?) AND deleted_at IS NULL LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    if email_exists {
        return Err(AppError::Conflict("email already registered".into()));
    }

    let mut tx = state.pool.begin().await?;
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(format!("Família {}", req.name.trim()))
        .execute(&mut *tx)
        .await?;

    let member_id = Uuid::new_v4();
    sqlx::query("INSERT INTO members (id, tenant_id, name, role) VALUES (?, ?, ?, 'adult')")
        .bind(member_id)
        .bind(tenant_id)
        .bind(req.name.trim())
        .execute(&mut *tx)
        .await?;

    let password_hash = hash_password(&req.password)?;
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (id, member_id, tenant_id, email, password_hash, name, role)
        VALUES (?, ?, ?, ?, ?, ?, 'adult')
        "#,
    )
    .bind(user_id)
    .bind(member_id)
    .bind(tenant_id)
    .bind(&email)
    .bind(password_hash)
    .bind(req.name.trim())
    .execute(&mut *tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(db_error) if db_error.code().as_deref() == Some("1062") => {
            AppError::Conflict("email already registered".into())
        }
        other => AppError::Database(other),
    })?;

    sqlx::query("UPDATE tenants SET owner_user_id = ? WHERE id = ?")
        .bind(user_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

    let device_id = req.device_id.unwrap_or_else(Uuid::new_v4);
    activate_device(
        &mut tx,
        tenant_id,
        user_id,
        device_id,
        true,
        req.push_token,
        req.public_key,
    )
    .await?;

    tx.commit().await?;

    issue_tokens(
        &state,
        AuthUser {
            user_id,
            member_id,
            tenant_id,
            device_id: Some(device_id),
            role: Role::Adult,
        },
        Some(email),
        req.name,
    )
    .await
}

pub(super) async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    let row = sqlx::query(
        r#"
        SELECT id, member_id, tenant_id, password_hash, name, role AS role, email
        FROM users
        WHERE lower(email) = lower(?) AND deleted_at IS NULL
        "#,
    )
    .bind(req.email.trim())
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let Some(password_hash) = row.try_get::<Option<String>, _>("password_hash")? else {
        return Err(AppError::Unauthorized);
    };
    if !verify_password(&req.password, &password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let tenant_id: Uuid = row.try_get("tenant_id")?;
    let user_id: Uuid = row.try_get("id")?;
    let member_id: Uuid = row.try_get("member_id")?;

    let mut tx = state.pool.begin().await?;
    activate_device(
        &mut tx,
        tenant_id,
        user_id,
        req.device_id,
        true,
        req.push_token,
        req.public_key,
    )
    .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        tenant_id,
        "device.updated",
        None,
        Some(req.device_id),
        json!({"user_id": user_id, "active": true}),
    )
    .await?;
    tx.commit().await?;

    let role_str: String = row.try_get("role")?;
    issue_tokens(
        &state,
        AuthUser {
            user_id,
            member_id,
            tenant_id,
            device_id: Some(req.device_id),
            role: Role::try_from(role_str.as_str())?,
        },
        row.try_get("email")?,
        row.try_get("name")?,
    )
    .await
}

pub(super) async fn child_login(
    State(state): State<AppState>,
    Json(req): Json<ChildLoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    let code = req.code.trim().to_string();
    let key = format!("child_login:{}:{}", req.device_id, &code);
    rate_limit(
        state.redis.as_ref(),
        &key,
        state.config.child_login_max_attempts,
        state.config.child_login_window_secs,
    )
    .await?;

    let row = sqlx::query(
        r#"
        SELECT lc.member_id, lc.tenant_id, m.name
        FROM login_codes lc
        JOIN members m ON m.id = lc.member_id
        WHERE lc.code = ? AND lc.revoked_at IS NULL AND m.role = 'child'
        "#,
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let mut tx = state.pool.begin().await?;
    let tenant_id: Uuid = row.try_get("tenant_id")?;
    let member_id: Uuid = row.try_get("member_id")?;
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (id, member_id, tenant_id, name, role)
        VALUES (?, ?, ?, ?, 'child')
        "#,
    )
    .bind(user_id)
    .bind(member_id)
    .bind(tenant_id)
    .bind(row.try_get::<String, _>("name")?)
    .execute(&mut *tx)
    .await?;

    activate_device(
        &mut tx,
        tenant_id,
        user_id,
        req.device_id,
        false,
        req.push_token,
        req.public_key,
    )
    .await?;
    emit_tx(
        &mut tx,
        &state.hub,
        tenant_id,
        "device.updated",
        None,
        Some(req.device_id),
        json!({"user_id": user_id, "approved": false}),
    )
    .await?;
    tx.commit().await?;

    issue_tokens(
        &state,
        AuthUser {
            user_id,
            member_id,
            tenant_id,
            device_id: Some(req.device_id),
            role: Role::Child,
        },
        None,
        row.try_get("name")?,
    )
    .await
}

pub(super) async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> AppResult<Json<TokenResponse>> {
    let token_hash = hash_refresh_token(&req.refresh_token);
    let row = sqlx::query(
        r#"
        SELECT u.id, u.member_id, u.tenant_id, u.name, u.email, u.role AS role, rt.device_id
        FROM refresh_tokens rt
        JOIN users u ON u.id = rt.user_id
        LEFT JOIN devices d ON d.id = rt.device_id
        WHERE rt.token_hash = ?
          AND rt.revoked_at IS NULL
          AND rt.expires_at > now()
          AND u.deleted_at IS NULL
          AND (rt.device_id IS NULL OR (d.approved = true AND d.active = true))
        "#,
    )
    .bind(token_hash)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE token_hash = ?")
        .bind(hash_refresh_token(&req.refresh_token))
        .execute(&state.pool)
        .await?;

    let role_str: String = row.try_get("role")?;
    issue_tokens(
        &state,
        AuthUser {
            user_id: row.try_get("id")?,
            member_id: row.try_get("member_id")?,
            tenant_id: row.try_get("tenant_id")?,
            device_id: row.try_get("device_id")?,
            role: Role::try_from(role_str.as_str())?,
        },
        row.try_get("email")?,
        row.try_get("name")?,
    )
    .await
}

pub(super) async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<Value>> {
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = ? AND revoked_at IS NULL",
    )
    .bind(auth.user_id)
    .execute(&state.pool)
    .await?;
    if let Some(device_id) = auth.device_id {
        sqlx::query("UPDATE devices SET active = false, deactivation_reason = 'logout' WHERE id = ? AND user_id = ?")
            .bind(device_id)
            .bind(auth.user_id)
            .execute(&state.pool)
            .await?;
        emit(
            &state.pool,
            &state.hub,
            auth.tenant_id,
            "device.updated",
            None,
            Some(device_id),
            json!({"active": false, "reason": "logout"}),
        )
        .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<UserResponse>> {
    let row = sqlx::query(
        "SELECT id, member_id, tenant_id, name, email, role AS role FROM users WHERE id = ?",
    )
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await?;
    let role: String = row.try_get("role")?;

    Ok(Json(UserResponse {
        id: row.try_get("id")?,
        member_id: row.try_get("member_id")?,
        tenant_id: row.try_get("tenant_id")?,
        name: row.try_get("name")?,
        role: Role::try_from(role.as_str())?,
        email: row.try_get("email")?,
        device_id: auth.device_id,
    }))
}
