use axum::{
    Router,
    routing::{delete, get, patch, post},
};

use crate::state::AppState;

mod dto;
pub mod handlers;

pub(crate) use handlers::activate_device;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/devices", post(handlers::create_device))
        .route("/devices/{device_id}", patch(handlers::update_device))
        .route(
            "/devices/{device_id}/heartbeat",
            post(handlers::heartbeat_device),
        )
        .route("/admin/devices/pending", get(handlers::pending_devices))
        .route(
            "/admin/devices/{device_id}/approve",
            post(handlers::approve_device),
        )
        .route(
            "/admin/devices/{device_id}",
            delete(handlers::delete_device),
        )
}
