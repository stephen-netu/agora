//! Matrix Client-Server API request/response types

pub mod auth;
pub mod e2ee;
pub mod events;
pub mod media;
pub mod rooms;
pub mod spaces;
pub mod sync;

pub use auth::*;
pub use e2ee::*;
pub use events::*;
pub use media::*;
pub use rooms::*;
pub use spaces::*;
pub use sync::*;

/// Standard Matrix error response
#[derive(Debug, serde::Serialize)]
pub struct ErrorResponse {
    pub errcode: String,
    pub error: String,
}

/// Standard Matrix error codes
pub mod errcode {
    pub const UNKNOWN: &str = "M_UNKNOWN";
    pub const NOT_FOUND: &str = "M_NOT_FOUND";
    pub const FORBIDDEN: &str = "M_FORBIDDEN";
    pub const USER_IN_USE: &str = "M_USER_IN_USE";
    pub const BAD_JSON: &str = "M_BAD_JSON";
    pub const MISSING_TOKEN: &str = "M_MISSING_TOKEN";
    pub const UNKNOWN_TOKEN: &str = "M_UNKNOWN_TOKEN";
    pub const INVALID_PARAM: &str = "M_INVALID_PARAM";
    pub const NOT_JSON: &str = "M_NOT_JSON";
    pub const TOO_LARGE: &str = "M_TOO_LARGE";
}
