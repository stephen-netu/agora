use axum::extract::{Path, State, Json};
use axum::http::StatusCode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

use agora_core::events::presence::*;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

/// PUT /_matrix/client/v3/presence/{userId}/status
/// 
/// Update the user's presence status.
pub async fn set_presence(
    State(state): State<AppState>,
    AuthUser(auth_user, _): AuthUser,
    Path(user_id): Path<String>,
    Json(body): Json<SetPresenceRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if auth_user.as_str() != user_id {
        return Err(ApiError::forbidden("can only set own presence status"));
    }

    let now = state.timestamp.now().as_u64();

    let mut presence_map = state.presence.write().await;
    
    let record = PresenceRecord {
        user_id: user_id.clone(),
        presence: body.presence,
        last_active_at: now,
        status_msg: body.status_msg,
        currently_active: true,
    };

    presence_map.insert(user_id.clone(), record);

    // Broadcast presence change to all rooms the user is in
    drop(presence_map);
    broadcast_presence_change(&state, &user_id).await;

    Ok(Json(serde_json::json!({})))
}

/// GET /_matrix/client/v3/presence/{userId}/status
/// 
/// Get the user's presence status.
pub async fn get_presence(
    State(state): State<AppState>,
    AuthUser(_auth_user, _): AuthUser,
    Path(user_id): Path<String>,
) -> Result<Json<GetPresenceResponse>, ApiError> {
    let now = state.timestamp.now().as_u64();
    let presence_map = state.presence.read().await;

    if let Some(record) = presence_map.get(&user_id) {
        let mut record = record.clone();
        
        // Auto-transition to unavailable/offline based on inactivity
        let last_active_ago = record.last_active_ago(now);
        update_presence_from_inactivity(&mut record, last_active_ago);
        
        return Ok(Json(record.to_response(now)));
    }

    // Return offline if no presence record found
    Ok(Json(GetPresenceResponse {
        presence: PresenceState::Offline,
        last_active_ago: None,
        status_msg: None,
        currently_active: None,
    }))
}

/// POST /_matrix/client/v3/presence/heartbeat
/// 
/// Heartbeat endpoint for clients to ping and update last_active_ago.
/// This keeps the user's presence as "online" and resets the inactivity timer.
pub async fn heartbeat(
    State(state): State<AppState>,
    AuthUser(auth_user, _): AuthUser,
    Json(body): Json<Option<HeartbeatRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = state.timestamp.now().as_u64();
    let user_id = auth_user.as_str().to_owned();

    let mut presence_map = state.presence.write().await;

    if let Some(record) = presence_map.get_mut(&user_id) {
        // Update last active timestamp
        record.last_active_at = now;
        record.presence = PresenceState::Online;
        
        // Update currently_active if provided
        if let Some(req) = body {
            if let Some(active) = req.currently_active {
                record.currently_active = active;
            }
        } else {
            record.currently_active = true;
        }
    } else {
        // Create new presence record if none exists
        presence_map.insert(user_id.clone(), PresenceRecord {
            user_id: user_id.clone(),
            presence: PresenceState::Online,
            last_active_at: now,
            status_msg: None,
            currently_active: body.and_then(|r| r.currently_active).unwrap_or(true),
        });
    }

    drop(presence_map);
    broadcast_presence_change(&state, &user_id).await;

    Ok(Json(serde_json::json!({})))
}

/// GET /_matrix/client/v3/presence/list
/// 
/// Get presence for multiple users (batch endpoint for UI).
pub async fn get_presence_list(
    State(state): State<AppState>,
    AuthUser(_auth_user, _): AuthUser,
    Json(body): Json<Vec<String>>,
) -> Result<Json<HashMap<String, GetPresenceResponse>>, ApiError> {
    let now = state.timestamp.now().as_u64();
    let presence_map = state.presence.read().await;
    
    let mut result = HashMap::new();
    
    for user_id in body {
        if let Some(record) = presence_map.get(&user_id) {
            let mut record = record.clone();
            let last_active_ago = record.last_active_ago(now);
            update_presence_from_inactivity(&mut record, last_active_ago);
            result.insert(user_id, record.to_response(now));
        } else {
            result.insert(user_id, GetPresenceResponse {
                presence: PresenceState::Offline,
                last_active_ago: None,
                status_msg: None,
                currently_active: None,
            });
        }
    }
    
    Ok(Json(result))
}

/// Update presence state based on inactivity thresholds.
/// 
/// - > 5 minutes inactive -> unavailable
/// - > 30 minutes inactive -> offline
fn update_presence_from_inactivity(record: &mut PresenceRecord, last_active_ago: u64) {
    const UNAVAILABLE_THRESHOLD_MS: u64 = 5 * 60 * 1000;  // 5 minutes
    const OFFLINE_THRESHOLD_MS: u64 = 30 * 60 * 1000;     // 30 minutes

    match record.presence {
        PresenceState::Online if last_active_ago > OFFLINE_THRESHOLD_MS => {
            record.presence = PresenceState::Offline;
            record.currently_active = false;
        }
        PresenceState::Online if last_active_ago > UNAVAILABLE_THRESHOLD_MS => {
            record.presence = PresenceState::Unavailable;
            record.currently_active = false;
        }
        _ => {}
    }
}

/// Broadcast presence change to all rooms the user is a member of.
async fn broadcast_presence_change(state: &AppState, user_id: &str) {
    // Get rooms the user is joined to
    let rooms = match state.store.get_joined_rooms(user_id).await {
        Ok(r) => r,
        Err(_) => return,
    };

    let now = state.timestamp.now().as_u64();
    let presence_map = state.presence.read().await;
    
    let Some(record) = presence_map.get(user_id) else {
        return;
    };

    let event = record.to_event(now);
    drop(presence_map);

    // Broadcast to each room via sync engine
    for room_id in rooms {
        let _ = state.sync_engine.broadcast_presence(&room_id, event.clone());
    }
}

/// Get presence for a list of users (internal helper for sync).
pub async fn get_users_presence(
    state: &AppState,
    user_ids: &[String],
) -> Vec<PresenceEvent> {
    let now = state.timestamp.now().as_u64();
    let presence_map = state.presence.read().await;
    
    let mut events = Vec::new();
    
    for user_id in user_ids {
        if let Some(record) = presence_map.get(user_id) {
            let mut record = record.clone();
            let last_active_ago = record.last_active_ago(now);
            update_presence_from_inactivity(&mut record, last_active_ago);
            events.push(record.to_event(now));
        } else {
            // Include offline state for users without presence records
            events.push(PresenceEvent::new(
                user_id,
                PresenceContent::new(PresenceState::Offline),
            ));
        }
    }
    
    events
}

/// Clean up stale presence entries (called periodically).
pub async fn cleanup_stale_presence(state: &AppState) {
    let now = state.timestamp.now().as_u64();
    let mut presence_map = state.presence.write().await;
    
    const STALE_THRESHOLD_MS: u64 = 24 * 60 * 60 * 1000; // 24 hours
    
    presence_map.retain(|_user_id, record| {
        record.last_active_ago(now) < STALE_THRESHOLD_MS
    });
}
