use axum::extract::{Path, State};
use axum::Json;

use agora_core::api::*;
use agora_core::events::{event_type, Membership, RoomEvent, RoomMemberContent, RoomNameContent, RoomTopicContent};
use agora_core::identifiers::{EventId, RoomId};

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;
use crate::store::RoomRecord;

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// POST /_matrix/client/v3/createRoom
pub async fn create_room(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Json(req): Json<CreateRoomRequest>,
) -> Result<Json<CreateRoomResponse>, ApiError> {
    let room_id = RoomId::new(&state.server_name);
    let now = now_millis();

    state
        .store
        .create_room(&RoomRecord {
            room_id: room_id.as_str().to_owned(),
            name: req.name.clone(),
            topic: req.topic.clone(),
            creator: user_id.as_str().to_owned(),
            created_at: now as i64,
        })
        .await?;

    // Creator joins automatically.
    state
        .store
        .set_membership(room_id.as_str(), user_id.as_str(), "join", now as i64)
        .await?;

    // Store m.room.create state event.
    let create_event = RoomEvent {
        event_id: EventId::new(),
        room_id: room_id.clone(),
        sender: user_id.clone(),
        event_type: event_type::CREATE.to_owned(),
        state_key: Some(String::new()),
        content: serde_json::json!({ "creator": user_id.as_str() }),
        origin_server_ts: now,
        stream_ordering: None,
    };
    let ordering = state.store.store_event(&create_event).await?;
    state.sync_engine.broadcast(room_id.as_str(), &create_event, ordering);

    // Store m.room.member state event for the creator.
    let member_event = RoomEvent {
        event_id: EventId::new(),
        room_id: room_id.clone(),
        sender: user_id.clone(),
        event_type: event_type::MEMBER.to_owned(),
        state_key: Some(user_id.as_str().to_owned()),
        content: serde_json::to_value(RoomMemberContent {
            membership: Membership::Join,
            displayname: Some(user_id.localpart().to_owned()),
        })
        .unwrap(),
        origin_server_ts: now,
        stream_ordering: None,
    };
    let ordering = state.store.store_event(&member_event).await?;
    state.sync_engine.broadcast(room_id.as_str(), &member_event, ordering);

    // Optional name state event.
    if let Some(name) = &req.name {
        let name_event = RoomEvent {
            event_id: EventId::new(),
            room_id: room_id.clone(),
            sender: user_id.clone(),
            event_type: event_type::NAME.to_owned(),
            state_key: Some(String::new()),
            content: serde_json::to_value(RoomNameContent { name: name.clone() }).unwrap(),
            origin_server_ts: now,
            stream_ordering: None,
        };
        let ordering = state.store.store_event(&name_event).await?;
        state.sync_engine.broadcast(room_id.as_str(), &name_event, ordering);
    }

    // Optional topic state event.
    if let Some(topic) = &req.topic {
        let topic_event = RoomEvent {
            event_id: EventId::new(),
            room_id: room_id.clone(),
            sender: user_id.clone(),
            event_type: event_type::TOPIC.to_owned(),
            state_key: Some(String::new()),
            content: serde_json::to_value(RoomTopicContent {
                topic: topic.clone(),
            })
            .unwrap(),
            origin_server_ts: now,
            stream_ordering: None,
        };
        let ordering = state.store.store_event(&topic_event).await?;
        state.sync_engine.broadcast(room_id.as_str(), &topic_event, ordering);
    }

    tracing::info!(%user_id, %room_id, "room created");

    Ok(Json(CreateRoomResponse { room_id }))
}

/// POST /_matrix/client/v3/join/{roomIdOrAlias}
pub async fn join_room(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id_or_alias): Path<String>,
) -> Result<Json<JoinRoomResponse>, ApiError> {
    let room_id_str = room_id_or_alias;

    state
        .store
        .get_room(&room_id_str)
        .await?
        .ok_or_else(|| ApiError::not_found("room not found"))?;

    let now = now_millis();

    state
        .store
        .set_membership(&room_id_str, user_id.as_str(), "join", now as i64)
        .await?;

    let room_id = RoomId::parse(&room_id_str)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    let member_event = RoomEvent {
        event_id: EventId::new(),
        room_id: room_id.clone(),
        sender: user_id.clone(),
        event_type: event_type::MEMBER.to_owned(),
        state_key: Some(user_id.as_str().to_owned()),
        content: serde_json::to_value(RoomMemberContent {
            membership: Membership::Join,
            displayname: Some(user_id.localpart().to_owned()),
        })
        .unwrap(),
        origin_server_ts: now,
        stream_ordering: None,
    };
    let ordering = state.store.store_event(&member_event).await?;
    state.sync_engine.broadcast(room_id.as_str(), &member_event, ordering);

    tracing::info!(%user_id, %room_id, "joined room");

    Ok(Json(JoinRoomResponse { room_id }))
}

/// POST /_matrix/client/v3/rooms/{roomId}/leave
pub async fn leave_room(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .store
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("room not found"))?;

    let now = now_millis();

    state
        .store
        .set_membership(&room_id, user_id.as_str(), "leave", now as i64)
        .await?;

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    let member_event = RoomEvent {
        event_id: EventId::new(),
        room_id: rid,
        sender: user_id.clone(),
        event_type: event_type::MEMBER.to_owned(),
        state_key: Some(user_id.as_str().to_owned()),
        content: serde_json::to_value(RoomMemberContent {
            membership: Membership::Leave,
            displayname: None,
        })
        .unwrap(),
        origin_server_ts: now,
        stream_ordering: None,
    };
    let ordering = state.store.store_event(&member_event).await?;
    state.sync_engine.broadcast(&room_id, &member_event, ordering);

    tracing::info!(%user_id, room_id, "left room");

    Ok(Json(serde_json::json!({})))
}
