//! Sync API types

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::events::RoomEvent;

/// Query parameters for `GET /_matrix/client/v3/sync`
#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    /// The sync token to resume from (from previous SyncResponse.next_batch).
    #[serde(default)]
    pub since: Option<String>,
    /// The maximum time to wait in milliseconds.
    #[serde(default = "default_sync_timeout")]
    pub timeout: u64,
    /// Whether to include full state in the response.
    #[serde(default)]
    pub full_state: Option<bool>,
}

fn default_sync_timeout() -> u64 {
    0
}

/// Response for `GET /_matrix/client/v3/sync`
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    /// The sync token to use in the next request.
    pub next_batch: String,
    /// Room-specific data for this sync response.
    pub rooms: SyncRooms,
    /// Device-specific messages to be sent to the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_device: Option<ToDevicePayload>,
    /// Number of one-time keys available for each algorithm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_one_time_keys_count: Option<BTreeMap<String, u64>>,
}

/// Room updates in a sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncRooms {
    /// Rooms the user has joined.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub join: BTreeMap<String, JoinedRoom>,
    /// Rooms the user has been invited to.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub invite: BTreeMap<String, InvitedRoom>,
    /// Rooms the user has left.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub leave: BTreeMap<String, Value>,
}

/// Joined room data in sync response
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinedRoom {
    /// Timeline of new events in the room.
    pub timeline: Timeline,
    /// Current state of the room.
    pub state: RoomState,
    /// Ephemeral events (typing, receipts, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<EphemeralEvents>,
}

/// Ephemeral events in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EphemeralEvents {
    /// List of ephemeral events.
    pub events: Vec<Value>,
}

/// Invited room data in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct InvitedRoom {
    /// The invite state for this room.
    pub invite_state: RoomState,
}

/// Timeline data in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Timeline {
    /// List of events in the timeline.
    pub events: Vec<RoomEvent>,
    /// A token to fetch earlier events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_batch: Option<String>,
    /// Whether the timeline was limited.
    pub limited: bool,
}

/// Room state in sync response
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RoomState {
    /// List of state events.
    pub events: Vec<RoomEvent>,
}

/// To-device message payload
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ToDevicePayload {
    /// List of to-device events.
    pub events: Vec<super::ToDeviceEvent>,
}
