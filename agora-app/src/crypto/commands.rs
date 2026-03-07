use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::Value;
use tauri::State;

use agora_crypto::{AgentId, agent_display_name};

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

/// Initialize the crypto machine for a user and device.
#[tauri::command]
pub fn init_crypto(
    state: State<'_, CryptoState>,
    user_id: String,
    device_id: String,
) -> Result<Value, String> {
    let data_dir = crypto_data_dir()?;
    let machine = CryptoMachine::new(&data_dir, &user_id, &device_id)?;
    let device_keys = machine.device_keys_payload();
    *state.0.lock().unwrap() = Some(machine);
    Ok(device_keys)
}

/// Generate one-time keys for the current device.
#[tauri::command]
pub fn generate_otks(
    state: State<'_, CryptoState>,
    current_count: u64,
) -> Result<BTreeMap<String, Value>, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    Ok(machine.generate_one_time_keys(current_count))
}

#[tauri::command]
pub fn needs_otk_upload(
    state: State<'_, CryptoState>,
    server_counts: BTreeMap<String, u64>,
) -> Result<bool, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.needs_more_otks(&server_counts))
}

#[tauri::command]
pub fn encrypt_message(
    state: State<'_, CryptoState>,
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
    state: State<'_, CryptoState>,
    room_id: String,
    sender_key: String,
    session_id: String,
    ciphertext: String,
) -> Result<DecryptedPayload, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.decrypt_group(&room_id, &sender_key, &session_id, &ciphertext)
}

/// Get the room key content for a given room.
#[tauri::command]
pub fn get_room_key_content(
    state: State<'_, CryptoState>,
    room_id: String,
) -> Result<Option<Value>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine
        .get_room_key_content(&room_id)
        .map(|k| serde_json::to_value(k).unwrap()))
}

/// Get devices that need room keys for a given room.
#[tauri::command]
pub fn devices_needing_keys(
    state: State<'_, CryptoState>,
    room_id: String,
    all_devices: Vec<DeviceInfo>,
) -> Result<Vec<DeviceInfo>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.devices_needing_session(&room_id, &all_devices))
}

/// Create an outbound Olm session from a one-time key.
#[tauri::command]
pub fn create_olm_session_from_otk(
    state: State<'_, CryptoState>,
    their_curve_key: String,
    one_time_key: String,
    otk_counter: Option<u64>,
) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.create_outbound_olm_from_otk(&their_curve_key, &one_time_key, otk_counter)
}

/// Encrypt an Olm message for a recipient device.
#[tauri::command]
pub fn encrypt_olm_event(
    state: State<'_, CryptoState>,
    recipient_curve_key: String,
    recipient_ed_key: String,
    plaintext: String,
) -> Result<Value, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.encrypt_olm(&recipient_curve_key, &recipient_ed_key, &plaintext)
}

/// Mark that room keys have been shared with a device.
#[tauri::command]
pub fn mark_keys_shared(
    state: State<'_, CryptoState>,
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

/// Process incoming to-device events from sync.
#[tauri::command]
pub fn process_sync_crypto(
    state: State<'_, CryptoState>,
    to_device_events: Vec<Value>,
) -> Result<Vec<String>, String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    Ok(machine.process_to_device_events(&to_device_events))
}

/// Get the identity keys (curve and ed25519) for this device.
#[tauri::command]
pub fn get_identity_keys(state: State<'_, CryptoState>) -> Result<(String, String), String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.identity_keys())
}

// ── Sigchain commands ─────────────────────────────────────────────────────────

/// Initialise (or restore) the agent sigchain identity.
/// Safe to call on every startup — no-op if already initialised.
#[tauri::command]
pub fn init_sigchain(state: State<'_, CryptoState>) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    let machine = guard.as_mut().ok_or("crypto not initialized")?;
    machine.init_sigchain()
}

/// Return the hex-encoded `AgentId` (64 hex chars) for this device's sigchain.
/// Returns `null` JSON if the sigchain is not yet initialised.
#[tauri::command]
pub fn get_agent_id(state: State<'_, CryptoState>) -> Result<Option<String>, String> {
    let guard = state.0.lock().unwrap();
    let machine = guard.as_ref().ok_or("crypto not initialized")?;
    Ok(machine.agent_id_hex())
}

/// Return the human-readable deterministic display name for an AgentId.
/// 
/// Takes a 64-character hex-encoded AgentId and returns a string in the format
/// word1-word2#NNNN (e.g., "clever-fox#5678").
#[tauri::command]
pub fn get_agent_display_name(agent_id_hex: String) -> Result<String, String> {
    let agent_id = AgentId::from_hex(&agent_id_hex)
        .map_err(|e| format!("invalid AgentId hex: {e}"))?;
    Ok(agent_display_name(&agent_id))
}


/// Return whether `correlation_path` (hex AgentIds) contains this agent's id.
///
/// Callers MUST check this before `append_sigchain_action`. If `true`, call
/// `append_sigchain_refusal` instead and surface an error to the UI.
#[tauri::command]
pub fn check_sigchain_loop(
    state: State<'_, CryptoState>,
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
    state: State<'_, CryptoState>,
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
    state: State<'_, CryptoState>,
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

#[cfg(test)]
mod tests {
    use super::*;
    
    // ─────────────────────────────────────────────────────────────────────────────
    // get_agent_display_name command tests
    // ─────────────────────────────────────────────────────────────────────────────
    
    #[test]
    fn test_get_agent_display_name_valid_hex() {
        // Test with all zeros
        let result = get_agent_display_name(
            "0000000000000000000000000000000000000000000000000000000000000000".to_string()
        );
        assert!(result.is_ok());
        let name = result.unwrap();
        assert!(name.contains('-'));
        assert!(name.contains('#'));
    }
    
    #[test]
    fn test_get_agent_display_name_known_vector() {
        // Test with a known AgentId - from agora-crypto test vectors
        let hex = "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";
        let result = get_agent_display_name(hex.to_string());
        assert!(result.is_ok(), "Failed with: {:?}", result);
        let name = result.unwrap();
        
        // Should be in format word-word#NNNN
        assert!(name.contains('-'));
        assert!(name.contains('#'));
        let parts: Vec<&str> = name.split('#').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 4);
    }
    
    #[test]
    fn test_get_agent_display_name_invalid_hex() {
        // Test with invalid hex (odd number of characters)
        let result = get_agent_display_name("abc".to_string());
        assert!(result.is_err());
        
        // Test with invalid characters
        let result = get_agent_display_name(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_string()
        );
        assert!(result.is_err());
    }
    
    #[test]
    fn test_get_agent_display_name_wrong_length() {
        // Test with too short hex
        let result = get_agent_display_name("abc".to_string());
        assert!(result.is_err());
        
        // Test with too long hex (65 chars instead of 64)
        let result = get_agent_display_name(
            "00000000000000000000000000000000000000000000000000000000000000000".to_string()
        );
        assert!(result.is_err());
    }
    
    #[test]
    fn test_get_agent_display_name_deterministic() {
        let hex = "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";
        
        let result1 = get_agent_display_name(hex.to_string()).unwrap();
        let result2 = get_agent_display_name(hex.to_string()).unwrap();
        
        assert_eq!(result1, result2, "Same hex should produce same display name");
    }
    
    #[test]
    fn test_get_agent_display_name_different_ids_different_names() {
        // Change byte[0] to get different adjectives (first byte in hex is first 2 chars)
        let hex1 = "0000000000000000000000000000000000000000000000000000000000000000";
        let hex2 = "0100000000000000000000000000000000000000000000000000000000000000";
        
        let name1 = get_agent_display_name(hex1.to_string()).unwrap();
        let name2 = get_agent_display_name(hex2.to_string()).unwrap();
        
        // They should differ - changing first byte changes adjective
        assert_ne!(name1, name2, "Different AgentIds should produce different names");
    }
    
    #[test]
    fn test_get_agent_display_name_matches_agora_crypto() {
        // Verify that the Tauri command produces the same result as the direct function
        use agora_crypto::{AgentId, agent_display_name};
        
        let hex = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        
        let command_result = get_agent_display_name(hex.to_string()).unwrap();
        let direct_result = {
            let agent_id = AgentId::from_hex(hex).unwrap();
            agent_display_name(&agent_id)
        };
        
        assert_eq!(
            command_result, direct_result,
            "Tauri command should produce same result as direct function call"
        );
    }
}
