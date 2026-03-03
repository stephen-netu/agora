use axum::extract::{Path, State};
use axum::Json;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /_matrix/client/v3/profile/{userId}
pub async fn get_profile(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = state
        .store
        .get_user(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    let avatar_url = state.store.get_avatar_url(&user_id).await.ok().flatten();
    Ok(Json(serde_json::json!({
        "displayname": user.display_name,
        "avatar_url": avatar_url,
    })))
}

/// GET /_matrix/client/v3/profile/{userId}/displayname
pub async fn get_displayname(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = state
        .store
        .get_user(&user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    Ok(Json(serde_json::json!({ "displayname": user.display_name })))
}

/// PUT /_matrix/client/v3/profile/{userId}/displayname
pub async fn set_displayname(
    State(state): State<AppState>,
    AuthUser(auth_user, _): AuthUser,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.as_str() != user_id {
        return Err(ApiError::forbidden("can only set own displayname"));
    }
    let name = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    state.store.update_display_name(&user_id, name).await?;
    Ok(Json(serde_json::json!({})))
}

/// GET /_matrix/client/v3/profile/{userId}/avatar_url
pub async fn get_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let avatar_url = state.store.get_avatar_url(&user_id).await.ok().flatten();
    Ok(Json(serde_json::json!({ "avatar_url": avatar_url })))
}

/// PUT /_matrix/client/v3/profile/{userId}/avatar_url
pub async fn set_avatar(
    State(state): State<AppState>,
    AuthUser(auth_user, _): AuthUser,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.as_str() != user_id {
        return Err(ApiError::forbidden("can only set own avatar"));
    }
    let url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    state.store.update_avatar_url(&user_id, url).await?;
    Ok(Json(serde_json::json!({})))
}
