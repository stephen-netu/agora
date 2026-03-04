//! Sigchain API — `/_agora/sigchain/{agent_id}`.
//!
//! Agents publish signed `SigchainLink`s here. The server verifies each link
//! before storing it, ensuring chain integrity at write time.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use agora_crypto::{Sigchain, SigchainLink};

use crate::error::ApiError;
use crate::state::AppState;
use crate::store::{SigchainLinkRecord, Storage};

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SinceQuery {
    since: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PublishResponse {
    seqno: u64,
    canonical_hash: String,
}

#[derive(Debug, Serialize)]
pub struct GetChainResponse {
    agent_id: String,
    links: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    valid: bool,
    length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `PUT /_agora/sigchain/{agent_id}` — publish a new link.
///
/// The caller submits a JSON-encoded `SigchainLink`. The server:
/// 1. Parses the link.
/// 2. Validates the `agent_id` path param matches the link's signer (for Genesis).
/// 3. Loads the existing chain and verifies the full chain + new link.
/// 4. Stores the link.
pub async fn publish_link(
    State(state): State<AppState>,
    Path(agent_id_hex): Path<String>,
    Json(link): Json<SigchainLink>,
) -> Result<Json<PublishResponse>, ApiError> {
    let agent_id = agora_crypto::AgentId::from_hex(&agent_id_hex)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, "AGORA_SIGCHAIN_INVALID", e.to_string()))?;

    // Load the existing chain for this agent (empty vec if none stored yet).
    let existing_records = state.store.get_sigchain(agent_id.as_bytes()).await
        .map_err(|e| ApiError::unknown(format!("storage error: {e}")))?;

    let mut existing_links: Vec<SigchainLink> = Vec::with_capacity(existing_records.len());
    for rec in &existing_records {
        let parsed: SigchainLink = serde_json::from_str(&rec.link_json)
            .map_err(|e| ApiError::unknown(format!("corrupt stored link at seqno {}: {e}", rec.seqno)))?;
        existing_links.push(parsed);
    }

    // Build verification chain = existing + new link.
    let mut chain = Sigchain {
        agent_id: agent_id.clone(),
        links: existing_links,
    };
    chain.links.push(link.clone());

    // Full chain verification.
    chain.verify_chain().map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, "AGORA_SIGCHAIN_INVALID", e.to_string())
    })?;

    // Compute canonical hash for the new link.
    let canonical_hash = link.canonical_hash()
        .map_err(|e| ApiError::unknown(format!("hash computation failed: {e}")))?;

    let link_json = serde_json::to_string(&link)
        .map_err(|e| ApiError::unknown(format!("serialization failed: {e}")))?;

    let ts = state.timestamp.next_timestamp()
        .map_err(|_| ApiError::unknown("sequence overflow"))?;

    let record = SigchainLinkRecord {
        agent_id: *agent_id.as_bytes(),
        seqno: link.seqno,
        link_json,
        canonical_hash,
        link_type: link.body.variant_name().to_string(),
        created_at: ts,
    };

    state.store.store_sigchain_link(&record).await.map_err(|e| {
        match e {
            crate::store::StorageError::Conflict(msg) => {
                ApiError::new(StatusCode::CONFLICT, "AGORA_SIGCHAIN_CONFLICT", msg)
            }
            other => ApiError::unknown(format!("storage error: {other}")),
        }
    })?;

    Ok(Json(PublishResponse {
        seqno: link.seqno,
        canonical_hash: canonical_hash.iter().map(|b| format!("{b:02x}")).collect(),
    }))
}

/// `GET /_agora/sigchain/{agent_id}[?since=N]` — fetch the chain.
pub async fn get_chain(
    State(state): State<AppState>,
    Path(agent_id_hex): Path<String>,
    Query(params): Query<SinceQuery>,
) -> Result<Json<GetChainResponse>, ApiError> {
    let agent_id = agora_crypto::AgentId::from_hex(&agent_id_hex)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, "AGORA_SIGCHAIN_INVALID", e.to_string()))?;

    let records = match params.since {
        Some(since) => {
            state.store.get_sigchain_since(agent_id.as_bytes(), since).await
        }
        None => {
            state.store.get_sigchain(agent_id.as_bytes()).await
        }
    }
    .map_err(|e| ApiError::unknown(format!("storage error: {e}")))?;

    if records.is_empty() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "AGORA_SIGCHAIN_NOT_FOUND", "agent has no sigchain"));
    }

    let links: Vec<Value> = records
        .iter()
        .map(|rec| {
            serde_json::from_str(&rec.link_json).unwrap_or(Value::Null)
        })
        .collect();

    Ok(Json(GetChainResponse {
        agent_id: agent_id_hex,
        links,
    }))
}

/// `GET /_agora/sigchain/{agent_id}/verify` — verify chain integrity.
pub async fn verify_chain(
    State(state): State<AppState>,
    Path(agent_id_hex): Path<String>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let agent_id = agora_crypto::AgentId::from_hex(&agent_id_hex)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, "AGORA_SIGCHAIN_INVALID", e.to_string()))?;

    let records = state.store.get_sigchain(agent_id.as_bytes()).await
        .map_err(|e| ApiError::unknown(format!("storage error: {e}")))?;

    let length = records.len();

    if records.is_empty() {
        return Ok(Json(VerifyResponse {
            valid: false,
            length: 0,
            error: Some("agent has no sigchain".into()),
        }));
    }

    let mut links: Vec<SigchainLink> = Vec::with_capacity(records.len());
    for rec in &records {
        match serde_json::from_str::<SigchainLink>(&rec.link_json) {
            Ok(link) => links.push(link),
            Err(e) => {
                return Ok(Json(VerifyResponse {
                    valid: false,
                    length,
                    error: Some(format!("corrupt stored link at seqno {}: {e}", rec.seqno)),
                }));
            }
        }
    }

    let chain = Sigchain {
        agent_id: agent_id.clone(),
        links,
    };

    match chain.verify_chain() {
        Ok(()) => Ok(Json(VerifyResponse { valid: true, length, error: None })),
        Err(e) => Ok(Json(VerifyResponse {
            valid: false,
            length,
            error: Some(e.to_string()),
        })),
    }
}
