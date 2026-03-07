//! Pairwise session (agora-crypto): to-device encryption and key exchange.

use serde_json::Value;

use agora_crypto::account::Account;

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
