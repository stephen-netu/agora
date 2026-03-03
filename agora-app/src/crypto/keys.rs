//! Key management for E2EE

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::store::CryptoStore;

/// Information about a device
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub user_id: String,
    pub device_id: String,
    pub curve25519_key: String,
    pub ed25519_key: String,
}

/// Room key content for sharing
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RoomKeyContent {
    pub algorithm: String,
    pub room_id: String,
    pub session_id: String,
    pub session_key: String,
}

/// Manages device keys and one-time keys
pub struct KeyManager<'a> {
    store: &'a mut CryptoStore,
}

impl<'a> KeyManager<'a> {
    pub fn new(store: &'a mut CryptoStore) -> Self {
        Self { store }
    }
}
