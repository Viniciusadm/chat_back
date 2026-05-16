use std::{collections::HashMap, sync::Arc};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

const EXPO_URL: &str = "https://exp.host/--/api/v2/push/send";

#[derive(Clone)]
pub struct ExpoPush {
    client: Client,
    access_token: Option<String>,
}

impl ExpoPush {
    pub fn new(access_token: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("chat_back/0.1")
                .build()
                .expect("reqwest client"),
            access_token,
        }
    }

    pub async fn notify_message_created(
        self: Arc<Self>,
        pool: MySqlPool,
        tenant_id: Uuid,
        chat_id: Uuid,
        message_id: Uuid,
        sender_member_id: Uuid,
        message_kind: String,
        ciphertext: Option<String>,
        iv: Option<String>,
    ) {
        if let Err(err) = self
            .send(
                pool,
                tenant_id,
                chat_id,
                message_id,
                sender_member_id,
                message_kind,
                ciphertext,
                iv,
            )
            .await
        {
            tracing::warn!(?err, "expo push notification failed");
        }
    }

    async fn send(
        &self,
        pool: MySqlPool,
        tenant_id: Uuid,
        chat_id: Uuid,
        message_id: Uuid,
        sender_member_id: Uuid,
        message_kind: String,
        ciphertext: Option<String>,
        iv: Option<String>,
    ) -> anyhow::Result<()> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT d.push_token
            FROM chat_participants cp
            JOIN users u ON u.member_id = cp.member_id AND u.deleted_at IS NULL
            JOIN devices d ON d.user_id = u.id
            WHERE cp.chat_id = ?
              AND cp.member_id <> ?
              AND d.approved = TRUE
              AND d.active = TRUE
              AND d.push_token IS NOT NULL
            "#,
        )
        .bind(chat_id)
        .bind(sender_member_id)
        .fetch_all(&pool)
        .await?;

        let tokens: Vec<String> = rows
            .into_iter()
            .filter_map(|row| {
                row.try_get::<Option<String>, _>("push_token")
                    .ok()
                    .flatten()
            })
            .collect();

        if tokens.is_empty() {
            return Ok(());
        }

        let mut data = serde_json::Map::new();
        data.insert("chat_id".into(), json!(chat_id));
        data.insert("tenant_id".into(), json!(tenant_id));
        data.insert("message_id".into(), json!(message_id));
        data.insert("sender_id".into(), json!(sender_member_id));
        data.insert("type".into(), json!(message_kind));
        if message_kind == "text" {
            if let (Some(ct), Some(iv)) = (ciphertext.as_ref(), iv.as_ref()) {
                let preview_fits = ct.len() + iv.len() <= 3000;
                if preview_fits {
                    data.insert("ciphertext".into(), json!(ct));
                    data.insert("iv".into(), json!(iv));
                }
            }
        }

        let messages: Vec<Value> = tokens
            .iter()
            .map(|token| {
                json!({
                    "to": token,
                    "data": data,
                    "priority": "high",
                    "_contentAvailable": true,
                })
            })
            .collect();

        let mut request = self.client.post(EXPO_URL).json(&messages);
        if let Some(token) = &self.access_token {
            request = request.bearer_auth(token);
        }
        let resp = request.send().await?.error_for_status()?;
        let body: ExpoResponse = resp.json().await?;

        let mut invalid_tokens: Vec<String> = Vec::new();
        for (idx, ticket) in body.data.iter().enumerate() {
            if ticket.status.as_deref() == Some("error") {
                let code = ticket
                    .details
                    .as_ref()
                    .and_then(|d| d.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if code == "DeviceNotRegistered" || code == "InvalidCredentials" {
                    if let Some(token) = tokens.get(idx) {
                        invalid_tokens.push(token.clone());
                    }
                }
            }
        }

        if !invalid_tokens.is_empty() {
            let placeholders = vec!["?"; invalid_tokens.len()].join(",");
            let sql = format!(
                "UPDATE devices SET push_token = NULL WHERE push_token IN ({placeholders})"
            );
            let mut q = sqlx::query(&sql);
            for token in &invalid_tokens {
                q = q.bind(token);
            }
            q.execute(&pool).await?;
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
struct ExpoResponse {
    #[serde(default)]
    data: Vec<ExpoTicket>,
}

#[derive(Deserialize, Serialize)]
struct ExpoTicket {
    status: Option<String>,
    #[serde(default)]
    details: Option<HashMap<String, Value>>,
}
