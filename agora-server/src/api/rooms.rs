use axum::extract::{Path, Query, State};
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

    let room_type = req
        .creation_content
        .as_ref()
        .and_then(|cc| cc.get("type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    state
        .store
        .create_room(&RoomRecord {
            room_id: room_id.as_str().to_owned(),
            name: req.name.clone(),
            topic: req.topic.clone(),
            creator: user_id.as_str().to_owned(),
            created_at: now as i64,
            room_type: room_type.clone(),
        })
        .await?;

    // Creator joins automatically.
    state
        .store
        .set_membership(room_id.as_str(), user_id.as_str(), "join", now as i64)
        .await?;

    // Build m.room.create content, merging creation_content if provided.
    let mut create_content = serde_json::json!({ "creator": user_id.as_str() });
    if let Some(cc) = &req.creation_content {
        if let (Some(base), Some(extra)) = (create_content.as_object_mut(), cc.as_object()) {
            for (k, v) in extra {
                base.insert(k.clone(), v.clone());
            }
        }
    }

    let create_event = RoomEvent {
        event_id: EventId::new(),
        room_id: room_id.clone(),
        sender: user_id.clone(),
        event_type: event_type::CREATE.to_owned(),
        state_key: Some(String::new()),
        content: create_content,
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

    for invited_user in &req.invite {
        let invite_ts = now_millis();
        state
            .store
            .set_membership(room_id.as_str(), invited_user.as_str(), "invite", invite_ts as i64)
            .await?;
        let invite_event = RoomEvent {
            event_id: EventId::new(),
            room_id: room_id.clone(),
            sender: user_id.clone(),
            event_type: event_type::MEMBER.to_owned(),
            state_key: Some(invited_user.as_str().to_owned()),
            content: serde_json::to_value(RoomMemberContent {
                membership: Membership::Invite,
                displayname: None,
            }).unwrap(),
            origin_server_ts: invite_ts,
            stream_ordering: None,
        };
        let ordering = state.store.store_event(&invite_event).await?;
        state.sync_engine.broadcast(room_id.as_str(), &invite_event, ordering);
    }

    tracing::info!(%user_id, %room_id, "room created");

    Ok(Json(CreateRoomResponse { room_id }))
}

/// DELETE /_matrix/client/v3/rooms/{roomId}
/// Agora extension: only the room creator can delete a room.
pub async fn delete_room(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let room = state
        .store
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("room not found"))?;

    if room.creator != user_id.as_str() {
        return Err(ApiError::forbidden("only the room creator can delete it"));
    }

    state.store.delete_room(&room_id).await?;

    tracing::info!(%user_id, room_id, "room deleted");

    Ok(Json(serde_json::json!({})))
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

/// GET /_matrix/client/v3/joined_rooms
pub async fn joined_rooms(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rooms = state.store.get_joined_rooms(user_id.as_str()).await?;
    Ok(Json(serde_json::json!({ "joined_rooms": rooms })))
}

/// POST /_matrix/client/v3/rooms/{roomId}/invite
pub async fn invite_to_room(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let membership = state.store.get_membership(&room_id, user_id.as_str()).await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let target_user = body.get("user_id").and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_json("missing user_id"))?;

    let now = now_millis();
    state.store.set_membership(&room_id, target_user, "invite", now as i64).await?;

    let rid = RoomId::parse(&room_id)
        .map_err(|e| ApiError::bad_json(format!("invalid room id: {e}")))?;

    let member_event = RoomEvent {
        event_id: EventId::new(),
        room_id: rid,
        sender: user_id.clone(),
        event_type: event_type::MEMBER.to_owned(),
        state_key: Some(target_user.to_owned()),
        content: serde_json::to_value(RoomMemberContent {
            membership: Membership::Invite,
            displayname: None,
        }).unwrap(),
        origin_server_ts: now,
        stream_ordering: None,
    };
    let ordering = state.store.store_event(&member_event).await?;
    state.sync_engine.broadcast(&room_id, &member_event, ordering);

    Ok(Json(serde_json::json!({})))
}

/// GET /_matrix/client/v3/rooms/{roomId}/joined_members
pub async fn get_joined_members(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let membership = state.store.get_membership(&room_id, user_id.as_str()).await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let members = state.store.get_room_members(&room_id).await?;
    let mut joined = serde_json::Map::new();
    for m in members {
        if m.membership == "join" {
            let user = state.store.get_user(&m.user_id).await?;
            let display_name = user.as_ref().and_then(|u| u.display_name.as_deref());
            let avatar_url = if let Some(u) = &user {
                state.store.get_avatar_url(&u.user_id).await.ok().flatten()
            } else {
                None
            };
            joined.insert(m.user_id, serde_json::json!({
                "display_name": display_name,
                "avatar_url": avatar_url,
            }));
        }
    }
    Ok(Json(serde_json::json!({ "joined": joined })))
}

/// GET /_matrix/client/v3/rooms/{roomId}/members
pub async fn get_members(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let membership = state.store.get_membership(&room_id, user_id.as_str()).await?;
    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("you are not a member of this room"));
    }

    let _members = state.store.get_room_members(&room_id).await?;
    let filter_membership = query.get("membership").map(|s| s.as_str());

    let state_events = state.store.get_state_events(&room_id).await?;
    let member_events: Vec<_> = state_events
        .into_iter()
        .filter(|e| {
            e.event_type == "m.room.member"
                && filter_membership.map_or(true, |f| {
                    e.content.get("membership").and_then(|v| v.as_str()) == Some(f)
                })
        })
        .collect();

    Ok(Json(serde_json::json!({ "chunk": member_events })))
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
