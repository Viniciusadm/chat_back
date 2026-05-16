use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

mod dto;
mod handlers;
mod tokens;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(handlers::register))
        .route("/auth/login", post(handlers::login))
        .route("/auth/child-login", post(handlers::child_login))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/me", get(handlers::me))
}
