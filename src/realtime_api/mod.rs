use axum::{
    Router,
    routing::{any, get},
};

use crate::{realtime::realtime_handler, state::AppState};

mod handlers;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/realtime/events", get(handlers::list_realtime_events))
        .route("/realtime", any(realtime_handler))
}
