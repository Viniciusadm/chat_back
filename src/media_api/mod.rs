use axum::{Router, routing::post};

use crate::state::AppState;

mod handlers;
mod uploads;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/media/profile-photo",
            post(handlers::upload_profile_photo).delete(handlers::delete_profile_photo),
        )
        .route(
            "/chats/{chat_id}/photo/upload",
            post(handlers::upload_chat_photo),
        )
        .route(
            "/chats/{chat_id}/messages/{message_id}/audio",
            post(handlers::upload_message_audio),
        )
        .route(
            "/chats/{chat_id}/messages/{message_id}/image",
            post(handlers::upload_message_image),
        )
}
