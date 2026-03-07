//! Group session (agora-crypto): broadcast encryption for room messages.

use serde_json::Value;

use agora_crypto::group::{InboundGroupSession, OutboundGroupSession};

use super::sessions::{
    pickle_inbound_group, pickle_outbound_group, unpickle_outbound_group,
};
use super::store::{CryptoStore, InboundGroupSessionData, OutboundGroupSessionData};

/// Manages group sessions for room encryption.
pub struct MegolmManager<'a> {
    store: &'a mut CryptoStore,
    sender_key: String,
}

impl<'a> MegolmManager<'a> {
    pub fn new(
        store: &'a mut CryptoStore,
        sender_key: String,
    ) -> Self {
        Self { store, sender_key }
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
        const MEGOLM_ROTATION_MESSAGE_COUNT: u64 = 100;
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
}
