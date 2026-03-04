use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CryptoStoreData {
    pub account_pickle: Option<String>,
    /// Olm sessions keyed by sender Curve25519 identity key.
    /// Each key maps to a list of pickled sessions (multiple sessions per device possible).
    #[serde(default)]
    pub olm_sessions: BTreeMap<String, Vec<String>>,
    /// Outbound Megolm sessions keyed by room_id -> pickled session.
    #[serde(default)]
    pub outbound_group_sessions: BTreeMap<String, OutboundGroupSessionData>,
    /// Inbound Megolm sessions keyed by "{room_id}|{sender_key}|{session_id}" -> pickled session.
    #[serde(default)]
    pub inbound_group_sessions: BTreeMap<String, InboundGroupSessionData>,
    /// Tracks which devices have received room keys for each outbound session.
    /// Keyed by room_id -> set of "{user_id}|{device_id}".
    #[serde(default)]
    pub shared_sessions: BTreeMap<String, Vec<String>>,
    /// Message indices seen per inbound session for replay prevention.
    /// Keyed by session composite key -> list of (message_index, event_id).
    #[serde(default)]
    pub seen_indices: BTreeMap<String, Vec<(u32, String)>>,
    /// 32-byte agent identity seed, stored as hex. Used to derive `AgentIdentity`
    /// and sigchain signer. `None` until `init_sigchain()` is called.
    #[serde(default)]
    pub identity_seed_hex: Option<String>,
    /// Full sigchain serialized as JSON. `None` until `init_sigchain()` is called.
    #[serde(default)]
    pub sigchain_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundGroupSessionData {
    pub pickle: String,
    pub session_id: String,
    pub created_at: u64,
    pub message_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundGroupSessionData {
    pub pickle: String,
    pub sender_key: String,
    pub signing_key: Option<String>,
    pub room_id: String,
}

pub struct CryptoStore {
    path: PathBuf,
    pub data: CryptoStoreData,
}

impl CryptoStore {
    pub fn open(data_dir: &std::path::Path, user_id: &str, device_id: &str) -> Self {
        let dir = data_dir.join("crypto");
        std::fs::create_dir_all(&dir).ok();

        let filename = format!(
            "{}_{}.json",
            user_id.replace(':', "_").replace('@', ""),
            device_id
        );
        let path = dir.join(filename);

        let data = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                Err(_) => CryptoStoreData::default(),
            }
        } else {
            CryptoStoreData::default()
        };

        Self { path, data }
    }

    pub fn save(&self) -> Result<(), String> {
        let json =
            serde_json::to_string_pretty(&self.data).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&self.path, json).map_err(|e| format!("write: {e}"))?;
        Ok(())
    }

    pub fn inbound_session_key(room_id: &str, sender_key: &str, session_id: &str) -> String {
        format!("{room_id}|{sender_key}|{session_id}")
    }

    pub fn shared_device_key(user_id: &str, device_id: &str) -> String {
        format!("{user_id}|{device_id}")
    }
}
