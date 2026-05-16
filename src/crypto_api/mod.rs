use axum::{
    Router,
    routing::{get, put},
};

use crate::state::AppState;

mod dto;
mod handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/crypto/password-settings",
            get(handlers::get_password_settings)
                .put(handlers::put_password_settings)
                .delete(handlers::delete_password_settings),
        )
        .route(
            "/crypto/key-backups",
            get(handlers::list_key_backups).delete(handlers::delete_key_backups),
        )
        .route(
            "/crypto/key-backups/{chat_id}",
            put(handlers::put_key_backup),
        )
        .route(
            "/devices/{device_id}/key-shares",
            get(handlers::list_key_shares),
        )
        .route(
            "/devices/{device_id}/key-shares/{chat_id}",
            put(handlers::put_key_share),
        )
}
