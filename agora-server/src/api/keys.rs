use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;

use agora_core::api::*;

use crate::api::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{DeviceKeysRecord, OneTimeKeyRecord};

/// POST /_matrix/client/v3/keys/upload
pub async fn upload_keys(
    State(state): State<AppState>,
    AuthUser(user_id, token): AuthUser,
    Json(body): Json<KeysUploadRequest>,
) -> Result<Json<KeysUploadResponse>, ApiError> {
    let device_id = state
        .store
        .get_token(&token)
        .await?
        .map(|t| t.device_id)
        .ok_or_else(|| ApiError::unknown("token lookup failed"))?;

    if let Some(dk) = body.device_keys {
        if dk.user_id != user_id.as_str() || dk.device_id != device_id {
            return Err(ApiError::forbidden(
                "device_keys user_id/device_id mismatch",
            ));
        }
        // S-02: deterministic timestamp instead of SystemTime::now()
        let ts = state.timestamp.next_timestamp()?;
        let record = DeviceKeysRecord {
            user_id: dk.user_id.clone(),
            device_id: dk.device_id.clone(),
            algorithms_json: serde_json::to_string(&dk.algorithms)
                .map_err(|e| ApiError::bad_json(e.to_string()))?,
            keys_json: serde_json::to_string(&dk.keys)
                .map_err(|e| ApiError::bad_json(e.to_string()))?,
            signatures_json: serde_json::to_string(&dk.signatures)
                .map_err(|e| ApiError::bad_json(e.to_string()))?,
            created_at: ts as i64,
        };
        state.store.upsert_device_keys(&record).await?;
    }

    if let Some(otks) = body.one_time_key {
        let mut records = Vec::new();
        for (full_key_id, key_value) in &otks {
            let (algorithm, _key_id) = full_key_id
                .split_once(':')
                .ok_or_else(|| ApiError::bad_json("invalid one_time_key id format"))?;
            let key_data = serde_json::to_string(key_value)
                .map_err(|e| ApiError::bad_json(e.to_string()))?;
            records.push(OneTimeKeyRecord {
                user_id: user_id.as_str().to_owned(),
                device_id: device_id.clone(),
                key_id: full_key_id.clone(),
                algorithm: algorithm.to_owned(),
                key_data,
            });
        }
        if !records.is_empty() {
            state.store.store_one_time_keys(&records).await?;
        }
    }

    let counts = state
        .store
        .count_one_time_keys(user_id.as_str(), &device_id)
        .await?;

    Ok(Json(KeysUploadResponse {
        one_time_key_counts: counts,
    }))
}

/// POST /_matrix/client/v3/keys/query
pub async fn query_keys(
    State(state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Json(body): Json<KeysQueryRequest>,
) -> Result<Json<KeysQueryResponse>, ApiError> {
    let pairs: Vec<(String, Vec<String>)> = body
        .device_keys
        .into_iter()
        .map(|(uid, dids)| (uid, dids))
        .collect();

    let records = state.store.get_device_keys_for_users(&pairs).await?;

    let mut result: BTreeMap<String, BTreeMap<String, DeviceKeysPayload>> = BTreeMap::new();
    for r in records {
        let algorithms: Vec<String> =
            serde_json::from_str(&r.algorithms_json).unwrap_or_default();
        let keys: BTreeMap<String, String> =
            serde_json::from_str(&r.keys_json).unwrap_or_default();
        let signatures: BTreeMap<String, BTreeMap<String, String>> =
            serde_json::from_str(&r.signatures_json).unwrap_or_default();

        let payload = DeviceKeysPayload {
            user_id: r.user_id.clone(),
            device_id: r.device_id.clone(),
            algorithms,
            keys,
            signatures,
        };
        result
            .entry(r.user_id)
            .or_default()
            .insert(r.device_id, payload);
    }

    Ok(Json(KeysQueryResponse {
        device_keys: result,
    }))
}

/// POST /_matrix/client/v3/keys/claim
pub async fn claim_keys(
    State(state): State<AppState>,
    AuthUser(_user_id, _): AuthUser,
    Json(body): Json<KeysClaimRequest>,
) -> Result<Json<KeysClaimResponse>, ApiError> {
    let mut result: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();

    for (uid, devices) in &body.one_time_keys {
        for (did, algorithm) in devices {
            if let Some(key) = state.store.claim_one_time_key(uid, did, algorithm).await? {
                let key_value: serde_json::Value =
                    serde_json::from_str(&key.key_data).unwrap_or(serde_json::Value::Null);
                let mut key_obj = serde_json::Map::new();
                key_obj.insert(key.key_id, key_value);
                result
                    .entry(uid.clone())
                    .or_default()
                    .insert(did.clone(), serde_json::Value::Object(key_obj));
            }
        }
    }

    Ok(Json(KeysClaimResponse {
        one_time_keys: result,
    }))
}
