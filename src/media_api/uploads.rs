use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    media::{LocalStorage, MediaKind},
};

pub(super) async fn assert_owner_of_kind(
    pool: &sqlx::MySqlPool,
    chat_id: Uuid,
    message_id: Uuid,
    member_id: Uuid,
    expected_kind: &str,
) -> AppResult<()> {
    let row = sqlx::query(
        "SELECT type AS kind, sender_member_id FROM messages WHERE id = ? AND chat_id = ?",
    )
    .bind(message_id)
    .bind(chat_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound("message not found".into()))?;
    let kind: String = row.try_get("kind")?;
    let sender: Uuid = row.try_get("sender_member_id")?;
    if sender != member_id {
        return Err(AppError::Forbidden);
    }
    if kind != expected_kind {
        return Err(AppError::BadRequest(format!(
            "message is type {kind}, expected {expected_kind}"
        )));
    }
    Ok(())
}

pub(super) async fn insert_media_file(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    tenant_id: Uuid,
    owner_member_id: Uuid,
    chat_id: Option<Uuid>,
    message_id: Option<Uuid>,
    kind: MediaKind,
    saved: &crate::media::SavedMedia,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO media_files (
            id, tenant_id, owner_member_id, chat_id, message_id, kind,
            storage_path, public_url, content_type, size_bytes, width, height
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(owner_member_id)
    .bind(chat_id)
    .bind(message_id)
    .bind(kind.as_str())
    .bind(&saved.storage_path)
    .bind(&saved.public_url)
    .bind(&saved.content_type)
    .bind(saved.size_bytes)
    .bind(saved.width)
    .bind(saved.height)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn cleanup_old(storage: &LocalStorage, path: Option<&str>) {
    if let Some(path) = path {
        storage.delete_relative(path).await;
    }
}
