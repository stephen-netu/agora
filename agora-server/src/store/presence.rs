//! In-memory presence tracking for users.
//!
//! Presence is ephemeral state that doesn't need to be persisted to the database.
//! It uses an in-memory HashMap with automatic cleanup of stale entries.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use agora_core::events::presence::PresenceRecord;

/// Thread-safe presence state storage.
pub type PresenceState = Arc<RwLock<HashMap<String, PresenceRecord>>>;

/// Create a new empty presence state.
pub fn new_presence_state() -> PresenceState {
    Arc::new(RwLock::new(HashMap::new()))
}
