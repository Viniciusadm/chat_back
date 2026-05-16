use axum::{
    Router,
    routing::{get, patch, post, put},
};

use crate::state::AppState;

mod dto;
mod handlers;
mod messages;
mod reactions;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/chats",
            get(handlers::list_chats).post(handlers::create_chat),
        )
        .route(
            "/chats/{chat_id}",
            get(handlers::get_chat)
                .patch(handlers::update_chat)
                .delete(handlers::delete_chat),
        )
        .route(
            "/chats/{chat_id}/photo",
            post(handlers::update_chat_photo).delete(handlers::delete_chat_photo),
        )
        .route("/chats/{chat_id}/read", post(handlers::mark_chat_read))
        .route(
            "/chats/{chat_id}/messages",
            get(messages::list_messages).post(messages::create_message),
        )
        .route(
            "/chats/{chat_id}/messages/{message_id}",
            patch(messages::update_message).delete(messages::delete_message),
        )
        .route("/chats/{chat_id}/reactions", get(reactions::list_reactions))
        .route(
            "/chats/{chat_id}/messages/{message_id}/reaction",
            put(reactions::upsert_reaction).delete(reactions::delete_reaction),
        )
}
