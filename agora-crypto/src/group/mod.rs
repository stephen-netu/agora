//! Group session: forward-only broadcast ratchet (Megolm-equivalent).
//!
//! Each sender owns one `OutboundGroupSession` per room.  Recipients hold an
//! `InboundGroupSession` seeded from the exported `GroupSessionKey`.
//!
//! # Ratchet construction
//! Identical to the Signal symmetric-chain step already used in `ratchet::chain`:
//! - `message_key   = HMAC-SHA256(chain_key, 0x01)`
//! - `next_chain_key = HMAC-SHA256(chain_key, 0x02)`
//!
//! Encryption: ChaCha20-Poly1305, nonce derived from message key via BLAKE3.
//! AEAD associated data: `message_index` as little-endian u64.
//!
//! # Session identity
//! `session_id` = first 32 bytes of `BLAKE3(initial_chain_key)`, encoded as
//! standard base64.  It never changes for the lifetime of a session.
//!
//! # S-05 Killability
//! `InboundGroupSession` caps skipped message keys at `MAX_SKIP` to prevent
//! unbounded memory growth.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use blake3;
use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305,
};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::CryptoError;

type HmacSha256 = Hmac<Sha256>;

/// Maximum skipped message keys stored per inbound session (S-05 bound).
const MAX_SKIP: u64 = 1_000;

// ── Chain helpers ─────────────────────────────────────────────────────────────

fn chain_message_key(chain_key: &[u8; 32]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(chain_key).expect("HMAC: any key size valid");
    mac.update(&[0x01]);
    let result = mac.finalize().into_bytes();
    let mut mk = [0u8; 32];
    mk.copy_from_slice(&result[..32]);
    mk
}

fn chain_advance(chain_key: &[u8; 32]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(chain_key).expect("HMAC: any key size valid");
    mac.update(&[0x02]);
    let result = mac.finalize().into_bytes();
    let mut ck = [0u8; 32];
    ck.copy_from_slice(&result[..32]);
    ck
}

// ── AEAD ──────────────────────────────────────────────────────────────────────

fn aead_encrypt(
    message_key: &[u8; 32],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let nonce_src = *blake3::hash(&[message_key.as_slice(), b"nonce"].concat()).as_bytes();
    let nonce = GenericArray::from_slice(&nonce_src[..12]);
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(message_key));
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Encryption(e.to_string()))
}

fn aead_decrypt(
    message_key: &[u8; 32],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let nonce_src = *blake3::hash(&[message_key.as_slice(), b"nonce"].concat()).as_bytes();
    let nonce = GenericArray::from_slice(&nonce_src[..12]);
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(message_key));
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Decryption(e.to_string()))
}

fn session_id_from_seed(initial_chain_key: &[u8; 32]) -> [u8; 32] {
    *blake3::hash(initial_chain_key).as_bytes()
}

// ── Wire types ────────────────────────────────────────────────────────────────

/// An encrypted group message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMessage {
    /// Monotonically increasing message counter for this session.
    pub message_index: u64,
    /// ChaCha20-Poly1305 ciphertext (includes 16-byte authentication tag).
    pub ciphertext: Vec<u8>,
}

impl GroupMessage {
    /// Encode to standard base64 (msgpack payload).
    pub fn to_base64(&self) -> Result<String, CryptoError> {
        let bytes =
            rmp_serde::to_vec_named(self).map_err(|e| CryptoError::Encoding(e.to_string()))?;
        Ok(B64.encode(bytes))
    }

    /// Decode from standard base64.
    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = B64
            .decode(s)
            .map_err(|e| CryptoError::Decoding(e.to_string()))?;
        rmp_serde::from_slice(&bytes).map_err(|e| CryptoError::Decoding(e.to_string()))
    }
}

/// Exportable ratchet state for sharing with group members.
///
/// Contains the chain key at a particular `message_index`, so recipients can
/// decrypt messages from that index onward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSessionKey {
    /// Session identity (BLAKE3 of the initial chain key seed).
    pub session_id: [u8; 32],
    /// Chain key at `message_index`.
    pub chain_key: [u8; 32],
    /// First message index this key can decrypt.
    pub message_index: u64,
}

impl GroupSessionKey {
    /// Encode to standard base64.
    pub fn to_base64(&self) -> Result<String, CryptoError> {
        let bytes =
            rmp_serde::to_vec_named(self).map_err(|e| CryptoError::Encoding(e.to_string()))?;
        Ok(B64.encode(bytes))
    }

    /// Decode from standard base64.
    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = B64
            .decode(s)
            .map_err(|e| CryptoError::Decoding(e.to_string()))?;
        rmp_serde::from_slice(&bytes).map_err(|e| CryptoError::Decoding(e.to_string()))
    }
}

// ── Snapshots (pickle) ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct OutboundSnapshot {
    session_id: [u8; 32],
    chain_key: [u8; 32],
    message_index: u64,
}

#[derive(Serialize, Deserialize)]
struct InboundSnapshot {
    session_id: [u8; 32],
    chain_key: [u8; 32],
    message_index: u64,
    /// Skipped message keys: index → message_key bytes.
    skipped: Vec<(u64, [u8; 32])>,
}

// ── OutboundGroupSession ──────────────────────────────────────────────────────

/// Sender-side group session.  Advances the ratchet on every `encrypt` call.
pub struct OutboundGroupSession {
    session_id: [u8; 32],
    chain_key: [u8; 32],
    message_index: u64,
}

impl OutboundGroupSession {
    /// Create a new session seeded from OS entropy.
    pub fn new() -> Result<Self, CryptoError> {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Ok(Self {
            session_id: session_id_from_seed(&seed),
            chain_key: seed,
            message_index: 0,
        })
    }

    /// Session identifier (fixed for the lifetime of the session).
    pub fn session_id(&self) -> String {
        B64.encode(self.session_id)
    }

    /// Export the current ratchet state so new members can decrypt future messages.
    pub fn session_key(&self) -> GroupSessionKey {
        GroupSessionKey {
            session_id: self.session_id,
            chain_key: self.chain_key,
            message_index: self.message_index,
        }
    }

    /// Encrypt `plaintext`, advancing the ratchet by one step.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<GroupMessage, CryptoError> {
        let mk = chain_message_key(&self.chain_key);
        let aad = self.message_index.to_le_bytes();
        let ciphertext = aead_encrypt(&mk, &aad, plaintext)?;
        let msg = GroupMessage {
            message_index: self.message_index,
            ciphertext,
        };
        self.chain_key = chain_advance(&self.chain_key);
        self.message_index = self
            .message_index
            .checked_add(1)
            .ok_or(CryptoError::SequenceOverflow)?;
        Ok(msg)
    }

    /// Serialize for persistent storage.
    pub fn to_snapshot(&self) -> Result<String, CryptoError> {
        let snap = OutboundSnapshot {
            session_id: self.session_id,
            chain_key: self.chain_key,
            message_index: self.message_index,
        };
        serde_json::to_string(&snap).map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Restore from serialized storage.
    pub fn from_snapshot(s: &str) -> Result<Self, CryptoError> {
        let snap: OutboundSnapshot =
            serde_json::from_str(s).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        Ok(Self {
            session_id: snap.session_id,
            chain_key: snap.chain_key,
            message_index: snap.message_index,
        })
    }
}

// ── InboundGroupSession ───────────────────────────────────────────────────────

/// Receiver-side group session seeded from an exported `GroupSessionKey`.
pub struct InboundGroupSession {
    session_id: [u8; 32],
    chain_key: [u8; 32],
    message_index: u64,
    /// Cached message keys for out-of-order delivery, bounded to `MAX_SKIP`.
    skipped: BTreeMap<u64, [u8; 32]>,
}

impl InboundGroupSession {
    /// Initialize from an exported session key (shared out-of-band).
    pub fn new(key: &GroupSessionKey) -> Self {
        Self {
            session_id: key.session_id,
            chain_key: key.chain_key,
            message_index: key.message_index,
            skipped: BTreeMap::new(),
        }
    }

    /// Session identifier.
    pub fn session_id(&self) -> String {
        B64.encode(self.session_id)
    }

    /// Decrypt a `GroupMessage`.
    ///
    /// Messages may arrive slightly out of order; up to `MAX_SKIP` keys will be
    /// cached.  Messages older than the session's starting index cannot be
    /// decrypted (forward secrecy).
    pub fn decrypt(&mut self, msg: &GroupMessage) -> Result<Vec<u8>, CryptoError> {
        if msg.message_index < self.message_index {
            // Check skipped cache.
            if let Some(mk) = self.skipped.remove(&msg.message_index) {
                let aad = msg.message_index.to_le_bytes();
                return aead_decrypt(&mk, &aad, &msg.ciphertext);
            }
            return Err(CryptoError::Decryption(format!(
                "message index {} is behind session index {} and not in skipped cache",
                msg.message_index, self.message_index
            )));
        }

        let ahead = msg.message_index - self.message_index;
        if ahead > MAX_SKIP {
            #[allow(clippy::cast_possible_truncation)]
            return Err(CryptoError::MaxSkipExceeded {
                limit: MAX_SKIP as u32,
                got: ahead as u32,
            });
        }

        // Advance chain to cache skipped keys.
        while self.message_index < msg.message_index {
            let mk = chain_message_key(&self.chain_key);
            self.skipped.insert(self.message_index, mk);
            self.chain_key = chain_advance(&self.chain_key);
            self.message_index += 1;
        }

        // Decrypt at current index.
        let mk = chain_message_key(&self.chain_key);
        let aad = msg.message_index.to_le_bytes();
        let plaintext = aead_decrypt(&mk, &aad, &msg.ciphertext)?;

        self.chain_key = chain_advance(&self.chain_key);
        self.message_index = self
            .message_index
            .checked_add(1)
            .ok_or(CryptoError::SequenceOverflow)?;

        Ok(plaintext)
    }

    /// Serialize for persistent storage.
    pub fn to_snapshot(&self) -> Result<String, CryptoError> {
        let snap = InboundSnapshot {
            session_id: self.session_id,
            chain_key: self.chain_key,
            message_index: self.message_index,
            skipped: self.skipped.iter().map(|(k, v)| (*k, *v)).collect(),
        };
        serde_json::to_string(&snap).map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Restore from serialized storage.
    pub fn from_snapshot(s: &str) -> Result<Self, CryptoError> {
        let snap: InboundSnapshot =
            serde_json::from_str(s).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        Ok(Self {
            session_id: snap.session_id,
            chain_key: snap.chain_key,
            message_index: snap.message_index,
            skipped: snap.skipped.into_iter().collect(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pair() -> (OutboundGroupSession, InboundGroupSession) {
        let outbound = OutboundGroupSession::new().unwrap();
        let key = outbound.session_key();
        let inbound = InboundGroupSession::new(&key);
        (outbound, inbound)
    }

    #[test]
    fn encrypt_decrypt_single() {
        let (mut out, mut inb) = make_pair();
        let msg = out.encrypt(b"hello group").unwrap();
        let pt = inb.decrypt(&msg).unwrap();
        assert_eq!(pt, b"hello group");
    }

    #[test]
    fn multiple_messages_in_order() {
        let (mut out, mut inb) = make_pair();
        for i in 0u8..10 {
            let msg = out.encrypt(&[i; 32]).unwrap();
            let pt = inb.decrypt(&msg).unwrap();
            assert_eq!(pt, &[i; 32]);
        }
    }

    #[test]
    fn session_id_is_stable() {
        let out = OutboundGroupSession::new().unwrap();
        let id1 = out.session_id();
        let id2 = out.session_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn session_key_base64_roundtrip() {
        let out = OutboundGroupSession::new().unwrap();
        let key = out.session_key();
        let b64 = key.to_base64().unwrap();
        let key2 = GroupSessionKey::from_base64(&b64).unwrap();
        assert_eq!(key.session_id, key2.session_id);
        assert_eq!(key.chain_key, key2.chain_key);
        assert_eq!(key.message_index, key2.message_index);
    }

    #[test]
    fn group_message_base64_roundtrip() {
        let (mut out, _) = make_pair();
        let msg = out.encrypt(b"roundtrip").unwrap();
        let b64 = msg.to_base64().unwrap();
        let msg2 = GroupMessage::from_base64(&b64).unwrap();
        assert_eq!(msg.message_index, msg2.message_index);
        assert_eq!(msg.ciphertext, msg2.ciphertext);
    }

    #[test]
    fn outbound_snapshot_roundtrip() {
        let (mut out, _) = make_pair();
        let _ = out.encrypt(b"msg").unwrap();
        let snap = out.to_snapshot().unwrap();
        let out2 = OutboundGroupSession::from_snapshot(&snap).unwrap();
        assert_eq!(out.session_id, out2.session_id);
        assert_eq!(out.chain_key, out2.chain_key);
        assert_eq!(out.message_index, out2.message_index);
    }

    #[test]
    fn inbound_snapshot_roundtrip() {
        let (mut out, mut inb) = make_pair();
        let msg = out.encrypt(b"snap").unwrap();
        let _ = inb.decrypt(&msg).unwrap();
        let snap = inb.to_snapshot().unwrap();
        let inb2 = InboundGroupSession::from_snapshot(&snap).unwrap();
        assert_eq!(inb.session_id, inb2.session_id);
        assert_eq!(inb.message_index, inb2.message_index);
    }

    #[test]
    fn out_of_order_delivery() {
        let (mut out, mut inb) = make_pair();
        let m0 = out.encrypt(b"msg0").unwrap();
        let m1 = out.encrypt(b"msg1").unwrap();
        let m2 = out.encrypt(b"msg2").unwrap();

        // Deliver 2, 0, 1
        let p2 = inb.decrypt(&m2).unwrap();
        assert_eq!(p2, b"msg2");
        let p0 = inb.decrypt(&m0).unwrap();
        assert_eq!(p0, b"msg0");
        let p1 = inb.decrypt(&m1).unwrap();
        assert_eq!(p1, b"msg1");
    }

    #[test]
    fn inbound_from_mid_session() {
        let (mut out, _) = make_pair();
        // Advance sender by 5 messages.
        for _ in 0..5 {
            let _ = out.encrypt(b"skip").unwrap();
        }
        // Share key at message_index=5, then decrypt message 5.
        let key = out.session_key();
        let mut inb = InboundGroupSession::new(&key);
        let msg = out.encrypt(b"future").unwrap();
        let pt = inb.decrypt(&msg).unwrap();
        assert_eq!(pt, b"future");
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let (mut out, mut inb) = make_pair();
        let mut msg = out.encrypt(b"secret").unwrap();
        msg.ciphertext[0] ^= 0xFF;
        assert!(inb.decrypt(&msg).is_err());
    }

    #[test]
    fn two_senders_independent_sessions() {
        let out_a = OutboundGroupSession::new().unwrap();
        let out_b = OutboundGroupSession::new().unwrap();
        // Session IDs must differ (overwhelmingly likely).
        assert_ne!(out_a.session_id(), out_b.session_id());
    }
}
