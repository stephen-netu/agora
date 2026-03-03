use axum::extract::{Path, Query, State};
use axum::Json;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /_matrix/client/v3/publicRooms
pub async fn get_public_rooms(
    State(state): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit: u64 = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let rooms = state.store.get_public_rooms(limit, None).await?;
    let chunks: Vec<serde_json::Value> = rooms
        .iter()
        .map(|r| {
            serde_json::json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "num_joined_members": 0,
                "world_readable": false,
                "guest_can_join": false,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "chunk": chunks,
        "total_room_count_estimate": chunks.len(),
    })))
}

/// POST /_matrix/client/v3/publicRooms
pub async fn search_public_rooms(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(50);
    let search_term = body
        .get("filter")
        .and_then(|f| f.get("generic_search_term"))
        .and_then(|v| v.as_str());

    let rooms = state
        .store
        .get_public_rooms(limit, search_term)
        .await?;

    let chunks: Vec<serde_json::Value> = rooms
        .iter()
        .map(|r| {
            serde_json::json!({
                "room_id": r.room_id,
                "name": r.name,
                "topic": r.topic,
                "num_joined_members": 0,
                "world_readable": false,
                "guest_can_join": false,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "chunk": chunks,
        "total_room_count_estimate": chunks.len(),
    })))
}

/// GET /_matrix/client/v3/directory/room/{roomAlias}
pub async fn get_room_alias(
    State(state): State<AppState>,
    Path(room_alias): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let room_id = state
        .store
        .get_room_alias(&room_alias)
        .await?
        .ok_or_else(|| ApiError::not_found("alias not found"))?;
    Ok(Json(serde_json::json!({
        "room_id": room_id,
        "servers": [state.server_name],
    })))
}

/// PUT /_matrix/client/v3/directory/room/{roomAlias}
pub async fn create_room_alias(
    State(state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Path(room_alias): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_json("missing room_id"))?;
    state
        .store
        .create_room_alias(&room_alias, room_id)
        .await?;
    Ok(Json(serde_json::json!({})))
}

/// DELETE /_matrix/client/v3/directory/room/{roomAlias}
pub async fn delete_room_alias(
    State(state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Path(room_alias): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.delete_room_alias(&room_alias).await?;
    Ok(Json(serde_json::json!({})))
}
