use axum::Json;
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, encode_access_token, hash_refresh_token},
    error::AppResult,
    state::AppState,
};

use super::dto::{TokenResponse, UserResponse};

pub(super) async fn issue_tokens(
    state: &AppState,
    auth: AuthUser,
    email: Option<String>,
    name: String,
) -> AppResult<Json<TokenResponse>> {
    let access_token = encode_access_token(state, &auth)?;
    let refresh_token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::days(state.config.refresh_token_ttl_days);

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, device_id, token_hash, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(auth.user_id)
    .bind(auth.device_id)
    .bind(hash_refresh_token(&refresh_token))
    .bind(expires_at)
    .execute(&state.pool)
    .await?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        user: UserResponse {
            id: auth.user_id,
            member_id: auth.member_id,
            tenant_id: auth.tenant_id,
            name,
            role: auth.role,
            email,
            device_id: auth.device_id,
        },
    }))
}
