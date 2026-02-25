mod api;
mod config;
mod error;
mod state;
mod store;
mod sync_engine;

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;
use crate::store::sqlite::SqliteStore;
use crate::sync_engine::SyncEngine;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config_path = std::env::args().nth(1);
    let config = Config::load(config_path.as_deref())?;

    tracing::info!(
        bind = %config.server.bind,
        server_name = %config.server.server_name,
        db_backend = %config.database.backend,
        "starting agora-server"
    );

    let db_uri = if config.database.uri.starts_with("sqlite:") {
        config.database.uri.clone()
    } else {
        format!("sqlite:{}?mode=rwc", config.database.uri)
    };

    let store = SqliteStore::open(&db_uri).await?;

    let media_path = std::path::PathBuf::from(&config.media.store_path);
    tokio::fs::create_dir_all(&media_path).await?;
    tracing::info!(path = %media_path.display(), "media store ready");

    let app_state = AppState {
        store: Arc::new(store),
        server_name: config.server.server_name,
        sync_engine: Arc::new(SyncEngine::new()),
        media_path,
        max_upload_bytes: config.media.max_upload_bytes,
    };

    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
        .expose_headers(tower_http::cors::Any);

    let body_limit = axum::extract::DefaultBodyLimit::max(
        config.media.max_upload_bytes as usize + 4096, // headroom for headers
    );

    let app = api::router(app_state).layer(cors).layer(body_limit);

    let listener = tokio::net::TcpListener::bind(&config.server.bind).await?;
    tracing::info!(addr = %config.server.bind, "listening");

    axum::serve(listener, app).await?;

    Ok(())
}
