use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

// S-02: module-level monotonic counter; no SystemTime::now()
// ARCHITECTURE_PENDING: replace vodozemac (Olm/Megolm) with agora-crypto Double Ratchet +
// a group-session primitive once agora-crypto gains a Megolm-compatible broadcast ratchet.
// Tracked: stephen-netu/agora#3 (feat/agora-crypto)
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_ts() -> u64 {
    SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
}
use vodozemac::megolm::{
    GroupSession as OutboundGroupSession, InboundGroupSession, MegolmMessage, SessionConfig,
    SessionKey,
};
use vodozemac::olm::{
    Account, AccountPickle, OlmMessage, PreKeyMessage, Session as OlmSession,
    SessionConfig as OlmSessionConfig,
};
use vodozemac::Curve25519PublicKey;

use super::sessions::{
    pickle_inbound_group, pickle_olm_session, pickle_outbound_group, unpickle_inbound_group,
    unpickle_olm_session, unpickle_outbound_group,
};
use super::store::{CryptoStore, InboundGroupSessionData, OutboundGroupSessionData};

const MAX_OTK_COUNT: u64 = 50;
const OTK_UPLOAD_THRESHOLD: u64 = 25;
const MEGOLM_ROTATION_MESSAGE_COUNT: u64 = 100;

pub struct CryptoMachine {
    pub user_id: String,
    pub device_id: String,
    account: Account,
    store: CryptoStore,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EncryptedPayload {
    pub algorithm: String,
    pub sender_key: String,
    pub ciphertext: String,
    pub session_id: String,
    pub device_id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DecryptedPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub content: Value,
    pub room_id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RoomKeyContent {
    pub algorithm: String,
    pub room_id: String,
    pub session_id: String,
    pub session_key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub user_id: String,
    pub device_id: String,
    pub curve25519_key: String,
    pub ed25519_key: String,
}

fn pickle_account(account: &Account) -> String {
    serde_json::to_string(&account.pickle()).unwrap_or_default()
}

fn unpickle_account(s: &str) -> Result<Account, String> {
    let pickle: AccountPickle =
        serde_json::from_str(s).map_err(|e| format!("unpickle account: {e}"))?;
    Ok(Account::from_pickle(pickle))
}

impl CryptoMachine {
    pub fn new(data_dir: &std::path::Path, user_id: &str, device_id: &str) -> Self {
        let mut store = CryptoStore::open(data_dir, user_id, device_id);

        let account = if let Some(ref pickle_str) = store.data.account_pickle {
            unpickle_account(pickle_str).unwrap_or_else(|_| Account::new())
        } else {
            Account::new()
        };

        let mut machine = Self {
            user_id: user_id.to_owned(),
            device_id: device_id.to_owned(),
            account,
            store,
        };
        let _ = machine.persist_account();
        machine
    }

    fn persist_account(&mut self) -> Result<(), String> {
        self.store.data.account_pickle = Some(pickle_account(&self.account));
        self.store
            .save()
            .map_err(|e| format!("persist account: {e}"))?;
        Ok(())
    }

    pub fn identity_keys(&self) -> (String, String) {
        let keys = self.account.identity_keys();
        (keys.curve25519.to_base64(), keys.ed25519.to_base64())
    }

    pub fn device_keys_payload(&self) -> Value {
        let (curve, ed) = self.identity_keys();
        let algorithms = vec!["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"];

        let mut keys = BTreeMap::new();
        keys.insert(format!("curve25519:{}", self.device_id), curve);
        keys.insert(format!("ed25519:{}", self.device_id), ed);

        let payload = serde_json::json!({
            "user_id": self.user_id,
            "device_id": self.device_id,
            "algorithms": algorithms,
            "keys": keys,
        });

        let canonical = serde_json::to_string(&payload).unwrap();
        let signature = self.account.sign(&canonical);

        let mut sigs = BTreeMap::new();
        let mut user_sigs = BTreeMap::new();
        user_sigs.insert(format!("ed25519:{}", self.device_id), signature.to_base64());
        sigs.insert(self.user_id.clone(), user_sigs);

        serde_json::json!({
            "user_id": self.user_id,
            "device_id": self.device_id,
            "algorithms": algorithms,
            "keys": keys,
            "signatures": sigs,
        })
    }

    pub fn generate_one_time_keys(&mut self, server_count: u64) -> BTreeMap<String, Value> {
        let to_generate = MAX_OTK_COUNT.saturating_sub(server_count);
        if to_generate == 0 {
            return BTreeMap::new();
        }
        self.account
            .generate_one_time_keys(to_generate.min(MAX_OTK_COUNT) as usize);

        let otks = self.account.one_time_keys();
        let mut result = BTreeMap::new();

        for (key_id, key) in otks.iter() {
            let key_base64 = key.to_base64();
            let key_obj = serde_json::json!({ "key": key_base64 });
            let canonical = serde_json::to_string(&key_obj).unwrap();
            let signature = self.account.sign(&canonical);

            let mut sigs = BTreeMap::new();
            let mut user_sigs = BTreeMap::new();
            user_sigs.insert(format!("ed25519:{}", self.device_id), signature.to_base64());
            sigs.insert(self.user_id.clone(), user_sigs);

            let signed_key = serde_json::json!({
                "key": key_base64,
                "signatures": sigs,
            });

            let full_key_id = format!("signed_curve25519:{}", key_id.to_base64());
            result.insert(full_key_id, signed_key);
        }

        self.account.mark_keys_as_published();
        let _ = self.persist_account();
        result
    }

    pub fn needs_more_otks(&self, server_counts: &BTreeMap<String, u64>) -> bool {
        let current = server_counts.get("signed_curve25519").copied().unwrap_or(0);
        current < OTK_UPLOAD_THRESHOLD
    }

    // --- Megolm encryption ---

    pub fn encrypt_room_event(
        &mut self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> Result<EncryptedPayload, String> {
        let mut session = self.get_or_create_outbound_session(room_id)?;

        let plaintext_payload = serde_json::json!({
            "type": event_type,
            "content": content,
            "room_id": room_id,
        });
        let plaintext =
            serde_json::to_string(&plaintext_payload).map_err(|e| format!("serialize: {e}"))?;

        let ciphertext = session.encrypt(&plaintext);
        let session_id = session.session_id();
        let (sender_key, _) = self.identity_keys();

        let data = self
            .store
            .data
            .outbound_group_sessions
            .get_mut(room_id)
            .unwrap();
        data.message_count += 1;
        data.pickle = pickle_outbound_group(&session)?;
        self.store
            .save()
            .map_err(|e| format!("persist megolm state: {e}"))?;

        Ok(EncryptedPayload {
            algorithm: "m.megolm.v1.aes-sha2".to_owned(),
            sender_key,
            ciphertext: ciphertext.to_base64(),
            session_id,
            device_id: self.device_id.clone(),
        })
    }

    fn get_or_create_outbound_session(
        &mut self,
        room_id: &str,
    ) -> Result<OutboundGroupSession, String> {
        if let Some(data) = self.store.data.outbound_group_sessions.get(room_id) {
            if data.message_count < MEGOLM_ROTATION_MESSAGE_COUNT {
                return unpickle_outbound_group(&data.pickle);
            }
        }
        self.create_outbound_session(room_id)
    }

    fn create_outbound_session(&mut self, room_id: &str) -> Result<OutboundGroupSession, String> {
        let session = OutboundGroupSession::new(SessionConfig::version_1());
        let session_id = session.session_id();
        let session_key = session.session_key();

        let inbound = InboundGroupSession::new(&session_key, SessionConfig::version_1());
        let (sender_key, _) = self.identity_keys();

        let igs_key = CryptoStore::inbound_session_key(room_id, &sender_key, &session_id);
        self.store.data.inbound_group_sessions.insert(
            igs_key,
            InboundGroupSessionData {
                pickle: pickle_inbound_group(&inbound)?,
                sender_key: sender_key.clone(),
                signing_key: None,
                room_id: room_id.to_owned(),
            },
        );

        self.store.data.outbound_group_sessions.insert(
            room_id.to_owned(),
            OutboundGroupSessionData {
                pickle: pickle_outbound_group(&session)?,
                session_id,
                // S-02: deterministic timestamp, no SystemTime::now()
                created_at: next_ts(),
                message_count: 0,
            },
        );

        self.store.data.shared_sessions.remove(room_id);
        self.store
            .save()
            .map_err(|e| format!("persist megolm state: {e}"))?;
        Ok(session)
    }

    pub fn get_room_key_content(&self, room_id: &str) -> Option<RoomKeyContent> {
        let data = self.store.data.outbound_group_sessions.get(room_id)?;
        let session = unpickle_outbound_group(&data.pickle).ok()?;
        Some(RoomKeyContent {
            algorithm: "m.megolm.v1.aes-sha2".to_owned(),
            room_id: room_id.to_owned(),
            session_id: session.session_id(),
            session_key: session.session_key().to_base64(),
        })
    }

    pub fn devices_needing_session(
        &self,
        room_id: &str,
        all_devices: &[DeviceInfo],
    ) -> Vec<DeviceInfo> {
        let shared = self.store.data.shared_sessions.get(room_id);
        let (our_curve, _) = self.identity_keys();

        all_devices
            .iter()
            .filter(|d| {
                if d.user_id == self.user_id && d.device_id == self.device_id {
                    return false;
                }
                if d.curve25519_key == our_curve {
                    return false;
                }
                if let Some(shared_list) = shared {
                    let key = CryptoStore::shared_device_key(&d.user_id, &d.device_id);
                    !shared_list.contains(&key)
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    pub fn mark_session_shared(
        &mut self,
        room_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), String> {
        let key = CryptoStore::shared_device_key(user_id, device_id);
        self.store
            .data
            .shared_sessions
            .entry(room_id.to_owned())
            .or_default()
            .push(key);
        self.store
            .save()
            .map_err(|e| format!("persist megolm state: {e}"))?;
        Ok(())
    }

    // --- Olm: encrypt to-device message ---

    pub fn encrypt_olm(
        &mut self,
        recipient_curve_key: &str,
        recipient_ed_key: &str,
        plaintext: &str,
    ) -> Result<Value, String> {
        let mut session = self.get_or_create_olm_session(recipient_curve_key)?;
        let olm_message = session.encrypt(plaintext);

        self.store.data.olm_sessions.insert(
            recipient_curve_key.to_owned(),
            vec![pickle_olm_session(&session)?],
        );
        self.store
            .save()
            .map_err(|e| format!("persist olm state: {e}"))?;

        let (msg_type, body) = match olm_message {
            OlmMessage::PreKey(m) => (0u8, m.to_base64()),
            OlmMessage::Normal(m) => (1u8, m.to_base64()),
        };

        let (our_curve, _) = self.identity_keys();
        let mut ciphertext = serde_json::Map::new();
        ciphertext.insert(
            recipient_curve_key.to_owned(),
            serde_json::json!({
                "type": msg_type,
                "body": body,
            }),
        );

        Ok(serde_json::json!({
            "algorithm": "m.olm.v1.curve25519-aes-sha2",
            "sender_key": our_curve,
            "ciphertext": ciphertext,
        }))
    }

    fn get_or_create_olm_session(&mut self, curve_key_str: &str) -> Result<OlmSession, String> {
        if let Some(pickles) = self.store.data.olm_sessions.get(curve_key_str) {
            if let Some(last) = pickles.last() {
                return unpickle_olm_session(last);
            }
        }
        Err("no existing Olm session — must create outbound session from claimed OTK".to_owned())
    }

    pub fn create_outbound_olm_from_otk(
        &mut self,
        their_curve_key: &str,
        one_time_key_base64: &str,
    ) -> Result<(), String> {
        let their_curve = Curve25519PublicKey::from_base64(their_curve_key)
            .map_err(|e| format!("bad curve key: {e}"))?;
        let otk = Curve25519PublicKey::from_base64(one_time_key_base64)
            .map_err(|e| format!("bad otk: {e}"))?;

        let session =
            self.account
                .create_outbound_session(OlmSessionConfig::version_1(), their_curve, otk);

        self.store
            .data
            .olm_sessions
            .entry(their_curve_key.to_owned())
            .or_default()
            .push(pickle_olm_session(&session)?);
        self.persist_account()?;
        Ok(())
    }

    // --- Olm: decrypt incoming to-device ---

    pub fn decrypt_olm_event(
        &mut self,
        sender_key: &str,
        msg_type: u8,
        body: &str,
    ) -> Result<String, String> {
        let their_curve = Curve25519PublicKey::from_base64(sender_key)
            .map_err(|e| format!("bad sender_key: {e}"))?;

        let olm_message = if msg_type == 0 {
            OlmMessage::PreKey(
                PreKeyMessage::from_base64(body).map_err(|e| format!("bad prekey msg: {e}"))?,
            )
        } else {
            OlmMessage::Normal(
                vodozemac::olm::Message::from_base64(body).map_err(|e| format!("bad msg: {e}"))?,
            )
        };

        if let Some(pickles) = self.store.data.olm_sessions.get(sender_key) {
            for pickle_str in pickles.iter().rev() {
                let mut session = match unpickle_olm_session(pickle_str) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if let Ok(plaintext_bytes) = session.decrypt(&olm_message) {
                    self.store.data.olm_sessions.insert(
                        sender_key.to_owned(),
                        vec![pickle_olm_session(&session)
                            .map_err(|e| format!("persist olm state: {e}"))?],
                    );
                    self.store
                        .save()
                        .map_err(|e| format!("persist olm state: {e}"))?;
                    return String::from_utf8(plaintext_bytes)
                        .map_err(|e| format!("invalid utf8: {e}"));
                }
            }
        }

        if msg_type == 0 {
            if let OlmMessage::PreKey(ref prekey) = olm_message {
                let result = self
                    .account
                    .create_inbound_session(their_curve, prekey)
                    .map_err(|e| format!("create inbound session: {e}"))?;
                let session = result.session;
                self.store.data.olm_sessions.insert(
                    sender_key.to_owned(),
                    vec![pickle_olm_session(&session)
                        .map_err(|e| format!("persist olm state: {e}"))?],
                );
                self.persist_account()?;
                return String::from_utf8(result.plaintext)
                    .map_err(|e| format!("invalid utf8: {e}"));
            }
        }

        Err("unable to decrypt Olm message".to_owned())
    }

    // --- Megolm: decrypt ---

    pub fn import_room_key(
        &mut self,
        room_id: &str,
        sender_key: &str,
        session_id: &str,
        session_key_base64: &str,
    ) -> Result<(), String> {
        let session_key = SessionKey::from_base64(session_key_base64)
            .map_err(|e| format!("bad session_key: {e}"))?;
        let inbound = InboundGroupSession::new(&session_key, SessionConfig::version_1());

        let composite = CryptoStore::inbound_session_key(room_id, sender_key, session_id);
        self.store.data.inbound_group_sessions.insert(
            composite,
            InboundGroupSessionData {
                pickle: pickle_inbound_group(&inbound)?,
                sender_key: sender_key.to_owned(),
                signing_key: None,
                room_id: room_id.to_owned(),
            },
        );
        self.store
            .save()
            .map_err(|e| format!("persist megolm state: {e}"))?;
        Ok(())
    }

    pub fn decrypt_megolm(
        &mut self,
        room_id: &str,
        sender_key: &str,
        session_id: &str,
        ciphertext: &str,
    ) -> Result<DecryptedPayload, String> {
        let composite = CryptoStore::inbound_session_key(room_id, sender_key, session_id);
        let data = self
            .store
            .data
            .inbound_group_sessions
            .get(&composite)
            .ok_or_else(|| "unknown Megolm session".to_owned())?;

        let mut session = unpickle_inbound_group(&data.pickle)?;

        let megolm_msg =
            MegolmMessage::from_base64(ciphertext).map_err(|e| format!("bad megolm msg: {e}"))?;
        let result = session
            .decrypt(&megolm_msg)
            .map_err(|e| format!("decrypt: {e}"))?;

        if let Some(entry) = self.store.data.inbound_group_sessions.get_mut(&composite) {
            entry.pickle = pickle_inbound_group(&session)?;
        }
        self.store
            .save()
            .map_err(|e| format!("persist megolm state: {e}"))?;

        let plaintext_str =
            String::from_utf8(result.plaintext).map_err(|e| format!("invalid utf8: {e}"))?;
        let payload: DecryptedPayload =
            serde_json::from_str(&plaintext_str).map_err(|e| format!("bad payload json: {e}"))?;
        Ok(payload)
    }

    // --- Process to-device events from sync ---

    pub fn process_to_device_events(&mut self, events: &[Value]) -> Vec<String> {
        let mut errors = Vec::new();
        for event in events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");
            let content = event.get("content").cloned().unwrap_or(Value::Null);

            match event_type {
                "m.room.encrypted" => {
                    if let Err(e) = self.handle_encrypted_to_device(sender, &content) {
                        errors.push(format!("olm decrypt from {sender}: {e}"));
                    }
                }
                "m.room_key" => {
                    if let Err(e) = self.handle_room_key(&content) {
                        errors.push(format!("room_key from {sender}: {e}"));
                    }
                }
                _ => {}
            }
        }
        errors
    }

    fn handle_encrypted_to_device(&mut self, _sender: &str, content: &Value) -> Result<(), String> {
        let algorithm = content
            .get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if algorithm != "m.olm.v1.curve25519-aes-sha2" {
            return Err(format!("unsupported algorithm: {algorithm}"));
        }

        let sender_key = content
            .get("sender_key")
            .and_then(|v| v.as_str())
            .ok_or("missing sender_key")?;

        let (our_curve, _) = self.identity_keys();
        let our_entry = content
            .get("ciphertext")
            .and_then(|c| c.get(&our_curve))
            .ok_or("no ciphertext for our device")?;

        let msg_type = our_entry
            .get("type")
            .and_then(|v| v.as_u64())
            .ok_or("missing type")? as u8;
        let body = our_entry
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or("missing body")?;

        let plaintext = self.decrypt_olm_event(sender_key, msg_type, body)?;
        let inner: Value =
            serde_json::from_str(&plaintext).map_err(|e| format!("bad inner json: {e}"))?;

        let inner_type = inner.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if inner_type == "m.room_key" {
            if let Some(inner_content) = inner.get("content") {
                self.handle_room_key(inner_content)?;
            }
        }

        Ok(())
    }

    fn handle_room_key(&mut self, content: &Value) -> Result<(), String> {
        let algorithm = content
            .get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if algorithm != "m.megolm.v1.aes-sha2" {
            return Err(format!("unsupported room_key algorithm: {algorithm}"));
        }

        let room_id = content
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or("missing room_id")?;
        let session_id = content
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or("missing session_id")?;
        let session_key = content
            .get("session_key")
            .and_then(|v| v.as_str())
            .ok_or("missing session_key")?;
        let sender_key = content
            .get("sender_key")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        self.import_room_key(room_id, sender_key, session_id, session_key)
    }
}
