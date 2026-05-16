use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub jwt_secret: String,
    pub access_token_ttl_minutes: i64,
    pub refresh_token_ttl_days: i64,
    pub media_root: PathBuf,
    pub media_public_base_url: String,
    pub redis_url: Option<String>,
    pub expo_access_token: Option<String>,
    pub child_login_max_attempts: u32,
    pub child_login_window_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "mysql://root:root@localhost:3306/chat_back".to_string()),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            access_token_ttl_minutes: env::var("ACCESS_TOKEN_TTL_MINUTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(15),
            refresh_token_ttl_days: env::var("REFRESH_TOKEN_TTL_DAYS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30),
            media_root: env::var("MEDIA_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./media")),
            media_public_base_url: env::var("MEDIA_PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000/static".to_string()),
            redis_url: env::var("REDIS_URL").ok(),
            expo_access_token: env::var("EXPO_ACCESS_TOKEN").ok(),
            child_login_max_attempts: env::var("CHILD_LOGIN_MAX_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5),
            child_login_window_secs: env::var("CHILD_LOGIN_WINDOW_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(60),
        }
    }
}
