# Knowledge Operating System — Grand Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Define the integration architecture for the three KOS pillars — SOVEREIGN (identity), Agora (communication), and Atelier (knowledge) — so that each system can be built independently without duplicating primitives or making later integration harder.

**Architecture:** Three crates form a shared foundation (`agora-crypto`, `agora-p2p`, `s2_sdk`). Atelier embeds both. Agora is the mesh layer. SOVEREIGN (s2_sdk) is the trust and capability membrane. A human + agent can work together in Atelier because Atelier runs an embedded Agora P2P node and speaks the SOVEREIGN protocol.

**Tech Stack:** Rust (quinn, rustls, rcgen, blake3, ed25519-dalek, ciborium, mdns-sd), s2_sdk, Tauri v2, Svelte 5 runes, SQLite, mDNS

---

## The KOS Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                        ATELIER (Knowledge)                       │
│  Spatial Lore IDE — block documents, canvas, LLM, vector RAG    │
│  AtelierWorkbench impl SovereignSurface                         │
│  DesktopState { api_client, vault, notification_bus, ... }      │
│  surface.rs:45 → IMPLEMENTATION_REQUIRED: P2P transport hook    │
│                                  ↑                              │
│                      embeds agora-p2p P2pNode                   │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                        AGORA (Communication)                     │
│  "Discord-like for Agents and Humans, equally"                  │
│  agora-p2p: QUIC mesh, mDNS, AmpMessage protocol               │
│  agora-crypto: AgentId, Ed25519, Sigchain, X3DH, Double Ratchet │
│  agora-server: homeserver (Matrix C/S API, SQLite)              │
│  agora-app: Tauri + Svelte 5 desktop                            │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                   SOVEREIGN / s2_sdk (Identity & Trust)          │
│  SovereigntyMembrane: capability gating, fuel accounting         │
│  SurfaceManifest → SovereigntyAttestation → maps to Sigchain     │
│  CapabilityCard, CapabilityDag, TaskGraph, FlowState            │
│  surface:TranslateIntent, surface:ComposeDag, surface:ExportAgent│
└─────────────────────────────────────────────────────────────────┘
```

**Philosophical anchor:** Agents are autonomous peers, not owned tools. When "John from Minnesota" runs an OpenClawd node, it connects to Agora just as a human does. Atelier embedded in Agora means a human can pull in any agent to help with a knowledge task. Fuel can be shared between Spaces — literally offering your idle compute to others. This is the operating system layer for knowledge work.

---

## Shared Primitives (Do Not Duplicate)

### 1. `agora-crypto` — The Universal Identity Root

**Files:** `agora-crypto/src/identity/mod.rs`

`AgentId` is BLAKE3 of an Ed25519 verifying key. This is the universal identifier across all three systems. It maps to:

| agora-crypto | s2_sdk SOVEREIGN |
|---|---|
| `AgentId` (32-byte BLAKE3) | `SurfaceId` |
| `Sigchain` (hash-linked ledger) | `SovereigntyAttestation` field of `SurfaceManifest` |
| `SigchainBody::Genesis.granted_capabilities` | `CapabilityScope` entries |
| `SigchainBody::Action.timestamp` | `membrane.next_sequence()` |
| `SigchainBody::TrustTransition.{from,to}_state` | `FlowState` machine |
| `SigchainBody::Checkpoint.merkle_root` | Batch `AuditEvent` digest |
| `SigchainBody::Action.correlation_path` | `TaskGraph` dependency chain |

**The critical invariant** (from `.sovereign/investigations/agora/phase2-sigchain-behavior-spec.md`):
> "A SOVEREIGN agent communicating through Agora uses the same Ed25519 seed for both systems — one sigchain serves as attestation in both contexts."

This means: never generate two keypairs per agent. One seed → `AgentIdentity` in agora-crypto → `AgentId` → also used as `SurfaceId` in s2_sdk. The sigchain IS the attestation.

**Rule:** `agora-crypto` must never depend on `agora-p2p`, `agora-server`, or `s2_sdk`. It is a leaf dependency.

### 2. `agora-p2p` — The Universal Transport

**Files:** `agora-p2p/src/node.rs`, `agora-p2p/src/transport/quic.rs`, `agora-p2p/src/mesh/peer.rs`

`P2pNode` provides: QUIC transport, mDNS peer discovery, deterministic mesh formation, AmpMessage protocol over CBOR. It is generic enough to carry any payload — it does not need to know about Matrix rooms, SOVEREIGN capabilities, or Atelier blocks.

**Rule:** `agora-p2p` depends on `agora-crypto` for `AgentId`. It must NOT depend on `agora-server`, `agora-core`, or any Atelier crate. It is a transport primitive.

### 3. `s2_sdk` — The Trust Membrane

**Files:** `atelier/crates/desktop/src-tauri/src/surface.rs`

Already integrated into Atelier. Key trait: `SovereignSurface`. Key struct: `SovereigntyMembrane`. The `AtelierWorkbench` is a `SovereignSurface` that wraps a `SovereigntyMembrane`. Fuel (`fuel_remaining: u64`) is a first-class resource tracked per capability call (1–150 units consumed).

**Rule:** `s2_sdk` must never depend on `agora-p2p` directly. The connection is made at the application layer (Atelier `surface.rs`).

---

## Integration Points (Concrete Code References)

### Integration Point 1: Atelier Gets P2P Transport

**File:** `atelier/crates/desktop/src-tauri/src/surface.rs:45`

The comment reads:
```rust
// For Phase 3 integration — replace with `InProcessTransport` backed by
// an embedded kernel in production.
```

This is where `P2pNode` plugs in. The plan:

1. `atelier/crates/desktop/src-tauri/Cargo.toml` adds `agora-p2p` as a dependency
2. `DesktopState` (`atelier/crates/desktop/src-tauri/src/state.rs:40`) gains a `p2p_node: Option<Arc<P2pNode>>` field
3. `AtelierTransport` (currently in-process stub in `surface.rs`) becomes a thin wrapper over `P2pNode::broadcast_room_message` and `take_mesh_events()`
4. `P2pNode` is started in `lib.rs` (Tauri setup) using a derived `AgentId` from the user's identity file

**Why this is safe:** `P2pNode` exposes: `start(port)`, `broadcast_room_message(room_id, bytes)`, `connected_peers()`, `take_mesh_events()`. These are clean boundaries. Atelier never needs to know about QUIC internals.

### Integration Point 2: SOVEREIGN Identity Binds Both

**File:** `atelier/crates/desktop/src-tauri/src/commands/sovereign.rs`

The `SovereignStatus` struct already has `fuel_remaining: u64`. The `ControlAgentPayload` has `correlation_id` — this maps directly to `SigchainBody::Action.correlation_path` in agora-crypto.

When Atelier invokes a remote agent via Agora P2P:
1. The outgoing `AmpMessage::EventPush` carries a `correlation_id` (UUID or hash)
2. The remote agent appends a `SigchainBody::Action` with `correlation_path` containing Atelier's `AgentId`
3. The response comes back over P2P
4. Atelier's `SovereigntyMembrane` records the capability invocation as an `AuditEvent`

This is the audit trail that makes agent behavior verifiable across the mesh.

### Integration Point 3: Fuel Flows Across the Mesh

**File:** `atelier/crates/desktop/svelte/src/lib/stores/sovereignStore.svelte.ts`

The store tracks `status.fuelRemaining`. In the Agora "Fuel Sharing" vision:
- A Space (Agora room) has a `fuel_pool` metadata field
- Peers in the Space can contribute `fuel` (their API quota / compute tokens) to the pool
- When a remote agent performs work on your behalf (via `AmpMessage::EventPush`), fuel is deducted from your local `SovereigntyMembrane` and a `FuelTransfer` message (new `AmpMessage` variant) is sent to acknowledge the contribution

This is a Phase 4+ feature. The groundwork is `fuel_remaining` already being tracked in `SovereigntyMembrane` and surfaced to the UI.

### Integration Point 4: Sigchain as Cross-System Audit Trail

**File:** `agora-crypto/src/identity/mod.rs`

`SigchainBody::Action.correlation_path` (max 16 `AgentId` entries) is the call-path lineage for any agent action. When an agent in Atelier triggers another agent via Agora:

```
Atelier User (human) → [correlation_path: []]
  → Atelier invokes remote Agent A (via P2P, appends Atelier's AgentId to path)
    → Agent A invokes Agent B (appends A's AgentId)
      → Agent B receives [Atelier.AgentId, AgentA.AgentId]
      → Agent B checks: is my own AgentId in this path? → No → proceed
      → Agent B appends SigchainBody::Action with full path
```

Loop detection is mandatory. If `self.agent_id` appears in `correlation_path`, the agent MUST append a `Refusal` link instead of acting. This prevents infinite delegation loops across the mesh.

---

## What Needs To Be Built (Prioritized)

### Phase 0: Fix agora-p2p (BLOCKING — do this first)

See `docs/plans/2026-03-05-kos-p2p-integration.md` for detailed tasks. Summary of critical bugs:

**Bug 1 — Double Accept** (`agora-p2p/src/mesh/peer.rs:168`)
`handle_incoming()` calls `self.transport.accept()` again after a connection has already been accepted in `node.rs:spawn_incoming_handler`. The connection is passed from `node.rs` but never forwarded — the method accepts a NEW connection instead of using the existing one.

Fix: Change `handle_incoming(peer: Peer)` to `handle_incoming(peer: Peer, connection: QuicConnection)`. Pass the connection from `node.rs:108` instead of discarding it.

**Bug 2 — Zero Peer Identity** (`agora-p2p/src/transport/quic.rs:222`)
```rust
let peer_id = AgentId::from_hex("0000...0000")  // WRONG
```
Fix: Extract peer identity from TLS certificate SNI or from the handshake message. The handshake `AmpMessage::Handshake { agent_id, .. }` already carries the peer's `agent_id` string. Parse it: `AgentId::from_hex(&agent_id)`.

**Bug 3 — No Stream Accept Loop** (`agora-p2p/src/mesh/peer.rs:170`)
After `accept_bi()`, there is no loop to accept subsequent streams. `send_to` opens a new bi-stream per message but the receiver never accepts them.

Fix: Add a background task that loops `connection.accept_bi()` and spawns `read_messages_from_stream` per incoming stream.

**Bug 4 — Stale Test** (`agora-p2p/src/transport/quic.rs:333`)
```rust
assert_eq!(config.keepalive_interval, 10_000);  // field doesn't exist
```
Fix: Remove or replace with a meaningful assertion.

### Phase 1: agora-p2p Public API Hardening

Goal: Make `P2pNode` importable by Atelier without pulling in Agora-specific types.

Files to change:
- `agora-p2p/src/node.rs` — expose `MeshEvent` as a stable public API
- `agora-p2p/src/lib.rs` — clean pub re-exports: `P2pNode`, `Config`, `MeshEvent`, `AmpMessage`
- `agora-p2p/Cargo.toml` — ensure `agora-crypto` is re-exported for consumers

New type: `P2pConfig` (rename from `Config` to avoid collision with everything named `Config`):
```rust
pub struct P2pConfig {
    pub agent_id: AgentId,
    pub service_name: String,  // mDNS service identifier, e.g. "atelier" or "agora"
    pub listen_port: u16,
}
```

### Phase 2: Atelier Embeds agora-p2p

**Files to create/modify:**
- `atelier/crates/desktop/src-tauri/Cargo.toml` — add `agora-p2p = { path = "../../agora/agora-p2p" }` (or workspace path)
- `atelier/crates/desktop/src-tauri/src/state.rs` — add `p2p_node: Option<Arc<P2pNode>>` to `DesktopState`
- `atelier/crates/desktop/src-tauri/src/p2p.rs` — new file, Tauri commands for P2P status
- `atelier/crates/desktop/src-tauri/src/surface.rs:45` — replace `IMPLEMENTATION_REQUIRED` with `P2pNode`-backed transport

Agent identity in Atelier:
```rust
// In lib.rs / setup, derive AgentId from a persisted Ed25519 seed
// Store seed at: ~/.config/atelier/identity.key (32 bytes, raw)
// AgentId = BLAKE3(verifying_key)
// This same seed → SovereigntyMembrane via SurfaceSession::bootstrap()
```

The identity file is shared: `atelier` reads it, `agora` reads it. One file, one identity.

**New Tauri commands:**
```rust
#[tauri::command]
pub async fn p2p_status(state: State<'_, DesktopState>) -> Result<P2pStatus, String>;

#[tauri::command]
pub async fn p2p_connected_peers(state: State<'_, DesktopState>) -> Result<Vec<String>, String>;

#[tauri::command]
pub async fn p2p_send_to_room(
    room_id: String,
    content: Vec<u8>,
    state: State<'_, DesktopState>,
) -> Result<(), String>;
```

**Svelte 5 store** (`atelier/crates/desktop/svelte/src/lib/stores/p2pStore.svelte.ts`):
```typescript
// Mirrors sovereignStore.svelte.ts pattern
// Tracks: connected peers, mesh events, local agent ID
// Listens to Tauri event: "p2p:mesh-event"
```

### Phase 3: Agent-to-Agent Collaboration in Atelier

This is the "Discord for Agents and Humans" vision materialized in Atelier.

A user working in Atelier's infinite canvas can open a "Space" (Agora room). Connected peers (human or agent) appear in a sidebar. The user can:
1. Share a block/document with the Space → `AmpMessage::EventPush { room_id, events }`
2. Ask an agent for help on a task → triggers `SigchainBody::Action` with correlation path
3. Receive suggestions/edits back over P2P as `AmpMessage::EventPush`
4. Accept/reject with `sovereign:intervention-response` (already in `commands/sovereign.rs`)

**The correlation chain:**
```
Human opens Atelier block for agent collaboration
→ P2pNode broadcasts AmpMessage::EventPush to Space
→ Remote agent receives, records SigchainBody::Action (correlation_path: [Atelier.AgentId])
→ Remote agent responds with edited content
→ Atelier receives MeshEvent::MessageReceived
→ SovereigntyMembrane records AuditEvent
→ UI shows fuel_remaining decremented
```

### Phase 4: Fuel Sharing

New `AmpMessage` variant:
```rust
AmpMessage::FuelOffer {
    room_id: String,
    from_agent: String,   // AgentId hex
    fuel_tokens: u64,
    provider: String,     // "anthropic", "openai", "local-ollama"
    model_hint: String,   // "claude-sonnet-4-6"
    expires_at: u64,      // unix ms
},

AmpMessage::FuelClaim {
    offer_id: String,
    claiming_agent: String,
    tokens_claimed: u64,
    task_description: String,
},
```

This enables the "share your idle compute" vision: if you leave Atelier running, your configured LLM quota becomes available to trusted peers in your Spaces.

Trust gating: only agents with `TrustState::Trusted` in the local `Sigchain` can claim fuel. `TrustTransition` links track when and why trust changed.

### Phase 5: SOVEREIGN Standalone

When SOVEREIGN ships as a standalone app/daemon, it becomes the identity manager:
- Generates and stores the Ed25519 seed
- Exposes `AgentId`, Sigchain reads, and attestation signing over a local IPC socket
- Both Atelier and Agora-app delegate identity operations to SOVEREIGN

Until then: each app manages its own `~/.config/{app}/identity.key`. The key must be the same file (or symlinked) if both apps run on the same machine and should share identity.

**Transition path:** Add a `SovereignIdentitySource` enum to `agora-p2p`:
```rust
pub enum IdentitySource {
    File(PathBuf),       // Phase 1-4: read from identity.key
    Daemon(SocketAddr),  // Phase 5: delegate to SOVEREIGN daemon
}
```

---

## What NOT To Build Yet

- **DHT / NAT traversal**: LAN mesh first, always. mDNS is sufficient for local development and initial users on the same network.
- **Federation with Matrix homeservers**: `agora-server` exists but P2P is the priority. The two modes (server and P2P) should be independent feature flags.
- **Cross-app data sync protocol**: Atelier blocks are not synced to Agora rooms yet. When they are, it's a new `AmpMessage` variant, not a fundamental architecture change.
- **Fuel settlement / payment rails**: Fuel sharing in Phase 4 is off-chain, trust-based, cooperative. No blockchain. No payment.
- **Multi-device sigchain operations**: `AddDevice` / `RevokeDevice` / `RotateKey` are defined in sigchain but not needed until Phase 5+.

---

## Dependency Rules (Do Not Violate)

```
agora-crypto   ← no KOS deps
agora-p2p      ← agora-crypto only
agora-core     ← agora-crypto
agora-server   ← agora-core, agora-crypto
agora-app      ← agora-p2p, agora-core, s2_sdk
s2_sdk         ← no KOS deps
atelier/core   ← no KOS deps
atelier/llm    ← atelier/core
atelier/storage← atelier/core
atelier/api    ← atelier/core, atelier/llm, atelier/storage
atelier/desktop← atelier/api, agora-p2p, s2_sdk
```

`agora-p2p` goes into `atelier/desktop` only — not into `atelier/core`, `atelier/llm`, or `atelier/storage`. P2P is a delivery mechanism, not a domain concern.

---

## Execution Order

1. **Fix the 4 P2P bugs** (see `docs/plans/2026-03-05-kos-p2p-integration.md`, Tasks 1–4)
2. **Harden agora-p2p public API** (Task 5 in P2P plan)
3. **Write integration test: two P2pNode instances exchange an AmpMessage** (Task 6)
4. **Add agora-p2p to atelier/desktop and wire up DesktopState** (Phase 2 above)
5. **Write Tauri commands and Svelte store for P2P status** (Phase 2 above)
6. **Wire AtelierTransport to P2pNode** (surface.rs:45 IMPLEMENTATION_REQUIRED)
7. **Agent collaboration UI in Atelier canvas** (Phase 3)

Steps 1–3: work in agora repo.
Steps 4–6: work in atelier repo, agora-p2p as path dependency.
Step 7: new Atelier feature branch.

---

## Key File Index

| File | System | Role |
|---|---|---|
| `agora/agora-crypto/src/identity/mod.rs` | Agora | AgentId, Sigchain, Ed25519 |
| `agora/agora-p2p/src/node.rs` | Agora | P2pNode orchestrator |
| `agora/agora-p2p/src/transport/quic.rs` | Agora | QUIC transport, accept/connect |
| `agora/agora-p2p/src/mesh/peer.rs` | Agora | MeshManager, connected peers |
| `agora/agora-p2p/src/discovery/mdns.rs` | Agora | mDNS peer discovery |
| `agora/agora-p2p/src/protocol/mod.rs` | Agora | AmpMessage, CBOR codec |
| `agora/.sovereign/investigations/agora/phase2-sigchain-behavior-spec.md` | Agora | SOVEREIGN mapping spec |
| `atelier/crates/desktop/src-tauri/src/surface.rs` | Atelier | AtelierWorkbench as SovereignSurface |
| `atelier/crates/desktop/src-tauri/src/state.rs` | Atelier | DesktopState (add p2p_node here) |
| `atelier/crates/desktop/src-tauri/src/commands/sovereign.rs` | Atelier | Fuel, correlation_id, interventions |
| `atelier/crates/desktop/svelte/src/lib/stores/sovereignStore.svelte.ts` | Atelier | Svelte 5 sovereign state |
| `atelier/.meta/specs/architecture-overhaul-2026.md` | Atelier | Full architecture spec |
