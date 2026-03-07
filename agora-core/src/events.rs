//! Event types for the Agora platform.
//!
//! This module provides types for room events, message content, and state events
//! compatible with the Matrix Client-Server API.
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
    /// The unique identifier for this event.
    pub event_id: EventId,
    /// The room where this event occurred.
    pub room_id: RoomId,
    /// The user who sent this event.
    pub sender: UserId,
    /// The type of this event (e.g., "m.room.message").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Non-null only for state events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_key: Option<String>,
    /// The content of this event.
    pub content: Value,
    /// The timestamp (in milliseconds) when this event was sent.
    pub origin_server_ts: u64,
    /// Monotonically increasing ordering used for sync tokens.
    /// Omitted on the wire (internal bookkeeping).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_ordering: Option<i64>,
}

// ---------------------------------------------------------------------------
// Standard Matrix message content (m.room.message)
// ---------------------------------------------------------------------------

/// The content of a message event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    /// The type of message (e.g., "m.text", "m.image").
    pub msgtype: String,
    /// The body of the message.
    pub body: String,
    /// Additional fields passed through opaquely.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Well-known Matrix message types.
///
/// These constants are used in the `msgtype` field of `MessageContent`
/// to indicate the type of message being sent.
pub mod msgtype {
    /// Plain text message.
    pub const TEXT: &str = "m.text";
    /// A notice (typically automated message).
    pub const NOTICE: &str = "m.notice";
    /// An emote/action message.
    pub const EMOTE: &str = "m.emote";
    /// An image message.
    pub const IMAGE: &str = "m.image";
    /// A file attachment.
    pub const FILE: &str = "m.file";
    /// An audio message.
    pub const AUDIO: &str = "m.audio";
    /// A video message.
    pub const VIDEO: &str = "m.video";
}

// ---------------------------------------------------------------------------
// Standard Matrix state event content types
// ---------------------------------------------------------------------------

/// Content for m.room.name state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomNameContent {
    /// The name of the room.
    pub name: String,
}

/// Content for m.room.topic state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTopicContent {
    /// The topic of the room.
    pub topic: String,
}

/// Content for m.room.member state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMemberContent {
    /// The membership state of the user.
    pub membership: Membership,
    /// The display name of the user (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
}

/// Membership state for a room member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Membership {
    /// The user has been invited to the room.
    Invite,
    /// The user has joined the room.
    Join,
    /// The user has left the room.
    Leave,
    /// The user has been banned from the room.
    Ban,
}

// ---------------------------------------------------------------------------
// Well-known Matrix event types
// ---------------------------------------------------------------------------

/// Well-known Matrix event types.
///
/// These constants represent the standard event types defined in the
/// Matrix Client-Server API specification.
pub mod event_type {
    /// A room message event (m.room.message).
    pub const MESSAGE: &str = "m.room.message";
    /// A room name state event (m.room.name).
    pub const NAME: &str = "m.room.name";
    /// A room topic state event (m.room.topic).
    pub const TOPIC: &str = "m.room.topic";
    /// A room member state event (m.room.member).
    pub const MEMBER: &str = "m.room.member";
    /// A room create state event (m.room.create).
    pub const CREATE: &str = "m.room.create";
    /// A room avatar state event (m.room.avatar).
    pub const AVATAR: &str = "m.room.avatar";
    /// A space child state event (m.space.child).
    pub const SPACE_CHILD: &str = "m.space.child";
    /// A space parent state event (m.space.parent).
    pub const SPACE_PARENT: &str = "m.space.parent";
}

/// The room type identifier for a Matrix Space.
pub const ROOM_TYPE_SPACE: &str = "m.space";

// ---------------------------------------------------------------------------
// Space-related state event content types
// ---------------------------------------------------------------------------

/// Content for m.space.child state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceChildContent {
    /// List of candidate servers that can be used to join the child room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Vec<String>>,
    /// A unique ordering string for the child within the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// Whether this child is suggested (for discovery).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested: Option<bool>,
}

/// Content for m.space.parent state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceParentContent {
    /// List of candidate servers that can be used to join the parent room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Vec<String>>,
    /// Whether this parent is the canonical (main) parent.
    #[serde(default)]
    pub canonical: bool,
}

/// Content for m.room.avatar state events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomAvatarContent {
    /// The URL of the avatar image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ---------------------------------------------------------------------------
// Reaction events
// ---------------------------------------------------------------------------

/// Reaction event types and content.
pub mod reaction;

// ---------------------------------------------------------------------------
// Presence events
// ---------------------------------------------------------------------------

/// Presence event types and content.
pub mod presence;

// ---------------------------------------------------------------------------
// Agora agent-first extensions
// ---------------------------------------------------------------------------

/// Agora-specific event types.
///
/// These event types extend the Matrix protocol for agent-first workflows.
pub mod agora_event_type {
    /// A tool call event (agora.tool_call).
    pub const TOOL_CALL: &str = "agora.tool_call";
    /// A tool result event (agora.tool_result).
    pub const TOOL_RESULT: &str = "agora.tool_result";
    /// A code execution event (agora.code).
    pub const CODE: &str = "agora.code";
}

/// Content for `agora.tool_call` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallContent {
    /// Unique identifier for this tool call.
    pub call_id: String,
    /// The name of the tool being called.
    pub tool_name: String,
    /// The parameters passed to the tool.
    pub parameters: Value,
    /// Plain-text fallback for clients that don't understand this event type.
    pub body: String,
}

/// Content for `agora.tool_result` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    /// The call_id this result is for.
    pub call_id: String,
    /// The status of the tool execution.
    pub status: ToolResultStatus,
    /// The result data from the tool.
    pub result: Value,
    /// Plain-text fallback for clients.
    pub body: String,
}

/// Status of a tool result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultStatus {
    /// The tool executed successfully.
    Success,
    /// The tool execution failed.
    Error,
}

/// Content for `agora.code` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContent {
    /// The programming language of the code.
    pub language: String,
    /// The code content.
    pub code: String,
    /// The filename (if specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Plain-text fallback.
    pub body: String,
}
