#![warn(
    missing_docs,
    rust_2018_idioms,
    unused_import_braces,
    unused_qualifications,
    clippy::all,
    clippy::pedantic,
)]
#![allow(
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
)]

mod api;
mod config;
mod error;
mod state;
mod store;
mod sync_engine;

use std::path::PathBuf;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use agora_crypto::{SequenceTimestamp, DEFAULT_EPOCH_MS};

use crate::config::Config;
use crate::state::AppState;
use crate::store::sqlite::SqliteStore;
use crate::store::Storage;
use crate::sync_engine::SyncEngine;

/// Resolve a path relative to the user's data directory if it's not absolute.
/// Falls back to current working directory if data directory is unavailable.
fn resolve_data_path(path: &str, app_name: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(path);

    if path.is_absolute() {
        return Ok(path);
    }

    let data_dir = dirs::data_dir()
        .or_else(|| {
            tracing::warn!(
                "data directory not available, using current directory as fallback"
            );
            std::env::current_dir().ok()
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory"))?
        .join(app_name);

    Ok(data_dir.join(path))
}

/// Check if a SQLite URI is a special in-memory URI that should not be modified.
fn is_special_sqlite_uri(uri: &str) -> bool {
    uri.contains(":memory:") || uri.contains("mode=memory")
}

/// Extract the file path from a SQLite URI, handling various formats.
/// Returns None for special URIs (in-memory) that should not be modified.
fn extract_sqlite_path(uri: &str) -> Option<String> {
    if is_special_sqlite_uri(uri) {
        return None;
    }

    if uri.starts_with("sqlite:") {
        let path_part = &uri[7..];
        let path_only = path_part.split('?').next().unwrap_or(path_part);
        Some(path_only.to_owned())
    } else {
        Some(uri.to_owned())
    }
}

/// Build a SQLite URI from a file path, preserving any query parameters.
fn build_sqlite_uri(base_uri: &str, new_path: &PathBuf) -> String {
    if base_uri.starts_with("sqlite:") {
        let path_part = &base_uri[7..];
        if let Some(query_start) = path_part.find('?') {
            let query = &path_part[query_start..];
            format!("sqlite:{}{}", new_path.display(), query)
        } else {
            format!("sqlite:{}", new_path.display())
        }
    } else {
        format!("sqlite:{}", new_path.display())
    }
}

/// Resolve a SQLite URI to use the data directory if the path is relative.
/// Special in-memory URIs are passed through unchanged.
/// Falls back to current working directory if data directory is unavailable.
async fn resolve_sqlite_uri(uri: &str, app_name: &str) -> anyhow::Result<String> {
    // Special SQLite URIs (in-memory) should be passed through unchanged
    if is_special_sqlite_uri(uri) {
        tracing::info!("using special SQLite URI without modification");
        return Ok(uri.to_owned());
    }

    let db_path_str = extract_sqlite_path(uri)
        .ok_or_else(|| anyhow::anyhow!("invalid SQLite URI"))?;

    let db_path = PathBuf::from(&db_path_str);

    if db_path.is_absolute() {
        tracing::info!(path = %db_path.display(), "using absolute database path");
        return Ok(uri.to_owned());
    }

    let data_dir = dirs::data_dir()
        .or_else(|| {
            tracing::warn!(
                "data directory not available, using current directory as fallback"
            );
            std::env::current_dir().ok()
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory"))?
        .join(app_name);

    let resolved_path = data_dir.join(&db_path);

    if let Some(parent) = resolved_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tracing::info!(
        original = %db_path_str,
        resolved = %resolved_path.display(),
        "resolved relative database path to data directory"
    );

    Ok(build_sqlite_uri(uri, &resolved_path))
}

/// Load a 32-byte server secret from disk, or generate and persist one on first boot.
///
/// The secret is stored as a raw 32-byte binary file. It is never transmitted to clients
/// and must not be deleted between restarts (doing so invalidates all existing sessions).
async fn load_or_generate_token_secret(data_dir: &std::path::Path) -> anyhow::Result<[u8; 32]> {
    let secret_path = data_dir.join("token_secret");

    if let Ok(data) = tokio::fs::read(&secret_path).await {
        if data.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&data);
            tracing::info!(path = %secret_path.display(), "loaded token secret from disk");
            return Ok(arr);
        }
        tracing::warn!(
            path = %secret_path.display(),
            len = data.len(),
            "token secret file has unexpected length — regenerating"
        );
    }

    use argon2::password_hash::rand_core::RngCore;
    let mut secret = [0u8; 32];
    argon2::password_hash::rand_core::OsRng.fill_bytes(&mut secret);

    if let Some(parent) = secret_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&secret_path, &secret).await?;
    tracing::info!(path = %secret_path.display(), "generated and persisted new token secret");

    Ok(secret)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config_path = std::env::args().nth(1);
    let config = Config::load(config_path.as_deref())?;

    if config.database.backend != "sqlite" {
        anyhow::bail!("unsupported database backend: {}, only 'sqlite' is supported", config.database.backend);
    }

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
    let db_uri = resolve_sqlite_uri(&db_uri, "agora").await?;

    let store = SqliteStore::open(&db_uri).await?;

    // S-02: resume sequence timestamp from last persisted value to prevent unique-constraint
    // collisions caused by counter reset after a server restart.
    let max_ts = store.get_max_timestamp().await?;
    let timestamp = if max_ts > DEFAULT_EPOCH_MS {
        let last_seq = max_ts - DEFAULT_EPOCH_MS;
        tracing::info!(max_ts, last_seq, "resuming sequence timestamp from persisted state");
        SequenceTimestamp::resume_from(DEFAULT_EPOCH_MS, last_seq)
    } else {
        tracing::info!("starting sequence timestamp from epoch (fresh database)");
        SequenceTimestamp::default()
    };

    let media_path = resolve_data_path(&config.media.store_path, "agora")?;
    tokio::fs::create_dir_all(&media_path).await?;
    tracing::info!(path = %media_path.display(), "media store ready");

    // Token secret: load from data dir (or generate on first boot). Invalidating all
    // active sessions requires deleting the token_secret file and restarting.
    let data_dir = dirs::data_dir()
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory"))?
        .join("agora");
    let token_secret = load_or_generate_token_secret(&data_dir).await?;

    let app_state = AppState {
        store: Arc::new(store),
        server_name: config.server.server_name,
        sync_engine: Arc::new(SyncEngine::new()),
        media_path,
        max_upload_bytes: config.media.max_upload_bytes,
        typing: Default::default(),
        presence: crate::store::presence::new_presence_state(),
        timestamp: timestamp.into_arc(),
        token_secret,
    };

    // Start background task to clean up stale presence entries every 5 minutes
    let presence_cleanup_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            crate::api::presence::cleanup_stale_presence(&presence_cleanup_state).await;
        }
    });

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
