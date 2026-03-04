use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::Value;
use tauri::State;

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
