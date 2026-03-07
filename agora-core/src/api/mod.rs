//! Matrix Client-Server API request/response types

/// Authentication and login API
pub mod auth;
/// End-to-end encryption API
pub mod e2ee;
/// Events API
pub mod events;
/// Media API
pub mod media;
/// Rooms API
pub mod rooms;
/// Spaces API
pub mod spaces;
/// Sync API
pub mod sync;

/// Re-exports authentication types
pub use auth::*;
/// Re-exports E2EE types
pub use e2ee::*;
/// Re-exports event types
pub use events::*;
/// Re-exports media types
pub use media::*;
/// Re-exports room types
pub use rooms::*;
/// Re-exports space types
pub use spaces::*;
/// Re-exports sync types
pub use sync::*;

/// Standard Matrix error response
#[derive(Debug, serde::Serialize)]
pub struct ErrorResponse {
    /// The error code (e.g., "M_NOT_FOUND").
    pub errcode: String,
    /// A human-readable error message.
    pub error: String,
}

/// Standard Matrix error codes
pub mod errcode {
    /// Unknown error
    pub const UNKNOWN: &str = "M_UNKNOWN";
    /// Resource not found
    pub const NOT_FOUND: &str = "M_NOT_FOUND";
    /// Forbidden access
    pub const FORBIDDEN: &str = "M_FORBIDDEN";
    /// Username already in use
    pub const USER_IN_USE: &str = "M_USER_IN_USE";
    /// Invalid JSON
    pub const BAD_JSON: &str = "M_BAD_JSON";
    /// Missing access token
    pub const MISSING_TOKEN: &str = "M_MISSING_TOKEN";
    /// Invalid or unknown access token
    pub const UNKNOWN_TOKEN: &str = "M_UNKNOWN_TOKEN";
    /// Invalid parameter
    pub const INVALID_PARAM: &str = "M_INVALID_PARAM";
    /// Not valid JSON
    pub const NOT_JSON: &str = "M_NOT_JSON";
    /// Resource too large
    pub const TOO_LARGE: &str = "M_TOO_LARGE";
}
