use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub(super) struct ChatRequest {
    pub name: Option<String>,
    pub is_group: Option<bool>,
    pub participant_ids: Option<Vec<Uuid>>,
    pub photo_url: Option<String>,
    pub photo_path: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ReadRequest {
    pub read_up_to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub(super) struct MessageQuery {
    pub after: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub(super) struct MessageRequest {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
    pub ciphertext: Option<String>,
    pub iv: Option<String>,
    pub enc_version: Option<i32>,
    pub audio_url: Option<String>,
    pub audio_duration: Option<i32>,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
    pub image_file_size: Option<i64>,
    pub reply_to_message_id: Option<Uuid>,
    pub reply_to_sender_id: Option<Uuid>,
    pub reply_to_sender_name: Option<String>,
    pub reply_to_type: Option<String>,
    pub reply_to_preview: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ReactionRequest {
    pub emoji: String,
}
