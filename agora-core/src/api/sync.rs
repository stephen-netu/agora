//! Sync API types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::events::RoomEvent;

/// Query parameters for `GET /_matrix/client/v3/sync`
#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default = "default_sync_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub full_state: Option<bool>,
}

fn default_sync_timeout() -> u64 {
    0
}

/// Response for `GET /_matrix/client/v3/sync`
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub next_batch: String,
    pub rooms: SyncRooms,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_device: Option<ToDevicePayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_one_time_keys_count: Option<BTreeMap<String, u64>>,
}

/// Room updates in a sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncRooms {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub join: BTreeMap<String, JoinedRoom>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub invite: BTreeMap<String, InvitedRoom>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub leave: BTreeMap<String, Value>,
}

/// Joined room data in sync response
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinedRoom {
    pub timeline: Timeline,
    pub state: RoomState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<EphemeralEvents>,
}

/// Ephemeral events in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EphemeralEvents {
    pub events: Vec<Value>,
}

/// Invited room data in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct InvitedRoom {
    pub invite_state: RoomState,
}

/// Timeline data in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Timeline {
    pub events: Vec<RoomEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_batch: Option<String>,
    pub limited: bool,
}

/// Room state in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RoomState {
    pub events: Vec<RoomEvent>,
}

/// To-device message payload
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ToDevicePayload {
    pub events: Vec<super::ToDeviceEvent>,
}
