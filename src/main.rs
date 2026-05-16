mod app;
mod auth;
mod auth_api;
mod chats;
mod config;
mod crypto_api;
mod devices;
mod error;
mod media;
mod media_api;
mod members;
mod push;
mod realtime;
mod realtime_api;
mod state;
mod utils;

use std::net::SocketAddr;

use anyhow::Context;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "chat_back=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let bind_addr: SocketAddr = config
        .bind_addr
        .parse()
        .with_context(|| format!("invalid BIND_ADDR {}", config.bind_addr))?;

    let state = AppState::connect(config).await?;
    sqlx::migrate!("./migrations").run(&state.pool).await?;

    let static_service = ServeDir::new(state.config.media_root.clone());

    let app = app::router(state.clone())
        .nest_service("/static", static_service)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("listening on http://{bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
