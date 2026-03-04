use axum::extract::{Path, Query, State};
use axum::Json;

use agora_core::api::*;
use agora_core::events::RoomEvent;
use agora_core::identifiers::{EventId, RoomId};

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}
pub async fn send_event(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((room_id, event_type, txn_id)): Path<(String, String, String)>,
    Json(content): Json<serde_json::Value>,
) -> Result<Json<SendEventResponse>, ApiError> {
    if let Some(existing) = state.store.get_txn_event_id(user_id.as_str(), &txn_id).await? {
        return Ok(Json(SendEventResponse { event_id: existing }));
    }

    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    // S-02: deterministic timestamp + content-addressed event ID
    let ts = state.timestamp.next_timestamp()?;
    let content_bytes = serde_json::to_vec(&content)
        .map_err(|e| ApiError::bad_json(format!("content serialization: {e}")))?;
    let event_id_str = agora_crypto::ids::event_id(
        &room_id,
        user_id.as_str(),
        &event_type,
        &content_bytes,
        ts,
    );
    let event_id = EventId::parse(&event_id_str)
        .map_err(|e| ApiError::unknown(format!("event id parse: {e}")))?;

    let event = RoomEvent {
        event_id: event_id.clone(),
        room_id: rid,
        sender: user_id.clone(),
        event_type,
        state_key: None,
        content,
        origin_server_ts: ts,
        stream_ordering: None,
    };

    let ordering = state.store.store_event(&event).await?;
    state.sync_engine.broadcast(&room_id, &event, ordering);
    state.store.store_txn(user_id.as_str(), &txn_id, event_id.as_str()).await?;

    Ok(Json(SendEventResponse {
        event_id: event_id.as_str().to_owned(),
    }))
}

/// PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}
pub async fn redact_event(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path((room_id, target_event_id, txn_id)): Path<(String, String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<SendEventResponse>, ApiError> {
    if let Some(existing) = state.store.get_txn_event_id(user_id.as_str(), &txn_id).await? {
        return Ok(Json(SendEventResponse { event_id: existing }));
    }

    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    state.store.redact_event(&target_event_id).await?;

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    // S-02: deterministic timestamp + content-addressed event ID
    let ts = state.timestamp.next_timestamp()?;
    let reason = body.get("reason").and_then(|v| v.as_str()).unwrap_or("");
    let content = serde_json::json!({ "redacts": target_event_id, "reason": reason });
    let content_bytes = serde_json::to_vec(&content)
        .map_err(|e| ApiError::bad_json(format!("content serialization: {e}")))?;
    let event_id_str = agora_crypto::ids::event_id(
        &room_id,
        user_id.as_str(),
        "m.room.redaction",
        &content_bytes,
        ts,
    );
    let event_id = EventId::parse(&event_id_str)
        .map_err(|e| ApiError::unknown(format!("event id parse: {e}")))?;

    let redaction_event = RoomEvent {
        event_id: event_id.clone(),
        room_id: rid,
        sender: user_id.clone(),
        event_type: "m.room.redaction".to_owned(),
        state_key: None,
        content,
        origin_server_ts: ts,
        stream_ordering: None,
    };

    let ordering = state.store.store_event(&redaction_event).await?;
    state.sync_engine.broadcast(&room_id, &redaction_event, ordering);
    state.store.store_txn(user_id.as_str(), &txn_id, event_id.as_str()).await?;

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

    let ts = state.timestamp.next_timestamp()?;
    let content_bytes = serde_json::to_vec(&content)
        .map_err(|e| ApiError::bad_json(format!("content serialization: {e}")))?;
    let event_id_str = agora_crypto::ids::event_id(
        &room_id,
        user_id.as_str(),
        &event_type,
        &content_bytes,
        ts,
    );
    let event_id = EventId::parse(&event_id_str)
        .map_err(|e| ApiError::unknown(format!("event id parse: {e}")))?;

    let event = RoomEvent {
        event_id: event_id.clone(),
        room_id: rid,
        sender: user_id,
        event_type,
        state_key: Some(state_key),
        content,
        origin_server_ts: ts,
        stream_ordering: None,
    };

    let ordering = state.store.store_event(&event).await?;
    state.sync_engine.broadcast(&room_id, &event, ordering);

    Ok(Json(SendEventResponse {
        event_id: event_id.as_str().to_owned(),
    }))
}

/// PUT /_matrix/client/v3/rooms/{roomId}/state/{eventType}  (empty state key)
pub async fn set_state_empty_key(
    state: State<AppState>,
    auth: AuthUser,
    Path((room_id, event_type)): Path<(String, String)>,
    body: Json<serde_json::Value>,
) -> Result<Json<SendEventResponse>, ApiError> {
    set_state(state, auth, Path((room_id, event_type, String::new())), body).await
}

/// GET /_matrix/client/v3/rooms/{roomId}/state/{eventType}  (empty state key)
pub async fn get_state_event_empty_key(
    state: State<AppState>,
    auth: AuthUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    get_state_event(state, auth, Path((room_id, event_type, String::new()))).await
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
