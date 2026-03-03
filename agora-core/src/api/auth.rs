//! Authentication API types

use serde::{Deserialize, Serialize};

use crate::identifiers::UserId;

/// Response for `GET /_matrix/client/versions`
#[derive(Debug, Serialize)]
pub struct VersionsResponse {
    pub versions: Vec<String>,
}

/// Request for `POST /_matrix/client/v3/register`
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub initial_device_display_name: Option<String>,
}

/// Response for `POST /_matrix/client/v3/register`
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub device_id: String,
}

/// Request for `POST /_matrix/client/v3/login`
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// We support `m.login.password`.
    #[serde(rename = "type")]
    pub login_type: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

/// Response for `POST /_matrix/client/v3/login`
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub device_id: String,
}
