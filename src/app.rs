use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(crate::auth_api::routes())
        .merge(crate::devices::routes())
        .merge(crate::members::routes())
        .merge(crate::chats::routes())
        .merge(crate::media_api::routes())
        .merge(crate::crypto_api::routes())
        .merge(crate::realtime_api::routes())
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}
