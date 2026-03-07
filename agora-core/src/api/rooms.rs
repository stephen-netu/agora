//! Room management API types

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::identifiers::{RoomId, UserId};

/// Request for `POST /_matrix/client/v3/createRoom`
#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    /// The name of the room.
    #[serde(default)]
    pub name: Option<String>,
    /// The topic of the room.
    #[serde(default)]
    pub topic: Option<String>,
    /// The room's alias (if any).
    #[serde(default)]
    pub room_alias_name: Option<String>,
    /// Whether this is a direct message room.
    #[serde(default)]
    pub is_direct: Option<bool>,
    /// Users to invite to the room.
    #[serde(default)]
    pub invite: Vec<UserId>,
    /// Additional creation content.
    #[serde(default)]
    pub creation_content: Option<Value>,
}

/// Response for `POST /_matrix/client/v3/createRoom`
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    /// The ID of the newly created room.
    pub room_id: RoomId,
}

/// Response for `POST /_matrix/client/v3/join/{roomId}`
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinRoomResponse {
    /// The ID of the room that was joined.
    pub room_id: RoomId,
}
