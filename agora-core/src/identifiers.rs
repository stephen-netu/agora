use serde::{Deserialize, Serialize};
use std::fmt;

/// A Matrix-compatible user identifier: `@localpart:server_name`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl UserId {
    /// Create a new UserId. Validates the `@localpart:server` format.
    pub fn new(localpart: &str, server_name: &str) -> Self {
        Self(format!("@{localpart}:{server_name}"))
    }

    /// Parse a UserId from a raw string, validating the format.
    pub fn parse(s: &str) -> Result<Self, IdentifierError> {
        if !s.starts_with('@') || !s.contains(':') {
            return Err(IdentifierError::InvalidFormat {
                kind: "UserId",
                value: s.to_owned(),
            });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn localpart(&self) -> &str {
        &self.0[1..self.0.find(':').unwrap()]
    }

    pub fn server_name(&self) -> &str {
        &self.0[self.0.find(':').unwrap() + 1..]
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Matrix-compatible room identifier: `!opaque_id:server_name`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomId(String);

impl RoomId {
    pub fn new(server_name: &str) -> Self {
        let opaque = uuid::Uuid::new_v4().simple().to_string();
        Self(format!("!{opaque}:{server_name}"))
    }

    pub fn parse(s: &str) -> Result<Self, IdentifierError> {
        if !s.starts_with('!') || !s.contains(':') {
            return Err(IdentifierError::InvalidFormat {
                kind: "RoomId",
                value: s.to_owned(),
            });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Matrix-compatible event identifier: `$opaque_id`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

impl EventId {
    pub fn new() -> Self {
        Self(format!("${}", uuid::Uuid::new_v4().simple()))
    }

    pub fn parse(s: &str) -> Result<Self, IdentifierError> {
        if !s.starts_with('$') {
            return Err(IdentifierError::InvalidFormat {
                kind: "EventId",
                value: s.to_owned(),
            });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A room alias: `#alias:server_name`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomAlias(String);

impl RoomAlias {
    pub fn new(alias: &str, server_name: &str) -> Self {
        Self(format!("#{alias}:{server_name}"))
    }

    pub fn parse(s: &str) -> Result<Self, IdentifierError> {
        if !s.starts_with('#') || !s.contains(':') {
            return Err(IdentifierError::InvalidFormat {
                kind: "RoomAlias",
                value: s.to_owned(),
            });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn alias(&self) -> &str {
        &self.0[1..self.0.find(':').unwrap()]
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentifierError {
    #[error("invalid {kind} format: {value:?}")]
    InvalidFormat { kind: &'static str, value: String },
}
