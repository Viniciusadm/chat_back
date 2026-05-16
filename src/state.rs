use std::sync::Arc;

use sqlx::{MySqlPool, mysql::MySqlPoolOptions};

use crate::{config::Config, media::LocalStorage, push::ExpoPush, realtime::Hub};

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub config: Config,
    pub storage: Arc<LocalStorage>,
    pub hub: Arc<Hub>,
    pub push: Arc<ExpoPush>,
    pub redis: Option<redis::Client>,
}

impl AppState {
    pub async fn connect(config: Config) -> anyhow::Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        let storage = Arc::new(LocalStorage::new(
            config.media_root.clone(),
            config.media_public_base_url.clone(),
        ));
        tokio::fs::create_dir_all(&config.media_root).await.ok();

        let push = Arc::new(ExpoPush::new(config.expo_access_token.clone()));
        let hub = Arc::new(Hub::new());

        let redis = match &config.redis_url {
            Some(url) => match redis::Client::open(url.as_str()) {
                Ok(client) => Some(client),
                Err(err) => {
                    tracing::warn!(?err, "failed to open redis client; rate limit disabled");
                    None
                }
            },
            None => None,
        };

        Ok(Self {
            pool,
            config,
            storage,
            hub,
            push,
            redis,
        })
    }
}
