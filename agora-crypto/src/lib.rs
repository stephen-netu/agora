//! Agora sovereign cryptographic primitives.
//!
//! # Modules
//! - `timestamp` — S-02 compliant deterministic timestamp provider
//! - `ids` — BLAKE3 content-addressed ID generation
//! - `ratchet` — Double Ratchet (Signal spec)
//! - `agreement` — X3DH key agreement with `KeyAgreement` trait
//! - `envelope` — Saltpack-inspired MessagePack envelopes
//! - `identity` — Ed25519 agent identity and per-agent sigchain
//! - `store` — Abstract crypto persistence trait

#![forbid(unsafe_code)]
#![deny(unused_imports)]
#![deny(unused_variables)]
#![deny(unused_mut)]

pub mod account;
pub mod agreement;
pub mod envelope;
pub mod group;
pub mod identity;
pub mod ids;
pub mod ratchet;
pub mod store;
pub mod timestamp;

pub use ids::{event_id, media_id, room_id};
pub use timestamp::{SequenceTimestamp, TimestampProvider, DEFAULT_EPOCH_MS};
pub use identity::{AgentId, AgentIdentity, AnchorPayload, SignedEntry, Sigchain, SigchainBody, SigchainLink, TrustState};
pub use identity::display::agent_display_name;

/// Errors produced by agora-crypto operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// A cryptographic key could not be generated.
    #[error("key generation failed: {0}")]
    KeyGeneration(String),

    /// Encryption of a message failed.
    #[error("encryption failed: {0}")]
    Encryption(String),

    /// Decryption of a message failed (e.g. bad tag, wrong key).
    #[error("decryption failed: {0}")]
    Decryption(String),

    /// An Ed25519 signature did not verify.
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// A ratchet or crypto session was not found in the store.
    #[error("session not found: {0}")]
    SessionNotFound(String),

    /// Serialization of a message or payload failed.
    #[error("message encoding failed: {0}")]
    Encoding(String),

    /// Deserialization of a message or payload failed.
    #[error("message decoding failed: {0}")]
    Decoding(String),

    /// The deterministic sequence counter has overflowed `u64::MAX`.
    #[error("sequence overflow")]
    SequenceOverflow,

    /// Key material was malformed or had an unexpected length.
    #[error("invalid key material: {0}")]
    InvalidKey(String),

    /// Too many skipped message keys were requested in the Double Ratchet.
    #[error("max skip exceeded: limit={limit}, got={got}")]
    MaxSkipExceeded {
        /// The configured maximum number of skippable messages.
        limit: u32,
        /// The number of messages that would need to be skipped.
        got: u32,
    },

    /// A sigchain link failed verification.
    #[error("sigchain verification failed: {0}")]
    SigchainVerification(String),

    /// A crypto store read or write operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// An agent identity operation failed.
    #[error("identity error: {0}")]
    Identity(String),
}
