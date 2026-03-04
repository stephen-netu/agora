//! Double Ratchet key types.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Root chain key. Fed by DH ratchet steps.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RootKey(pub(crate) [u8; 32]);

/// Sending or receiving chain key. Advanced per-message.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ChainKey(pub(crate) [u8; 32]);

/// One-time message encryption key. Used once then dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MessageKey(pub(crate) [u8; 32]);

/// An X25519 keypair used for DH ratchet steps.
pub struct RatchetKeyPair {
    pub private: x25519_dalek::StaticSecret,
    pub public: x25519_dalek::PublicKey,
}

impl RatchetKeyPair {
    /// Create from a 32-byte deterministic seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let private = x25519_dalek::StaticSecret::from(seed);
        let public = x25519_dalek::PublicKey::from(&private);
        Self { private, public }
    }

    /// Perform DH with remote public key, returning raw shared secret bytes.
    pub fn dh(&self, remote: &x25519_dalek::PublicKey) -> [u8; 32] {
        self.private.diffie_hellman(remote).to_bytes()
    }
}

impl Clone for RatchetKeyPair {
    fn clone(&self) -> Self {
        Self {
            private: x25519_dalek::StaticSecret::from(self.private.to_bytes()),
            public: self.public,
        }
    }
}

/// Serializable snapshot of a RatchetSession for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetSessionSnapshot {
    pub dh_pair_seed: [u8; 32],
    pub dh_remote: Option<[u8; 32]>,
    pub root_key: [u8; 32],
    pub send_chain_key: Option<[u8; 32]>,
    pub recv_chain_key: Option<[u8; 32]>,
    pub send_count: u32,
    pub recv_count: u32,
    pub prev_chain_length: u32,
    /// Skipped message keys: (remote_dh_pubkey_bytes, msg_num) -> message_key
    pub skipped_keys: std::collections::BTreeMap<([u8; 32], u32), [u8; 32]>,
}
