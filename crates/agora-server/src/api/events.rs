use axum::extract::{Path, Query, State};
use axum::Json;

use agora_core::api::*;
use agora_core::events::RoomEvent;
use agora_core::identifiers::{EventId, RoomId};

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}
pub async fn send_event(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((room_id, event_type, _txn_id)): Path<(String, String, String)>,
    Json(content): Json<serde_json::Value>,
) -> Result<Json<SendEventResponse>, ApiError> {
    // Verify membership.
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    let event_id = EventId::new();
    let event = RoomEvent {
        event_id: event_id.clone(),
        room_id: rid,
        sender: user_id,
        event_type,
        state_key: None,
        content,
        origin_server_ts: now_millis(),
        stream_ordering: None,
    };

    let ordering = state.store.store_event(&event).await?;
    state.sync_engine.broadcast(&room_id, &event, ordering);

    Ok(Json(SendEventResponse {
        event_id: event_id.as_str().to_owned(),
    }))
}

/// GET /_matrix/client/v3/rooms/{roomId}/messages
pub async fn get_messages(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<MessagesResponse>, ApiError> {
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let from_ordering = query.from.as_deref().and_then(|s| s.parse::<i64>().ok());
    let forward = query.dir == "f";

    let events = state
        .store
        .get_events_in_room(&room_id, from_ordering, query.limit, forward)
        .await?;

    let end = events
        .last()
        .and_then(|e| e.stream_ordering)
        .map(|o| o.to_string());

    let start = query
        .from
        .unwrap_or_else(|| from_ordering.unwrap_or(0).to_string());

    Ok(Json(MessagesResponse {
        start,
        end,
        chunk: events,
    }))
}

/// PUT /_matrix/client/v3/rooms/{roomId}/state/{eventType}/{stateKey}
pub async fn set_state(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    Json(content): Json<serde_json::Value>,
) -> Result<Json<SendEventResponse>, ApiError> {
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    let event_id = EventId::new();
    let event = RoomEvent {
        event_id: event_id.clone(),
        room_id: rid,
        sender: user_id,
        event_type,
        state_key: Some(state_key),
        content,
        origin_server_ts: now_millis(),
        stream_ordering: None,
    };

    let ordering = state.store.store_event(&event).await?;
    state.sync_engine.broadcast(&room_id, &event, ordering);

    Ok(Json(SendEventResponse {
        event_id: event_id.as_str().to_owned(),
    }))
}

/// GET /_matrix/client/v3/rooms/{roomId}/state/{eventType}/{stateKey}
pub async fn get_state_event(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let all_state = state.store.get_state_events(&room_id).await?;
    let event = all_state
        .into_iter()
        .find(|e| e.event_type == event_type && e.state_key.as_deref() == Some(&state_key))
        .ok_or_else(|| ApiError::not_found("state event not found"))?;

    Ok(Json(event.content))
}

/// GET /_matrix/client/v3/rooms/{roomId}/state
pub async fn get_all_state(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
) -> Result<Json<Vec<RoomEvent>>, ApiError> {
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let events = state.store.get_state_events(&room_id).await?;
    Ok(Json(events))
}
