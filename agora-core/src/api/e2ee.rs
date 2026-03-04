//! End-to-end encryption API types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Device keys payload for E2EE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeysPayload {
    pub user_id: String,
    pub device_id: String,
    pub algorithms: Vec<String>,
    pub keys: BTreeMap<String, String>,
    #[serde(default)]
    pub signatures: BTreeMap<String, BTreeMap<String, String>>,
}

/// Request for `POST /_matrix/client/v3/keys/upload`
#[derive(Debug, Deserialize)]
pub struct KeysUploadRequest {
    #[serde(default)]
    pub device_keys: Option<DeviceKeysPayload>,
    #[serde(default)]
    pub one_time_keys: Option<BTreeMap<String, Value>>,
}

/// Response for `POST /_matrix/client/v3/keys/upload`
#[derive(Debug, Serialize)]
pub struct KeysUploadResponse {
    pub one_time_key_counts: BTreeMap<String, u64>,
}

/// Request for `POST /_matrix/client/v3/keys/query`
#[derive(Debug, Deserialize)]
pub struct KeysQueryRequest {
    pub device_keys: BTreeMap<String, Vec<String>>,
}

/// Response for `POST /_matrix/client/v3/keys/query`
#[derive(Debug, Serialize)]
pub struct KeysQueryResponse {
    pub device_keys: BTreeMap<String, BTreeMap<String, DeviceKeysPayload>>,
}

/// Request for `POST /_matrix/client/v3/keys/claim`
#[derive(Debug, Deserialize)]
pub struct KeysClaimRequest {
    pub one_time_keys: BTreeMap<String, BTreeMap<String, String>>,
}

/// Response for `POST /_matrix/client/v3/keys/claim`
#[derive(Debug, Serialize)]
pub struct KeysClaimResponse {
    pub one_time_keys: BTreeMap<String, BTreeMap<String, Value>>,
}

/// Request for `PUT /_matrix/client/v3/sendToDevice/{eventType}/{txnId}`
#[derive(Debug, Deserialize)]
pub struct SendToDeviceRequest {
    pub messages: BTreeMap<String, BTreeMap<String, Value>>,
}

/// To-device event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToDeviceEvent {
    pub sender: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub content: Value,
}
