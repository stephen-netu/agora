pub mod auth;
pub mod events;
pub mod keys;
pub mod media;
pub mod rooms;
pub mod spaces;
pub mod sync;
pub mod to_device;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Router;

use agora_core::api::errcode;
use agora_core::identifiers::UserId;

use crate::error::ApiError;
use crate::state::AppState;

/// Auth extractor — pulls the access token from the Authorization header
/// and resolves it to a UserId.
pub struct AuthUser(pub UserId, pub String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    errcode::MISSING_TOKEN,
                    "missing access token",
                )
            })?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    errcode::MISSING_TOKEN,
                    "malformed Authorization header",
                )
            })?;

        let record = state
            .store
            .get_token(token)
            .await
            .map_err(|_| ApiError::unknown("token lookup failed"))?
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    errcode::UNKNOWN_TOKEN,
                    "unknown or expired access token",
                )
            })?;

        let user_id = UserId::parse(&record.user_id)
            .map_err(|e| ApiError::unknown(format!("bad stored user_id: {e}")))?;

        Ok(AuthUser(user_id, token.to_owned()))
    }
}

/// Build the full API router.
pub fn router(state: AppState) -> Router {
    use axum::routing::{delete, get, post, put};

    let matrix = Router::new()
        // Auth
        .route("/v3/register", post(auth::register))
        .route("/v3/login", post(auth::login))
        .route("/v3/logout", post(auth::logout))
        // Rooms
        .route("/v3/createRoom", post(rooms::create_room))
        .route("/v3/join/{room_id_or_alias}", post(rooms::join_room))
        .route("/v3/rooms/{room_id}/leave", post(rooms::leave_room))
        .route("/v3/rooms/{room_id}", delete(rooms::delete_room))
        // Events
        .route(
            "/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
            put(events::send_event),
        )
        .route("/v3/rooms/{room_id}/messages", get(events::get_messages))
        .route(
            "/v3/rooms/{room_id}/state/{event_type}/{state_key}",
            put(events::set_state).get(events::get_state_event),
        )
        .route(
            "/v3/rooms/{room_id}/state/{event_type}",
            put(events::set_state_empty_key).get(events::get_state_event_empty_key),
        )
        .route("/v3/rooms/{room_id}/state", get(events::get_all_state))
        // Sync
        .route("/v3/sync", get(sync::sync))
        // E2EE: Key management
        .route("/v3/keys/upload", post(keys::upload_keys))
        .route("/v3/keys/query", post(keys::query_keys))
        .route("/v3/keys/claim", post(keys::claim_keys))
        // E2EE: To-device messaging
        .route(
            "/v3/sendToDevice/{event_type}/{txn_id}",
            put(to_device::send_to_device),
        )
        // Spaces
        .route("/v1/rooms/{room_id}/hierarchy", get(spaces::get_hierarchy));

    let media = Router::new()
        .route("/v3/upload", post(media::upload))
        .route("/v3/download/{server_name}/{media_id}", get(media::download))
        .route(
            "/v3/download/{server_name}/{media_id}/{filename}",
            get(media::download),
        )
        .route("/v3/config", get(media::config));

    Router::new()
        .route("/_matrix/client/versions", get(versions))
        .nest("/_matrix/client", matrix)
        .nest("/_matrix/media", media)
        .with_state(state)
}

async fn versions() -> axum::Json<agora_core::api::VersionsResponse> {
    axum::Json(agora_core::api::VersionsResponse {
        versions: vec!["v1.11".to_owned()],
    })
}
