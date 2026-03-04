use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::Value;
use tauri::State;

use agora_crypto::AgentId;

use super::machine::{CryptoMachine, DecryptedPayload, DeviceInfo, EncryptedPayload};

pub struct CryptoState(pub Mutex<Option<CryptoMachine>>);

fn crypto_data_dir() -> Result<std::path::PathBuf, String> {
    let base = dirs_next::data_dir()
        .or_else(|| dirs_next::home_dir().map(|h| h.join(".local/share")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let dir = base.join("agora").join("crypto");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    Ok(dir)
}

#[tauri::command]
pub fn init_crypto(
    state: State<CryptoState>,
    user_id: String,
    device_id: String,
) -> Result<Value, String> {
    let data_dir = crypto_data_dir()?;
    let machine = CryptoMachine::new(&data_dir, &user_id, &device_id)?;
    let device_keys = machine.device_keys_payload();
    *state.0.lock().unwrap() = Some(machine);
    Ok(device_keys)
}

#[tauri::command]
pub fn generate_otks(
    state: State<CryptoState>,
    current_count: u64,
) -> Result<BTreeMap<String, Value>, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    Ok(machine.generate_one_time_keys(current_count))
}

#[tauri::command]
pub fn needs_otk_upload(
    state: State<CryptoState>,
    server_counts: BTreeMap<String, u64>,
) -> Result<bool, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.needs_more_otks(&server_counts))
}

#[tauri::command]
pub fn encrypt_message(
    state: State<CryptoState>,
    room_id: String,
    event_type: String,
    content: Value,
) -> Result<EncryptedPayload, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.encrypt_room_event(&room_id, &event_type, &content)
}

#[tauri::command]
pub fn decrypt_event(
    state: State<CryptoState>,
    room_id: String,
    sender_key: String,
    session_id: String,
    ciphertext: String,
) -> Result<DecryptedPayload, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.decrypt_group(&room_id, &sender_key, &session_id, &ciphertext)
}

#[tauri::command]
pub fn get_room_key_content(
    state: State<CryptoState>,
    room_id: String,
) -> Result<Option<Value>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine
        .get_room_key_content(&room_id)
        .map(|k| serde_json::to_value(k).unwrap()))
}

#[tauri::command]
pub fn devices_needing_keys(
    state: State<CryptoState>,
    room_id: String,
    all_devices: Vec<DeviceInfo>,
) -> Result<Vec<DeviceInfo>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.devices_needing_session(&room_id, &all_devices))
}

#[tauri::command]
pub fn create_olm_session_from_otk(
    state: State<CryptoState>,
    their_curve_key: String,
    one_time_key: String,
    otk_counter: Option<u64>,
) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.create_outbound_olm_from_otk(&their_curve_key, &one_time_key, otk_counter)
}

#[tauri::command]
pub fn encrypt_olm_event(
    state: State<CryptoState>,
    recipient_curve_key: String,
    recipient_ed_key: String,
    plaintext: String,
) -> Result<Value, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.encrypt_olm(&recipient_curve_key, &recipient_ed_key, &plaintext)
}

#[tauri::command]
pub fn mark_keys_shared(
    state: State<CryptoState>,
    room_id: String,
    user_id: String,
    device_id: String,
) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine
        .mark_session_shared(&room_id, &user_id, &device_id)
        .map_err(|e| format!("mark session shared: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn process_sync_crypto(
    state: State<CryptoState>,
    to_device_events: Vec<Value>,
) -> Result<Vec<String>, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    Ok(machine.process_to_device_events(&to_device_events))
}

#[tauri::command]
pub fn get_identity_keys(state: State<CryptoState>) -> Result<(String, String), String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.identity_keys())
}

// ── Sigchain commands ─────────────────────────────────────────────────────────

/// Initialise (or restore) the agent sigchain identity.
/// Safe to call on every startup — no-op if already initialised.
#[tauri::command]
pub fn init_sigchain(state: State<CryptoState>) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.init_sigchain()
}

/// Return the hex-encoded `AgentId` (64 hex chars) for this device's sigchain.
/// Returns `null` JSON if the sigchain is not yet initialised.
#[tauri::command]
pub fn get_agent_id(state: State<CryptoState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.agent_id_hex())
}

/// Return whether `correlation_path` (hex AgentIds) contains this agent's id.
///
/// Callers MUST check this before `append_sigchain_action`. If `true`, call
/// `append_sigchain_refusal` instead and surface an error to the UI.
#[tauri::command]
pub fn check_sigchain_loop(
    state: State<CryptoState>,
    correlation_path: Vec<String>,
) -> Result<bool, String> {
    let path: Result<Vec<AgentId>, String> = correlation_path
        .iter()
        .map(|hex| AgentId::from_hex(hex).map_err(|e| format!("invalid AgentId hex: {e}")))
        .collect();
    let path = path?;

    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.has_loop_in_path(&path))
}

/// Append a `Refusal` link when a loop is detected in the correlation path.
///
/// Records the refusal on-chain (auditable). Returns the link JSON for
/// optional publication to the server.
#[tauri::command]
pub fn append_sigchain_refusal(
    state: State<CryptoState>,
    refused_event_type: String,
    correlation_path: Vec<String>,
) -> Result<Value, String> {
    let path: Result<Vec<AgentId>, String> = correlation_path
        .iter()
        .map(|hex| AgentId::from_hex(hex).map_err(|e| format!("invalid AgentId hex: {e}")))
        .collect();
    let path = path?;

    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;

    let link = machine.append_refusal_link(&refused_event_type, path)?;
    serde_json::to_value(&link).map_err(|e| format!("serialize link: {e}"))
}

/// Proof returned by `append_sigchain_action` — the minimum data the frontend
/// needs to include a `sigchain_proof` field in the outgoing event content.
#[derive(serde::Serialize)]
pub struct SigchainActionProof {
    /// Sequence number of the new Action link.
    pub seqno: u64,
    /// Hex-encoded `AgentId` of the signing agent.
    pub agent_id: String,
}

/// Append an `Action` link to the local sigchain for an outgoing Matrix event.
///
/// - `event_type`: Matrix event type string (e.g. `"m.room.message"`).
/// - `room_id`:    Matrix room ID — will be BLAKE3-hashed before storage.
/// - `content`:    Event content JSON — will be BLAKE3-hashed before storage.
/// - `correlation_path`: upstream agent hex IDs (max 16, empty for top-level).
///
/// Returns `{ seqno, agent_id }` — include this as `sigchain_proof` in the
/// outgoing event content so verifiers can cross-reference the link.
#[tauri::command]
pub fn append_sigchain_action(
    state: State<CryptoState>,
    event_type: String,
    room_id: String,
    content: Value,
    correlation_path: Vec<String>,
) -> Result<SigchainActionProof, String> {
    // Decode hex-encoded AgentIds.
    let path: Result<Vec<AgentId>, String> = correlation_path
        .iter()
        .map(|hex| AgentId::from_hex(hex).map_err(|e| format!("invalid AgentId hex: {e}")))
        .collect();
    let path = path?;

    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;

    let link = machine.append_action_link(&event_type, &room_id, &content, path)?;
    let agent_id = machine.agent_id_hex().ok_or("sigchain not initialized")?;
    Ok(SigchainActionProof { seqno: link.seqno, agent_id })
}
