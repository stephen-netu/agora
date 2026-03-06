use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::identifiers::{EventId, RoomId, UserId};

// ---------------------------------------------------------------------------
// Top-level event envelope — the canonical wire format
// ---------------------------------------------------------------------------

/// A room event as stored and transmitted. Compatible with the Matrix
/// Client-Server API event format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomEvent {
    pub event_id: EventId,
    pub room_id: RoomId,
    pub sender: UserId,
    #[serde(rename = "type")]
    pub event_type: String,
    /// Non-null only for state events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_key: Option<String>,
    pub content: Value,
    pub origin_server_ts: u64,
    /// Monotonically increasing ordering used for sync tokens.
    /// Omitted on the wire (internal bookkeeping).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_ordering: Option<i64>,
}

// ---------------------------------------------------------------------------
// Standard Matrix message content (m.room.message)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    pub msgtype: String,
    pub body: String,
    /// Additional fields passed through opaquely.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Well-known Matrix message types.
pub mod msgtype {
    pub const TEXT: &str = "m.text";
    pub const NOTICE: &str = "m.notice";
    pub const EMOTE: &str = "m.emote";
    pub const IMAGE: &str = "m.image";
    pub const FILE: &str = "m.file";
    pub const AUDIO: &str = "m.audio";
    pub const VIDEO: &str = "m.video";
}

// ---------------------------------------------------------------------------
// Standard Matrix state event content types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomNameContent {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTopicContent {
    pub topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMemberContent {
    pub membership: Membership,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Membership {
    Invite,
    Join,
    Leave,
    Ban,
}

// ---------------------------------------------------------------------------
// Well-known Matrix event types
// ---------------------------------------------------------------------------

pub mod event_type {
    pub const MESSAGE: &str = "m.room.message";
    pub const NAME: &str = "m.room.name";
    pub const TOPIC: &str = "m.room.topic";
    pub const MEMBER: &str = "m.room.member";
    pub const CREATE: &str = "m.room.create";
    pub const AVATAR: &str = "m.room.avatar";
    pub const SPACE_CHILD: &str = "m.space.child";
    pub const SPACE_PARENT: &str = "m.space.parent";
}

pub const ROOM_TYPE_SPACE: &str = "m.space";

// ---------------------------------------------------------------------------
// Space-related state event content types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceChildContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceParentContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Vec<String>>,
    #[serde(default)]
    pub canonical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomAvatarContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ---------------------------------------------------------------------------
// Reaction events
// ---------------------------------------------------------------------------

pub mod reaction;

// ---------------------------------------------------------------------------
// Presence events
// ---------------------------------------------------------------------------

pub mod presence;

// ---------------------------------------------------------------------------
// Agora agent-first extensions
// ---------------------------------------------------------------------------

pub mod agora_event_type {
    pub const TOOL_CALL: &str = "agora.tool_call";
    pub const TOOL_RESULT: &str = "agora.tool_result";
    pub const CODE: &str = "agora.code";
}

/// Content for `agora.tool_call` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallContent {
    pub call_id: String,
    pub tool_name: String,
    pub parameters: Value,
    /// Plain-text fallback for clients that don't understand this event type.
    pub body: String,
}

/// Content for `agora.tool_result` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    pub call_id: String,
    pub status: ToolResultStatus,
    pub result: Value,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultStatus {
    Success,
    Error,
}

/// Content for `agora.code` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContent {
    pub language: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Plain-text fallback.
    pub body: String,
}
