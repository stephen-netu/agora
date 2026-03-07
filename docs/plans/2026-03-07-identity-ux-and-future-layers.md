# Identity UX and Future Layers — Design Record

**Date:** 2026-03-07
**Status:** Approved — tasks added to world-tree.yaml (wt-doc-005, wt-103–wt-110)
**Scope:** KOS/agora (primary), cross-repo SOVEREIGN (Phase 5+)
**Does not affect:** Phase 1 Yggdrasil transport work — zero overlap

---

## Purpose

This document canonises findings from a codebase investigation into the identity
layer of `agora-crypto` and `agora-p2p`. It maps those findings against a set of
external design ideas (THOUGHT_STARTERS.md), corrects what was wrong, preserves
what was genuinely new, and records additional findings that the external agent
could not have known without codebase access.

The external agent had no access to the codebase. This document is the canonical
record. THOUGHT_STARTERS.md is a reference artefact only.

---

## What Already Exists (Investigation Findings)

The following structures are fully designed and implemented in
`agora-crypto/src/identity/mod.rs`. They are not new ideas — they are the current
state of the system.

| Structure | Location | Notes |
|-----------|----------|-------|
| `AgentId` — 32-byte BLAKE3(pubkey) | `identity/mod.rs:21` | Canonical identity. Never changes. |
| `AgentId::Display` → `agnt-{first_8_bytes_hex}` | `identity/mod.rs:71` | Current machine-readable short form |
| `AgentIdentity::from_seed(&[u8; 32])` | `identity/mod.rs:96` | S-02 deterministic — same seed always produces same identity |
| `TrustState` enum (5 states) | `identity/mod.rs:134` | Untrusted / Provisional / Trusted / Suspended / Revoked |
| `SigchainBody::Action` with `correlation_path` | `identity/mod.rs:186` | Behavioral tracking with call-path lineage |
| `SigchainBody::Checkpoint` with `merkle_root` | `identity/mod.rs:207` | Designed for batch verification — not yet implemented |
| `SigchainBody::TrustTransition` | `identity/mod.rs:218` | Trust level changes, append-only record |
| `SigchainBody::Refusal` and `DisputeRefusal` | `identity/mod.rs:232` | Loop detection and dispute non-participation, auditable |
| `Genesis.cosigner_key` + `cosigner_signature` | `identity/mod.rs:161,168` | Two-party genesis signing, fully implemented |
| `SignedEntry { link, canonical_hash }` | `identity/mod.rs:356` | Dispute evidence export |
| Checkpoint Merkle algorithm (full spec) | `phase2-sigchain-behavior-spec.md` | Algorithm written, implementation absent |

**Key finding:** A large portion of the ideas in THOUGHT_STARTERS describes
architecture that is already designed. The external agent was reasoning from
first principles about things that are already in the codebase.

---

## Corrections to External Ideas

### Capability class must not be embedded in identity strings

THOUGHT_STARTERS proposed formats like `ax.c2.t3:iron-falcon-2219` where
capability class and trust tier are part of the identity string.

This is wrong for this codebase. Capabilities are stored in
`Genesis.granted_capabilities: Vec<String>` and trust state is tracked via
`SigchainBody::TrustTransition`. Both are mutable sigchain attributes —
capabilities can be upgraded, trust can be revoked. Embedding mutable state
in an identity string violates the system's core invariant:

```
identity = cryptographic root (immutable)
representation = derived metadata (displayable, not authoritative)
```

The correct model: `AgentId` is the identity. Display formatters in UI and
logs may show trust/capability context alongside the name, but it is never
baked into the identity string itself.

### "Domain prefix ax" does not exist

THOUGHT_STARTERS used `ax` and `axiom` as domain prefixes. No such namespace
exists in `agora-crypto`, `agora-p2p`, or any KOS crate. Domain/federation
prefixes are a Phase 5+ concern (SOVEREIGN standalone) and will be designed
at that time, grounded in the actual federation architecture.

### The "unified history" is already designed via SigchainBody::Action

THOUGHT_STARTERS proposed building a new unified Merkle log for identity,
governance, and execution. The design is already present: `SigchainBody::Action`
records every behavioral event with `content_hash`, `effect_hash`, `timestamp`,
and `correlation_path`. The missing piece is wiring SOVEREIGN's `AuditEvent`
writes to also append `Action` links — which is Phase 5 work, tracked as wt-109.

---

## Genuine Gaps (New Work)

The following items are confirmed absent from the codebase and worth adding.

### 1. Human-readable deterministic names (wt-103)

`AgentId::Display` emits `agnt-7f3c2a8d9c5e4ab1`. This is machine-readable
but not human-memorable. Agents in logs, UI, and CLI would be significantly
more usable with a stable, memorable name derived deterministically from the
same `AgentId`.

Proposed format: `word1-word2#NNNN`

Derivation from `AgentId` bytes:
- bits 0–10 → adjective index (2048-word list)
- bits 11–21 → noun index (2048-word list)
- bits 22–35 → 4-digit decimal checksum

Total collision-free identity space: ~8 trillion. The checksum ensures two
`AgentId`s that map to the same word pair remain visually distinct.

A `name_schema: u8 = 1` constant marks the wordlist and algorithm version.
If the wordlist or bit-extraction algorithm ever changes, `name_schema`
increments and the previous schema remains resolvable. Builds on top of
`AgentId::Display` — does not replace it.

### 2. Encrypted seed bundle (wt-104)

`~/.config/agora/identity.key` currently stores 32 raw bytes. This is unsafe
for user-facing applications. An encrypted bundle is required before agora-app
ships to real users.

Design:
- Key derivation: `Argon2id(passphrase, random_salt_16b)` → 32-byte key
- Encryption: `ChaCha20-Poly1305(seed, nonce, key)`
- Bundle: `{ schema_v: 1, salt: [u8;16], nonce: [u8;12], ciphertext: [u8;32+16] }`
- At first launch: generate seed, encrypt, offer `.agora-identity` download,
  display BIP39 mnemonic for paper backup
- Recovery: accept `.agora-identity` + passphrase, or mnemonic reconstitution

No new crypto dependencies. `argon2` and `chacha20poly1305` are standard
Rust crates, S-02 compliant (deterministic given same passphrase + salt).

### 3. Co-signer as P2P 2FA (wt-105)

The sigchain already has the primitive. `Genesis.cosigner_key` and
`Genesis.cosigner_signature` (identity/mod.rs:161,168) are fully implemented.
Both the primary signer and co-signer sign identical bytes; the co-signature
field is stripped before signing so neither party's signature commits to the
other's. This is exactly the 2FA model for a P2P system with no server:

- Primary key = something you have (the seed file)
- Co-signer = a second device

The gap is UX: agora-app does not yet surface this during signup. The task
is to wire the existing primitive to the signup flow, not to design new crypto.

Flow: primary generates genesis bytes → shows QR or short token → second
device scans → co-signs → primary submits genesis with both signatures.

### 4. IdentitySnapshot for fast state bootstrapping (wt-106)

When a peer joins a mesh, verifying their full sigchain from genesis is
expensive for long-running agents. An `IdentitySnapshot` is a signed summary
of current state at a given `seqno`, verifiable without chain replay.

```rust
pub struct IdentitySnapshot {
    pub agent_id: AgentId,
    pub display_name: String,       // from agent_display_name() — wt-103
    pub trust_state: TrustState,    // from last TrustTransition link
    pub granted_capabilities: Vec<String>,  // from Genesis + any updates
    pub last_seqno: u64,
    pub snapshot_hash: [u8; 32],    // BLAKE3 of serialized fields (excl. signature)
    pub signature: Vec<u8>,         // Ed25519 over snapshot_hash
}
```

This is independent of `SigchainBody::Checkpoint` (which is Merkle-based,
for batch verification of Action links). A snapshot is a signed summary;
a checkpoint is a cryptographic proof. Both are needed.

### 5. Agora-native event type system (wt-107) — PRE-PRODUCTION BLOCKER

`SigchainBody::Action.event_type: String` stores Matrix strings
(`"m.room.message"`, `"agora.tool_call"`) in cryptographically signed,
append-only data (identity/mod.rs:187). Once a production sigchain contains
these strings, they cannot be renamed without invalidating every `content_hash`
that committed to the old string.

Matrix is being phased out of the KOS stack. This is a pre-production
blocker: an Agora-native event type system must replace `event_type: String`
before any production sigchains are written.

The `phase2-sigchain-behavior-spec.md` notes "breaking change acceptable for
pre-launch system" — this task is the sanctioned breaking change, done once,
cleanly, before production use.

---

## New Findings from Investigation

These are KOS-specific design properties not present in any external reference.

### Yggdrasil co-derived identity (wt-doc-005)

Yggdrasil derives its node IPv6 address from an Ed25519 public key via SHA-512
(confirmed: `agora-p2p/src/identity/yggdrasil.rs:7-17`). The KOS identity
stack derives `AgentId = BLAKE3(pubkey)` from the same key.

This means every agent has three co-derived identifiers from one 32-byte seed:

```
seed (32 bytes, private)
  └─ Ed25519 signing key
       ├─ AgentId = BLAKE3(verifying_key.to_bytes())   — sigchain identity
       └─ Yggdrasil IPv6 = f(SHA-512(verifying_key))   — network address
```

Properties:
- No registration required for a network address
- No DHCP, no coordination, no central authority
- The network address is cryptographically bound to the identity
- A peer's Yggdrasil address is computable from their public key alone

This is a KOS differentiator with no equivalent in Matrix or standard P2P
systems. It should be documented as a first-class design property.

### Matrix strings in signed data are a migration risk

`Action.event_type: String` uses Matrix namespace strings in signed data.
This is covered by wt-107 above. Documented here as a system-level finding:
any field in a `SigchainBody` variant that stores human-readable strings
controlled by an external standard is a migration risk. Future variant design
should prefer enums or versioned newtypes over raw `String` for type-tagged
fields.

---

## Deferred (Not Tasks)

The following ideas from THOUGHT_STARTERS are noted but not tasked, as they
duplicate existing planned work or are Phase 8+ concerns:

- **On-chain checkpoint anchoring** — covered by RFC-007 (wt-doc-003)
- **ZK execution proving** — covered by RFC-006 (wt-doc-002)
- **Compute credit economy** — covered by wt-100–wt-102
- **DHT fallback** — Phase 6, no design work needed yet
- **Federation domain prefixes** — Phase 5+, design at Phase 5

---

## Task Summary

| ID | Title | Phase | Priority |
|----|-------|-------|----------|
| wt-doc-005 | Document Yggdrasil co-derived identity property | 1 | p3 |
| wt-103 | `agent_display_name()` — deterministic human names | 2 | p2 |
| wt-104 | Encrypted seed bundle — Argon2id + ChaCha20 | 2 | p2 |
| wt-105 | Co-signer 2FA UX — wire Genesis.cosigner_key to signup | 2 | p2 |
| wt-106 | `IdentitySnapshot` for fast state bootstrapping | 2 | p2 |
| wt-107 | Agora-native event type system (pre-production blocker) | 3 | p1 |
| wt-108 | Implement `compute_checkpoint_merkle_root()` | 4 | p2 |
| wt-109 | Wire SOVEREIGN `AuditEvent` → `SigchainBody::Action` | 5 | p2 |
| wt-110 | `applied_rule_hash` in Action — VDK proof bundle | 7 | p2 |

---

## References

- `agora-crypto/src/identity/mod.rs` — all sigchain types and implementations
- `agora-crypto/src/ids.rs` — BLAKE3-based deterministic ID generation patterns
- `agora-p2p/src/identity/yggdrasil.rs:7-17` — Yggdrasil IPv6 derivation
- `agora/.sovereign/investigations/agora/phase2-sigchain-behavior-spec.md` — canonical spec for Action, Checkpoint, TrustTransition, Merkle algorithm, SOVEREIGN mapping
- `docs/plans/2026-03-05-kos-grand-plan.md` — phase definitions and dependency rules
- `docs/plans/2026-03-05-kos-p2p-integration.md` — Phase 0/1 P2P work (do not overlap)
- `THOUGHT_STARTERS.md` — external reference artefact (no codebase access; superseded by this document)
