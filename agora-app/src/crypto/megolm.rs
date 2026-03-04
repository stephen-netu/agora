//! Group session (agora-crypto): broadcast encryption for room messages.

use serde_json::Value;

use agora_crypto::group::{GroupMessage, GroupSessionKey, InboundGroupSession, OutboundGroupSession};

use super::keys::DeviceInfo;
use super::keys::RoomKeyContent;
use super::sessions::{
    pickle_inbound_group, pickle_outbound_group, unpickle_inbound_group, unpickle_outbound_group,
};
use super::store::{CryptoStore, InboundGroupSessionData, OutboundGroupSessionData};

const MEGOLM_ROTATION_MESSAGE_COUNT: u64 = 100;

/// Manages group sessions for room encryption.
pub struct MegolmManager<'a> {
    store: &'a mut CryptoStore,
    user_id: &'a str,
    device_id: &'a str,
    sender_key: String,
}

impl<'a> MegolmManager<'a> {
    pub fn new(
        store: &'a mut CryptoStore,
        user_id: &'a str,
        device_id: &'a str,
        sender_key: String,
    ) -> Self {
        Self { store, user_id, device_id, sender_key }
    }

    /// Encrypt a room event using the group session.
    pub fn encrypt_room_event(
        &mut self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> Result<(String, String, String, String), String> {
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
        let ciphertext = msg.to_base64().map_err(|e| format!("encode message: {e}"))?;

        let data = self
            .store
            .data
            .outbound_group_sessions
            .get_mut(room_id)
            .unwrap();
        data.message_count += 1;
        data.pickle = pickle_outbound_group(&session)?;
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;

        Ok((
            "m.agora.group.v1".to_owned(),
            self.sender_key.clone(),
            ciphertext,
            session_id,
        ))
    }

    /// Get or create an outbound group session for a room.
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

    /// Create a new outbound group session for a room.
    pub fn create_outbound_session(
        &mut self,
        room_id: &str,
    ) -> Result<OutboundGroupSession, String> {
        let session =
            OutboundGroupSession::new().map_err(|e| format!("create group session: {e}"))?;
        let session_id = session.session_id();
        let session_key = session.session_key();

        let inbound = InboundGroupSession::new(&session_key);
        let igs_key = CryptoStore::inbound_session_key(room_id, &self.sender_key, &session_id);
        self.store.data.inbound_group_sessions.insert(
            igs_key,
            InboundGroupSessionData {
                pickle: pickle_inbound_group(&inbound)?,
                sender_key: self.sender_key.clone(),
                signing_key: None,
                room_id: room_id.to_owned(),
            },
        );

        self.store.data.outbound_group_sessions.insert(
            room_id.to_owned(),
            OutboundGroupSessionData {
                pickle: pickle_outbound_group(&session)?,
                session_id,
                // S-02: deterministic counter replaces SystemTime::now()
                created_at: 0,
                message_count: 0,
            },
        );

        self.store.data.shared_sessions.remove(room_id);
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;
        Ok(session)
    }

    /// Get the room key content for sharing with other devices.
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

    /// Find devices that need a room key.
    pub fn devices_needing_session(
        &self,
        room_id: &str,
        all_devices: &[DeviceInfo],
    ) -> Vec<DeviceInfo> {
        let shared = self.store.data.shared_sessions.get(room_id);
        all_devices
            .iter()
            .filter(|d| {
                if d.user_id == self.user_id && d.device_id == self.device_id {
                    return false;
                }
                if d.curve25519_key == self.sender_key {
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

    /// Mark a session as shared with a device.
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

    /// Import a room key from a to-device event.
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

    /// Decrypt a group-encrypted event.
    pub fn decrypt(
        &mut self,
        room_id: &str,
        sender_key: &str,
        session_id: &str,
        ciphertext: &str,
    ) -> Result<Value, String> {
        let composite = CryptoStore::inbound_session_key(room_id, sender_key, session_id);
        let data = self
            .store
            .data
            .inbound_group_sessions
            .get(&composite)
            .ok_or_else(|| "unknown group session".to_owned())?;

        let mut session = unpickle_inbound_group(&data.pickle)?;

        let msg = GroupMessage::from_base64(ciphertext).map_err(|e| format!("bad msg: {e}"))?;
        let plaintext_bytes = session.decrypt(&msg).map_err(|e| format!("decrypt: {e}"))?;

        if let Some(entry) = self.store.data.inbound_group_sessions.get_mut(&composite) {
            entry.pickle = pickle_inbound_group(&session)?;
        }
        self.store.save().map_err(|e| format!("persist group state: {e}"))?;

        let plaintext_str =
            String::from_utf8(plaintext_bytes).map_err(|e| format!("invalid utf8: {e}"))?;
        let payload: Value =
            serde_json::from_str(&plaintext_str).map_err(|e| format!("bad payload json: {e}"))?;
        Ok(payload)
    }
}
