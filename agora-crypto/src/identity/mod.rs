//! Agent identity and append-only sigchain.
//!
//! Each agent owns one `AgentIdentity` (Ed25519 signing key derived from a
//! 32-byte seed) and an associated `Sigchain` that records key events in an
//! authenticated, hash-linked log.

use std::fmt;

use blake3;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::CryptoError;

// ── AgentId ───────────────────────────────────────────────────────────────────

/// Compact 32-byte agent identifier derived from an Ed25519 verifying key.
///
/// Computed as `BLAKE3(verifying_key.to_bytes())`.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct AgentId(pub [u8; 32]);

impl AgentId {
    /// Derive an `AgentId` from an Ed25519 verifying key.
    pub fn from_public_key(key: &VerifyingKey) -> Self {
        Self(*blake3::hash(key.as_bytes()).as_bytes())
    }

    /// Return a reference to the raw 32-byte identifier.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex: String = self.0[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        write!(f, "agnt-{hex}")
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({self})")
    }
}

// ── AgentIdentity ─────────────────────────────────────────────────────────────

/// An agent's cryptographic identity: Ed25519 signing key and its derived ID.
pub struct AgentIdentity {
    /// The stable identifier for this agent.
    pub agent_id: AgentId,
    signing_key: ed25519_dalek::SigningKey,
}

impl AgentIdentity {
    /// Create an `AgentIdentity` from a deterministic 32-byte seed.
    ///
    /// The seed must be kept secret; the same seed always produces the same
    /// identity (S-02 determinism guarantee).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(seed);
        let agent_id = AgentId::from_public_key(&signing_key.verifying_key());
        Self { agent_id, signing_key }
    }

    /// Return the Ed25519 verifying key for this identity.
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Return a reference to the `AgentId`.
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// Sign arbitrary bytes and return the 64-byte `Signature`.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify that `signature` was produced by this identity over `message`.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), CryptoError> {
        self.signing_key
            .verifying_key()
            .verify(message, signature)
            .map_err(|e| CryptoError::InvalidSignature(e.to_string()))
    }
}

// ── SigchainBody ─────────────────────────────────────────────────────────────

/// The typed payload of one sigchain link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SigchainBody {
    /// First link — establishes the agent's identity.
    Genesis { agent_id: AgentId },
    /// Associate a named device with an Ed25519 verifying key.
    AddDevice {
        device_id: String,
        /// `VerifyingKey::to_bytes()` (32 bytes).
        device_key: Vec<u8>,
    },
    /// Remove a previously added device.
    RevokeDevice { device_id: String },
    /// Rotate to a new Ed25519 verifying key.
    RotateKey {
        /// `VerifyingKey::to_bytes()` (32 bytes).
        new_key: Vec<u8>,
    },
}

// ── SigchainLink ─────────────────────────────────────────────────────────────

/// One authenticated entry in the sigchain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigchainLink {
    /// 0-based monotonic sequence number.
    pub seqno: u64,
    /// BLAKE3 hash of the previous link's canonical encoding.
    /// Genesis link uses `[0u8; 32]`.
    pub prev_hash: [u8; 32],
    /// Typed payload.
    pub body: SigchainBody,
    /// Ed25519 signature bytes (64 bytes).
    pub signature: Vec<u8>,
    /// Signer's `VerifyingKey::to_bytes()` (32 bytes).
    pub signer: [u8; 32],
}

impl SigchainLink {
    /// Compute the bytes that are signed for this link.
    ///
    /// We sign `rmp_serde::to_vec_named((seqno, prev_hash, body))`.
    fn signed_bytes(seqno: u64, prev_hash: &[u8; 32], body: &SigchainBody) -> Result<Vec<u8>, CryptoError> {
        rmp_serde::to_vec_named(&(seqno, prev_hash, body))
            .map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Compute the BLAKE3 hash of the canonical serialization of this link.
    ///
    /// The hash is over the full link (including the signature) so that a
    /// later link's `prev_hash` commits to the entire prior record.
    pub fn canonical_hash(&self) -> Result<[u8; 32], CryptoError> {
        let bytes = rmp_serde::to_vec_named(self)
            .map_err(|e| CryptoError::Encoding(e.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

// ── Sigchain ─────────────────────────────────────────────────────────────────

/// Append-only chain of signed identity events for one agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sigchain {
    /// The agent whose events are recorded here.
    pub agent_id: AgentId,
    /// The ordered list of authenticated links.
    pub links: Vec<SigchainLink>,
}

impl Sigchain {
    /// Create a new sigchain with a single `Genesis` link signed by `identity`.
    pub fn genesis(identity: &AgentIdentity) -> Result<Self, CryptoError> {
        let body = SigchainBody::Genesis {
            agent_id: identity.agent_id.clone(),
        };
        let prev_hash = [0u8; 32];
        let seqno: u64 = 0;

        let to_sign = SigchainLink::signed_bytes(seqno, &prev_hash, &body)?;
        let signature = identity.sign(&to_sign);
        let signer: [u8; 32] = identity.public_key().to_bytes();

        let link = SigchainLink {
            seqno,
            prev_hash,
            body,
            signature: signature.to_bytes().to_vec(),
            signer,
        };

        Ok(Self {
            agent_id: identity.agent_id.clone(),
            links: vec![link],
        })
    }

    /// Append a new signed link to the chain.
    ///
    /// The signer does not have to be the same identity as the chain owner —
    /// devices with delegated keys can append under their own key.
    pub fn append(
        &mut self,
        body: SigchainBody,
        signer: &AgentIdentity,
    ) -> Result<(), CryptoError> {
        let seqno = self.links.len() as u64;

        let prev_hash = self
            .links
            .last()
            .ok_or_else(|| {
                CryptoError::SigchainVerification("chain is empty — call genesis first".into())
            })?
            .canonical_hash()?;

        let to_sign = SigchainLink::signed_bytes(seqno, &prev_hash, &body)?;
        let signature = signer.sign(&to_sign);
        let signer_bytes: [u8; 32] = signer.public_key().to_bytes();

        let link = SigchainLink {
            seqno,
            prev_hash,
            body,
            signature: signature.to_bytes().to_vec(),
            signer: signer_bytes,
        };

        self.links.push(link);
        Ok(())
    }

    /// Verify the integrity of the entire chain.
    ///
    /// Checks:
    /// 1. Sequence numbers are contiguous starting from 0.
    /// 2. Each link's `prev_hash` matches the BLAKE3 hash of the prior link.
    /// 3. Every signature is valid against the recorded signer key.
    /// 4. The first link is a `Genesis` body with `prev_hash = [0u8; 32]`.
    pub fn verify_chain(&self) -> Result<(), CryptoError> {
        if self.links.is_empty() {
            return Err(CryptoError::SigchainVerification("chain is empty".into()));
        }

        for (idx, link) in self.links.iter().enumerate() {
            // Sequence number must be contiguous.
            if link.seqno != idx as u64 {
                return Err(CryptoError::SigchainVerification(format!(
                    "link {idx}: expected seqno {idx}, got {}",
                    link.seqno
                )));
            }

            // Genesis checks.
            if idx == 0 {
                if link.prev_hash != [0u8; 32] {
                    return Err(CryptoError::SigchainVerification(
                        "genesis link must have prev_hash = [0; 32]".into(),
                    ));
                }
                match &link.body {
                    SigchainBody::Genesis { .. } => {}
                    _ => {
                        return Err(CryptoError::SigchainVerification(
                            "first link must have Genesis body".into(),
                        ))
                    }
                }
            } else {
                // Hash-link continuity.
                let expected_hash = self.links[idx - 1].canonical_hash()?;
                if link.prev_hash != expected_hash {
                    return Err(CryptoError::SigchainVerification(format!(
                        "link {idx}: prev_hash mismatch"
                    )));
                }
            }

            // Signature verification.
            let vk = VerifyingKey::from_bytes(&link.signer)
                .map_err(|e| CryptoError::SigchainVerification(format!("link {idx}: bad signer key: {e}")))?;

            let to_sign = SigchainLink::signed_bytes(link.seqno, &link.prev_hash, &link.body)?;

            let sig_bytes: [u8; 64] = link.signature.as_slice().try_into().map_err(|_| {
                CryptoError::SigchainVerification(format!(
                    "link {idx}: signature must be 64 bytes"
                ))
            })?;
            let signature = Signature::from_bytes(&sig_bytes);

            vk.verify(&to_sign, &signature).map_err(|e| {
                CryptoError::SigchainVerification(format!("link {idx}: invalid signature: {e}"))
            })?;
        }

        Ok(())
    }

    /// Return the number of links in the chain.
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// Return `true` if the chain has no links.
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity(seed_byte: u8) -> AgentIdentity {
        AgentIdentity::from_seed(&[seed_byte; 32])
    }

    #[test]
    fn test_agent_id_display() {
        let identity = make_identity(0x01);
        let display = identity.agent_id.to_string();
        assert!(display.starts_with("agnt-"), "display: {display}");
        // 8 bytes = 16 hex chars.
        assert_eq!(display.len(), 5 + 16, "display: {display}");
    }

    #[test]
    fn test_agent_id_from_same_key_is_deterministic() {
        let id_a = make_identity(0xAA).agent_id;
        let id_b = make_identity(0xAA).agent_id;
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn test_agent_id_different_seeds_differ() {
        let id_a = make_identity(0x01).agent_id;
        let id_b = make_identity(0x02).agent_id;
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn test_sign_verify_roundtrip() {
        let identity = make_identity(0x10);
        let message = b"sovereign message";
        let sig = identity.sign(message);
        assert!(identity.verify(message, &sig).is_ok());
    }

    #[test]
    fn test_verify_wrong_message_fails() {
        let identity = make_identity(0x20);
        let sig = identity.sign(b"original");
        assert!(identity.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn test_genesis_creates_valid_chain() {
        let identity = make_identity(0x30);
        let chain = Sigchain::genesis(&identity).expect("genesis failed");

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.agent_id, identity.agent_id);

        chain.verify_chain().expect("chain verification failed");
    }

    #[test]
    fn test_append_add_device() {
        let identity = make_identity(0x40);
        let device_identity = make_identity(0x41);
        let mut chain = Sigchain::genesis(&identity).expect("genesis failed");

        chain
            .append(
                SigchainBody::AddDevice {
                    device_id: "laptop".to_string(),
                    device_key: device_identity.public_key().to_bytes().to_vec(),
                },
                &identity,
            )
            .expect("append failed");

        assert_eq!(chain.len(), 2);
        chain.verify_chain().expect("chain verification failed after add_device");
    }

    #[test]
    fn test_append_revoke_device() {
        let identity = make_identity(0x50);
        let mut chain = Sigchain::genesis(&identity).expect("genesis failed");

        chain
            .append(
                SigchainBody::AddDevice {
                    device_id: "phone".to_string(),
                    device_key: vec![0u8; 32],
                },
                &identity,
            )
            .expect("add device failed");

        chain
            .append(
                SigchainBody::RevokeDevice {
                    device_id: "phone".to_string(),
                },
                &identity,
            )
            .expect("revoke device failed");

        assert_eq!(chain.len(), 3);
        chain.verify_chain().expect("chain verification failed after revoke");
    }

    #[test]
    fn test_multiple_appends_verify_cleanly() {
        let identity = make_identity(0x60);
        let new_identity = make_identity(0x61);
        let mut chain = Sigchain::genesis(&identity).expect("genesis failed");

        chain
            .append(
                SigchainBody::RotateKey {
                    new_key: new_identity.public_key().to_bytes().to_vec(),
                },
                &identity,
            )
            .expect("rotate key failed");

        chain
            .append(
                SigchainBody::AddDevice {
                    device_id: "tablet".to_string(),
                    device_key: make_identity(0x62).public_key().to_bytes().to_vec(),
                },
                &new_identity,
            )
            .expect("add device after rotate failed");

        assert_eq!(chain.len(), 3);
        chain.verify_chain().expect("multi-link chain failed verification");
    }

    #[test]
    fn test_tampered_body_fails_verification() {
        let identity = make_identity(0x70);
        let mut chain = Sigchain::genesis(&identity).expect("genesis failed");

        chain
            .append(
                SigchainBody::AddDevice {
                    device_id: "original-device".to_string(),
                    device_key: vec![0u8; 32],
                },
                &identity,
            )
            .expect("append failed");

        // Tamper: change the body of link 1 without re-signing.
        chain.links[1].body = SigchainBody::RevokeDevice {
            device_id: "different-device".to_string(),
        };

        let result = chain.verify_chain();
        assert!(result.is_err(), "tampered body should fail verification");
    }

    #[test]
    fn test_tampered_prev_hash_fails_verification() {
        let identity = make_identity(0x80);
        let mut chain = Sigchain::genesis(&identity).expect("genesis failed");

        chain
            .append(
                SigchainBody::AddDevice {
                    device_id: "device".to_string(),
                    device_key: vec![0u8; 32],
                },
                &identity,
            )
            .expect("append failed");

        // Corrupt the prev_hash of the second link.
        chain.links[1].prev_hash[0] ^= 0xFF;

        let result = chain.verify_chain();
        assert!(result.is_err(), "tampered prev_hash should fail verification");
    }

    #[test]
    fn test_empty_chain_verification_fails() {
        let identity = make_identity(0x90);
        let chain = Sigchain {
            agent_id: identity.agent_id.clone(),
            links: vec![],
        };
        assert!(chain.verify_chain().is_err());
        assert!(chain.is_empty());
    }

    #[test]
    fn test_is_empty_and_len() {
        let identity = make_identity(0xA0);
        let chain = Sigchain::genesis(&identity).expect("genesis failed");
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }
}
