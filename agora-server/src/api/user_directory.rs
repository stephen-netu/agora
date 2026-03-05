use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

const MAX_LIMIT: u64 = 50;

#[derive(Debug, serde::Deserialize)]
pub struct SearchRequest {
    pub search_term: String,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    10
}

#[derive(Debug, serde::Serialize)]
pub struct UserResult {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchResponse {
    pub results: Vec<UserResult>,
    pub limited: bool,
}

/// POST /_matrix/client/v3/user_directory/search
pub async fn search_users(
    State(state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    let search_term = req.search_term.trim().to_owned();
    if search_term.len() < 2 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "M_INVALID_PARAM",
            "search_term must be at least 2 characters",
        ));
    }
    if search_term.len() > 128 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "M_INVALID_PARAM",
            "search_term must not exceed 128 characters",
        ));
    }

    // Clamp limit to [1, MAX_LIMIT]. Over-fetch by 1 so we can accurately
    // report `limited` without a separate COUNT query.
    let limit = req.limit.clamp(1, MAX_LIMIT);
    let fetch_limit = limit + 1;

    let records = state
        .store
        .search_users(&search_term, fetch_limit)
        .await
        .map_err(|e| ApiError::unknown(format!("user search failed: {e}")))?;

    let limited = records.len() as u64 > limit;

    let results = records
        .into_iter()
        .take(limit as usize)
        .map(|r| UserResult {
            user_id: r.user_id,
            display_name: r.display_name,
            avatar_url: r.avatar_url,
        })
        .collect();

    Ok(Json(SearchResponse { results, limited }))
}
