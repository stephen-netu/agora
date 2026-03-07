//! Agent identity and append-only sigchain.
//!
//! Each agent owns one `AgentIdentity` (Ed25519 signing key derived from a
//! 32-byte seed) and an associated `Sigchain` that records key events and
//! behavioral actions in an authenticated, hash-linked log.

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

    /// Return the lower-hex encoding of the 32-byte identifier.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse an `AgentId` from a 64-character lower-hex string.
    pub fn from_hex(s: &str) -> Result<Self, CryptoError> {
        if s.len() != 64 {
            return Err(CryptoError::InvalidSignature(format!(
                "AgentId hex must be 64 chars, got {}",
                s.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|e| CryptoError::InvalidSignature(format!("invalid hex: {e}")))?;
        }
        Ok(Self(bytes))
    }

    /// Parse an `AgentId` from a 32-byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKey(format!(
                "AgentId requires 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex: String = self.0[..8].iter().map(|b| format!("{b:02x}")).collect();
        write!(f, "agnt-{hex}")
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({self})")
    }
}

// ── Display Names ─────────────────────────────────────────────────────────────

pub mod display;

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
        Self {
            agent_id,
            signing_key,
        }
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

// ── TrustState ────────────────────────────────────────────────────────────────

/// Trust level of an agent, recorded in `TrustTransition` sigchain links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    /// No trust established.
    Untrusted,
    /// Limited trust, pending full verification.
    Provisional,
    /// Full trust granted.
    Trusted,
    /// Trust temporarily suspended (pending review).
    Suspended,
    /// Trust permanently revoked.
    Revoked,
}

// ── SigchainBody ─────────────────────────────────────────────────────────────

/// The typed payload of one sigchain link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SigchainBody {
    /// First link — establishes the agent's identity and initial capabilities.
    Genesis {
        /// The AgentId being established.
        agent_id: AgentId,
        /// Capabilities granted at creation. Format: "namespace:Name:version".
        /// Default empty — backward-compatible with Phase 1 Genesis links.
        #[serde(default)]
        granted_capabilities: Vec<String>,
        /// Optional co-signer's Ed25519 verifying key (32 bytes).
        #[serde(default)]
        cosigner_key: Option<Vec<u8>>,
        /// Optional co-signer's Ed25519 signature.
        ///
        /// Both the primary signer and the co-signer sign the same bytes:
        /// `rmp_serde::to_vec_named((0u64, [0u8;32], body_with_cosigner_sig_none))`.
        /// This field is NOT included in the signed bytes — `signing_view()` strips it.
        #[serde(default)]
        cosigner_signature: Option<Vec<u8>>,
    },
    /// Associate a named device with an Ed25519 verifying key.
    AddDevice {
        /// Unique identifier for the device.
        device_id: String,
        /// `VerifyingKey::to_bytes()` (32 bytes).
        device_key: Vec<u8>,
    },
    /// Remove a previously added device.
    RevokeDevice {
        /// Unique identifier for the device to revoke.
        device_id: String
    },
    /// Rotate to a new Ed25519 verifying key.
    RotateKey {
        /// `VerifyingKey::to_bytes()` (32 bytes).
        new_key: Vec<u8>,
    },

    // ── Phase 2: Behavioral Tracking ─────────────────────────────────────────
    /// Records one behavioral event taken by the agent (tool call, message, etc.).
    Action {
        /// Matrix event type (e.g., `"agora.tool_call"`, `"m.room.message"`).
        event_type: String,
        /// `BLAKE3(event_id.as_bytes())` — hashed to avoid leaking room context.
        event_id_hash: [u8; 32],
        /// `BLAKE3(room_id.as_bytes())`.
        room_id_hash: [u8; 32],
        /// `BLAKE3(rmp_serde::to_vec_named(&event_content))` — commits to content.
        content_hash: [u8; 32],
        /// `BLAKE3(tool_output_bytes)` if applicable. `None` for pure messages.
        effect_hash: Option<[u8; 32]>,
        /// Sequence timestamp from `SequenceTimestamp` (S-02 — no `SystemTime::now()`).
        timestamp: u64,
        /// Call-path ancestor `AgentId`s, outermost caller first. Self excluded.
        /// **Maximum 16 entries** (S-05 killability guarantee).
        correlation_path: Vec<AgentId>,
    },

    /// Periodic Merkle root over a range of `Action` links.
    ///
    /// Enables batch verification without replaying every action link.
    Checkpoint {
        /// Inclusive seqno of the last `Action` covered by this checkpoint.
        covers_through_seqno: u64,
        /// Binary Merkle root over `canonical_hash()` of each covered `Action` link.
        /// Computed by `Sigchain::compute_checkpoint_merkle_root()`.
        merkle_root: [u8; 32],
        /// Number of `Action` links in this range (sanity check).
        action_count: u64,
    },

    /// Records a trust level transition for this agent.
    TrustTransition {
        /// The trust state before this transition.
        from_state: TrustState,
        /// The trust state after this transition.
        to_state: TrustState,
        /// Human-readable reason. **Maximum 256 bytes**.
        reason: String,
        /// Seqno of the link that triggered this transition, if any.
        triggered_by_seqno: Option<u64>,
    },

    /// Proves this agent detected and refused a call-loop.
    ///
    /// Appended instead of `Action` when the incoming `correlation_path`
    /// already contains this agent's `AgentId`. The link is signed and
    /// hash-linked, making loop detection auditable and non-repudiable.
    Refusal {
        /// Matrix event type that was refused.
        refused_event_type: String,
        /// Why the agent refused. **Maximum 256 bytes**.
        reason: String,
        /// Snapshot of the `correlation_path` that triggered the refusal.
        /// **Maximum 16 entries** (S-05).
        correlation_path_snapshot: Vec<AgentId>,
        /// S-02 sequence timestamp (chain length before append).
        timestamp: u64,
    },
    /// Proves this agent refused to engage with a dispute.
    ///
    /// Appended when a party refuses to participate in a dispute resolution.
    /// The link is signed and hash-linked, creating a permanent record
    /// of non-participation that verifiers may weigh.
    DisputeRefusal {
        /// The dispute_id they are refusing to engage with.
        dispute_id: [u8; 32],
        /// Reason for refusal. **Maximum 256 bytes**.
        reason: String,
        /// S-02 sequence timestamp (chain length before append).
        timestamp: u64,
    },
}

impl SigchainBody {
    /// Return the variant name as a string (used for DB `link_type` column).
    pub fn variant_name(&self) -> &'static str {
        match self {
            SigchainBody::Genesis { .. } => "Genesis",
            SigchainBody::AddDevice { .. } => "AddDevice",
            SigchainBody::RevokeDevice { .. } => "RevokeDevice",
            SigchainBody::RotateKey { .. } => "RotateKey",
            SigchainBody::Action { .. } => "Action",
            SigchainBody::Checkpoint { .. } => "Checkpoint",
            SigchainBody::TrustTransition { .. } => "TrustTransition",
            SigchainBody::Refusal { .. } => "Refusal",
            SigchainBody::DisputeRefusal { .. } => "DisputeRefusal",
        }
    }

    /// Return a signing-canonical view of this body.
    ///
    /// For `Genesis`, strips `cosigner_signature` so both the primary signer and
    /// co-signer sign identical bytes. All other variants return a clone of `self`.
    fn signing_view(&self) -> Self {
        match self {
            SigchainBody::Genesis {
                agent_id,
                granted_capabilities,
                cosigner_key,
                ..
            } => SigchainBody::Genesis {
                agent_id: agent_id.clone(),
                granted_capabilities: granted_capabilities.clone(),
                cosigner_key: cosigner_key.clone(),
                cosigner_signature: None,
            },
            SigchainBody::DisputeRefusal {
                dispute_id,
                reason,
                timestamp,
            } => SigchainBody::DisputeRefusal {
                dispute_id: *dispute_id,
                reason: reason.clone(),
                timestamp: *timestamp,
            },
            other => other.clone(),
        }
    }
}

// ── AnchorPayload ───────────────────────────────────────────────────────────────

/// On-chain anchor payload for checkpoint verification.
///
/// This structure is submitted to an external chain (e.g., Solana) to make
/// the checkpoint's merkle_root unforgeable to third parties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorPayload {
    /// AgentId committing to this checkpoint.
    pub agent_id: [u8; 32],
    /// The Merkle root from the Checkpoint link.
    pub merkle_root: [u8; 32],
    /// Seqno of the Checkpoint link.
    pub checkpoint_seqno: u64,
    /// Number of Action links covered by this checkpoint.
    pub action_count: u64,
    /// Block height when anchor was submitted.
    pub submitted_at: u64,
    /// Transaction hash on the external chain.
    pub tx_hash: [u8; 32],
}

impl SigchainBody {
    /// Convert a Checkpoint variant to an on-chain anchor payload.
    ///
    /// Returns an error if `self` is not a Checkpoint variant.
    pub fn as_on_chain_anchor(
        &self,
        agent_id: &AgentId,
        tx_hash: [u8; 32],
        submitted_at: u64,
    ) -> Result<AnchorPayload, CryptoError> {
        match self {
            SigchainBody::Checkpoint {
                covers_through_seqno,
                merkle_root,
                action_count,
            } => Ok(AnchorPayload {
                agent_id: agent_id.0,
                merkle_root: *merkle_root,
                checkpoint_seqno: *covers_through_seqno,
                action_count: *action_count,
                submitted_at,
                tx_hash,
            }),
            _ => Err(CryptoError::Encoding(
                "only Checkpoint bodies can be converted to AnchorPayload".into(),
            )),
        }
    }
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
    /// Serializes `(seqno, prev_hash, body.signing_view())` with `rmp_serde`.
    /// The signing view strips co-signing material from Genesis bodies so that
    /// both the primary signer and co-signer sign identical bytes.
    fn signed_bytes(
        seqno: u64,
        prev_hash: &[u8; 32],
        body: &SigchainBody,
    ) -> Result<Vec<u8>, CryptoError> {
        let view = body.signing_view();
        rmp_serde::to_vec_named(&(seqno, prev_hash, &view))
            .map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Compute the BLAKE3 hash of the canonical serialization of this link.
    ///
    /// The hash is over the full link (including the signature) so that a
    /// later link's `prev_hash` commits to the entire prior record.
    pub fn canonical_hash(&self) -> Result<[u8; 32], CryptoError> {
        let bytes =
            rmp_serde::to_vec_named(self).map_err(|e| CryptoError::Encoding(e.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

// ── SignedEntry ───────────────────────────────────────────────────────────────

/// A signed sigchain entry with its canonical hash.
///
/// Used for dispute evidence export and verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEntry {
    /// The authenticated sigchain link.
    pub link: SigchainLink,
    /// The BLAKE3 hash of the link's canonical encoding.
    pub canonical_hash: [u8; 32],
}

// ── Sigchain ─────────────────────────────────────────────────────────────────

/// Append-only chain of signed identity and behavioral events for one agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sigchain {
    /// The agent whose events are recorded here.
    pub agent_id: AgentId,
    /// The ordered list of authenticated links.
    pub links: Vec<SigchainLink>,
}

impl Sigchain {
    /// Create a new sigchain with a single `Genesis` link signed by `identity`.
    ///
    /// Pass `granted_capabilities` to record what this agent is authorized to do.
    /// Pass `cosigner` if a second party must co-sign the genesis.
    pub fn genesis(
        identity: &AgentIdentity,
        granted_capabilities: Vec<String>,
        cosigner: Option<&AgentIdentity>,
    ) -> Result<Self, CryptoError> {
        let cosigner_key = cosigner.map(|c| c.public_key().to_bytes().to_vec());

        let body = SigchainBody::Genesis {
            agent_id: identity.agent_id.clone(),
            granted_capabilities,
            cosigner_key,
            cosigner_signature: None, // set after signing
        };

        let prev_hash = [0u8; 32];
        let seqno: u64 = 0;

        let to_sign = SigchainLink::signed_bytes(seqno, &prev_hash, &body)?;
        let signature = identity.sign(&to_sign);
        let signer: [u8; 32] = identity.public_key().to_bytes();

        // Compute co-signer signature over identical bytes.
        let cosigner_signature = cosigner.map(|c| c.sign(&to_sign).to_bytes().to_vec());

        let body_with_cosig = if let SigchainBody::Genesis {
            agent_id,
            granted_capabilities,
            cosigner_key,
            ..
        } = body
        {
            SigchainBody::Genesis {
                agent_id,
                granted_capabilities,
                cosigner_key,
                cosigner_signature,
            }
        } else {
            unreachable!("body was constructed as Genesis above")
        };

        let link = SigchainLink {
            seqno,
            prev_hash,
            body: body_with_cosig,
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
    /// 5. Co-signer signature verified for Genesis links that carry one.
    /// 6. `Action.correlation_path.len() <= 16` (S-05).
    /// 7. `TrustTransition.reason.len() <= 256`.
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

            // Body-level constraints.
            match &link.body {
                SigchainBody::Action {
                    correlation_path, ..
                } => {
                    if correlation_path.len() > 16 {
                        return Err(CryptoError::SigchainVerification(format!(
                            "link {idx}: correlation_path exceeds 16-hop limit (S-05)"
                        )));
                    }
                }
                SigchainBody::TrustTransition { reason, .. } => {
                    if reason.len() > 256 {
                        return Err(CryptoError::SigchainVerification(format!(
                            "link {idx}: TrustTransition reason exceeds 256 bytes"
                        )));
                    }
                }
                SigchainBody::Refusal {
                    reason,
                    correlation_path_snapshot,
                    ..
                } => {
                    if reason.len() > 256 {
                        return Err(CryptoError::SigchainVerification(format!(
                            "link {idx}: Refusal reason exceeds 256 bytes"
                        )));
                    }
                    if correlation_path_snapshot.len() > 16 {
                        return Err(CryptoError::SigchainVerification(format!(
                            "link {idx}: Refusal correlation_path_snapshot exceeds 16-hop limit (S-05)"
                        )));
                    }
                }
                SigchainBody::DisputeRefusal { reason, .. } => {
                    if reason.len() > 256 {
                        return Err(CryptoError::SigchainVerification(format!(
                            "link {idx}: DisputeRefusal reason exceeds 256 bytes"
                        )));
                    }
                }
                _ => {}
            }

            // Signature verification — use signing_view so Genesis co-sig fields are stripped.
            let vk = VerifyingKey::from_bytes(&link.signer).map_err(|e| {
                CryptoError::SigchainVerification(format!("link {idx}: bad signer key: {e}"))
            })?;

            let to_sign = SigchainLink::signed_bytes(link.seqno, &link.prev_hash, &link.body)?;

            let sig_bytes: [u8; 64] = link.signature.as_slice().try_into().map_err(|_| {
                CryptoError::SigchainVerification(format!("link {idx}: signature must be 64 bytes"))
            })?;
            let signature = Signature::from_bytes(&sig_bytes);

            vk.verify(&to_sign, &signature).map_err(|e| {
                CryptoError::SigchainVerification(format!("link {idx}: invalid signature: {e}"))
            })?;

            // Co-signer verification for Genesis.
            if let SigchainBody::Genesis {
                cosigner_key,
                cosigner_signature,
                ..
            } = &link.body
            {
                if let (Some(ck), Some(cs)) = (cosigner_key, cosigner_signature) {
                    let cosigner_vk_bytes: [u8; 32] = ck.as_slice().try_into().map_err(|_| {
                        CryptoError::SigchainVerification(
                            "genesis cosigner_key must be 32 bytes".into(),
                        )
                    })?;
                    let cosigner_vk =
                        VerifyingKey::from_bytes(&cosigner_vk_bytes).map_err(|e| {
                            CryptoError::SigchainVerification(format!(
                                "genesis: bad cosigner_key: {e}"
                            ))
                        })?;

                    let cosig_bytes: [u8; 64] = cs.as_slice().try_into().map_err(|_| {
                        CryptoError::SigchainVerification(
                            "genesis cosigner_signature must be 64 bytes".into(),
                        )
                    })?;
                    let cosig = Signature::from_bytes(&cosig_bytes);

                    // Co-signer signs the same bytes as the primary signer.
                    cosigner_vk.verify(&to_sign, &cosig).map_err(|e| {
                        CryptoError::SigchainVerification(format!(
                            "genesis: invalid cosigner signature: {e}"
                        ))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Compute the binary Merkle root over a slice of leaf hashes.
    ///
    /// Used to build and verify `Checkpoint` links. `leaf_hashes` must be the
    /// `canonical_hash()` of each `Action` link in the checkpoint range, in seqno order.
    ///
    /// # Algorithm
    ///
    /// - Empty: `BLAKE3(b"agora:merkle:empty")`
    /// - Leaves: `BLAKE3(b"agora:merkle:leaf" || canonical_hash)`
    /// - Nodes: `BLAKE3(b"agora:merkle:node" || left || right)` (last entry duplicated if odd)
    pub fn compute_checkpoint_merkle_root(leaf_hashes: &[[u8; 32]]) -> [u8; 32] {
        if leaf_hashes.is_empty() {
            return *blake3::hash(b"agora:merkle:empty").as_bytes();
        }

        let mut level: Vec<[u8; 32]> = leaf_hashes
            .iter()
            .map(|h| {
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"agora:merkle:leaf");
                hasher.update(h);
                *hasher.finalize().as_bytes()
            })
            .collect();

        // S-05: bounded loop — at most ceil(log2(leaf_count)) iterations.
        while level.len() > 1 {
            let mut next: Vec<[u8; 32]> = Vec::new();
            let mut i = 0;
            while i < level.len() {
                let left = level[i];
                let right = if i + 1 < level.len() {
                    level[i + 1]
                } else {
                    level[i]
                };
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"agora:merkle:node");
                hasher.update(&left);
                hasher.update(&right);
                next.push(*hasher.finalize().as_bytes());
                i += 2;
            }
            level = next;
        }

        level[0]
    }

    /// Return `true` if `agent_id` already appears in `path`.
    ///
    /// Used to detect call-loops before appending an `Action` link. An agent
    /// MUST call this before accepting an incoming tool-call; if `true`, it
    /// should append a `Refusal` link and return an error to the caller.
    ///
    /// O(n) where n = path length. Bounded by the 16-hop limit (S-05).
    pub fn has_loop(agent_id: &AgentId, path: &[AgentId]) -> bool {
        path.iter().any(|id| id == agent_id)
    }

    /// Return the number of links in the chain.
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// Return `true` if the chain has no links.
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Export hash-linked entries between two Checkpoint seqnos (inclusive).
    ///
    /// The exported entries form a continuous hash-linked chain suitable for
    /// dispute evidence. Each entry includes its canonical hash for verification.
    ///
    /// If Checkpoints don't exist at the exact seqno positions specified, this
    /// method finds the nearest Checkpoint at or before `from_seqno` and the
    /// nearest Checkpoint at or after `to_seqno`, then exports from those
    /// boundary Checkpoints. This ensures the exported chain is verifiable.
    ///
    /// # Parameters
    /// - `from_seqno`: Desired starting seqno; will use nearest Checkpoint at or before this value
    /// - `to_seqno`: Desired ending seqno; will use nearest Checkpoint at or after this value
    ///
    /// # Returns
    /// Vector of `SignedEntry` from both Checkpoints and all intervening links.
    pub fn export_range(
        &self,
        from_seqno: u64,
        to_seqno: u64,
    ) -> Result<Vec<SignedEntry>, CryptoError> {
        if from_seqno > to_seqno {
            return Err(CryptoError::SigchainVerification(
                "from_seqno must be <= to_seqno".into(),
            ));
        }

        let from_boundary = self.find_checkpoint_at_or_before(from_seqno);
        let to_boundary = self.find_checkpoint_at_or_after(to_seqno);

        let (Some(start_seqno), Some(end_seqno)) = (from_boundary, to_boundary) else {
            return Err(CryptoError::SigchainVerification(
                "no checkpoints found in specified range".into(),
            ));
        };

        let entries: Vec<SignedEntry> = self
            .links
            .iter()
            .filter(|link| link.seqno >= start_seqno && link.seqno <= end_seqno)
            .map(|link| {
                let hash = link.canonical_hash()?;
                Ok(SignedEntry {
                    link: link.clone(),
                    canonical_hash: hash,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if entries.is_empty() {
            return Err(CryptoError::SigchainVerification(
                "no links found in specified range".into(),
            ));
        }

        Ok(entries)
    }

    /// Find the nearest Checkpoint at or before the given seqno.
    ///
    /// Returns the seqno of the Checkpoint whose `covers_through_seqno` is the
    /// largest value that is <= the given seqno.
    fn find_checkpoint_at_or_before(&self, seqno: u64) -> Option<u64> {
        let mut best: Option<(u64, u64)> = None;

        for link in &self.links {
            if let SigchainBody::Checkpoint { covers_through_seqno, .. } = &link.body {
                if *covers_through_seqno <= seqno {
                    match best {
                        None => best = Some((link.seqno, *covers_through_seqno)),
                        Some((_, best_covers)) if *covers_through_seqno > best_covers => {
                            best = Some((link.seqno, *covers_through_seqno));
                        }
                        _ => {}
                    }
                }
            }
        }

        best.map(|(link_seqno, _)| link_seqno)
    }

    /// Find the nearest Checkpoint at or after the given seqno.
    ///
    /// Returns the seqno of the Checkpoint whose `covers_through_seqno` is the
    /// smallest value that is >= the given seqno.
    fn find_checkpoint_at_or_after(&self, seqno: u64) -> Option<u64> {
        let mut best: Option<(u64, u64)> = None;

        for link in &self.links {
            if let SigchainBody::Checkpoint { covers_through_seqno, .. } = &link.body {
                if *covers_through_seqno >= seqno {
                    match best {
                        None => best = Some((link.seqno, *covers_through_seqno)),
                        Some((_, best_covers)) if *covers_through_seqno < best_covers => {
                            best = Some((link.seqno, *covers_through_seqno));
                        }
                        _ => {}
                    }
                }
            }
        }

        best.map(|(link_seqno, _)| link_seqno)
    }

    /// Verify a received segment against an expected Checkpoint merkle root.
    ///
    /// This allows any node to verify dispute evidence without trusting the sender.
    /// Checks:
    /// 1. Hash-link continuity: entry[i].prev_hash == entry[i-1].canonical_hash
    /// 2. Signature validity: each entry's signature verifies against its signer
    /// 3. Merkle root: the Checkpoint's merkle_root matches the computed root
    ///
    /// # Parameters
    /// - `entries`: The signed entries to verify
    /// - `expected_merkle_root`: The merkle_root from the Checkpoint link
    ///
    /// # Returns
    /// `Ok(())` if verification succeeds, error otherwise.
    pub fn verify_segment(
        entries: &[SignedEntry],
        expected_merkle_root: [u8; 32],
    ) -> Result<(), CryptoError> {
        if entries.is_empty() {
            return Err(CryptoError::SigchainVerification(
                "segment is empty".into(),
            ));
        }

        let mut action_hashes: Vec<[u8; 32]> = Vec::new();
        let mut prev_hash: Option<[u8; 32]> = None;

        for (idx, entry) in entries.iter().enumerate() {
            if let Some(expected_prev) = prev_hash {
                if entry.canonical_hash != expected_prev {
                    return Err(CryptoError::SigchainVerification(format!(
                        "entry {}: hash-link continuity broken",
                        idx
                    )));
                }
            }

            let vk = VerifyingKey::from_bytes(&entry.link.signer).map_err(|e| {
                CryptoError::SigchainVerification(format!("entry {}: bad signer key: {e}", idx))
            })?;

            let to_sign = SigchainLink::signed_bytes(
                entry.link.seqno,
                &entry.link.prev_hash,
                &entry.link.body,
            )?;

            let sig_bytes: [u8; 64] = entry.link.signature.as_slice().try_into().map_err(
                |_| CryptoError::SigchainVerification(format!("entry {}: signature must be 64 bytes", idx)),
            )?;
            let signature = Signature::from_bytes(&sig_bytes);

            vk.verify(&to_sign, &signature).map_err(|e| {
                CryptoError::SigchainVerification(format!(
                    "entry {}: invalid signature: {e}",
                    idx
                ))
            })?;

            if let SigchainBody::Action { event_id_hash, .. } = &entry.link.body {
                action_hashes.push(*event_id_hash);
            }

            prev_hash = Some(entry.canonical_hash);
        }

        let computed_merkle_root = Sigchain::compute_checkpoint_merkle_root(&action_hashes);
        if computed_merkle_root != expected_merkle_root {
            return Err(CryptoError::SigchainVerification(format!(
                "merkle root mismatch: expected {:02x?}, computed {:02x?}",
                expected_merkle_root, computed_merkle_root
            )));
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity(seed_byte: u8) -> AgentIdentity {
        AgentIdentity::from_seed(&[seed_byte; 32])
    }

    // ── AgentId ───────────────────────────────────────────────────────────────

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
    fn test_agent_id_hex_roundtrip() {
        let id = make_identity(0x42).agent_id;
        let hex = id.to_hex();
        assert_eq!(hex.len(), 64);
        let parsed = AgentId::from_hex(&hex).expect("parse failed");
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_agent_id_from_hex_wrong_length() {
        assert!(AgentId::from_hex("abc").is_err());
    }

    // ── AgentIdentity ─────────────────────────────────────────────────────────

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

    // ── Genesis ───────────────────────────────────────────────────────────────

    #[test]
    fn test_genesis_creates_valid_chain() {
        let identity = make_identity(0x30);
        let chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        assert_eq!(chain.len(), 1);
        assert_eq!(chain.agent_id, identity.agent_id);
        chain.verify_chain().expect("chain verification failed");
    }

    #[test]
    fn test_genesis_with_capabilities() {
        let identity = make_identity(0x31);
        let caps = vec![
            "io:FileRead:1.0".to_string(),
            "llm:Generate:1.0".to_string(),
        ];
        let chain = Sigchain::genesis(&identity, caps.clone(), None).expect("genesis failed");

        chain.verify_chain().expect("chain verification failed");
        match &chain.links[0].body {
            SigchainBody::Genesis {
                granted_capabilities,
                ..
            } => {
                assert_eq!(*granted_capabilities, caps);
            }
            _ => panic!("expected Genesis"),
        }
    }

    #[test]
    fn test_genesis_with_cosigner() {
        let agent = make_identity(0x32);
        let cosigner = make_identity(0x33);
        let chain = Sigchain::genesis(&agent, vec![], Some(&cosigner)).expect("genesis failed");

        chain
            .verify_chain()
            .expect("co-signed genesis failed verification");

        match &chain.links[0].body {
            SigchainBody::Genesis {
                cosigner_key,
                cosigner_signature,
                ..
            } => {
                assert!(cosigner_key.is_some());
                assert!(cosigner_signature.is_some());
            }
            _ => panic!("expected Genesis"),
        }
    }

    #[test]
    fn test_genesis_cosigner_tampered_fails() {
        let agent = make_identity(0x34);
        let cosigner = make_identity(0x35);
        let mut chain = Sigchain::genesis(&agent, vec![], Some(&cosigner)).expect("genesis failed");

        // Tamper with the co-signer signature.
        if let SigchainBody::Genesis {
            cosigner_signature, ..
        } = &mut chain.links[0].body
        {
            if let Some(sig) = cosigner_signature {
                sig[0] ^= 0xFF;
            }
        }

        assert!(chain.verify_chain().is_err(), "tampered cosig should fail");
    }

    // ── AddDevice / RevokeDevice ───────────────────────────────────────────────

    #[test]
    fn test_append_add_device() {
        let identity = make_identity(0x40);
        let device_identity = make_identity(0x41);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

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
        chain
            .verify_chain()
            .expect("chain verification failed after add_device");
    }

    #[test]
    fn test_append_revoke_device() {
        let identity = make_identity(0x50);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

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
        chain
            .verify_chain()
            .expect("chain verification failed after revoke");
    }

    // ── Action ────────────────────────────────────────────────────────────────

    #[test]
    fn test_action_link() {
        let identity = make_identity(0x60);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        let event_id_hash = *blake3::hash(b"$event1:localhost").as_bytes();
        let room_id_hash = *blake3::hash(b"!room1:localhost").as_bytes();
        let content_hash = *blake3::hash(b"hello").as_bytes();

        chain
            .append(
                SigchainBody::Action {
                    event_type: "m.room.message".to_string(),
                    event_id_hash,
                    room_id_hash,
                    content_hash,
                    effect_hash: None,
                    timestamp: 1_000,
                    correlation_path: vec![],
                },
                &identity,
            )
            .expect("action append failed");

        chain
            .verify_chain()
            .expect("chain verification failed after action");
    }

    #[test]
    fn test_action_with_correlation_path() {
        let identity = make_identity(0x61);
        let caller_id = make_identity(0x62).agent_id;
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        chain
            .append(
                SigchainBody::Action {
                    event_type: "agora.tool_call".to_string(),
                    event_id_hash: [0u8; 32],
                    room_id_hash: [0u8; 32],
                    content_hash: [0u8; 32],
                    effect_hash: Some([1u8; 32]),
                    timestamp: 2_000,
                    correlation_path: vec![caller_id],
                },
                &identity,
            )
            .expect("action with path failed");

        chain.verify_chain().expect("verify failed");
    }

    #[test]
    fn test_action_correlation_path_too_long() {
        let identity = make_identity(0x63);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        // 17 entries — exceeds the 16-hop limit.
        let path: Vec<AgentId> = (0u8..17).map(|b| make_identity(b).agent_id).collect();

        chain
            .append(
                SigchainBody::Action {
                    event_type: "agora.tool_call".to_string(),
                    event_id_hash: [0u8; 32],
                    room_id_hash: [0u8; 32],
                    content_hash: [0u8; 32],
                    effect_hash: None,
                    timestamp: 3_000,
                    correlation_path: path,
                },
                &identity,
            )
            .expect("append succeeded (verification happens separately)");

        // verify_chain should reject this.
        assert!(
            chain.verify_chain().is_err(),
            "chain with oversized correlation_path should fail verification"
        );
    }

    // ── Checkpoint ────────────────────────────────────────────────────────────

    #[test]
    fn test_checkpoint_link() {
        let identity = make_identity(0x70);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        // Append two Action links.
        for ts in [100u64, 200u64] {
            chain
                .append(
                    SigchainBody::Action {
                        event_type: "m.room.message".to_string(),
                        event_id_hash: [0u8; 32],
                        room_id_hash: [0u8; 32],
                        content_hash: [0u8; 32],
                        effect_hash: None,
                        timestamp: ts,
                        correlation_path: vec![],
                    },
                    &identity,
                )
                .expect("action append failed");
        }

        // Collect hashes of the two Action links (seqno 1 and 2).
        let leaf_hashes: Vec<[u8; 32]> = chain.links[1..=2]
            .iter()
            .map(|l| l.canonical_hash().expect("hash failed"))
            .collect();

        let merkle_root = Sigchain::compute_checkpoint_merkle_root(&leaf_hashes);

        chain
            .append(
                SigchainBody::Checkpoint {
                    covers_through_seqno: 2,
                    merkle_root,
                    action_count: 2,
                },
                &identity,
            )
            .expect("checkpoint append failed");

        assert_eq!(chain.len(), 4); // Genesis + 2 Action + 1 Checkpoint
        chain
            .verify_chain()
            .expect("verify failed after checkpoint");
    }

    #[test]
    fn test_merkle_root_empty() {
        let root = Sigchain::compute_checkpoint_merkle_root(&[]);
        let expected = *blake3::hash(b"agora:merkle:empty").as_bytes();
        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_single_leaf() {
        let leaf = [0x42u8; 32];
        let root = Sigchain::compute_checkpoint_merkle_root(&[leaf]);
        // Single leaf: root = BLAKE3("agora:merkle:leaf" || leaf).
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"agora:merkle:leaf");
        hasher.update(&leaf);
        let expected = *hasher.finalize().as_bytes();
        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let leaves = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let r1 = Sigchain::compute_checkpoint_merkle_root(&leaves);
        let r2 = Sigchain::compute_checkpoint_merkle_root(&leaves);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_merkle_root_order_sensitive() {
        let leaves_a = [[1u8; 32], [2u8; 32]];
        let leaves_b = [[2u8; 32], [1u8; 32]];
        assert_ne!(
            Sigchain::compute_checkpoint_merkle_root(&leaves_a),
            Sigchain::compute_checkpoint_merkle_root(&leaves_b),
        );
    }

    // ── TrustTransition ───────────────────────────────────────────────────────

    #[test]
    fn test_trust_transition_link() {
        let identity = make_identity(0x80);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        chain
            .append(
                SigchainBody::TrustTransition {
                    from_state: TrustState::Provisional,
                    to_state: TrustState::Trusted,
                    reason: "identity verified".to_string(),
                    triggered_by_seqno: None,
                },
                &identity,
            )
            .expect("trust transition append failed");

        chain
            .verify_chain()
            .expect("verify failed after trust transition");
    }

    #[test]
    fn test_trust_transition_reason_too_long() {
        let identity = make_identity(0x81);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        // 257 bytes — exceeds the 256-byte limit.
        let long_reason = "x".repeat(257);

        chain
            .append(
                SigchainBody::TrustTransition {
                    from_state: TrustState::Untrusted,
                    to_state: TrustState::Revoked,
                    reason: long_reason,
                    triggered_by_seqno: None,
                },
                &identity,
            )
            .expect("append succeeded (verified separately)");

        assert!(
            chain.verify_chain().is_err(),
            "chain with oversized reason should fail verification"
        );
    }

    // ── Tamper detection ─────────────────────────────────────────────────────

    #[test]
    fn test_tampered_body_fails_verification() {
        let identity = make_identity(0x90);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

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

        assert!(
            chain.verify_chain().is_err(),
            "tampered body should fail verification"
        );
    }

    #[test]
    fn test_tampered_prev_hash_fails_verification() {
        let identity = make_identity(0xA0);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

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

        assert!(
            chain.verify_chain().is_err(),
            "tampered prev_hash should fail verification"
        );
    }

    #[test]
    fn test_multiple_appends_verify_cleanly() {
        let identity = make_identity(0xB0);
        let new_identity = make_identity(0xB1);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

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
                    device_key: make_identity(0xB2).public_key().to_bytes().to_vec(),
                },
                &new_identity,
            )
            .expect("add device after rotate failed");

        assert_eq!(chain.len(), 3);
        chain
            .verify_chain()
            .expect("multi-link chain failed verification");
    }

    #[test]
    fn test_empty_chain_verification_fails() {
        let identity = make_identity(0xC0);
        let chain = Sigchain {
            agent_id: identity.agent_id.clone(),
            links: vec![],
        };
        assert!(chain.verify_chain().is_err());
        assert!(chain.is_empty());
    }

    #[test]
    fn test_is_empty_and_len() {
        let identity = make_identity(0xD0);
        let chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }

    // ── Refusal / loop-detection tests ───────────────────────────────────────

    #[test]
    fn test_has_loop_empty_path() {
        let identity = make_identity(0xE0);
        assert!(!Sigchain::has_loop(&identity.agent_id, &[]));
    }

    #[test]
    fn test_has_loop_not_present() {
        let identity = make_identity(0xE1);
        let other = make_identity(0xE2);
        assert!(!Sigchain::has_loop(
            &identity.agent_id,
            &[other.agent_id.clone()]
        ));
    }

    #[test]
    fn test_has_loop_self_in_path() {
        let identity = make_identity(0xE3);
        let other = make_identity(0xE4);
        let path = vec![other.agent_id.clone(), identity.agent_id.clone()];
        assert!(Sigchain::has_loop(&identity.agent_id, &path));
    }

    #[test]
    fn test_append_refusal_link() {
        let identity = make_identity(0xE5);
        let caller = make_identity(0xE6);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        let path = vec![caller.agent_id.clone(), identity.agent_id.clone()];
        assert!(Sigchain::has_loop(&identity.agent_id, &path));

        chain
            .append(
                SigchainBody::Refusal {
                    refused_event_type: "agora.tool_call".to_owned(),
                    reason: "loop detected: agent_id appears in correlation_path".to_owned(),
                    correlation_path_snapshot: path.clone(),
                    timestamp: chain.len() as u64,
                },
                &identity,
            )
            .expect("append refusal failed");

        assert_eq!(chain.len(), 2);
        chain
            .verify_chain()
            .expect("chain with refusal failed verification");

        match &chain.links[1].body {
            SigchainBody::Refusal {
                refused_event_type,
                correlation_path_snapshot,
                ..
            } => {
                assert_eq!(refused_event_type, "agora.tool_call");
                assert_eq!(correlation_path_snapshot.len(), 2);
            }
            _ => panic!("expected Refusal body"),
        }
    }

    #[test]
    fn test_refusal_reason_too_long_rejected() {
        let identity = make_identity(0xE7);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        // Reason > 256 bytes should fail verify_chain.
        chain
            .append(
                SigchainBody::Refusal {
                    refused_event_type: "agora.tool_call".to_owned(),
                    reason: "x".repeat(257),
                    correlation_path_snapshot: vec![],
                    timestamp: 1,
                },
                &identity,
            )
            .expect("append did not fail (verify_chain should fail)");

        assert!(chain.verify_chain().is_err());
    }

    #[test]
    fn test_refusal_path_too_long_rejected() {
        let identity = make_identity(0xE8);
        let mut chain = Sigchain::genesis(&identity, vec![], None).expect("genesis failed");

        let oversized_path: Vec<AgentId> = (0u8..17)
            .map(|i| make_identity(0xF0 + i).agent_id)
            .collect();

        chain
            .append(
                SigchainBody::Refusal {
                    refused_event_type: "agora.tool_call".to_owned(),
                    reason: "loop".to_owned(),
                    correlation_path_snapshot: oversized_path,
                    timestamp: 1,
                },
                &identity,
            )
            .expect("append did not fail (verify_chain should fail)");

        assert!(chain.verify_chain().is_err());
    }

    #[test]
    fn test_refusal_variant_name() {
        let body = SigchainBody::Refusal {
            refused_event_type: "agora.tool_call".to_owned(),
            reason: "loop".to_owned(),
            correlation_path_snapshot: vec![],
            timestamp: 0,
        };
        assert_eq!(body.variant_name(), "Refusal");
    }
}
