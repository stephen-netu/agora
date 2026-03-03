//! Megolm protocol: group encryption for room messages

use serde_json::Value;
use vodozemac::megolm::{
    GroupSession as OutboundGroupSession, InboundGroupSession, MegolmMessage, SessionConfig,
    SessionKey,
};

use super::keys::DeviceInfo;
use super::keys::RoomKeyContent;
use super::sessions::{
    pickle_inbound_group, pickle_outbound_group, unpickle_inbound_group, unpickle_outbound_group,
};
use super::store::{CryptoStore, InboundGroupSessionData, OutboundGroupSessionData};

const MEGOLM_ROTATION_MESSAGE_COUNT: u64 = 100;

/// Manages Megolm sessions for room encryption
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
        Self {
            store,
            user_id,
            device_id,
            sender_key,
        }
    }

    /// Encrypt a room event using Megolm
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

        let ciphertext = session.encrypt(&plaintext);
        let session_id = session.session_id();

        let data = self
            .store
            .data
            .outbound_group_sessions
            .get_mut(room_id)
            .unwrap();
        data.message_count += 1;
        data.pickle = pickle_outbound_group(&session);
        self.store.save().ok();

        Ok((
            "m.megolm.v1.aes-sha2".to_owned(),
            self.sender_key.clone(),
            ciphertext.to_base64(),
            session_id,
        ))
    }

    /// Get or create an outbound Megolm session for a room
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

    /// Create a new outbound Megolm session for a room
    pub fn create_outbound_session(
        &mut self,
        room_id: &str,
    ) -> Result<OutboundGroupSession, String> {
        let session = OutboundGroupSession::new(SessionConfig::version_1());
        let session_id = session.session_id();
        let session_key = session.session_key();

        let inbound = InboundGroupSession::new(&session_key, SessionConfig::version_1());

        let igs_key = CryptoStore::inbound_session_key(room_id, &self.sender_key, &session_id);
        self.store.data.inbound_group_sessions.insert(
            igs_key,
            InboundGroupSessionData {
                pickle: pickle_inbound_group(&inbound),
                sender_key: self.sender_key.clone(),
                signing_key: None,
                room_id: room_id.to_owned(),
            },
        );

        self.store.data.outbound_group_sessions.insert(
            room_id.to_owned(),
            OutboundGroupSessionData {
                pickle: pickle_outbound_group(&session),
                session_id,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                message_count: 0,
            },
        );

        self.store.data.shared_sessions.remove(room_id);
        self.store.save().ok();
        Ok(session)
    }

    /// Get the room key content for sharing with other devices
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

    /// Find devices that need a room key
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

    /// Mark a session as shared with a device
    pub fn mark_session_shared(&mut self, room_id: &str, user_id: &str, device_id: &str) {
        let key = CryptoStore::shared_device_key(user_id, device_id);
        self.store
            .data
            .shared_sessions
            .entry(room_id.to_owned())
            .or_default()
            .push(key);
        self.store.save().ok();
    }

    /// Import a room key from a to-device event
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
                pickle: pickle_inbound_group(&inbound),
                sender_key: sender_key.to_owned(),
                signing_key: None,
                room_id: room_id.to_owned(),
            },
        );
        self.store.save().ok();
        Ok(())
    }

    /// Decrypt a Megolm-encrypted event
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
            .ok_or_else(|| "unknown Megolm session".to_owned())?;

        let mut session = unpickle_inbound_group(&data.pickle)?;

        let megolm_msg =
            MegolmMessage::from_base64(ciphertext).map_err(|e| format!("bad megolm msg: {e}"))?;
        let result = session
            .decrypt(&megolm_msg)
            .map_err(|e| format!("decrypt: {e}"))?;

        if let Some(entry) = self.store.data.inbound_group_sessions.get_mut(&composite) {
            entry.pickle = pickle_inbound_group(&session);
        }
        self.store.save().ok();

        let plaintext_str =
            String::from_utf8(result.plaintext).map_err(|e| format!("invalid utf8: {e}"))?;
        let payload: Value =
            serde_json::from_str(&plaintext_str).map_err(|e| format!("bad payload json: {e}"))?;
        Ok(payload)
    }
}
