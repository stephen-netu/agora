//! Olm protocol: session establishment and to-device encryption

use serde_json::Value;
use vodozemac::olm::{
    Account, OlmMessage, PreKeyMessage, Session as OlmSession, SessionConfig as OlmSessionConfig,
};
use vodozemac::Curve25519PublicKey;

use super::sessions::{pickle_olm_session, unpickle_olm_session};
use super::store::CryptoStore;

/// Manages Olm sessions for to-device encryption
pub struct OlmManager<'a> {
    pub account: &'a mut Account,
    pub store: &'a mut CryptoStore,
}

impl<'a> OlmManager<'a> {
    pub fn new(account: &'a mut Account, store: &'a mut CryptoStore) -> Self {
        Self { account, store }
    }

    /// Encrypt a message for a recipient using Olm
    pub fn encrypt(
        &mut self,
        our_curve_key: &str,
        recipient_curve_key: &str,
        plaintext: &str,
    ) -> Result<Value, String> {
        let mut session = self.get_or_create_session(recipient_curve_key)?;
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
            "sender_key": our_curve_key,
            "ciphertext": ciphertext,
        }))
    }

    /// Create an outbound Olm session using a one-time key
    pub fn create_outbound_session_from_otk(
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
        Ok(())
    }

    /// Decrypt an incoming Olm event
    pub fn decrypt(
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
                return String::from_utf8(result.plaintext)
                    .map_err(|e| format!("invalid utf8: {e}"));
            }
        }

        Err("unable to decrypt Olm message".to_owned())
    }

    fn get_or_create_session(&mut self, curve_key_str: &str) -> Result<OlmSession, String> {
        if let Some(pickles) = self.store.data.olm_sessions.get(curve_key_str) {
            if let Some(last) = pickles.last() {
                return unpickle_olm_session(last);
            }
        }
        Err("no existing Olm session — must create outbound session from claimed OTK".to_owned())
    }
}
