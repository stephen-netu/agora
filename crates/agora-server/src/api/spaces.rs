use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::Json;

use agora_core::api::{HierarchyQuery, HierarchyResponse, HierarchyRoom};
use agora_core::events::event_type;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// GET /_matrix/client/v1/rooms/{roomId}/hierarchy
pub async fn get_hierarchy(
    State(state): State<AppState>,
    AuthUser(user_id, _): AuthUser,
    Path(room_id): Path<String>,
    Query(query): Query<HierarchyQuery>,
) -> Result<Json<HierarchyResponse>, ApiError> {
    let membership = state
        .store
        .get_membership(&room_id, user_id.as_str())
        .await?;

    if membership.as_deref() != Some("join") {
        return Err(ApiError::forbidden("not a member of this room"));
    }

    let mut result = Vec::new();
    let mut visited = HashSet::new();

    walk_hierarchy(
        &state,
        &room_id,
        0,
        query.max_depth,
        query.limit as usize,
        query.suggested_only,
        &mut result,
        &mut visited,
    )
    .await?;

    Ok(Json(HierarchyResponse { rooms: result }))
}

#[async_recursion::async_recursion]
async fn walk_hierarchy(
    state: &AppState,
    room_id: &str,
    depth: u64,
    max_depth: u64,
    limit: usize,
    suggested_only: bool,
    result: &mut Vec<HierarchyRoom>,
    visited: &mut HashSet<String>,
) -> Result<(), ApiError> {
    if visited.contains(room_id) || result.len() >= limit {
        return Ok(());
    }
    visited.insert(room_id.to_owned());

    let room = match state.store.get_room(room_id).await? {
        Some(r) => r,
        None => return Ok(()),
    };

    let state_events = state.store.get_state_events(room_id).await?;

    let children_state: Vec<_> = state_events
        .iter()
        .filter(|e| e.event_type == event_type::SPACE_CHILD && e.state_key.is_some())
        .filter(|e| {
            // An empty content object means the child was removed
            e.content.as_object().map_or(false, |o| !o.is_empty())
        })
        .filter(|e| {
            if !suggested_only {
                return true;
            }
            e.content
                .get("suggested")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let num_joined = state.store.count_room_members(room_id).await?;

    result.push(HierarchyRoom {
        room_id: room.room_id.clone(),
        name: room.name,
        topic: room.topic,
        num_joined_members: num_joined,
        room_type: room.room_type.clone(),
        children_state: children_state.clone(),
    });

    if depth < max_depth {
        for child_event in &children_state {
            if result.len() >= limit {
                break;
            }
            if let Some(child_id) = &child_event.state_key {
                walk_hierarchy(
                    state,
                    child_id,
                    depth + 1,
                    max_depth,
                    limit,
                    suggested_only,
                    result,
                    visited,
                )
                .await?;
            }
        }
    }

    Ok(())
}
