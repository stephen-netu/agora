//! CryptoMachine: device-level E2EE orchestration using agora-crypto.
//!
//! Replaces the previous vodozemac (Olm/Megolm) implementation with
//! agora-crypto's `Account` (pairwise sessions) and `group` (broadcast
//! sessions).  Algorithm identifiers are `m.agora.pairwise.v1` and
//! `m.agora.group.v1`; these are agora-internal and are not wire-compatible
//! with the standard Matrix Olm/Megolm protocols.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use agora_crypto::account::{Account, AgoraSignature};
use agora_crypto::group::{GroupSessionKey, InboundGroupSession, OutboundGroupSession};
use agora_crypto::timestamp::{SequenceTimestamp, TimestampProvider};
use agora_crypto::{AgentId, AgentIdentity, Sigchain, SigchainBody, SigchainLink};
use rand_core::{OsRng, RngCore};

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
    timestamp: Arc<dyn TimestampProvider>,
    /// Agent identity for sigchain signing. `None` until `init_sigchain()`.
    identity: Option<AgentIdentity>,
    /// Append-only behavioral ledger. `None` until `init_sigchain()`.
    chain: Option<Sigchain>,
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
    pub fn new(data_dir: &std::path::Path, user_id: &str, device_id: &str) -> Result<Self, String> {
        let store = CryptoStore::open(data_dir, user_id, device_id);

        let account = if let Some(ref snap) = store.data.account_pickle {
            // Fail loudly on a corrupted pickle: silently falling back to a new
            // identity would create a second device with a different key under
            // the same user/device pair, which is a silent key-loss event.
            Account::from_snapshot(snap)
                .map_err(|e| format!("account restore failed — store may be corrupted: {e}"))?
        } else {
            Account::generate().map_err(|e| format!("account generation failed: {e}"))?
        };

        // S-02: deterministic session timestamps; no SystemTime::now().
        // Defaults to 2024-03-01 as epoch offset so IDs stay in a sane numeric
        // range. Replace with SequenceTimestamp::resume_from(offset, last_seq)
        // once the last sequence is persisted in the store.
        let timestamp: Arc<dyn TimestampProvider> = Arc::new(SequenceTimestamp::default());

        // Load sigchain identity and chain from store if available.
        let (identity, chain) = load_sigchain_from_store(&store)
            .map_err(|e| format!("sigchain restore failed — store may be corrupted: {e}"))?;

        let mut machine = Self {
            user_id: user_id.to_owned(),
            device_id: device_id.to_owned(),
            account,
            store,
            timestamp,
            identity,
            chain,
        };
        machine.persist_account()?;
        Ok(machine)
    }

    fn persist_account(&mut self) -> Result<(), String> {
        let snap = self.account.to_snapshot().map_err(|e| format!("account snapshot: {e}"))?;
        self.store.data.account_pickle = Some(snap);
        self.store.save().map_err(|e| format!("persist account: {e}"))?;
        Ok(())
    }

    // ── Sigchain identity ─────────────────────────────────────────────────────

    /// Initialise (or restore) the agent sigchain identity.
    ///
    /// Safe to call repeatedly — if already initialised, returns `Ok(())` immediately.
    /// On first call: generates a fresh 32-byte seed via `OsRng`, creates a
    /// genesis link, and persists both to the crypto store.
    pub fn init_sigchain(&mut self) -> Result<(), String> {
        if self.identity.is_some() && self.chain.is_some() {
            return Ok(());
        }
        if self.identity.is_some() && self.chain.is_none() {
            return Err(
                "sigchain in partial state: identity present but chain missing — store may be corrupted".to_owned()
            );
        }

        // Generate a fresh identity seed.
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);

        let identity = AgentIdentity::from_seed(&seed);
        let chain = Sigchain::genesis(&identity, vec![], None)
            .map_err(|e| format!("sigchain genesis: {e}"))?;

        // Persist seed (hex) and chain (JSON) into the crypto store.
        let seed_hex = hex::encode(seed);
        let chain_json =
            serde_json::to_string(&chain).map_err(|e| format!("sigchain serialize: {e}"))?;

        self.store.data.identity_seed_hex = Some(seed_hex);
        self.store.data.sigchain_json = Some(chain_json);
        self.store.save().map_err(|e| format!("persist sigchain: {e}"))?;

        self.identity = Some(identity);
        self.chain = Some(chain);
        Ok(())
    }

    /// Return the hex-encoded `AgentId` if the sigchain is initialised.
    pub fn agent_id_hex(&self) -> Option<String> {
        self.chain.as_ref().map(|c| c.agent_id.to_hex())
    }

    /// Return `true` if `correlation_path` contains this agent's `AgentId`.
    ///
    /// Must be checked before `append_action_link` when the path is non-empty.
    /// If `true`, call `append_refusal_link` instead and return an error.
    pub fn has_loop_in_path(&self, correlation_path: &[AgentId]) -> bool {
        match &self.chain {
            Some(chain) => Sigchain::has_loop(&chain.agent_id, correlation_path),
            // Fail-closed: if the sigchain isn't initialised and the path is
            // non-empty, block the action — an uninitialised machine cannot
            // safely participate in agent-to-agent routing (S-05).
            None => !correlation_path.is_empty(),
        }
    }

    /// Append a `Refusal` link (loop detected) and persist it.
    ///
    /// Call when `has_loop_in_path()` returns `true`. Records the refusal
    /// on-chain so it is auditable and non-repudiable.
    pub fn append_refusal_link(
        &mut self,
        refused_event_type: &str,
        correlation_path_snapshot: Vec<AgentId>,
    ) -> Result<SigchainLink, String> {
        let identity =
            self.identity.as_ref().ok_or("sigchain not initialised — call init_sigchain()")?;
        let chain =
            self.chain.as_mut().ok_or("sigchain not initialised — call init_sigchain()")?;

        if correlation_path_snapshot.len() > 16 {
            return Err("correlation_path_snapshot exceeds 16-hop limit (S-05)".to_owned());
        }

        let timestamp = chain.len() as u64;

        let body = SigchainBody::Refusal {
            refused_event_type: refused_event_type.to_owned(),
            reason: "loop detected: agent_id appears in correlation_path".to_owned(),
            correlation_path_snapshot,
            timestamp,
        };

        chain.append(body, identity).map_err(|e| format!("sigchain append refusal: {e}"))?;

        let link = chain.links.last().expect("just appended").clone();

        let chain_json =
            serde_json::to_string(chain).map_err(|e| format!("sigchain serialize: {e}"))?;
        self.store.data.sigchain_json = Some(chain_json);
        self.store.save().map_err(|e| format!("persist sigchain: {e}"))?;

        Ok(link)
    }

    /// Append an `Action` link to the local sigchain and persist it.
    ///
    /// - `event_type`: Matrix event type (e.g. `"m.room.message"`).
    /// - `room_id`:    Matrix room ID — BLAKE3-hashed before storage.
    /// - `content`:    Event content JSON — BLAKE3-hashed before storage.
    /// - `correlation_path`: upstream `AgentId` chain (max 16, S-05).
    ///
    /// Returns the new link so the caller can publish it to the server.
    pub fn append_action_link(
        &mut self,
        event_type: &str,
        room_id: &str,
        content: &Value,
        correlation_path: Vec<AgentId>,
    ) -> Result<SigchainLink, String> {
        let identity =
            self.identity.as_ref().ok_or("sigchain not initialised — call init_sigchain()")?;
        let chain =
            self.chain.as_mut().ok_or("sigchain not initialised — call init_sigchain()")?;

        if correlation_path.len() > 16 {
            return Err("correlation_path exceeds 16-hop limit (S-05)".to_owned());
        }

        // S-05: enforce loop detection here so callers cannot bypass it.
        if Sigchain::has_loop(&chain.agent_id, &correlation_path) {
            return Err(
                "loop detected: agent_id appears in correlation_path — call append_refusal_link() instead".to_owned()
            );
        }

        let room_id_hash = *blake3::hash(room_id.as_bytes()).as_bytes();
        let content_bytes =
            serde_json::to_vec(content).map_err(|e| format!("serialize content: {e}"))?;
        let content_hash = *blake3::hash(&content_bytes).as_bytes();

        // S-02: timestamp = chain length before append (monotonically increasing).
        let timestamp = chain.len() as u64;

        let body = SigchainBody::Action {
            event_type: event_type.to_owned(),
            event_id_hash: [0u8; 32], // unknown until server assigns event_id
            room_id_hash,
            content_hash,
            effect_hash: None,
            timestamp,
            correlation_path,
        };

        chain.append(body, identity).map_err(|e| format!("sigchain append: {e}"))?;

        let link = chain.links.last().expect("just appended").clone();

        // Persist updated chain.
        let chain_json =
            serde_json::to_string(chain).map_err(|e| format!("sigchain serialize: {e}"))?;
        self.store.data.sigchain_json = Some(chain_json);
        self.store.save().map_err(|e| format!("persist sigchain: {e}"))?;

        Ok(link)
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

        let created_at = self
            .timestamp
            .next_timestamp()
            .map_err(|e| format!("timestamp overflow: {e}"))?;
        self.store.data.outbound_group_sessions.insert(
            room_id.to_owned(),
            OutboundGroupSessionData {
                pickle: pickle_outbound_group(&session)?,
                session_id,
                created_at,
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

// ── Sigchain helpers ───────────────────────────────────────────────────────────

/// Attempt to restore the agent sigchain identity and chain from a persisted
/// `CryptoStore`. Returns `Ok((None, None))` if the store has no sigchain data
/// yet. Returns `Err` if the store is partially or fully corrupted so the caller
/// can fail loudly rather than silently accepting invalid state.
fn load_sigchain_from_store(
    store: &CryptoStore,
) -> Result<(Option<AgentIdentity>, Option<Sigchain>), String> {
    let seed_hex = match store.data.identity_seed_hex.as_deref() {
        Some(s) => s,
        None => {
            // Inverse partial state: chain persisted but seed is gone.
            if store.data.sigchain_json.is_some() {
                return Err(
                    "sigchain present but identity_seed missing — store is corrupted".to_owned()
                );
            }
            return Ok((None, None));
        }
    };

    let seed_bytes = hex::decode(seed_hex)
        .map_err(|_| "identity_seed_hex is not valid hex — store is corrupted".to_owned())?;

    if seed_bytes.len() != 32 {
        return Err("identity_seed has wrong length — store is corrupted".to_owned());
    }

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&seed_bytes);
    let identity = AgentIdentity::from_seed(&seed);

    let sigchain_json = match store.data.sigchain_json.as_deref() {
        Some(j) => j,
        None => {
            return Err(
                "identity present but sigchain missing — store is corrupted".to_owned()
            )
        }
    };

    let chain: Sigchain = serde_json::from_str(sigchain_json)
        .map_err(|e| format!("sigchain JSON is invalid — store is corrupted: {e}"))?;

    // Full chain integrity check (seqno, hash-links, signatures).
    chain
        .verify_chain()
        .map_err(|e| format!("sigchain integrity check failed: {e}"))?;

    // Identity↔chain consistency: the stored chain must belong to this identity.
    if chain.agent_id != identity.agent_id {
        return Err(
            "identity agent_id does not match sigchain agent_id — store is corrupted".to_owned()
        );
    }

    Ok((Some(identity), Some(chain)))
}
