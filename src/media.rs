use std::{io::Cursor, path::PathBuf};

use axum::extract::multipart::Field;
use image::ImageReader;
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, Debug)]
pub enum MediaKind {
    ProfilePhoto,
    GroupPhoto,
    MessageAudio,
    MessageImage,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfilePhoto => "profile",
            Self::GroupPhoto => "group",
            Self::MessageAudio => "audio",
            Self::MessageImage => "image",
        }
    }

    fn max_bytes(self) -> usize {
        match self {
            Self::ProfilePhoto | Self::GroupPhoto | Self::MessageImage => 5 * 1024 * 1024,
            Self::MessageAudio => 20 * 1024 * 1024,
        }
    }

    fn allows(self, content_type: &str) -> bool {
        let lc = content_type.to_ascii_lowercase();
        match self {
            Self::ProfilePhoto | Self::GroupPhoto | Self::MessageImage => matches!(
                lc.as_str(),
                "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
            ),
            Self::MessageAudio => {
                lc.starts_with("audio/")
                    && matches!(
                        lc.as_str(),
                        "audio/m4a"
                            | "audio/x-m4a"
                            | "audio/mp4"
                            | "audio/aac"
                            | "audio/mpeg"
                            | "audio/mp3"
                            | "audio/ogg"
                            | "audio/webm"
                    )
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalStorage {
    pub root: PathBuf,
    pub public_base_url: String,
}

#[derive(Debug)]
pub struct SavedMedia {
    pub storage_path: String,
    pub public_url: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

impl LocalStorage {
    pub fn new(root: PathBuf, public_base_url: String) -> Self {
        Self {
            root,
            public_base_url: public_base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn save(
        &self,
        kind: MediaKind,
        relative_dir: &str,
        file_stem: &str,
        field: Field<'_>,
    ) -> AppResult<SavedMedia> {
        let content_type = field
            .content_type()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        if !kind.allows(&content_type) {
            return Err(AppError::BadRequest(format!(
                "unsupported content-type {content_type} for {}",
                kind.as_str()
            )));
        }

        let original_name = field.file_name().map(|value| value.to_string());
        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::BadRequest(format!("invalid multipart payload: {err}")))?;
        let size_bytes = bytes.len();
        if size_bytes == 0 {
            return Err(AppError::BadRequest("empty upload".into()));
        }
        if size_bytes > kind.max_bytes() {
            return Err(AppError::BadRequest(format!(
                "file too large: {size_bytes} bytes (limit {})",
                kind.max_bytes()
            )));
        }

        let extension = pick_extension(&content_type, original_name.as_deref());
        let filename = format!("{file_stem}.{extension}");
        let dir = self.root.join(relative_dir);
        fs::create_dir_all(&dir)
            .await
            .map_err(|err| AppError::Internal(err.into()))?;

        let absolute = dir.join(&filename);
        let mut file = fs::File::create(&absolute)
            .await
            .map_err(|err| AppError::Internal(err.into()))?;
        file.write_all(&bytes)
            .await
            .map_err(|err| AppError::Internal(err.into()))?;
        file.flush()
            .await
            .map_err(|err| AppError::Internal(err.into()))?;

        let (width, height) = match kind {
            MediaKind::MessageImage | MediaKind::ProfilePhoto | MediaKind::GroupPhoto => {
                read_dimensions(&bytes)
                    .ok()
                    .map(|(w, h)| (Some(w), Some(h)))
                    .unwrap_or((None, None))
            }
            _ => (None, None),
        };

        let storage_path = format!("{relative_dir}/{filename}");
        let public_url = format!("{}/{}", self.public_base_url, storage_path);

        Ok(SavedMedia {
            storage_path,
            public_url,
            content_type,
            size_bytes: size_bytes as i64,
            width,
            height,
        })
    }

    pub async fn delete_relative(&self, storage_path: &str) {
        if storage_path.is_empty() {
            return;
        }
        let candidate = self.root.join(storage_path);
        let _ = fs::remove_file(candidate).await;
    }
}

fn pick_extension(content_type: &str, original: Option<&str>) -> String {
    if let Some(name) = original {
        if let Some((_, ext)) = name.rsplit_once('.') {
            let cleaned: String = ext
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(8)
                .collect::<String>()
                .to_ascii_lowercase();
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }
    match content_type.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg".into(),
        "image/png" => "png".into(),
        "image/webp" => "webp".into(),
        "audio/m4a" | "audio/mp4" => "m4a".into(),
        "audio/aac" => "aac".into(),
        "audio/mpeg" | "audio/mp3" => "mp3".into(),
        "audio/ogg" => "ogg".into(),
        "audio/webm" => "webm".into(),
        _ => "bin".into(),
    }
}

fn read_dimensions(bytes: &[u8]) -> anyhow::Result<(i32, i32)> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let dim = reader.into_dimensions()?;
    Ok((dim.0 as i32, dim.1 as i32))
}

pub fn profile_photo_dir(tenant_id: Uuid, member_id: Uuid) -> String {
    format!("profile_photos/{tenant_id}/{member_id}")
}

pub fn group_photo_dir(tenant_id: Uuid, chat_id: Uuid) -> String {
    format!("group_photos/{tenant_id}/{chat_id}")
}

pub fn audio_dir(tenant_id: Uuid, chat_id: Uuid) -> String {
    format!("audios/{tenant_id}/{chat_id}")
}

pub fn image_dir(tenant_id: Uuid, chat_id: Uuid) -> String {
    format!("images/{tenant_id}/{chat_id}")
}
