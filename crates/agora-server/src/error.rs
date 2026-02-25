use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use agora_core::api::ErrorResponse;

/// Unified error type for API handlers.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub errcode: String,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, errcode: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            errcode: errcode.to_owned(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            agora_core::api::errcode::NOT_FOUND,
            message,
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            agora_core::api::errcode::FORBIDDEN,
            message,
        )
    }

    pub fn bad_json(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            agora_core::api::errcode::BAD_JSON,
            message,
        )
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            agora_core::api::errcode::UNKNOWN,
            message,
        )
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            agora_core::api::errcode::TOO_LARGE,
            message,
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse {
            errcode: self.errcode,
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

impl From<crate::store::StorageError> for ApiError {
    fn from(e: crate::store::StorageError) -> Self {
        tracing::error!("storage error: {e}");
        ApiError::unknown("internal database error")
    }
}
