use axum::extract::{Path, State};
use axum::Json;

use agora_core::api::*;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;
use crate::store::ToDeviceRecord;

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// PUT /_matrix/client/v3/sendToDevice/{eventType}/{txnId}
pub async fn send_to_device(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((event_type, _txn_id)): Path<(String, String)>,
    Json(body): Json<SendToDeviceRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut records = Vec::new();
    let ts = now_millis();

    for (recipient_user, devices) in &body.messages {
        for (recipient_device, content) in devices {
            let content_json = serde_json::to_string(content)
                .map_err(|e| ApiError::bad_json(e.to_string()))?;

            if recipient_device == "*" {
                let tokens_for_user: Vec<String> = {
                    let joined = state.store.get_joined_rooms(recipient_user).await?;
                    vec![recipient_user.clone()]
                };
                // Wildcard: send to all devices of the user.
                // We look up all access tokens (each has a device_id).
                // For simplicity, query device_keys table for all devices.
                let pairs = vec![(recipient_user.clone(), vec![])];
                let device_records = state.store.get_device_keys_for_users(&pairs).await?;
                for dr in device_records {
                    records.push(ToDeviceRecord {
                        id: 0,
                        recipient_user: recipient_user.clone(),
                        recipient_device: dr.device_id,
                        sender: user_id.as_str().to_owned(),
                        event_type: event_type.clone(),
                        content_json: content_json.clone(),
                        created_at: ts,
                    });
                }
            } else {
                records.push(ToDeviceRecord {
                    id: 0,
                    recipient_user: recipient_user.clone(),
                    recipient_device: recipient_device.clone(),
                    sender: user_id.as_str().to_owned(),
                    event_type: event_type.clone(),
                    content_json: content_json.clone(),
                    created_at: ts,
                });
            }
        }
    }

    if !records.is_empty() {
        state.store.queue_to_device(&records).await?;
    }

    Ok(Json(serde_json::json!({})))
}
