//! Authentication API types

use serde::{Deserialize, Serialize};

use crate::identifiers::UserId;

/// Response for `GET /_matrix/client/versions`
#[derive(Debug, Serialize)]
pub struct VersionsResponse {
    /// Supported Matrix protocol versions.
    pub versions: Vec<String>,
}

/// Request for `POST /_matrix/client/v3/register`
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// The desired username.
    pub username: String,
    /// The desired password.
    pub password: String,
    /// The device ID (if not provided, one will be generated).
    #[serde(default)]
    pub device_id: Option<String>,
    /// The initial display name for the device.
    #[serde(default)]
    pub initial_device_display_name: Option<String>,
}

/// Response for `POST /_matrix/client/v3/register`
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    /// The newly created user's ID.
    pub user_id: UserId,
    /// The access token for the session.
    pub access_token: String,
    /// The device ID.
    pub device_id: String,
}

/// Request for `POST /_matrix/client/v3/login`
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// We support `m.login.password`.
    #[serde(rename = "type")]
    pub login_type: String,
    /// The user identifier (username or user ID).
    #[serde(default)]
    pub user: Option<String>,
    /// The user's password.
    #[serde(default)]
    pub password: Option<String>,
    /// The device ID (if not provided, one will be generated).
    #[serde(default)]
    pub device_id: Option<String>,
}

/// Response for `POST /_matrix/client/v3/login`
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    /// The authenticated user's ID.
    pub user_id: UserId,
    /// The access token for the session.
    pub access_token: String,
    /// The device ID.
    pub device_id: String,
}
