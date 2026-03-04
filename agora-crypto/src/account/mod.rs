//! Device account and pairwise session (Olm-equivalent).
//!
//! `Account` holds a device's long-term cryptographic identity:
//! - Ed25519 signing key (derived from a 32-byte master seed)
//! - X25519 identity key (HKDF-derived from the master seed)
//! - One-time prekey pool (deterministically derived, S-02 compliant)
//!
//! # Pairwise session establishment (3DH)
//! Sender (Alice):
//! 1. Derives ephemeral X25519 key from master seed + recipient context.
//! 2. Computes 3-DH:
//!    `DH(IK_A, IK_B) || DH(EK_A, IK_B) || DH(EK_A, OTK_B)`
//! 3. Derives shared secret via HKDF-SHA256.
//! 4. Initializes a Double Ratchet session as Alice.
//! 5. Encrypts the first message, wrapping header + ciphertext in a
//!    `PreKeyEnvelope` alongside the X3DH public keys.
//!
//! Receiver (Bob):
//! 1. Reads Alice's public keys from the `PreKeyEnvelope`.
//! 2. Derives the same shared secret via the symmetric 3-DH.
//! 3. Initializes a Double Ratchet session as Bob.
//! 4. Decrypts the embedded ciphertext.
//!
//! # Wire protocol
//! Algorithm identifier: `m.agora.pairwise.v1`
//! Message types: `0` = PreKey (first message, contains X3DH setup),
//!                `1` = Normal (subsequent Double Ratchet messages)
//!
//! # S-02 Compliance
//! OTK derivation and ephemeral key derivation are deterministic HKDF functions
//! of the master seed.  `Account::generate()` uses OS entropy exactly once.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{
    ratchet::{MessageHeader, RatchetSession},
    CryptoError,
};

// ── KDF helpers ───────────────────────────────────────────────────────────────

fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let (_, hk) = Hkdf::<Sha256>::extract(Some(salt), ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm).expect("HKDF expand 32 bytes is valid");
    okm
}

fn derive_identity_x25519(seed: &[u8; 32]) -> x25519_dalek::StaticSecret {
    x25519_dalek::StaticSecret::from(hkdf32(seed, b"agora-account", b"identity-x25519"))
}

fn derive_otk(seed: &[u8; 32], counter: u64) -> x25519_dalek::StaticSecret {
    let ikm: Vec<u8> = seed.iter().chain(counter.to_le_bytes().iter()).copied().collect();
    x25519_dalek::StaticSecret::from(hkdf32(&ikm, b"agora-account", b"otk"))
}

/// Derive the ephemeral key Alice uses when initiating with a specific recipient.
/// Deterministic from (seed, their_ik, their_otk) — S-02 compliant.
fn derive_ephemeral(
    seed: &[u8; 32],
    their_ik_pub: &[u8; 32],
    their_otk_pub: &[u8; 32],
) -> x25519_dalek::StaticSecret {
    let mut ikm = Vec::with_capacity(96);
    ikm.extend_from_slice(seed);
    ikm.extend_from_slice(their_ik_pub);
    ikm.extend_from_slice(their_otk_pub);
    x25519_dalek::StaticSecret::from(hkdf32(&ikm, b"agora-account", b"ephemeral"))
}

/// Derive Alice's initial Double Ratchet keypair seed from the shared secret.
fn derive_alice_ratchet_seed(shared_secret: &[u8; 32]) -> [u8; 32] {
    hkdf32(shared_secret, b"agora-pairwise-v1", b"alice-ratchet-seed")
}

/// 3DH KDF: concatenate three DH outputs, run HKDF-SHA256.
fn three_dh_kdf(dh1: &[u8; 32], dh2: &[u8; 32], dh3: &[u8; 32]) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(128);
    ikm.extend_from_slice(&[0xFFu8; 32]); // F padding (Signal convention)
    ikm.extend_from_slice(dh1);
    ikm.extend_from_slice(dh2);
    ikm.extend_from_slice(dh3);
    hkdf32(&ikm, b"agora-pairwise-v1", b"shared-secret")
}

// ── Wire envelope types ───────────────────────────────────────────────────────

/// Embedded in a PreKey (msg_type=0) message.  Contains all X3DH public data
/// and the first ratchet ciphertext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyEnvelope {
    /// Alice's long-term identity X25519 public key (32 bytes).
    pub ik: [u8; 32],
    /// Alice's ephemeral X25519 public key (32 bytes).
    pub ek: [u8; 32],
    /// Counter identifying which of Bob's OTKs was consumed (None if no OTK).
    pub otk_counter: Option<u64>,
    /// Serialized `MessageHeader` (msgpack bytes).
    pub hdr: Vec<u8>,
    /// ChaCha20-Poly1305 ciphertext of the first ratchet message.
    pub ct: Vec<u8>,
}

/// Subsequent Double Ratchet message (msg_type=1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalEnvelope {
    /// Serialized `MessageHeader` (msgpack bytes).
    pub hdr: Vec<u8>,
    /// ChaCha20-Poly1305 ciphertext.
    pub ct: Vec<u8>,
}

// ── Ed25519 Signature wrapper ─────────────────────────────────────────────────

/// An Ed25519 signature.
pub struct AgoraSignature(pub [u8; 64]);

impl AgoraSignature {
    /// Encode the signature as standard base64.
    pub fn to_base64(&self) -> String {
        B64.encode(self.0)
    }
}

// ── OneTimeKey ────────────────────────────────────────────────────────────────

/// A one-time prekey (X25519) ready for upload or use.
pub struct OneTimeKey {
    /// Monotonic counter identifying this key.
    pub counter: u64,
    /// X25519 public key bytes.
    pub public: [u8; 32],
}

impl OneTimeKey {
    /// Base64 of counter bytes — used as the key-id in upload payloads.
    pub fn key_id_base64(&self) -> String {
        B64.encode(self.counter.to_le_bytes())
    }

    /// Base64 of public key bytes.
    pub fn public_base64(&self) -> String {
        B64.encode(self.public)
    }
}

// ── PairwiseSession ───────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct PreKeyState {
    our_ik_pub: [u8; 32],
    our_ek_pub: [u8; 32],
    otk_counter: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct PairwiseSnapshot {
    ratchet: Vec<u8>,
    prekey_state: Option<PreKeyState>,
}

/// A bidirectional pairwise encryption session.
///
/// - Created outbound via `Account::create_outbound_session`: first `encrypt`
///   emits a PreKey envelope (msg_type=0); subsequent `encrypt` calls emit
///   Normal (msg_type=1) once the session is confirmed by a received message.
/// - Created inbound via `Account::create_inbound_session`: always emits
///   Normal (msg_type=1) envelopes.
pub struct PairwiseSession {
    ratchet: RatchetSession,
    prekey_state: Option<PreKeyState>,
}

impl PairwiseSession {
    /// Encrypt `plaintext`.  Returns `(msg_type, base64_body)`.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(u8, String), CryptoError> {
        let (header, ct) = self.ratchet.encrypt(plaintext, b"")?;
        let hdr = header_to_bytes(&header)?;

        if let Some(ref pks) = self.prekey_state {
            let envelope = PreKeyEnvelope {
                ik: pks.our_ik_pub,
                ek: pks.our_ek_pub,
                otk_counter: pks.otk_counter,
                hdr,
                ct,
            };
            let bytes = rmp_serde::to_vec_named(&envelope)
                .map_err(|e| CryptoError::Encoding(e.to_string()))?;
            Ok((0, B64.encode(bytes)))
        } else {
            let envelope = NormalEnvelope { hdr, ct };
            let bytes = rmp_serde::to_vec_named(&envelope)
                .map_err(|e| CryptoError::Encoding(e.to_string()))?;
            Ok((1, B64.encode(bytes)))
        }
    }

    /// Decrypt a Normal (msg_type=1) message.  Returns the plaintext.
    ///
    /// Also clears the prekey state, transitioning the session to Normal mode.
    pub fn decrypt_normal(&mut self, body: &str) -> Result<Vec<u8>, CryptoError> {
        let bytes = B64.decode(body).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        let env: NormalEnvelope = rmp_serde::from_slice(&bytes)
            .map_err(|e| CryptoError::Decoding(e.to_string()))?;
        let header = bytes_to_header(&env.hdr)?;
        let pt = self.ratchet.decrypt(&header, &env.ct, b"")?;
        // Receiving any message confirms the session; switch to Normal mode.
        self.prekey_state = None;
        Ok(pt)
    }

    /// Serialize for persistent storage.
    pub fn to_snapshot(&self) -> Result<String, CryptoError> {
        let snap = PairwiseSnapshot {
            ratchet: self.ratchet.to_snapshot()?,
            prekey_state: self.prekey_state.as_ref().map(|pks| PreKeyState {
                our_ik_pub: pks.our_ik_pub,
                our_ek_pub: pks.our_ek_pub,
                otk_counter: pks.otk_counter,
            }),
        };
        serde_json::to_string(&snap).map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Restore from serialized storage.
    pub fn from_snapshot(s: &str) -> Result<Self, CryptoError> {
        let snap: PairwiseSnapshot =
            serde_json::from_str(s).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        Ok(Self {
            ratchet: RatchetSession::from_snapshot(&snap.ratchet)?,
            prekey_state: snap.prekey_state,
        })
    }
}

// ── MessageHeader serialization ───────────────────────────────────────────────

fn header_to_bytes(h: &MessageHeader) -> Result<Vec<u8>, CryptoError> {
    rmp_serde::to_vec_named(h).map_err(|e| CryptoError::Encoding(e.to_string()))
}

fn bytes_to_header(b: &[u8]) -> Result<MessageHeader, CryptoError> {
    rmp_serde::from_slice(b).map_err(|e| CryptoError::Decoding(e.to_string()))
}

// ── Account ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct AccountSnapshot {
    seed: [u8; 32],
    otk_counter: u64,
    /// Counters of OTKs generated but not yet marked as published.
    unpublished: Vec<u64>,
}

/// Device cryptographic identity: signing key, X25519 DH key, OTK pool.
pub struct Account {
    seed: [u8; 32],
    signing_key: SigningKey,
    identity_x25519: x25519_dalek::StaticSecret,
    otk_counter: u64,
    unpublished: Vec<u64>,
}

impl Account {
    /// Create a new account using OS entropy (one-time randomness for the seed).
    pub fn generate() -> Result<Self, CryptoError> {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Ok(Self::from_seed(seed))
    }

    /// Restore an account from a deterministic 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        let identity_x25519 = derive_identity_x25519(&seed);
        Self {
            seed,
            signing_key,
            identity_x25519,
            otk_counter: 0,
            unpublished: Vec::new(),
        }
    }

    /// Return `(x25519_pub_base64, ed25519_pub_base64)`.
    pub fn identity_keys(&self) -> (String, String) {
        let x25519_pub = x25519_dalek::PublicKey::from(&self.identity_x25519);
        let ed25519_pub = self.signing_key.verifying_key();
        (B64.encode(x25519_pub.as_bytes()), B64.encode(ed25519_pub.as_bytes()))
    }

    /// Sign `message` with the Ed25519 identity key.
    pub fn sign(&self, message: &[u8]) -> AgoraSignature {
        AgoraSignature(self.signing_key.sign(message).to_bytes())
    }

    /// Generate `count` new one-time prekeys, recording them as unpublished.
    pub fn generate_one_time_keys(&mut self, count: usize) {
        for _ in 0..count {
            self.unpublished.push(self.otk_counter);
            self.otk_counter += 1;
        }
    }

    /// Return all unpublished one-time prekeys (public key side).
    pub fn one_time_keys(&self) -> Vec<OneTimeKey> {
        self.unpublished
            .iter()
            .map(|&counter| {
                let secret = derive_otk(&self.seed, counter);
                let public = x25519_dalek::PublicKey::from(&secret);
                OneTimeKey {
                    counter,
                    public: *public.as_bytes(),
                }
            })
            .collect()
    }

    /// Clear the unpublished list (called after successful server upload).
    pub fn mark_keys_as_published(&mut self) {
        self.unpublished.clear();
    }

    /// Establish an outbound pairwise session with a recipient.
    ///
    /// # Arguments
    /// - `their_ik_pub_b64` — recipient's identity X25519 public key (base64)
    /// - `their_otk_pub_b64` — recipient's one-time prekey public key (base64)
    /// - `their_otk_counter` — counter embedded in PreKey envelopes (for
    ///   recipient to derive the private key); `None` if no OTK was available
    pub fn create_outbound_session(
        &self,
        their_ik_pub_b64: &str,
        their_otk_pub_b64: &str,
        their_otk_counter: Option<u64>,
    ) -> Result<PairwiseSession, CryptoError> {
        let their_ik_bytes = b64_to_32(their_ik_pub_b64, "their_ik")?;
        let their_otk_bytes = b64_to_32(their_otk_pub_b64, "their_otk")?;

        let their_ik_pub = x25519_dalek::PublicKey::from(their_ik_bytes);
        let their_otk_pub = x25519_dalek::PublicKey::from(their_otk_bytes);

        let ek_secret = derive_ephemeral(&self.seed, &their_ik_bytes, &their_otk_bytes);
        let ek_pub = x25519_dalek::PublicKey::from(&ek_secret);

        let dh1 = self.identity_x25519.diffie_hellman(&their_ik_pub).to_bytes();
        let dh2 = ek_secret.diffie_hellman(&their_ik_pub).to_bytes();
        let dh3 = ek_secret.diffie_hellman(&their_otk_pub).to_bytes();

        let shared = three_dh_kdf(&dh1, &dh2, &dh3);
        let alice_ratchet_seed = derive_alice_ratchet_seed(&shared);
        let ratchet = RatchetSession::init_alice(&shared, &their_ik_bytes, alice_ratchet_seed)?;

        let our_ik_pub = x25519_dalek::PublicKey::from(&self.identity_x25519);

        Ok(PairwiseSession {
            ratchet,
            prekey_state: Some(PreKeyState {
                our_ik_pub: *our_ik_pub.as_bytes(),
                our_ek_pub: *ek_pub.as_bytes(),
                otk_counter: their_otk_counter,
            }),
        })
    }

    /// Create an inbound session from a received `PreKeyEnvelope`.
    ///
    /// Returns `(session, plaintext)` where `plaintext` is the decrypted
    /// payload of the initial message embedded in the envelope.
    pub fn create_inbound_session(
        &self,
        envelope: &PreKeyEnvelope,
    ) -> Result<(PairwiseSession, Vec<u8>), CryptoError> {
        let their_ik_pub = x25519_dalek::PublicKey::from(envelope.ik);
        let their_ek_pub = x25519_dalek::PublicKey::from(envelope.ek);

        // Derive the OTK private key from counter (if one was used).
        let otk_secret = envelope.otk_counter.map(|c| derive_otk(&self.seed, c));

        let dh1 = self.identity_x25519.diffie_hellman(&their_ik_pub).to_bytes();
        let dh2 = self.identity_x25519.diffie_hellman(&their_ek_pub).to_bytes();
        let dh3 = otk_secret
            .as_ref()
            .map(|s| s.diffie_hellman(&their_ek_pub).to_bytes())
            .unwrap_or([0u8; 32]);

        let dh3_input: &[u8; 32] = if envelope.otk_counter.is_some() { &dh3 } else { &[0u8; 32] };
        let shared = three_dh_kdf(&dh1, &dh2, dh3_input);

        // Bob's ratchet key IS his identity X25519 key.
        let bob_ik_seed = hkdf32(&self.seed, b"agora-account", b"identity-x25519");
        let mut ratchet = RatchetSession::init_bob(&shared, bob_ik_seed)?;

        let header = bytes_to_header(&envelope.hdr)?;
        let plaintext = ratchet.decrypt(&header, &envelope.ct, b"")?;

        Ok((PairwiseSession { ratchet, prekey_state: None }, plaintext))
    }

    /// Decode a PreKey envelope from a base64 body string.
    pub fn decode_prekey_envelope(body: &str) -> Result<PreKeyEnvelope, CryptoError> {
        let bytes = B64.decode(body).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        rmp_serde::from_slice(&bytes).map_err(|e| CryptoError::Decoding(e.to_string()))
    }

    /// Serialize for persistent storage.
    pub fn to_snapshot(&self) -> Result<String, CryptoError> {
        let snap = AccountSnapshot {
            seed: self.seed,
            otk_counter: self.otk_counter,
            unpublished: self.unpublished.clone(),
        };
        serde_json::to_string(&snap).map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Restore from serialized storage.
    pub fn from_snapshot(s: &str) -> Result<Self, CryptoError> {
        let snap: AccountSnapshot =
            serde_json::from_str(s).map_err(|e| CryptoError::Decoding(e.to_string()))?;
        Ok(Self::from_seed_with_state(snap.seed, snap.otk_counter, snap.unpublished))
    }

    fn from_seed_with_state(seed: [u8; 32], otk_counter: u64, unpublished: Vec<u64>) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        let identity_x25519 = derive_identity_x25519(&seed);
        Self { seed, signing_key, identity_x25519, otk_counter, unpublished }
    }
}

// ── Utility ───────────────────────────────────────────────────────────────────

fn b64_to_32(s: &str, field: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = B64.decode(s).map_err(|e| CryptoError::InvalidKey(format!("{field}: base64: {e}")))?;
    bytes.try_into().map_err(|_| CryptoError::InvalidKey(format!("{field}: expected 32 bytes")))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account(seed_byte: u8) -> Account {
        Account::from_seed([seed_byte; 32])
    }

    #[test]
    fn identity_keys_deterministic() {
        let a = make_account(0x01);
        let b = make_account(0x01);
        assert_eq!(a.identity_keys(), b.identity_keys());
    }

    #[test]
    fn identity_keys_differ_by_seed() {
        let a = make_account(0x01);
        let b = make_account(0x02);
        assert_ne!(a.identity_keys(), b.identity_keys());
    }

    #[test]
    fn sign_and_verify() {
        let account = make_account(0x10);
        let msg = b"sovereign message";
        let sig = account.sign(msg);
        // Verify using the public key directly.
        use ed25519_dalek::Verifier;
        let vk = account.signing_key.verifying_key();
        let sig_obj = ed25519_dalek::Signature::from_bytes(&sig.0);
        assert!(vk.verify(msg, &sig_obj).is_ok());
    }

    #[test]
    fn one_time_keys_deterministic() {
        let mut a = make_account(0x20);
        a.generate_one_time_keys(5);
        let keys1 = a.one_time_keys();

        let mut b = make_account(0x20);
        b.generate_one_time_keys(5);
        let keys2 = b.one_time_keys();

        for (k1, k2) in keys1.iter().zip(keys2.iter()) {
            assert_eq!(k1.counter, k2.counter);
            assert_eq!(k1.public, k2.public);
        }
    }

    #[test]
    fn account_snapshot_roundtrip() {
        let mut a = make_account(0x30);
        a.generate_one_time_keys(3);
        let snap = a.to_snapshot().unwrap();
        let b = Account::from_snapshot(&snap).unwrap();
        assert_eq!(a.identity_keys(), b.identity_keys());
        assert_eq!(a.otk_counter, b.otk_counter);
        assert_eq!(a.unpublished, b.unpublished);
    }

    #[test]
    fn pairwise_session_alice_to_bob() {
        let alice = make_account(0xAA);
        let mut bob = make_account(0xBB);

        // Bob publishes one OTK.
        bob.generate_one_time_keys(1);
        let bob_otks = bob.one_time_keys();
        let otk = &bob_otks[0];

        let (bob_ik_b64, _) = bob.identity_keys();
        let mut alice_session = alice
            .create_outbound_session(&bob_ik_b64, &otk.public_base64(), Some(otk.counter))
            .unwrap();

        let plaintext = b"hello bob";
        let (msg_type, body) = alice_session.encrypt(plaintext).unwrap();
        assert_eq!(msg_type, 0, "first message must be PreKey");

        let envelope = Account::decode_prekey_envelope(&body).unwrap();
        let (_, decrypted) = bob.create_inbound_session(&envelope).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn pairwise_session_subsequent_messages_are_normal() {
        let alice = make_account(0xAA);
        let mut bob = make_account(0xBB);

        bob.generate_one_time_keys(1);
        let bob_otks = bob.one_time_keys();
        let otk = &bob_otks[0];
        let (bob_ik_b64, _) = bob.identity_keys();

        let mut alice_session = alice
            .create_outbound_session(&bob_ik_b64, &otk.public_base64(), Some(otk.counter))
            .unwrap();

        // First message establishes session with Bob.
        let (_, body1) = alice_session.encrypt(b"msg1").unwrap();
        let env1 = Account::decode_prekey_envelope(&body1).unwrap();
        let (mut bob_session, _) = bob.create_inbound_session(&env1).unwrap();

        // Bob sends a Normal reply to Alice.
        let (bob_msg_type, bob_body) = bob_session.encrypt(b"reply from bob").unwrap();
        assert_eq!(bob_msg_type, 1, "inbound session should emit Normal");

        // Alice decrypts Bob's reply; her session transitions to Normal mode.
        let alice_decrypted = alice_session.decrypt_normal(&bob_body).unwrap();
        assert_eq!(alice_decrypted, b"reply from bob");

        // Alice's next message should now be Normal.
        let (alice_type2, _) = alice_session.encrypt(b"msg2").unwrap();
        assert_eq!(alice_type2, 1, "Alice should emit Normal after confirmation");
    }

    #[test]
    fn pairwise_session_snapshot_roundtrip() {
        let alice = make_account(0xAA);
        let mut bob = make_account(0xBB);

        bob.generate_one_time_keys(1);
        let otk = &bob.one_time_keys()[0];
        let (bob_ik_b64, _) = bob.identity_keys();

        let alice_session = alice
            .create_outbound_session(&bob_ik_b64, &otk.public_base64(), Some(otk.counter))
            .unwrap();

        let snap = alice_session.to_snapshot().unwrap();
        let alice_session2 = PairwiseSession::from_snapshot(&snap).unwrap();
        let snap2 = alice_session2.to_snapshot().unwrap();
        assert_eq!(snap, snap2);
    }

    #[test]
    fn mark_keys_as_published_clears_unpublished() {
        let mut a = make_account(0x40);
        a.generate_one_time_keys(10);
        assert_eq!(a.one_time_keys().len(), 10);
        a.mark_keys_as_published();
        assert!(a.one_time_keys().is_empty());
    }
}
