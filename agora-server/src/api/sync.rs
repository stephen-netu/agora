use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::Json;

use agora_core::api::*;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /_matrix/client/v3/sync
///
/// Simplified Matrix sync: returns new timeline events for each joined room
/// since the `since` token (a stream_ordering value). Long-polls for up to
/// `timeout` milliseconds if there are no new events.
pub async fn sync(
    State(state): State<AppState>,
    AuthUser(user_id, token): AuthUser,
    Query(query): Query<SyncQuery>,
) -> Result<Json<SyncResponse>, ApiError> {
    let since: i64 = query
        .since
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let timeout = std::time::Duration::from_millis(query.timeout);
    let full_state = query.full_state.unwrap_or(false);

    let joined_rooms = state.store.get_joined_rooms(user_id.as_str()).await?;

    let device_id = state
        .store
        .get_token(&token)
        .await?
        .map(|t| t.device_id)
        .unwrap_or_default();

    // First pass: check for events already in the database.
    let mut join_map = BTreeMap::new();
    let mut max_ordering = since;

    for room_id in &joined_rooms {
        let events = state.store.get_events_since(room_id, since).await?;
        if let Some(last) = events.last() {
            if let Some(ord) = last.stream_ordering {
                max_ordering = max_ordering.max(ord);
            }
        }
        if !events.is_empty() {
            let state_events = if since == 0 || full_state {
                state.store.get_state_events(room_id).await?
            } else {
                events
                    .iter()
                    .filter(|e| e.state_key.is_some())
                    .cloned()
                    .collect()
            };
            join_map.insert(
                room_id.clone(),
                JoinedRoom {
                    timeline: Timeline {
                        events,
                        prev_batch: Some(since.to_string()),
                        limited: false,
                    },
                    state: RoomState {
                        events: state_events,
                    },
                    ephemeral: None,
                },
            );
        }
    }

    // If we already have events, return immediately.
    if !join_map.is_empty() || timeout.is_zero() {
        // For rooms with no new events, still include them on initial sync.
        if since == 0 {
            for room_id in &joined_rooms {
                if !join_map.contains_key(room_id) {
                    let state_events = state.store.get_state_events(room_id).await?;
                    join_map.insert(
                        room_id.clone(),
                        JoinedRoom {
                            timeline: Timeline::default(),
                            state: RoomState {
                                events: state_events,
                            },
                            ephemeral: None,
                        },
                    );
                }
            }
        }

        let (to_device, otk_counts) =
            collect_to_device(&state, user_id.as_str(), &device_id).await?;
        add_ephemeral_typing(&state, &mut join_map).await;
        let invite_map = collect_invites(&state, user_id.as_str()).await?;

        return Ok(Json(SyncResponse {
            next_batch: max_ordering.to_string(),
            rooms: SyncRooms {
                join: join_map,
                invite: invite_map,
                leave: BTreeMap::new(),
            },
            to_device: Some(to_device),
            device_one_time_keys_count: Some(otk_counts),
        }));
    }

    // Long-poll: subscribe to all joined rooms and wait for events.
    let mut receivers: Vec<(String, tokio::sync::broadcast::Receiver<crate::sync_engine::SyncEvent>)> = joined_rooms
        .iter()
        .map(|rid| (rid.clone(), state.sync_engine.subscribe(rid)))
        .collect();

    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        // Try to receive from any room.
        let mut got_event = false;

        for (room_id, rx) in &mut receivers {
            match rx.try_recv() {
                Ok(sync_event) => {
                    max_ordering = max_ordering.max(sync_event.stream_ordering);

                    let entry = join_map.entry(room_id.clone()).or_insert_with(|| {
                        JoinedRoom {
                            timeline: Timeline {
                                events: vec![],
                                prev_batch: Some(since.to_string()),
                                limited: false,
                            },
                            state: RoomState::default(),
                            ephemeral: None,
                        }
                    });
                    if sync_event.event.state_key.is_some() {
                        entry.state.events.push(sync_event.event.clone());
                    }
                    entry.timeline.events.push(sync_event.event);
                    got_event = true;
                }
                Err(_) => {}
            }
        }

        if got_event {
            let (to_device, otk_counts) =
                collect_to_device(&state, user_id.as_str(), &device_id).await?;
            add_ephemeral_typing(&state, &mut join_map).await;
            let invite_map = collect_invites(&state, user_id.as_str()).await?;

            return Ok(Json(SyncResponse {
                next_batch: max_ordering.to_string(),
                rooms: SyncRooms {
                    join: join_map,
                    invite: invite_map,
                    leave: BTreeMap::new(),
                },
                to_device: Some(to_device),
                device_one_time_keys_count: Some(otk_counts),
            }));
        }

        if tokio::time::Instant::now() >= deadline {
            break;
        }

        // Sleep briefly before polling again.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Timed out with no events.
    let current_max = state.store.get_max_stream_ordering().await?;
    let (to_device, otk_counts) =
        collect_to_device(&state, user_id.as_str(), &device_id).await?;
    let invite_map = collect_invites(&state, user_id.as_str()).await?;

    Ok(Json(SyncResponse {
        next_batch: current_max.to_string(),
        rooms: SyncRooms {
            join: BTreeMap::new(),
            invite: invite_map,
            leave: BTreeMap::new(),
        },
        to_device: Some(to_device),
        device_one_time_keys_count: Some(otk_counts),
    }))
}

async fn collect_invites(
    state: &AppState,
    user_id: &str,
) -> Result<BTreeMap<String, InvitedRoom>, ApiError> {
    let invited_rooms = state
        .store
        .get_rooms_with_membership(user_id, "invite")
        .await?;
    let mut invite_map = BTreeMap::new();
    for room_id in invited_rooms {
        let state_events = state.store.get_state_events(&room_id).await?;
        let stripped: Vec<_> = state_events
            .into_iter()
            .filter(|e| {
                matches!(
                    e.event_type.as_str(),
                    "m.room.create" | "m.room.name" | "m.room.member" | "m.room.join_rules"
                )
            })
            .collect();
        invite_map.insert(
            room_id,
            InvitedRoom {
                invite_state: RoomState { events: stripped },
            },
        );
    }
    Ok(invite_map)
}

async fn add_ephemeral_typing(
    state: &AppState,
    join_map: &mut BTreeMap<String, JoinedRoom>,
) {
    for (room_id, joined) in join_map.iter_mut() {
        let typing_users = crate::api::typing::get_typing_users(state, room_id).await;
        if !typing_users.is_empty() {
            joined.ephemeral = Some(EphemeralEvents {
                events: vec![serde_json::json!({
                    "type": "m.typing",
                    "content": { "user_ids": typing_users },
                })],
            });
        }
    }
}

async fn collect_to_device(
    state: &AppState,
    user_id: &str,
    device_id: &str,
) -> Result<(ToDevicePayload, BTreeMap<String, u64>), ApiError> {
    let messages = state.store.get_to_device_messages(user_id, device_id).await?;
    let ids: Vec<i64> = messages.iter().map(|m| m.id).collect();

    let events: Vec<ToDeviceEvent> = messages
        .into_iter()
        .filter_map(|m| {
            let content: serde_json::Value = serde_json::from_str(&m.content_json).ok()?;
            Some(ToDeviceEvent {
                sender: m.sender,
                event_type: m.event_type,
                content,
            })
        })
        .collect();

    if !ids.is_empty() {
        state.store.delete_to_device_messages(&ids).await?;
    }

    let otk_counts = state.store.count_one_time_keys(user_id, device_id).await?;

    Ok((ToDevicePayload { events }, otk_counts))
}
