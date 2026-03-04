//! CryptoMachine: device-level E2EE orchestration using agora-crypto.
//!
//! Replaces the previous vodozemac (Olm/Megolm) implementation with
//! agora-crypto's `Account` (pairwise sessions) and `group` (broadcast
//! sessions).  Algorithm identifiers are `m.agora.pairwise.v1` and
//! `m.agora.group.v1`; these are agora-internal and are not wire-compatible
//! with the standard Matrix Olm/Megolm protocols.

use std::collections::BTreeMap;

use serde_json::Value;

use agora_crypto::account::{Account, AgoraSignature};
use agora_crypto::group::{GroupSessionKey, InboundGroupSession, OutboundGroupSession};

use super::sessions::{
    pickle_inbound_group, pickle_outbound_group, pickle_pairwise_session,
    unpickle_inbound_group, unpickle_outbound_group, unpickle_pairwise_session,
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

impl CryptoMachine {
    pub fn new(data_dir: &std::path::Path, user_id: &str, device_id: &str) -> Self {
        let store = CryptoStore::open(data_dir, user_id, device_id);

        let account = if let Some(ref snap) = store.data.account_pickle {
            Account::from_snapshot(snap).unwrap_or_else(|_| {
                Account::generate().unwrap_or_else(|_| Account::from_seed([0u8; 32]))
            })
        } else {
            Account::generate().unwrap_or_else(|_| Account::from_seed([0u8; 32]))
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
        let snap = self.account.to_snapshot().map_err(|e| format!("account snapshot: {e}"))?;
        self.store.data.account_pickle = Some(snap);
        self.store.save().map_err(|e| format!("persist account: {e}"))?;
        Ok(())
    }

    pub fn identity_keys(&self) -> (String, String) {
        self.account.identity_keys()
    }

    pub fn device_keys_payload(&self) -> Value {
        let (curve, ed) = self.identity_keys();
        let algorithms = vec!["m.agora.pairwise.v1", "m.agora.group.v1"];

        let mut keys = BTreeMap::new();
        keys.insert(format!("curve25519:{}", self.device_id), curve);
        keys.insert(format!("ed25519:{}", self.device_id), ed);

        let payload = serde_json::json!({
            "user_id": self.user_id,
            "device_id": self.device_id,
            "algorithms": algorithms,
            "keys": keys,
        });

        let canonical = serde_json::to_string(&payload).unwrap_or_default();
        let sig: AgoraSignature = self.account.sign(canonical.as_bytes());

        let mut sigs = BTreeMap::new();
        let mut user_sigs = BTreeMap::new();
        user_sigs.insert(format!("ed25519:{}", self.device_id), sig.to_base64());
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
        self.account.generate_one_time_keys(to_generate.min(MAX_OTK_COUNT) as usize);

        let otks = self.account.one_time_keys();
        let mut result = BTreeMap::new();

        for otk in &otks {
            let key_b64 = otk.public_base64();
            let key_obj = serde_json::json!({ "key": key_b64 });
            let canonical = serde_json::to_string(&key_obj).unwrap_or_default();
            let sig: AgoraSignature = self.account.sign(canonical.as_bytes());

            let mut user_sigs = BTreeMap::new();
            user_sigs.insert(format!("ed25519:{}", self.device_id), sig.to_base64());
            let mut sigs = BTreeMap::new();
            sigs.insert(self.user_id.clone(), user_sigs);

            let signed_key = serde_json::json!({
                "key": key_b64,
                "signatures": sigs,
            });

            let full_key_id = format!("signed_curve25519:{}", otk.key_id_base64());
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

    // ── Group encryption ─────────────────────────────────────────────────────

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

        let msg = session.encrypt(plaintext.as_bytes()).map_err(|e| format!("encrypt: {e}"))?;
        let session_id = session.session_id();
        let ciphertext = msg.to_base64().map_err(|e| format!("encode msg: {e}"))?;
        let (sender_key, _) = self.identity_keys();

        let data = self
            .store
            .data
            .outbound_group_sessions
            .get_mut(room_id)
            .unwrap();
        data.message_count += 1;
        data.pickle = pickle_outbound_group(&session)?;
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;

        Ok(EncryptedPayload {
            algorithm: "m.agora.group.v1".to_owned(),
            sender_key,
            ciphertext,
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
        let session =
            OutboundGroupSession::new().map_err(|e| format!("create group session: {e}"))?;
        let session_id = session.session_id();
        let session_key = session.session_key();
        let (sender_key, _) = self.identity_keys();

        let inbound = InboundGroupSession::new(&session_key);
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
                // S-02: no SystemTime::now()
                created_at: 0,
                message_count: 0,
            },
        );

        self.store.data.shared_sessions.remove(room_id);
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;
        Ok(session)
    }

    pub fn get_room_key_content(&self, room_id: &str) -> Option<RoomKeyContent> {
        let data = self.store.data.outbound_group_sessions.get(room_id)?;
        let session = unpickle_outbound_group(&data.pickle).ok()?;
        let session_key = session.session_key();
        Some(RoomKeyContent {
            algorithm: "m.agora.group.v1".to_owned(),
            room_id: room_id.to_owned(),
            session_id: session.session_id(),
            session_key: session_key.to_base64().ok()?,
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
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;
        Ok(())
    }

    // ── Pairwise encryption ───────────────────────────────────────────────────

    pub fn encrypt_olm(
        &mut self,
        recipient_curve_key: &str,
        _recipient_ed_key: &str,
        plaintext: &str,
    ) -> Result<Value, String> {
        let mut session = self.get_existing_pairwise_session(recipient_curve_key)?;
        let (msg_type, body) =
            session.encrypt(plaintext.as_bytes()).map_err(|e| format!("encrypt: {e}"))?;

        self.store.data.olm_sessions.insert(
            recipient_curve_key.to_owned(),
            vec![pickle_pairwise_session(&session)?],
        );
        self.store.save().map_err(|e| format!("persist pairwise state: {e}"))?;

        let (our_curve, _) = self.identity_keys();
        let mut ciphertext = serde_json::Map::new();
        ciphertext.insert(
            recipient_curve_key.to_owned(),
            serde_json::json!({ "type": msg_type, "body": body }),
        );

        Ok(serde_json::json!({
            "algorithm": "m.agora.pairwise.v1",
            "sender_key": our_curve,
            "ciphertext": ciphertext,
        }))
    }

    fn get_existing_pairwise_session(
        &mut self,
        curve_key: &str,
    ) -> Result<agora_crypto::account::PairwiseSession, String> {
        if let Some(pickles) = self.store.data.olm_sessions.get(curve_key) {
            if let Some(last) = pickles.last() {
                return unpickle_pairwise_session(last);
            }
        }
        Err("no existing pairwise session — must create from claimed OTK first".to_owned())
    }

    pub fn create_outbound_olm_from_otk(
        &mut self,
        their_curve_key: &str,
        one_time_key_b64: &str,
        otk_counter: Option<u64>,
    ) -> Result<(), String> {
        let session = self
            .account
            .create_outbound_session(their_curve_key, one_time_key_b64, otk_counter)
            .map_err(|e| format!("create outbound session: {e}"))?;

        self.store
            .data
            .olm_sessions
            .entry(their_curve_key.to_owned())
            .or_default()
            .push(pickle_pairwise_session(&session)?);
        let _ = self.persist_account();
        Ok(())
    }

    // ── Pairwise decryption ───────────────────────────────────────────────────

    pub fn decrypt_olm_event(
        &mut self,
        sender_key: &str,
        msg_type: u8,
        body: &str,
    ) -> Result<String, String> {
        // Try existing sessions for Normal messages.
        if msg_type == 1 {
            if let Some(pickles) = self.store.data.olm_sessions.get(sender_key) {
                for pickle_str in pickles.iter().rev() {
                    let mut session = match unpickle_pairwise_session(pickle_str) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if let Ok(pt) = session.decrypt_normal(body) {
                        self.store.data.olm_sessions.insert(
                            sender_key.to_owned(),
                            vec![pickle_pairwise_session(&session)
                                .map_err(|e| format!("persist pairwise state: {e}"))?],
                        );
                        self.store.save().map_err(|e| format!("persist: {e}"))?;
                        return String::from_utf8(pt).map_err(|e| format!("invalid utf8: {e}"));
                    }
                }
            }
        }

        // PreKey message: establish inbound session.
        if msg_type == 0 {
            let envelope = Account::decode_prekey_envelope(body)
                .map_err(|e| format!("decode prekey: {e}"))?;
            let (session, pt) = self
                .account
                .create_inbound_session(&envelope)
                .map_err(|e| format!("create inbound session: {e}"))?;
            self.store.data.olm_sessions.insert(
                sender_key.to_owned(),
                vec![pickle_pairwise_session(&session)
                    .map_err(|e| format!("persist pairwise state: {e}"))?],
            );
            self.persist_account()?;
            return String::from_utf8(pt).map_err(|e| format!("invalid utf8: {e}"));
        }

        Err("unable to decrypt pairwise message".to_owned())
    }

    // ── Group decryption ──────────────────────────────────────────────────────

    pub fn import_room_key(
        &mut self,
        room_id: &str,
        sender_key: &str,
        session_id: &str,
        session_key_b64: &str,
    ) -> Result<(), String> {
        let session_key = GroupSessionKey::from_base64(session_key_b64)
            .map_err(|e| format!("bad session_key: {e}"))?;
        let inbound = InboundGroupSession::new(&session_key);

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
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;
        Ok(())
    }

    pub fn decrypt_group(
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
            .ok_or_else(|| "unknown group session".to_owned())?;

        let mut session = unpickle_inbound_group(&data.pickle)?;

        let msg = agora_crypto::group::GroupMessage::from_base64(ciphertext)
            .map_err(|e| format!("bad msg: {e}"))?;
        let plaintext_bytes = session.decrypt(&msg).map_err(|e| format!("decrypt: {e}"))?;

        if let Some(entry) = self.store.data.inbound_group_sessions.get_mut(&composite) {
            entry.pickle = pickle_inbound_group(&session)?;
        }
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;

        let plaintext_str =
            String::from_utf8(plaintext_bytes).map_err(|e| format!("invalid utf8: {e}"))?;
        serde_json::from_str(&plaintext_str).map_err(|e| format!("bad payload json: {e}"))
    }

    // ── To-device event processing ────────────────────────────────────────────

    pub fn process_to_device_events(&mut self, events: &[Value]) -> Vec<String> {
        let mut errors = Vec::new();
        for event in events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let sender = event.get("sender").and_then(|v| v.as_str()).unwrap_or("");
            let content = event.get("content").cloned().unwrap_or(Value::Null);

            match event_type {
                "m.room.encrypted" => {
                    if let Err(e) = self.handle_encrypted_to_device(sender, &content) {
                        errors.push(format!("pairwise decrypt from {sender}: {e}"));
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

    fn handle_encrypted_to_device(
        &mut self,
        _sender: &str,
        content: &Value,
    ) -> Result<(), String> {
        let algorithm = content.get("algorithm").and_then(|v| v.as_str()).unwrap_or("");
        if algorithm != "m.agora.pairwise.v1" {
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
        let algorithm = content.get("algorithm").and_then(|v| v.as_str()).unwrap_or("");
        if algorithm != "m.agora.group.v1" {
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
