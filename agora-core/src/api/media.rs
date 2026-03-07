//! Media upload/download API types

use serde::{Deserialize, Serialize};

/// Response for `POST /_matrix/media/v3/upload`
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    /// The content URI of the uploaded media.
    pub content_uri: String,
}

/// Response for `GET /_matrix/media/v3/config`
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaConfigResponse {
    /// Maximum upload size in bytes.
    #[serde(rename = "m.upload.size")]
    pub m_upload_size: Option<u64>,
}
