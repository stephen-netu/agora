use std::path::PathBuf;
use std::sync::Arc;

use crate::store::Storage;
use crate::sync_engine::SyncEngine;

/// Shared application state, passed to all handlers via axum's State extractor.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Storage>,
    pub server_name: String,
    pub sync_engine: Arc<SyncEngine>,
    pub media_path: PathBuf,
    pub max_upload_bytes: u64,
}
