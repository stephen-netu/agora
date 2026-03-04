use axum::extract::{Path, State};
use axum::Json;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /_matrix/client/v3/devices
pub async fn list_devices(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tokens = state.store.get_devices_for_user(user_id.as_str()).await?;

    let mut seen = std::collections::BTreeMap::new();
    for t in &tokens {
        seen.entry(t.device_id.clone()).or_insert_with(|| {
            serde_json::json!({
                "device_id": t.device_id,
                "last_seen_ts": t.created_at,
            })
        });
    }

    let devices: Vec<_> = seen.into_values().collect();
    Ok(Json(serde_json::json!({ "devices": devices })))
}

/// GET /_matrix/client/v3/devices/{deviceId}
pub async fn get_device(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tokens = state.store.get_devices_for_user(user_id.as_str()).await?;
    let device = tokens
        .iter()
        .find(|t| t.device_id == device_id)
        .ok_or_else(|| ApiError::not_found("device not found"))?;

    Ok(Json(serde_json::json!({
        "device_id": device.device_id,
        "last_seen_ts": device.created_at,
    })))
}

/// PUT /_matrix/client/v3/devices/{deviceId}
pub async fn update_device(
    State(_state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Path(_device_id): Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(serde_json::json!({})))
}

/// DELETE /_matrix/client/v3/devices/{deviceId}
pub async fn delete_device(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .delete_device(user_id.as_str(), &device_id)
        .await?;
    Ok(Json(serde_json::json!({})))
}
