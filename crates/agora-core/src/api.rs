use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::events::RoomEvent;
use crate::identifiers::{RoomId, UserId};

// ---------------------------------------------------------------------------
// /_matrix/client/versions
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VersionsResponse {
    pub versions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Auth: /register, /login, /logout
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub initial_device_display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub device_id: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// We support `m.login.password`.
    #[serde(rename = "type")]
    pub login_type: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub device_id: String,
}

// ---------------------------------------------------------------------------
// Rooms: /createRoom, /join, /leave
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub room_alias_name: Option<String>,
    #[serde(default)]
    pub is_direct: Option<bool>,
    #[serde(default)]
    pub invite: Vec<UserId>,
    #[serde(default)]
    pub creation_content: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: RoomId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinRoomResponse {
    pub room_id: RoomId,
}

// ---------------------------------------------------------------------------
// Events: /send, /messages, /state
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct SendEventResponse {
    pub event_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MessagesQuery {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default = "default_messages_limit")]
    pub limit: u64,
    #[serde(default = "default_messages_dir")]
    pub dir: String,
}

fn default_messages_limit() -> u64 {
    50
}

fn default_messages_dir() -> String {
    "b".to_owned()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessagesResponse {
    pub start: String,
    pub end: Option<String>,
    pub chunk: Vec<RoomEvent>,
}

// ---------------------------------------------------------------------------
// Sync: /sync
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default = "default_sync_timeout")]
    pub timeout: u64,
}

fn default_sync_timeout() -> u64 {
    0
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub next_batch: String,
    pub rooms: SyncRooms,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_device: Option<ToDevicePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_one_time_keys_count: Option<HashMap<String, u64>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncRooms {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub join: HashMap<String, JoinedRoom>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub invite: HashMap<String, Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub leave: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinedRoom {
    pub timeline: Timeline,
    pub state: RoomState,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Timeline {
    pub events: Vec<RoomEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_batch: Option<String>,
    pub limited: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RoomState {
    pub events: Vec<RoomEvent>,
}

// ---------------------------------------------------------------------------
// Spaces: /hierarchy
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HierarchyQuery {
    #[serde(default = "default_hierarchy_limit")]
    pub limit: u64,
    #[serde(default = "default_max_depth")]
    pub max_depth: u64,
    #[serde(default)]
    pub suggested_only: bool,
}

fn default_hierarchy_limit() -> u64 {
    50
}

fn default_max_depth() -> u64 {
    5
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HierarchyResponse {
    pub rooms: Vec<HierarchyRoom>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HierarchyRoom {
    pub room_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub num_joined_members: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_type: Option<String>,
    pub children_state: Vec<RoomEvent>,
}

// ---------------------------------------------------------------------------
// E2EE: /keys/upload, /keys/query, /keys/claim, /sendToDevice
// ---------------------------------------------------------------------------

/// POST /_matrix/client/v3/keys/upload
#[derive(Debug, Deserialize)]
pub struct KeysUploadRequest {
    #[serde(default)]
    pub device_keys: Option<DeviceKeysPayload>,
    #[serde(default)]
    pub one_time_keys: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeysPayload {
    pub user_id: String,
    pub device_id: String,
    pub algorithms: Vec<String>,
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub signatures: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct KeysUploadResponse {
    pub one_time_key_counts: HashMap<String, u64>,
}

/// POST /_matrix/client/v3/keys/query
#[derive(Debug, Deserialize)]
pub struct KeysQueryRequest {
    pub device_keys: HashMap<String, Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct KeysQueryResponse {
    pub device_keys: HashMap<String, HashMap<String, DeviceKeysPayload>>,
}

/// POST /_matrix/client/v3/keys/claim
#[derive(Debug, Deserialize)]
pub struct KeysClaimRequest {
    pub one_time_keys: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct KeysClaimResponse {
    pub one_time_keys: HashMap<String, HashMap<String, Value>>,
}

/// PUT /_matrix/client/v3/sendToDevice/{eventType}/{txnId}
#[derive(Debug, Deserialize)]
pub struct SendToDeviceRequest {
    pub messages: HashMap<String, HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToDeviceEvent {
    pub sender: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub content: Value,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ToDevicePayload {
    pub events: Vec<ToDeviceEvent>,
}

// ---------------------------------------------------------------------------
// Media: /upload, /download, /config
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    pub content_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaConfigResponse {
    #[serde(rename = "m.upload.size")]
    pub m_upload_size: Option<u64>,
}

// ---------------------------------------------------------------------------
// Error response (Matrix standard format)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub errcode: String,
    pub error: String,
}

/// Standard Matrix error codes used in our responses.
pub mod errcode {
    pub const UNKNOWN: &str = "M_UNKNOWN";
    pub const NOT_FOUND: &str = "M_NOT_FOUND";
    pub const FORBIDDEN: &str = "M_FORBIDDEN";
    pub const USER_IN_USE: &str = "M_USER_IN_USE";
    pub const BAD_JSON: &str = "M_BAD_JSON";
    pub const MISSING_TOKEN: &str = "M_MISSING_TOKEN";
    pub const UNKNOWN_TOKEN: &str = "M_UNKNOWN_TOKEN";
    pub const INVALID_PARAM: &str = "M_INVALID_PARAM";
    pub const NOT_JSON: &str = "M_NOT_JSON";
    pub const TOO_LARGE: &str = "M_TOO_LARGE";
}
