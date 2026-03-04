use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

use agora_crypto::TimestampProvider;

use crate::store::Storage;
use crate::sync_engine::SyncEngine;

/// Per-room typing state: user_id -> expiry instant.
pub type TypingState = Arc<Mutex<BTreeMap<String, BTreeMap<String, Instant>>>>;

/// Shared application state, passed to all handlers via axum's State extractor.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Storage>,
    pub server_name: String,
    pub sync_engine: Arc<SyncEngine>,
    pub media_path: PathBuf,
    pub max_upload_bytes: u64,
    pub typing: TypingState,
    /// S-02 compliant timestamp provider. Never use SystemTime::now() — use this.
    pub timestamp: Arc<dyn TimestampProvider>,
    /// Server-side secret key for access token generation. Loaded from disk on startup;
    /// generated on first boot and persisted. Never transmitted to clients.
    pub token_secret: [u8; 32],
}
