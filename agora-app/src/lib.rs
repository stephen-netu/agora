#![warn(
    missing_docs,
    rust_2018_idioms,
    unused_import_braces,
    unused_qualifications,
    clippy::all,
    clippy::pedantic
)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

//! Tauri-based desktop application for Agora.

mod crypto;

use crypto::commands::CryptoState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(CryptoState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            crypto::commands::init_crypto,
            crypto::commands::generate_otks,
            crypto::commands::needs_otk_upload,
            crypto::commands::encrypt_message,
            crypto::commands::decrypt_event,
            crypto::commands::get_room_key_content,
            crypto::commands::devices_needing_keys,
            crypto::commands::create_olm_session_from_otk,
            crypto::commands::encrypt_olm_event,
            crypto::commands::mark_keys_shared,
            crypto::commands::process_sync_crypto,
            crypto::commands::get_identity_keys,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
