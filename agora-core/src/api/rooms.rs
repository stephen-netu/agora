//! Room management API types

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::identifiers::{RoomId, UserId};

/// Request for `POST /_matrix/client/v3/createRoom`
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
    pub creation_content: Option<Value>,
}

/// Response for `POST /_matrix/client/v3/createRoom`
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: RoomId,
}

/// Response for `POST /_matrix/client/v3/join/{roomId}`
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinRoomResponse {
    pub room_id: RoomId,
}
