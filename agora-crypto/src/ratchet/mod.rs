//! Double Ratchet Algorithm implementation.
//!
//! Implements the Signal specification:
//! <https://signal.org/docs/specifications/doubleratchet/>
//!
//! Provides forward secrecy (compromise of current keys does not expose past
//! messages) and post-compromise security (compromise is healed after new
//! DH exchange).
//!
//! # S-05 Killability
//! `max_skip` bounds the number of skipped message keys stored. Sessions
//! exceeding this limit reject messages with `CryptoError::MaxSkipExceeded`.

pub mod chain;
pub mod header;
pub mod keys;

use std::collections::BTreeMap;

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305,
};
use zeroize::ZeroizeOnDrop;

use crate::CryptoError;
use chain::{chain_advance, chain_message_key, root_kdf};
use keys::{ChainKey, MessageKey, RatchetKeyPair, RatchetSessionSnapshot, RootKey};

pub use header::MessageHeader;
pub use keys::RatchetKeyPair as KeyPair;

/// Maximum number of skipped message keys to store.
/// Prevents unbounded memory growth (S-05 killability).
const DEFAULT_MAX_SKIP: u32 = 1000;

/// A Double Ratchet session state.
///
/// # Initialization
/// Use `RatchetSession::init_alice` (initiator) or `RatchetSession::init_bob` (responder)
/// after completing X3DH key agreement to obtain a `shared_secret`.
#[derive(ZeroizeOnDrop)]
pub struct RatchetSession {
    /// DH ratchet keypair (current sending keypair).
    #[zeroize(skip)]
    dh_pair: RatchetKeyPair,
    /// Remote party's most recent DH ratchet public key.
    dh_remote: Option<[u8; 32]>,
    /// Root chain key.
    root_key: RootKey,
    /// Current sending chain key (None until first send after DH ratchet step).
    send_chain: Option<ChainKey>,
    /// Current receiving chain key (None until first receive).
    recv_chain: Option<ChainKey>,
    /// Message counter for the current sending chain.
    send_count: u32,
    /// Message counter for the current receiving chain.
    recv_count: u32,
    /// Previous sending chain length (for header PN field).
    prev_chain_length: u32,
    /// Skipped message keys: (remote_dh_pubkey, message_number) -> message_key.
    /// BTreeMap for S-02 deterministic ordering.
    #[zeroize(skip)]
    skipped_keys: BTreeMap<([u8; 32], u32), MessageKey>,
    /// Maximum skipped message keys to store (S-05 bound).
    max_skip: u32,
}

impl RatchetSession {
    /// Initialize as Alice (session initiator), after X3DH.
    ///
    /// # Arguments
    /// - `shared_secret` — 32-byte shared secret from X3DH
    /// - `bob_dh_public` — Bob's initial DH ratchet public key
    /// - `alice_dh_seed` — deterministic seed for Alice's initial DH keypair
    pub fn init_alice(
        shared_secret: &[u8; 32],
        bob_dh_public: &[u8; 32],
        alice_dh_seed: [u8; 32],
    ) -> Result<Self, CryptoError> {
        let alice_pair = RatchetKeyPair::from_seed(alice_dh_seed);
        let bob_pub = x25519_dalek::PublicKey::from(*bob_dh_public);

        let root_key = RootKey(*shared_secret);
        let dh_out = alice_pair.dh(&bob_pub);
        let (new_root, send_chain) = root_kdf(&root_key, &dh_out);

        Ok(Self {
            dh_pair: alice_pair,
            dh_remote: Some(*bob_dh_public),
            root_key: new_root,
            send_chain: Some(send_chain),
            recv_chain: None,
            send_count: 0,
            recv_count: 0,
            prev_chain_length: 0,
            skipped_keys: BTreeMap::new(),
            max_skip: DEFAULT_MAX_SKIP,
        })
    }

    /// Initialize as Bob (session responder), after X3DH.
    ///
    /// # Arguments
    /// - `shared_secret` — 32-byte shared secret from X3DH
    /// - `bob_dh_seed` — deterministic seed for Bob's initial DH keypair (must match
    ///   the `bob_dh_public` Alice used in `init_alice`)
    pub fn init_bob(shared_secret: &[u8; 32], bob_dh_seed: [u8; 32]) -> Result<Self, CryptoError> {
        let bob_pair = RatchetKeyPair::from_seed(bob_dh_seed);
        let root_key = RootKey(*shared_secret);

        Ok(Self {
            dh_pair: bob_pair,
            dh_remote: None,
            root_key,
            send_chain: None,
            recv_chain: None,
            send_count: 0,
            recv_count: 0,
            prev_chain_length: 0,
            skipped_keys: BTreeMap::new(),
            max_skip: DEFAULT_MAX_SKIP,
        })
    }

    /// Encrypt a plaintext message.
    ///
    /// Returns `(header, ciphertext)`. The header must be transmitted alongside
    /// the ciphertext. It is used as AEAD associated data (authenticated but not
    /// encrypted).
    pub fn encrypt(
        &mut self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<(MessageHeader, Vec<u8>), CryptoError> {
        let send_chain = self.send_chain.as_ref().ok_or_else(|| {
            CryptoError::Encryption("no sending chain — session not initialized as Alice".into())
        })?;

        let mk = chain_message_key(send_chain);
        let next_ck = chain_advance(send_chain);

        let header = MessageHeader::new(
            self.dh_pair.public.to_bytes(),
            self.prev_chain_length,
            self.send_count,
        );

        let mut full_ad = header.to_associated_data().to_vec();
        full_ad.extend_from_slice(associated_data);

        let ciphertext = aead_encrypt(&mk, &full_ad, plaintext)?;

        self.send_chain = Some(next_ck);
        self.send_count += 1;

        Ok((header, ciphertext))
    }

    /// Decrypt a received message.
    ///
    /// Performs DH ratchet stepping if the header contains a new remote DH key.
    pub fn decrypt(
        &mut self,
        header: &MessageHeader,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        // Check skipped message keys first.
        let skip_key = (header.dh_public, header.message_number);
        if let Some(mk) = self.skipped_keys.remove(&skip_key) {
            let mut full_ad = header.to_associated_data().to_vec();
            full_ad.extend_from_slice(associated_data);
            return aead_decrypt(&mk, &full_ad, ciphertext);
        }

        let is_new_dh = self
            .dh_remote
            .map(|remote| remote != header.dh_public)
            .unwrap_or(true);

        if is_new_dh {
            // Skip remaining messages in current receiving chain.
            self.skip_message_keys(header.prev_chain_length)?;
            // DH ratchet step.
            self.dh_ratchet_step(&header.dh_public)?;
        }

        // Skip messages in new receiving chain up to header.message_number.
        self.skip_message_keys(header.message_number)?;

        let recv_chain = self.recv_chain.as_ref().ok_or_else(|| {
            CryptoError::Decryption("no receiving chain after ratchet step".into())
        })?;

        let mk = chain_message_key(recv_chain);
        let next_ck = chain_advance(recv_chain);

        let mut full_ad = header.to_associated_data().to_vec();
        full_ad.extend_from_slice(associated_data);

        let plaintext = aead_decrypt(&mk, &full_ad, ciphertext)?;

        self.recv_chain = Some(next_ck);
        self.recv_count += 1;

        Ok(plaintext)
    }

    /// Perform a DH ratchet step, advancing the root chain.
    fn dh_ratchet_step(&mut self, new_remote_dh: &[u8; 32]) -> Result<(), CryptoError> {
        let remote_pub = x25519_dalek::PublicKey::from(*new_remote_dh);

        // Receiving chain from new remote key.
        let dh_recv = self.dh_pair.dh(&remote_pub);
        let (new_root, recv_chain) = root_kdf(&self.root_key, &dh_recv);
        self.root_key = new_root;
        self.recv_chain = Some(recv_chain);
        self.recv_count = 0;

        // New sending DH keypair (deterministic from current root key state).
        let new_seed = derive_new_dh_seed(&self.root_key, new_remote_dh);
        let new_pair = RatchetKeyPair::from_seed(new_seed);
        let dh_send = new_pair.dh(&remote_pub);
        let (new_root2, send_chain) = root_kdf(&self.root_key, &dh_send);
        self.root_key = new_root2;

        self.prev_chain_length = self.send_count;
        self.send_count = 0;
        self.send_chain = Some(send_chain);
        self.dh_pair = new_pair;
        self.dh_remote = Some(*new_remote_dh);

        Ok(())
    }

    /// Store skipped message keys up to `until` for the current receiving chain.
    fn skip_message_keys(&mut self, until: u32) -> Result<(), CryptoError> {
        if until > self.recv_count + self.max_skip {
            return Err(CryptoError::MaxSkipExceeded {
                limit: self.max_skip,
                got: until.saturating_sub(self.recv_count),
            });
        }

        if let Some(ref recv_chain) = self.recv_chain.clone() {
            let remote_dh = self.dh_remote.unwrap_or([0u8; 32]);
            let mut current_ck = recv_chain.clone();

            while self.recv_count < until {
                let mk = chain_message_key(&current_ck);
                current_ck = chain_advance(&current_ck);
                self.skipped_keys.insert((remote_dh, self.recv_count), mk);
                self.recv_count += 1;
            }

            self.recv_chain = Some(current_ck);
        }

        Ok(())
    }

    /// Serialize session to bytes for persistence.
    pub fn to_snapshot(&self) -> Result<Vec<u8>, CryptoError> {
        let snapshot = RatchetSessionSnapshot {
            dh_pair_seed: self.dh_pair.private.to_bytes(),
            dh_remote: self.dh_remote,
            root_key: self.root_key.0,
            send_chain_key: self.send_chain.as_ref().map(|ck| ck.0),
            recv_chain_key: self.recv_chain.as_ref().map(|ck| ck.0),
            send_count: self.send_count,
            recv_count: self.recv_count,
            prev_chain_length: self.prev_chain_length,
            skipped_keys: self.skipped_keys.iter().map(|(k, v)| (*k, v.0)).collect(),
        };
        rmp_serde::to_vec_named(&snapshot).map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Restore session from serialized bytes.
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, CryptoError> {
        let snap: RatchetSessionSnapshot =
            rmp_serde::from_slice(bytes).map_err(|e| CryptoError::Decoding(e.to_string()))?;

        Ok(Self {
            dh_pair: RatchetKeyPair::from_seed(snap.dh_pair_seed),
            dh_remote: snap.dh_remote,
            root_key: RootKey(snap.root_key),
            send_chain: snap.send_chain_key.map(ChainKey),
            recv_chain: snap.recv_chain_key.map(ChainKey),
            send_count: snap.send_count,
            recv_count: snap.recv_count,
            prev_chain_length: snap.prev_chain_length,
            skipped_keys: snap
                .skipped_keys
                .into_iter()
                .map(|(k, v)| (k, MessageKey(v)))
                .collect(),
            max_skip: DEFAULT_MAX_SKIP,
        })
    }
}

/// Derive a deterministic new DH keypair seed from root key + remote DH key.
fn derive_new_dh_seed(root_key: &RootKey, remote_dh: &[u8; 32]) -> [u8; 32] {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let (_, hk) = Hkdf::<Sha256>::extract(Some(&root_key.0), remote_dh);
    let mut seed = [0u8; 32];
    hk.expand(b"NewDHSeed", &mut seed)
        .expect("HKDF expand 32 bytes is valid");
    seed
}

/// Encrypt with ChaCha20-Poly1305.
fn aead_encrypt(
    key: &MessageKey,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    use chacha20poly1305::aead::generic_array::GenericArray;
    // Nonce: first 12 bytes of BLAKE3(key || "nonce")
    let nonce_src = {
        let mut h = blake3::Hasher::new();
        h.update(&key.0);
        h.update(b"nonce");
        *h.finalize().as_bytes()
    };
    let nonce = GenericArray::from_slice(&nonce_src[..12]);
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(&key.0));
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|e| CryptoError::Encryption(e.to_string()))
}

/// Decrypt with ChaCha20-Poly1305.
fn aead_decrypt(
    key: &MessageKey,
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    use chacha20poly1305::aead::generic_array::GenericArray;
    let nonce_src = {
        let mut h = blake3::Hasher::new();
        h.update(&key.0);
        h.update(b"nonce");
        *h.finalize().as_bytes()
    };
    let nonce = GenericArray::from_slice(&nonce_src[..12]);
    let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(&key.0));
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|e| CryptoError::Decryption(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sessions() -> (RatchetSession, RatchetSession) {
        let secret = [0x42u8; 32];
        let bob_seed = [0x01u8; 32];
        let bob_pair = RatchetKeyPair::from_seed(bob_seed);
        let alice_seed = [0x02u8; 32];

        let alice =
            RatchetSession::init_alice(&secret, &bob_pair.public.to_bytes(), alice_seed).unwrap();
        let bob = RatchetSession::init_bob(&secret, bob_seed).unwrap();
        (alice, bob)
    }

    #[test]
    fn basic_alice_to_bob() {
        let (mut alice, mut bob) = make_sessions();
        let plaintext = b"hello sovereign";
        let (hdr, ct) = alice.encrypt(plaintext, b"ad").unwrap();
        let pt = bob.decrypt(&hdr, &ct, b"ad").unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn ping_pong() {
        let (mut alice, mut bob) = make_sessions();

        let (h1, c1) = alice.encrypt(b"a-to-b msg1", b"").unwrap();
        let pt1 = bob.decrypt(&h1, &c1, b"").unwrap();
        assert_eq!(pt1, b"a-to-b msg1");

        let (h2, c2) = bob.encrypt(b"b-to-a reply", b"").unwrap();
        let pt2 = alice.decrypt(&h2, &c2, b"").unwrap();
        assert_eq!(pt2, b"b-to-a reply");

        let (h3, c3) = alice.encrypt(b"a-to-b msg2", b"").unwrap();
        let pt3 = bob.decrypt(&h3, &c3, b"").unwrap();
        assert_eq!(pt3, b"a-to-b msg2");
    }

    #[test]
    fn out_of_order_delivery() {
        let (mut alice, mut bob) = make_sessions();

        let (h1, c1) = alice.encrypt(b"msg1", b"").unwrap();
        let (h2, c2) = alice.encrypt(b"msg2", b"").unwrap();
        let (h3, c3) = alice.encrypt(b"msg3", b"").unwrap();

        // Deliver out of order: 3, 1, 2
        let pt3 = bob.decrypt(&h3, &c3, b"").unwrap();
        assert_eq!(pt3, b"msg3");

        let pt1 = bob.decrypt(&h1, &c1, b"").unwrap();
        assert_eq!(pt1, b"msg1");

        let pt2 = bob.decrypt(&h2, &c2, b"").unwrap();
        assert_eq!(pt2, b"msg2");
    }

    #[test]
    fn snapshot_round_trip() {
        let (mut alice, _bob) = make_sessions();
        let (hdr, ct) = alice.encrypt(b"before snapshot", b"").unwrap();
        let _ = (hdr, ct); // just advance state

        let snap = alice.to_snapshot().unwrap();
        let alice2 = RatchetSession::from_snapshot(&snap).unwrap();
        let snap2 = alice2.to_snapshot().unwrap();
        assert_eq!(snap, snap2);
    }

    #[test]
    fn wrong_associated_data_rejected() {
        let (mut alice, mut bob) = make_sessions();
        let (hdr, ct) = alice.encrypt(b"secret", b"correct-ad").unwrap();
        let result = bob.decrypt(&hdr, &ct, b"wrong-ad");
        assert!(result.is_err());
    }
}
