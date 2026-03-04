# Agora Phase 2: Sigchain Behavioral Ledger — Specification

## Overview

Phase 1 established `AgentId` (BLAKE3 of Ed25519 verifying key) and `Sigchain` as an
append-only, hash-linked, signature-verified identity ledger. The `SigchainBody` enum
covers four identity events: `Genesis`, `AddDevice`, `RevokeDevice`, `RotateKey`.

Phase 2 extends the sigchain to record **behavior** — every action an agent takes,
with cryptographic proof of authorship, content integrity, and call-path lineage.

This is an Agora-internal protocol. SOVEREIGN integration is covered in task-031.

---

## Breaking Change Notice

Adding fields to `Genesis` changes the signed bytes for that variant. Existing
development chains will fail `verify_chain()` after this upgrade. This is
acceptable for a pre-launch system. All `[u8; 32]` byte arrays serialize as
integer arrays in JSON/MessagePack — no base64 encoding by default.

---

## New Enum: `TrustState`

```rust
pub enum TrustState {
    Untrusted,
    Provisional,
    Trusted,
    Suspended,
    Revoked,
}
```

Serialized as a string variant name (snake_case via `#[serde(rename_all = "snake_case")]`).

---

## Updated `SigchainBody` Variants

### `Genesis` (updated)

```rust
Genesis {
    agent_id: AgentId,
    /// Capability addresses granted at creation. Format: "namespace:Name:version".
    /// Default empty for backward-compat deserialization of old chains.
    #[serde(default)]
    granted_capabilities: Vec<String>,
    /// Co-signer's Ed25519 verifying key (32 bytes), if any.
    #[serde(default)]
    cosigner_key: Option<Vec<u8>>,
    /// Co-signer's Ed25519 signature over signed_bytes(0, [0;32], genesis_with_cosigner_sig_none).
    /// Present iff cosigner_key is Some.
    #[serde(default)]
    cosigner_signature: Option<Vec<u8>>,
}
```

**Co-signing protocol**:
1. Build `GenesisDraft` = Genesis body with `cosigner_signature: None`.
2. Agent signs `rmp_serde::to_vec_named((0u64, [0u8;32], &draft))` → agent signature (goes in `SigchainLink.signature`).
3. Co-signer signs the same bytes → cosigner signature (goes in `Genesis.cosigner_signature`).
4. Both parties sign identical bytes. The `cosigner_signature` field is NOT part of what either party signs.

**Verification**: `verify_chain()` strips `cosigner_signature` before recomputing the signed bytes for both the outer signature and the co-signer signature check.

### `Action` (new)

Records one behavioral event taken by the agent.

```rust
Action {
    /// Matrix event type (e.g., "agora.tool_call", "m.room.message").
    event_type: String,
    /// BLAKE3(event_id.as_bytes()). Hashed to avoid leaking room/user context.
    event_id_hash: [u8; 32],
    /// BLAKE3(room_id.as_bytes()). Hashed for the same reason.
    room_id_hash: [u8; 32],
    /// BLAKE3(rmp_serde::to_vec_named(&event_content)). Commits to full content.
    content_hash: [u8; 32],
    /// BLAKE3 of the tool output, if applicable. None for pure messages.
    effect_hash: Option<[u8; 32]>,
    /// Sequence timestamp from SequenceTimestamp (S-02 — no SystemTime::now()).
    timestamp: u64,
    /// Call-path ancestors. Self excluded. Max 16 entries (S-05 killability).
    /// Populated by the caller's correlation_path + caller's AgentId.
    correlation_path: Vec<AgentId>,
}
```

**Constraints enforced by `verify_chain()`**:
- `correlation_path.len() <= 16`

**Loop detection** (enforced by application layer, not crypto layer):
When an agent receives a request, if `self.agent_id` appears in the incoming
`correlation_path`, the agent MUST decline and append a `Refusal` link instead of an
`Action` link. The `Refusal` variant is added in task-030.

### `Checkpoint` (new)

Periodic Merkle summary over a run of `Action` links. Enables verification of a
batch without replaying every link.

```rust
Checkpoint {
    /// Inclusive seqno of the last Action link covered by this checkpoint.
    /// Exclusive lower bound is the seqno of the previous Checkpoint (or 0 for genesis).
    covers_through_seqno: u64,
    /// Merkle root. See algorithm below.
    merkle_root: [u8; 32],
    /// Number of Action links in this range (for sanity checks).
    action_count: u64,
}
```

### `TrustTransition` (new)

Records a trust level change. Triggered by policy, admin action, or FLOW state
machine transitions.

```rust
TrustTransition {
    from_state: TrustState,
    to_state: TrustState,
    /// Human-readable reason. Max 256 bytes (enforced by verify_chain).
    reason: String,
    /// Seqno of the Action or prior link that triggered this transition, if any.
    triggered_by_seqno: Option<u64>,
}
```

---

## Checkpoint Merkle Root Algorithm

All byte-level details for `Sigchain::compute_checkpoint_merkle_root()`:

```
INPUTS: leaf_hashes: &[[u8; 32]]   (canonical_hash() of each Action link in range, seqno order)

CASE: empty range
  return BLAKE3(b"agora:merkle:empty")

CASE: non-empty
  1. Compute leaf nodes:
     for each h in leaf_hashes:
       leaf[i] = BLAKE3(b"agora:merkle:leaf" || h)   // 17 prefix bytes + 32 hash bytes

  2. Reduce to root (binary tree, bottom-up):
     level = leaf[...]
     while level.len() > 1:
       next = []
       for i in [0, 2, 4, ...]:
         left  = level[i]
         right = level[i+1] if i+1 < level.len() else level[i]   // duplicate last if odd
         node  = BLAKE3(b"agora:merkle:node" || left || right)    // 17 prefix + 64 bytes
         next.push(node)
       level = next
     return level[0]
```

Domain separation prefixes prevent second-preimage attacks across levels.

---

## `signing_view()` Method

To handle co-signing correctly, `SigchainBody` gains a `signing_view()` method that
returns a version of the body suitable for signing (co-sign material stripped):

```rust
impl SigchainBody {
    fn signing_view(&self) -> Self {
        match self {
            SigchainBody::Genesis { agent_id, granted_capabilities, cosigner_key, .. } => {
                SigchainBody::Genesis {
                    agent_id: agent_id.clone(),
                    granted_capabilities: granted_capabilities.clone(),
                    cosigner_key: cosigner_key.clone(),
                    cosigner_signature: None,
                }
            }
            other => other.clone(),
        }
    }
}
```

`SigchainLink::signed_bytes()` calls `body.signing_view()` before serializing.

---

## Server Storage Schema

```sql
CREATE TABLE IF NOT EXISTS sigchain_links (
    agent_id    BLOB    NOT NULL,           -- 32-byte AgentId raw bytes
    seqno       INTEGER NOT NULL,
    link_json   TEXT    NOT NULL,           -- JSON-serialized SigchainLink
    canonical_hash BLOB NOT NULL,           -- 32-byte BLAKE3 canonical hash
    link_type   TEXT    NOT NULL,           -- "Genesis", "Action", "Checkpoint", etc.
    created_at  INTEGER NOT NULL,           -- SequenceTimestamp u64 (S-02)
    PRIMARY KEY (agent_id, seqno)
);

CREATE INDEX IF NOT EXISTS idx_sigchain_agent
    ON sigchain_links(agent_id);
CREATE INDEX IF NOT EXISTS idx_sigchain_type
    ON sigchain_links(agent_id, link_type);
```

`link_type` is the enum variant name as a string (used for checkpoint range queries).

---

## Storage Trait Methods

```rust
/// Persist a verified sigchain link. Returns StorageError if (agent_id, seqno) already exists.
async fn store_sigchain_link(&self, record: &SigchainLinkRecord) -> Result<(), StorageError>;

/// Return all links for an agent, ordered by seqno ascending.
async fn get_sigchain(&self, agent_id: &[u8; 32]) -> Result<Vec<SigchainLinkRecord>, StorageError>;

/// Return links for an agent with seqno > since_seqno, ordered ascending.
async fn get_sigchain_since(
    &self,
    agent_id: &[u8; 32],
    since_seqno: u64,
) -> Result<Vec<SigchainLinkRecord>, StorageError>;
```

`SigchainLinkRecord`:
```rust
pub struct SigchainLinkRecord {
    pub agent_id: [u8; 32],
    pub seqno: u64,
    pub link_json: String,
    pub canonical_hash: [u8; 32],
    pub link_type: String,
    pub created_at: u64,
}
```

---

## HTTP API Surface

All endpoints under `/_agora/sigchain/`. No authentication required for GET (chains
are public audit trails). PUT requires authentication.

| Method | Path | Description |
|--------|------|-------------|
| `PUT`  | `/_agora/sigchain/{agent_id}` | Publish a new link |
| `GET`  | `/_agora/sigchain/{agent_id}` | Fetch full chain |
| `GET`  | `/_agora/sigchain/{agent_id}?since={seqno}` | Fetch chain since seqno |
| `GET`  | `/_agora/sigchain/{agent_id}/verify` | Verify chain integrity |

`{agent_id}` is the 64-character hex encoding of the 32-byte `AgentId`.

### `PUT /_agora/sigchain/{agent_id}`

Request body (JSON): a `SigchainLink`.

Server actions:
1. Deserialize the link.
2. Verify that `link.signer` corresponds to `agent_id` (for Genesis) or to a
   key in the agent's current approved device list.
3. Verify the link's signature via `verify_chain()` run on the full chain + new link.
4. Store in `sigchain_links`.

Response 200: `{ "seqno": N, "canonical_hash": "<hex>" }`
Response 400: `{ "errcode": "AGORA_SIGCHAIN_INVALID", "error": "..." }`
Response 409: duplicate seqno

### `GET /_agora/sigchain/{agent_id}`

Response 200: `{ "agent_id": "<hex>", "links": [ SigchainLink, ... ] }`
Response 404: agent not found

### `GET /_agora/sigchain/{agent_id}/verify`

Response 200: `{ "valid": true, "length": N }` or `{ "valid": false, "length": N, "error": "..." }`

---

## Correlation Path Propagation Protocol

When agent A calls agent B (via `agora.tool_call`):

1. A's `agora.tool_call` event includes:
   ```json
   { "call_id": "...", "correlation_path": ["<A's agent_id hex>", ...ancestors...] }
   ```
2. B receives the event and extracts `correlation_path`.
3. B checks: if `B.agent_id.as_hex()` is in `correlation_path` → loop detected, refuse.
4. B's `Action` sigchain link uses the `correlation_path` from the event as-is (A already
   prepended its own id).
5. Path is bounded at 16 entries max. A must truncate if it would exceed 16; the call
   is rejected server-side if `correlation_path.len() > 16`.

---

## SOVEREIGN Primitive Mapping (task-031 reference)

| Agora Concept | SOVEREIGN Primitive |
|---------------|---------------------|
| `AgentId` | `SurfaceId` |
| `Sigchain` | `SovereigntyAttestation` field of `SurfaceManifest` |
| `Action.timestamp` | `membrane.next_sequence()` |
| `Genesis.granted_capabilities` | `CapabilityScope` entries |
| `TrustTransition.{from,to}_state` | FLOW state machine `FlowState` |
| `Checkpoint.merkle_root` | Batch `AuditEvent` digest |
| `Action.correlation_path` | `TaskGraph` dependency chain |

A SOVEREIGN agent communicating through Agora uses the same Ed25519 seed for both
systems — one sigchain serves as attestation in both contexts.

---

*Spec version: 1.0 — 2026-03-04*
