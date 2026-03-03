//! Space hierarchy API types

use serde::{Deserialize, Serialize};

use crate::events::RoomEvent;

/// Query parameters for `GET /_matrix/client/v1/rooms/{roomId}/hierarchy`
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

/// Response for `GET /_matrix/client/v1/rooms/{roomId}/hierarchy`
#[derive(Debug, Serialize, Deserialize)]
pub struct HierarchyResponse {
    pub rooms: Vec<HierarchyRoom>,
}

/// Room in space hierarchy
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
