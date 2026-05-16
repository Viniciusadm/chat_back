use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Role;

#[derive(Serialize)]
pub(super) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserResponse,
}

#[derive(Serialize)]
pub(super) struct UserResponse {
    pub id: Uuid,
    pub member_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub role: Role,
    pub email: Option<String>,
    pub device_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub(super) struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    pub device_id: Option<Uuid>,
    pub push_token: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_id: Uuid,
    pub push_token: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ChildLoginRequest {
    pub code: String,
    pub device_id: Uuid,
    pub push_token: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RefreshRequest {
    pub refresh_token: String,
}
