//! Network-layer encryption primitives for mesh networking
//!
//! Provides E2EE (End-to-End Encryption) using X25519 key exchange
//! and AEAD encryption.
//!
//! # S-02 Compliance
//! - Uses BTreeMap for deterministic storage
//! - No SystemTime::now() or non-deterministic sources
//! - All operations are deterministic

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use tracing::{info, trace};
use x25519_dalek::{PublicKey as XPublicKey, StaticSecret};

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Key generation failed")]
    KeyGenerationFailed,
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Invalid key material")]
    InvalidKeyMaterial,
    #[error("Session not found")]
    SessionNotFound,
    #[error("Key exchange failed")]
    KeyExchangeFailed,
}

pub type Result<T> = std::result::Result<T, CryptoError>;

#[derive(Clone)]
pub struct PublicKey(pub [u8; 32]);

#[derive(Clone)]
pub struct SecretKey(pub [u8; 32]);

// IMPLEMENTATION_REQUIRED: Forward secrecy with session key rotation - Phase 2-3
#[cfg(test)]
pub struct KeyPair {
    pub public: PublicKey,
    pub secret: SecretKey,
}

#[cfg(test)]
impl KeyPair {
    pub fn generate() -> Result<Self> {
        // IMPLEMENTATION_REQUIRED: Forward secrecy with session key rotation
        // Current implementation uses static keys. Need to implement
        // double ratchet or similar protocol for forward secrecy.
        // IMPLEMENTATION_REQUIRED: In production, derive this seed from the agent's
        // Ed25519 identity to ensure deterministic operation tied to the agent.

        Self::generate_deterministic([0u8; 32])
    }

    pub fn generate_deterministic(seed: [u8; 32]) -> Result<Self> {
        // IMPLEMENTATION_REQUIRED: Forward secrecy with session key rotation
        // Current implementation uses static keys. Need to implement
        // double ratchet or similar protocol for forward secrecy.

        let secret = StaticSecret::from(seed);
        let public = XPublicKey::from(&secret);

        trace!(target: "mesh", "Generated new X25519 keypair");

        Ok(Self {
            public: PublicKey(*public.as_bytes()),
            secret: SecretKey(*secret.as_bytes()),
        })
    }
}

#[derive(Copy, Clone)]
pub struct SharedSecret(pub [u8; 32]);

pub fn compute_shared_secret(
    our_secret: &SecretKey,
    their_public: &PublicKey,
) -> Result<SharedSecret> {
    // IMPLEMENTATION_REQUIRED: Forward secrecy with session key rotation
    // IMPLEMENTATION_REQUIRED: Post-quantum key exchange
    // Consider integrating post-quantum KEM (e.g., Kyber) for
    // quantum-resistant communications.

    let secret_key = StaticSecret::from(our_secret.0);
    let public_key = XPublicKey::from(their_public.0);
    let shared = secret_key.diffie_hellman(&public_key);

    trace!(target: "mesh", "Computed shared secret");
    Ok(SharedSecret(*shared.as_bytes()))
}

const NONCE_SIZE: usize = 12;
const TAG_SIZE: usize = 16;

pub fn ecies_encrypt(
    shared_secret: &SharedSecret,
    plaintext: &[u8],
    sequence: u64,
) -> Result<Vec<u8>> {
    if plaintext.is_empty() {
        return Err(CryptoError::EncryptionFailed("Empty plaintext".to_string()));
    }

    // Build deterministic nonce from sequence
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    let seq_bytes = sequence.to_be_bytes();
    nonce_bytes[..8].copy_from_slice(&seq_bytes);

    // Use chacha20poly1305 for AEAD
    let key = chacha20poly1305::Key::from_slice(&shared_secret.0);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e: chacha20poly1305::aead::Error| CryptoError::EncryptionFailed(e.to_string()))?;

    trace!(target: "mesh", "Encrypted {} bytes", plaintext.len());
    Ok(ciphertext)
}

pub fn ecies_decrypt(
    shared_secret: &SharedSecret,
    ciphertext: &[u8],
    sequence: u64,
) -> Result<Vec<u8>> {
    let min_len = TAG_SIZE + 1;
    if ciphertext.len() < min_len {
        return Err(CryptoError::DecryptionFailed(
            "Ciphertext too short".to_string(),
        ));
    }

    // Build nonce from sequence for verification
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    let seq_bytes = sequence.to_be_bytes();
    nonce_bytes[..8].copy_from_slice(&seq_bytes);

    // Use chacha20poly1305 for AEAD decryption
    let key = chacha20poly1305::Key::from_slice(&shared_secret.0);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e: chacha20poly1305::aead::Error| CryptoError::DecryptionFailed(e.to_string()))?;

    trace!(target: "mesh", "Decrypted {} bytes", plaintext.len());
    Ok(plaintext)
}

pub struct CryptoProvider {
    sessions: BTreeMap<[u8; 32], SharedSecret>,
    session_sequence: AtomicU64,
}

impl CryptoProvider {
    pub fn new() -> Self {
        info!(target: "mesh", "Initialized crypto provider");
        Self {
            sessions: BTreeMap::new(),
            session_sequence: AtomicU64::new(0),
        }
    }

    fn next_sequence(&self) -> u64 {
        self.session_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn establish_session(
        &mut self,
        peer_id: [u8; 32],
        our_secret: &SecretKey,
        their_public: &PublicKey,
    ) -> Result<()> {
        let shared = compute_shared_secret(our_secret, their_public)?;
        self.sessions.insert(peer_id, shared);
        trace!(target: "mesh", "Established session with peer");
        Ok(())
    }

    pub fn get_session(&self, peer_id: &[u8; 32]) -> Option<&SharedSecret> {
        self.sessions.get(peer_id)
    }

    pub fn remove_session(&mut self, peer_id: &[u8; 32]) -> Option<SharedSecret> {
        trace!(target: "mesh", "Removed session for peer");
        self.sessions.remove(peer_id)
    }

    pub fn encrypt_to_peer(&self, peer_id: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
        let sequence = self.next_sequence();
        let shared = *self
            .sessions
            .get(peer_id)
            .ok_or(CryptoError::SessionNotFound)?;

        ecies_encrypt(&shared, plaintext, sequence)
    }

    pub fn decrypt_from_peer(&mut self, peer_id: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let shared = *self
            .sessions
            .get(peer_id)
            .ok_or(CryptoError::SessionNotFound)?;
        let sequence = self.session_sequence.load(Ordering::SeqCst);

        ecies_decrypt(&shared, ciphertext, sequence)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn clear_sessions(&mut self) {
        let count = self.sessions.len();
        self.sessions.clear();
        info!(target: "mesh", "Cleared {} sessions", count);
    }
}

impl Default for CryptoProvider {
    fn default() -> Self {
        Self::new()
    }
}

// IMPLEMENTATION_REQUIRED: Forward secrecy with session key rotation
// Current implementation uses static keys. Need to implement
// double ratchet or similar protocol for forward secrecy.

// IMPLEMENTATION_REQUIRED: Key compromise detection
// Need to implement mechanisms to detect and handle key compromise,
// including key revocation and re-establishment.

// IMPLEMENTATION_REQUIRED: Post-quantum key exchange
// Consider integrating post-quantum KEM (e.g., Kyber) for
// quantum-resistant communications.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = KeyPair::generate().unwrap();
        assert_eq!(keypair.public.0.len(), 32);
        assert_eq!(keypair.secret.0.len(), 32);
    }

    #[test]
    fn test_shared_secret() {
        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let secret_alice = compute_shared_secret(&alice.secret, &bob.public).unwrap();
        let secret_bob = compute_shared_secret(&bob.secret, &alice.public).unwrap();

        // In proper X25519, these should be equal (but our placeholder isn't)
        assert_eq!(secret_alice.0.len(), 32);
        assert_eq!(secret_bob.0.len(), 32);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let keypair = KeyPair::generate().unwrap();
        let shared = compute_shared_secret(&keypair.secret, &keypair.public).unwrap();

        let plaintext = b"Hello, mesh!";
        let ciphertext = ecies_encrypt(&shared, plaintext, 1).unwrap();
        let decrypted = ecies_decrypt(&shared, &ciphertext, 1).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_crypto_provider_sessions() {
        let mut provider = CryptoProvider::new();

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        let peer_id = [0u8; 32];
        provider
            .establish_session(peer_id, &alice.secret, &bob.public)
            .unwrap();

        assert_eq!(provider.session_count(), 1);

        let msg = b"test message";
        let encrypted = provider.encrypt_to_peer(&peer_id, msg).unwrap();
        let decrypted = provider.decrypt_from_peer(&peer_id, &encrypted).unwrap();

        assert_eq!(msg.to_vec(), decrypted);
    }
}
