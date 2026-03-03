use axum::extract::{Path, State};
use axum::Json;
use tokio::time::{Duration, Instant};

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}
pub async fn set_typing(
    State(state): State<AppState>,
    AuthUser(auth_user, _): AuthUser,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.as_str() != user_id {
        return Err(ApiError::forbidden("can only set own typing status"));
    }

    let typing = body.get("typing").and_then(|v| v.as_bool()).unwrap_or(false);
    let timeout_ms = body.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30000);

    let mut map = state.typing.lock().await;
    let room_typing = map.entry(room_id).or_default();

    if typing {
        room_typing.insert(user_id, Instant::now() + Duration::from_millis(timeout_ms));
    } else {
        room_typing.remove(&user_id);
    }

    Ok(Json(serde_json::json!({})))
}

pub async fn get_typing_users(state: &AppState, room_id: &str) -> Vec<String> {
    let mut map = state.typing.lock().await;
    let now = Instant::now();
    if let Some(room_typing) = map.get_mut(room_id) {
        room_typing.retain(|_, expiry| *expiry > now);
        room_typing.keys().cloned().collect()
    } else {
        vec![]
    }
}
