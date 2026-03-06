use serde::{Deserialize, Serialize};

/// User presence states as defined by Matrix spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceState {
    /// The user is online and active.
    Online,
    /// The user is not currently active (idle).
    Unavailable,
    /// The user is offline.
    Offline,
}

impl Default for PresenceState {
    fn default() -> Self {
        PresenceState::Offline
    }
}

impl std::fmt::Display for PresenceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresenceState::Online => write!(f, "online"),
            PresenceState::Unavailable => write!(f, "unavailable"),
            PresenceState::Offline => write!(f, "offline"),
        }
    }
}

impl std::str::FromStr for PresenceState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "online" => Ok(PresenceState::Online),
            "unavailable" => Ok(PresenceState::Unavailable),
            "offline" => Ok(PresenceState::Offline),
            _ => Err(format!("unknown presence state: {s}")),
        }
    }
}

/// Presence content for m.presence events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceContent {
    /// The user's presence state.
    pub presence: PresenceState,
    /// Milliseconds since the user was last active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ago: Option<u64>,
    /// An optional status message for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    /// Whether the user is currently active (typing/interacting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

impl PresenceContent {
    /// Create a new presence content with the given state.
    pub fn new(presence: PresenceState) -> Self {
        Self {
            presence,
            last_active_ago: None,
            status_msg: None,
            currently_active: None,
        }
    }

    /// Set the last_active_ago field.
    pub fn with_last_active_ago(mut self, ago: u64) -> Self {
        self.last_active_ago = Some(ago);
        self
    }

    /// Set the status message.
    pub fn with_status_msg(mut self, msg: impl Into<String>) -> Self {
        self.status_msg = Some(msg.into());
        self
    }

    /// Set the currently_active flag.
    pub fn with_currently_active(mut self, active: bool) -> Self {
        self.currently_active = Some(active);
        self
    }
}

/// Full presence event as stored and transmitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceEvent {
    /// The user ID this presence event is for.
    pub sender: String,
    /// The type of event (always "m.presence").
    #[serde(rename = "type")]
    pub event_type: String,
    /// The presence content.
    pub content: PresenceContent,
}

impl PresenceEvent {
    /// Create a new presence event for the given user.
    pub fn new(sender: impl Into<String>, content: PresenceContent) -> Self {
        Self {
            sender: sender.into(),
            event_type: "m.presence".to_owned(),
            content,
        }
    }
}

/// Request body for updating presence status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPresenceRequest {
    /// The new presence state.
    pub presence: PresenceState,
    /// An optional status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
}

/// Response for getting presence status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPresenceResponse {
    /// The user's presence state.
    pub presence: PresenceState,
    /// Milliseconds since the user was last active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ago: Option<u64>,
    /// The user's status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    /// Whether the user is currently active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

/// Request body for heartbeat (updating last_active_ago).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Optional flag to indicate if user is currently active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currently_active: Option<bool>,
}

/// Internal record for storing presence state.
#[derive(Debug, Clone)]
pub struct PresenceRecord {
    pub user_id: String,
    pub presence: PresenceState,
    pub last_active_at: u64,
    pub status_msg: Option<String>,
    pub currently_active: bool,
}

impl PresenceRecord {
    /// Calculate milliseconds since last activity.
    pub fn last_active_ago(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_active_at)
    }

    /// Convert to a GetPresenceResponse.
    pub fn to_response(&self, now: u64) -> GetPresenceResponse {
        GetPresenceResponse {
            presence: self.presence,
            last_active_ago: Some(self.last_active_ago(now)),
            status_msg: self.status_msg.clone(),
            currently_active: Some(self.currently_active),
        }
    }

    /// Convert to a PresenceEvent.
    pub fn to_event(&self, now: u64) -> PresenceEvent {
        PresenceEvent::new(
            &self.user_id,
            PresenceContent {
                presence: self.presence,
                last_active_ago: Some(self.last_active_ago(now)),
                status_msg: self.status_msg.clone(),
                currently_active: Some(self.currently_active),
            },
        )
    }
}
