use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, header, request::Parts},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Adult,
    Child,
}

impl Role {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Adult => "adult",
            Self::Child => "child",
        }
    }
}

impl TryFrom<&str> for Role {
    type Error = AppError;

    fn try_from(value: &str) -> AppResult<Self> {
        match value {
            "adult" => Ok(Self::Adult),
            "child" => Ok(Self::Child),
            _ => Err(AppError::Internal(anyhow::anyhow!("invalid role {value}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub user_id: Uuid,
    pub member_id: Uuid,
    pub tenant_id: Uuid,
    pub device_id: Option<Uuid>,
    pub role: Role,
    pub exp: usize,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub member_id: Uuid,
    pub tenant_id: Uuid,
    pub device_id: Option<Uuid>,
    pub role: Role,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error.to_string())))
}

pub fn verify_password(password: &str, password_hash: &str) -> AppResult<bool> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error.to_string())))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn hash_refresh_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub fn encode_access_token(state: &AppState, user: &AuthUser) -> AppResult<String> {
    let exp = Utc::now() + Duration::minutes(state.config.access_token_ttl_minutes);
    let claims = Claims {
        sub: user.user_id,
        user_id: user.user_id,
        member_id: user.member_id,
        tenant_id: user.tenant_id,
        device_id: user.device_id,
        role: user.role,
        exp: exp.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub async fn assert_chat_participant(
    pool: &MySqlPool,
    chat_id: Uuid,
    member_id: Uuid,
    tenant_id: Uuid,
) -> AppResult<()> {
    let exists = sqlx::query(
        r#"
        SELECT 1
        FROM chats c
        JOIN chat_participants cp ON cp.chat_id = c.id
        WHERE c.id = ? AND c.tenant_id = ? AND cp.member_id = ?
        "#,
    )
    .bind(chat_id)
    .bind(tenant_id)
    .bind(member_id)
    .fetch_optional(pool)
    .await?
    .is_some();

    if exists {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn rate_limit(
    redis: Option<&redis::Client>,
    key: &str,
    max_attempts: u32,
    window_secs: u64,
) -> AppResult<()> {
    let Some(client) = redis else {
        return Ok(());
    };
    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(err) => {
            tracing::warn!(?err, "rate limit redis unavailable, allowing");
            return Ok(());
        }
    };
    let count: i64 = match redis::cmd("INCR").arg(key).query_async(&mut conn).await {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(?err, "rate limit incr failed, allowing");
            return Ok(());
        }
    };
    if count == 1 {
        let _: Result<(), _> = redis::cmd("EXPIRE")
            .arg(key)
            .arg(window_secs)
            .query_async(&mut conn)
            .await;
    }
    if count as u32 > max_attempts {
        return Err(AppError::TooManyRequests);
    }
    Ok(())
}

pub fn require_adult(user: &AuthUser) -> AppResult<()> {
    if user.role == Role::Adult {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token"))?;

        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(app_state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid bearer token"))?
        .claims;

        let user = sqlx::query(
            r#"
            SELECT u.deleted_at, d.approved, d.active, d.deactivation_reason AS reason
            FROM users u
            LEFT JOIN devices d ON d.id = ? AND d.user_id = u.id
            WHERE u.id = ? AND u.tenant_id = ? AND u.member_id = ?
            "#,
        )
        .bind(claims.device_id)
        .bind(claims.user_id)
        .bind(claims.tenant_id)
        .bind(claims.member_id)
        .fetch_optional(&app_state.pool)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid session"))?
        .ok_or((StatusCode::UNAUTHORIZED, "invalid session"))?;

        if user
            .try_get::<Option<chrono::DateTime<Utc>>, _>("deleted_at")
            .ok()
            .flatten()
            .is_some()
        {
            return Err((StatusCode::UNAUTHORIZED, "user deleted"));
        }

        if claims.device_id.is_some() {
            let approved = user
                .try_get::<Option<bool>, _>("approved")
                .ok()
                .flatten()
                .unwrap_or(false);
            let active = user
                .try_get::<Option<bool>, _>("active")
                .ok()
                .flatten()
                .unwrap_or(false);

            if !approved || !active {
                return Err((StatusCode::FORBIDDEN, "device is not approved and active"));
            }
        }

        Ok(Self {
            user_id: claims.user_id,
            member_id: claims.member_id,
            tenant_id: claims.tenant_id,
            device_id: claims.device_id,
            role: claims.role,
        })
    }
}
