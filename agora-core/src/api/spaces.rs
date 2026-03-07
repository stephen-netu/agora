//! Space hierarchy API types

use serde::{Deserialize, Serialize};

use crate::events::RoomEvent;

/// Query parameters for `GET /_matrix/client/v1/rooms/{roomId}/hierarchy`
#[derive(Debug, Deserialize)]
pub struct HierarchyQuery {
    /// Maximum number of rooms to return per page.
    #[serde(default = "default_hierarchy_limit")]
    pub limit: u64,
    /// Maximum depth in the space hierarchy to traverse.
    #[serde(default = "default_max_depth")]
    pub max_depth: u64,
    /// Whether to only return suggested rooms.
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
    /// List of rooms in the hierarchy.
    pub rooms: Vec<HierarchyRoom>,
}

/// Room in space hierarchy
#[derive(Debug, Serialize, Deserialize)]
pub struct HierarchyRoom {
    /// The room's ID.
    pub room_id: String,
    /// The room's name (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The room's topic (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// Number of joined members.
    pub num_joined_members: u64,
    /// The room's type (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_type: Option<String>,
    /// State events for child rooms.
    pub children_state: Vec<RoomEvent>,
}
