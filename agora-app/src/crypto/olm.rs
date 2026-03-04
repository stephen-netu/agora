//! Pairwise session (agora-crypto): to-device encryption and key exchange.

use serde_json::Value;

use agora_crypto::account::{Account, PreKeyEnvelope};

use super::sessions::{pickle_pairwise_session, unpickle_pairwise_session};
use super::store::CryptoStore;

/// Manages pairwise sessions for to-device encryption.
pub struct OlmManager<'a> {
    pub account: &'a mut Account,
    pub store: &'a mut CryptoStore,
}

impl<'a> OlmManager<'a> {
    pub fn new(account: &'a mut Account, store: &'a mut CryptoStore) -> Self {
        Self { account, store }
    }

    /// Encrypt a message for a recipient using a pairwise session.
    pub fn encrypt(
        &mut self,
        our_curve_key: &str,
        recipient_curve_key: &str,
        plaintext: &str,
    ) -> Result<Value, String> {
        let mut session = self.get_existing_session(recipient_curve_key)?;
        let (msg_type, body) =
            session.encrypt(plaintext.as_bytes()).map_err(|e| format!("encrypt: {e}"))?;

        self.store.data.olm_sessions.insert(
            recipient_curve_key.to_owned(),
            vec![pickle_pairwise_session(&session)?],
        );
        self.store.save().map_err(|e| format!("persist pairwise state: {e}"))?;

        let mut ciphertext = serde_json::Map::new();
        ciphertext.insert(
            recipient_curve_key.to_owned(),
            serde_json::json!({ "type": msg_type, "body": body }),
        );

        Ok(serde_json::json!({
            "algorithm": "m.agora.pairwise.v1",
            "sender_key": our_curve_key,
            "ciphertext": ciphertext,
        }))
    }

    /// Create an outbound pairwise session using a claimed one-time key.
    pub fn create_outbound_session_from_otk(
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
        Ok(())
    }

    /// Decrypt an incoming pairwise event.
    pub fn decrypt(
        &mut self,
        sender_key: &str,
        msg_type: u8,
        body: &str,
    ) -> Result<String, String> {
        // Try existing sessions first (Normal messages).
        if msg_type == 1 {
            if let Some(pickles) = self.store.data.olm_sessions.get(sender_key) {
                for pickle_str in pickles.iter().rev() {
                    let mut session = match unpickle_pairwise_session(pickle_str) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if let Ok(plaintext_bytes) = session.decrypt_normal(body) {
                        self.store.data.olm_sessions.insert(
                            sender_key.to_owned(),
                            vec![pickle_pairwise_session(&session)
                                .map_err(|e| format!("persist pairwise state: {e}"))?],
                        );
                        self.store
                            .save()
                            .map_err(|e| format!("persist pairwise state: {e}"))?;
                        return String::from_utf8(plaintext_bytes)
                            .map_err(|e| format!("invalid utf8: {e}"));
                    }
                }
            }
        }

        // PreKey message: create inbound session.
        if msg_type == 0 {
            let envelope = Account::decode_prekey_envelope(body)
                .map_err(|e| format!("decode prekey: {e}"))?;
            let (session, plaintext_bytes) = self
                .account
                .create_inbound_session(&envelope)
                .map_err(|e| format!("create inbound session: {e}"))?;
            self.store.data.olm_sessions.insert(
                sender_key.to_owned(),
                vec![pickle_pairwise_session(&session)
                    .map_err(|e| format!("persist pairwise state: {e}"))?],
            );
            self.store.save().map_err(|e| format!("persist pairwise state: {e}"))?;
            return String::from_utf8(plaintext_bytes)
                .map_err(|e| format!("invalid utf8: {e}"));
        }

        Err("unable to decrypt pairwise message".to_owned())
    }

    fn get_existing_session(
        &mut self,
        curve_key_str: &str,
    ) -> Result<agora_crypto::account::PairwiseSession, String> {
        if let Some(pickles) = self.store.data.olm_sessions.get(curve_key_str) {
            if let Some(last) = pickles.last() {
                return unpickle_pairwise_session(last);
            }
        }
        Err("no existing pairwise session — must create outbound session from claimed OTK".to_owned())
    }
}

/// Parse a PreKey envelope from a base64 body.
pub fn parse_prekey_envelope(body: &str) -> Result<PreKeyEnvelope, String> {
    Account::decode_prekey_envelope(body).map_err(|e| format!("decode prekey: {e}"))
}
