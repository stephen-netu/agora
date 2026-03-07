//! Event sending and retrieval API types

use serde::{Deserialize, Serialize};

use crate::events::RoomEvent;

/// Response for `PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}`
#[derive(Debug, Serialize, Deserialize)]
pub struct SendEventResponse {
    /// The event ID of the newly created event.
    pub event_id: String,
}

/// Query parameters for `GET /_matrix/client/v3/rooms/{roomId}/messages`
#[derive(Debug, Deserialize)]
pub struct MessagesQuery {
    /// A token from a previous sync, to fetch new events after this point.
    #[serde(default)]
    pub from: Option<String>,
    /// A token from a previous sync, to fetch events before this point.
    #[serde(default)]
    pub to: Option<String>,
    /// The maximum number of events to return.
    #[serde(default = "default_messages_limit")]
    pub limit: u64,
    /// The direction to fetch events: "b" for backwards, "f" for forwards.
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
    /// A token to fetch the next batch of events, for backwards pagination.
    pub start: String,
    /// A token to fetch the next batch of events, for forwards pagination.
    pub end: Option<String>,
    /// The list of room events in the requested direction.
    pub chunk: Vec<RoomEvent>,
}
