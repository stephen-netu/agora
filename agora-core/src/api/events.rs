//! Event sending and retrieval API types

use serde::{Deserialize, Serialize};

use crate::events::RoomEvent;

/// Response for `PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}`
#[derive(Debug, Serialize, Deserialize)]
pub struct SendEventResponse {
    pub event_id: String,
}

/// Query parameters for `GET /_matrix/client/v3/rooms/{roomId}/messages`
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

/// Response for `GET /_matrix/client/v3/rooms/{roomId}/messages`
#[derive(Debug, Serialize, Deserialize)]
pub struct MessagesResponse {
    pub start: String,
    pub end: Option<String>,
    pub chunk: Vec<RoomEvent>,
}
