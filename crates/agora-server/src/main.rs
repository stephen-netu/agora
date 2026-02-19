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

    let app_state = AppState {
        store: Arc::new(store),
        server_name: config.server.server_name,
        sync_engine: Arc::new(SyncEngine::new()),
    };

    let app = api::router(app_state);

    let listener = tokio::net::TcpListener::bind(&config.server.bind).await?;
    tracing::info!(addr = %config.server.bind, "listening");

    axum::serve(listener, app).await?;

    Ok(())
}
