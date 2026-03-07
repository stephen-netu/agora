//! End-to-end encryption API types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Device keys payload for E2EE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeysPayload {
    /// The user ID who owns these device keys.
    pub user_id: String,
    /// The device ID.
    pub device_id: String,
    /// The encryption algorithms supported by this device.
    pub algorithms: Vec<String>,
    /// The public keys for this device.
    pub keys: BTreeMap<String, String>,
    /// Signatures for the keys.
    #[serde(default)]
    pub signatures: BTreeMap<String, BTreeMap<String, String>>,
}

/// Request for `POST /_matrix/client/v3/keys/upload`
#[derive(Debug, Deserialize)]
pub struct KeysUploadRequest {
    /// The device keys to upload.
    #[serde(default)]
    pub device_keys: Option<DeviceKeysPayload>,
    /// One-time key to upload.
    #[serde(default)]
    pub one_time_key: Option<BTreeMap<String, Value>>,
}

/// Response for `POST /_matrix/client/v3/keys/upload`
#[derive(Debug, Serialize)]
pub struct KeysUploadResponse {
    /// The number of one-time keys remaining for each algorithm.
    pub one_time_key_counts: BTreeMap<String, u64>,
}

/// Request for `POST /_matrix/client/v3/keys/query`
#[derive(Debug, Deserialize)]
pub struct KeysQueryRequest {
    /// The device keys to query, mapped by user ID to device IDs.
    pub device_keys: BTreeMap<String, Vec<String>>,
}

/// Response for `POST /_matrix/client/v3/keys/query`
#[derive(Debug, Serialize)]
pub struct KeysQueryResponse {
    /// The device keys, mapped by user ID then device ID.
    pub device_keys: BTreeMap<String, BTreeMap<String, DeviceKeysPayload>>,
}

/// Request for `POST /_matrix/client/v3/keys/claim`
#[derive(Debug, Deserialize)]
pub struct KeysClaimRequest {
    /// The one-time keys to claim, mapped by user ID to device ID to key ID.
    pub one_time_keys: BTreeMap<String, BTreeMap<String, String>>,
}

/// Response for `POST /_matrix/client/v3/keys/claim`
#[derive(Debug, Serialize)]
pub struct KeysClaimResponse {
    /// The claimed one-time keys, mapped by user ID then device ID then key ID.
    pub one_time_keys: BTreeMap<String, BTreeMap<String, Value>>,
}

/// Request for `PUT /_matrix/client/v3/sendToDevice/{eventType}/{txnId}`
#[derive(Debug, Deserialize)]
pub struct SendToDeviceRequest {
    /// The messages to send, mapped by user ID then device ID.
    pub messages: BTreeMap<String, BTreeMap<String, Value>>,
}

/// To-device event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToDeviceEvent {
    /// The sender of the event.
    pub sender: String,
    /// The type of the event.
    #[serde(rename = "type")]
    pub event_type: String,
    /// The content of the event.
    pub content: Value,
}
