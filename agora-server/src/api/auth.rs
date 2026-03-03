use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use agora_core::api::*;
use agora_core::identifiers::UserId;

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{AccessTokenRecord, UserRecord};

use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn generate_token() -> String {
    format!("agora_{}", uuid::Uuid::new_v4().simple())
}

fn generate_device_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_uppercase()
}

/// POST /_matrix/client/v3/register
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), ApiError> {
    let user_id = UserId::new(&req.username, &state.server_name);

    if state.store.get_user(user_id.as_str()).await?.is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            errcode::USER_IN_USE,
            "user already exists",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| ApiError::unknown(format!("password hash error: {e}")))?
        .to_string();

    let now = now_millis();

    state
        .store
        .create_user(&UserRecord {
            user_id: user_id.as_str().to_owned(),
            display_name: Some(req.username.clone()),
            password_hash,
            created_at: now,
        })
        .await?;

    let device_id = req.device_id.unwrap_or_else(generate_device_id);
    let token = generate_token();

    state
        .store
        .create_token(&AccessTokenRecord {
            token: token.clone(),
            user_id: user_id.as_str().to_owned(),
            device_id: device_id.clone(),
            created_at: now,
        })
        .await?;

    tracing::info!(%user_id, "user registered");

    Ok((
        StatusCode::OK,
        Json(RegisterResponse {
            user_id,
            access_token: token,
            device_id,
        }),
    ))
}

/// POST /_matrix/client/v3/login
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    if req.login_type != "m.login.password" {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            errcode::INVALID_PARAM,
            "only m.login.password is supported",
        ));
    }

    let username = req.user.as_deref().ok_or_else(|| {
        ApiError::bad_json("missing 'user' field")
    })?;

    let password = req.password.as_deref().ok_or_else(|| {
        ApiError::bad_json("missing 'password' field")
    })?;

    // Support both `@user:server` and bare `user` formats.
    let user_id_str = if username.starts_with('@') {
        username.to_owned()
    } else {
        format!("@{username}:{}", state.server_name)
    };

    let user = state
        .store
        .get_user(&user_id_str)
        .await?
        .ok_or_else(|| ApiError::forbidden("invalid username or password"))?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|e| ApiError::unknown(format!("stored hash invalid: {e}")))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| ApiError::forbidden("invalid username or password"))?;

    let device_id = req.device_id.unwrap_or_else(generate_device_id);
    let token = generate_token();

    state
        .store
        .create_token(&AccessTokenRecord {
            token: token.clone(),
            user_id: user_id_str.clone(),
            device_id: device_id.clone(),
            created_at: now_millis(),
        })
        .await?;

    let user_id = UserId::parse(&user_id_str)
        .map_err(|e| ApiError::unknown(format!("bad stored user_id: {e}")))?;

    tracing::info!(%user_id, "user logged in");

    Ok(Json(LoginResponse {
        user_id,
        access_token: token,
        device_id,
    }))
}

/// GET /_matrix/client/v3/account/whoami
pub async fn whoami(
    State(state): State<AppState>,
    crate::api::AuthUser(user_id, token): crate::api::AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device_id = state
        .store
        .get_token(&token)
        .await?
        .map(|t| t.device_id)
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "user_id": user_id.as_str(),
        "device_id": device_id,
    })))
}

/// POST /_matrix/client/v3/logout
pub async fn logout(
    State(state): State<AppState>,
    crate::api::AuthUser(user_id, token): crate::api::AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.store.delete_token(&token).await?;
    tracing::info!(%user_id, "user logged out");
    Ok(Json(serde_json::json!({})))
}
