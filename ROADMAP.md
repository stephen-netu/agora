# Agora Roadmap

## Current State

Agora is a self-hosted, privacy-first communications platform implementing a substantial subset of the Matrix Client-Server API (v1.11). The server runs as a single binary with SQLite storage. The core infrastructure is in place and all three clients (desktop app, web frontend via Tauri, interactive TUI) can run — recent fixes addressed critical bugs in room creation and event ordering that were blocking practical use.

### What ships today

**Protocol**: Authentication, rooms, spaces, messaging, file media upload/download, E2E encryption, profiles, typing indicators, devices, room directory, user directory search, pinned messages, room invitations (send and receive), room deletion, space hierarchy.

> **Note**: Room creation and event streaming had critical bugs (stream_ordering, timestamp sequence) that were recently fixed. The protocol implementation is functional but still maturing — some edge cases may surface during heavier usage.

**Cryptography** (`agora-crypto`): Built on audited primitives — X25519, Ed25519, ChaCha20-Poly1305, BLAKE3, HKDF. All IDs are BLAKE3 content-addressed; timestamps are deterministic sequence counters that survive restarts without collision. E2E uses Signal-spec Double Ratchet + X3DH (`m.agora.pairwise.v1`) and sender-key group sessions (`m.agora.group.v1`). These are internal algorithm identifiers — encrypted rooms are only readable by Agora clients, not Element or other standard Matrix clients.

**Sigchain Behavioral Ledger**: Every outgoing message and file send is signed and appended to the sender's local sigchain — an append-only, BLAKE3-hash-linked, Ed25519-signed action log. Each signed action is published to the server and embedded in the Matrix event as `sigchain_proof: { seqno, agent_id }`. Recipients see a ⛓ #N badge on verified messages. S-05 loop detection prevents recursive agent amplification by appending a `Refusal` link if the sender's Agent ID is already present in the correlation path. Agent IDs are displayed in the Settings → Encryption tab and via `agora agent-id` in the CLI.

**Clients**:
- **Desktop app** (Tauri + Svelte 5): full messaging, E2EE, file uploads, pinned messages, invite modal with live user search, sigchain badges, theme switcher (dark/light/seraphim), settings panel.
- **Interactive TUI** (`agora connect`): ratatui-based terminal client with room switching, scrollback, sigchain-signed sending.
- **CLI** (`agora-cli`): `register`, `login`, `logout`, `rooms`, `spaces`, `send`, `messages`, `upload`, `download`, `connect`, `agent-id`.

**Startup scripts** (`scripts/`): `.sh` for Mac/Linux, `.bat` and `.ps1` for Windows — no hardcoded paths, work from any location.

---

## Decentralized Architecture: Agora Mesh Protocol (AMP)

> **Vision**: No server required. Every user is a peer. Identity is a keypair you own, not an account a server grants you. Rooms are shared append-only DAGs replicated across their members. Messages reach offline peers through volunteer relay nodes. The network has no single point of failure, no single point of surveillance, and no single point of control.

### Why Agora Is Already Positioned for This

Most messaging systems attempting decentralization have to retrofit cryptographic identity and content addressing onto an architecture never designed for it. Agora does not. The primitives that make Agora decentralizable are already implemented and in production:

| Primitive | Current use | Decentralized role |
|-----------|------------|-------------------|
| **AgentID** (Ed25519 keypair) | Sigchain identity, behavioral ledger | Sovereign peer identity — no server issues or revokes it |
| **BLAKE3 content addressing** | Event IDs, room IDs, media IDs | DHT keys, content-integrity verification, deduplication |
| **Sigchain** (append-only, hash-linked, signed) | Behavioral audit log | Room event DAG, tamper-evident history without server authority |
| **Double Ratchet + X3DH** | E2E encryption for rooms | P2P encryption — designed for this; works without a rendezvous server |
| **BTreeMap / deterministic ordering** | S-02 compliance | CRDT merge determinism — same events, same state, on every peer |
| **ChaCha20-Poly1305** | Message encryption | Relay payload encryption — relays store ciphertext, never plaintext |

The path to decentralization is not a rewrite. It is adding transport, discovery, and replication layers on top of cryptographic primitives that were designed for exactly this.

---

### Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                         APPLICATION LAYER                            │
│   Desktop App (Tauri)  │  TUI (ratatui)  │  CLI  │  Future: Mobile  │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────┐
│                        AGORA MESH PROTOCOL                           │
│                                                                      │
│  ┌─────────────────┐  ┌──────────────────┐  ┌─────────────────────┐ │
│  │  Identity Layer  │  │   Room Protocol  │  │   Relay Protocol    │ │
│  │                 │  │                  │  │                     │ │
│  │  AgentID        │  │  Merkle DAG      │  │  Encrypted          │ │
│  │  (Ed25519)      │  │  Event History   │  │  Store-and-Forward  │ │
│  │                 │  │                  │  │                     │ │
│  │  Petname graph  │  │  CRDT Room State │  │  TTL-bounded        │ │
│  │  (local trust)  │  │                  │  │  Blind relay        │ │
│  └────────┬────────┘  └────────┬─────────┘  └──────────┬──────────┘ │
│           │                   │                        │             │
│  ┌────────▼───────────────────▼────────────────────────▼──────────┐ │
│  │                    REPLICATION LAYER                            │ │
│  │   Selective sync  │  Gossip fanout  │  Content-addressed pull  │ │
│  └────────────────────────────┬────────────────────────────────────┘ │
│                               │                                      │
│  ┌────────────────────────────▼────────────────────────────────────┐ │
│  │                     DISCOVERY LAYER                             │ │
│  │   Kademlia DHT  │  mDNS (LAN)  │  Bootstrap peers              │ │
│  └────────────────────────────┬────────────────────────────────────┘ │
│                               │                                      │
│  ┌────────────────────────────▼────────────────────────────────────┐ │
│  │                     TRANSPORT LAYER                             │ │
│  │   QUIC (primary)  │  WebRTC  │  TCP/TLS (fallback)             │ │
│  │   STUN/TURN  │  NAT hole punching  │  Circuit relay             │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────┘
```

Existing `agora-server` nodes do not disappear — they become **super-peers**: always-online, high-bandwidth relay and indexer nodes that happen to also speak the current client-server API. They are peers in the mesh, not gatekeepers of it.

---

### Layer 1: Identity

**The fundamental change**: identity is no longer `@alice:someserver.example`. Identity is an Ed25519 keypair you generate locally and own forever, expressed as an AgentID.

```
AgentID:  ed25519:<hex-encoded-blake3-of-public-key>
Short ID: <first-12-hex-chars>     (for display)
```

User-facing display names are petnames — names you locally assign to known AgentIDs. There is no global namespace authority. Two users can both call themselves "alice"; the network resolves ambiguity through trust graphs, not central registration.

**Identity documents** are signed records published to the DHT keyed by AgentID:

```
IdentityDocument {
    agent_id:     AgentId,          // Ed25519 public key (BLAKE3-addressed)
    display_name: String,           // user's chosen name, not globally unique
    avatar_hash:  Option<Blake3>,   // content-addressed avatar
    devices:      Vec<DeviceKey>,   // all active devices for this identity
    relays:       Vec<RelayAddr>,   // preferred relay nodes for offline delivery
    sigchain_tip: Blake3,           // latest sigchain hash for this agent
    timestamp:    u64,              // Lamport sequence number
    signature:    Ed25519Sig,       // signed by AgentID private key
}
```

**Key operations**:
- `agora identity new` — generate a new AgentID keypair, stored locally
- `agora identity export` — export identity for import on another device
- `agora identity publish` — push updated IdentityDocument to DHT
- `agora identity follow <agent-id>` — add to local petname graph
- Existing `agora-server` accounts can export their AgentID key; it is already generated and stored in `agora-crypto`

**Multi-device**: A single identity can span multiple devices. Each device has its own Ed25519 keypair listed in the IdentityDocument. Cross-device key verification uses SAS (Short Authentication Strings) — two devices display matching emoji sequences derived from a shared secret established via QR code or out-of-band channel.

---

### Layer 2: Transport

**Primary transport: QUIC** via the [`quinn`](https://github.com/quinn-rs/quinn) crate.

QUIC provides:
- Multiplexed streams over a single UDP connection (no head-of-line blocking)
- 0-RTT reconnection for known peers
- Built-in TLS 1.3 (transport-layer encryption, separate from E2E content encryption)
- Better NAT behavior than TCP

**Transport stack by environment**:

| Environment | Transport |
|-------------|-----------|
| Desktop, native | QUIC (primary), TCP/TLS (fallback) |
| Browser | WebTransport (QUIC over HTTP/3) or WebRTC data channels |
| Behind symmetric NAT | QUIC hole punching → circuit relay fallback |
| Same LAN | QUIC direct (discovered via mDNS, no DHT needed) |

**NAT traversal** (in order of preference):
1. **Direct connection** — works if at least one peer has a public IP or UPnP-mapped port
2. **QUIC hole punching** — both peers simultaneously attempt connections, coordinated via a third peer they both know; works for ~80% of NAT configurations
3. **Circuit relay** — a mutually known peer forwards traffic; used as fallback; any peer with a public address can volunteer as a relay
4. **TURN server** — last resort for symmetric NAT; operator-run; no knowledge of message content

**Wire protocol**: length-prefixed, CBOR-encoded `AmpMessage` frames over QUIC streams. Message types:

```
AmpMessage {
    Handshake      { agent_id, protocol_version, capabilities },
    Ping           { nonce },
    Pong           { nonce },
    EventPush      { events: Vec<SignedEvent> },       // push new events to peer
    EventRequest   { event_hashes: Vec<Blake3> },      // request specific events
    StateRequest   { room_id, since_hash },             // sync room state
    StateResponse  { events: Vec<SignedEvent> },
    RelayStore     { payload: EncryptedRelayPayload },  // drop a relay message
    RelayFetch     { agent_id, since: u64 },            // retrieve relay messages
    DhtGet         { key: Blake3 },
    DhtPut         { key: Blake3, value: Bytes, sig: Ed25519Sig },
    DhtResponse    { key: Blake3, value: Option<Bytes> },
}
```

---

### Layer 3: Discovery

Peers need to find each other without a central directory. Discovery operates at three scopes:

**Local (mDNS)**

On the same LAN or VPN (including Tailscale/ZeroTier), peers discover each other via multicast DNS with zero configuration. A peer announces its AgentID and QUIC address on the local multicast group. Works offline from the internet. This is the path for home/office use with no external infrastructure.

**Internet (Kademlia DHT)**

A standard Kademlia DHT provides global peer and content routing. Each peer maintains a routing table of ~20 "buckets" of peers at exponentially increasing XOR distances from its own ID. Lookups converge in O(log n) hops.

DHT record types:

| Record | Key | Value | TTL |
|--------|-----|-------|-----|
| Peer addresses | `AgentID` | `PeerRecord { addrs, timestamp, sig }` | 24h (re-announced hourly) |
| Identity document | `"id:" + AgentID` | `IdentityDocument` (signed) | 72h |
| Room membership | `"room:" + RoomID` | `Vec<AgentID>` (signed by room key) | 24h |
| Relay advertisements | `"relay:" + AgentID` | `RelayRecord { capacity, addrs }` | 1h |
| Content (media) | `Blake3(content)` | `EncryptedBlob` or `PeerList` who has it | 48h |

**Bootstrap peers**

New peers need at least one known address to join the DHT. Bootstrap mechanisms (in priority order):
1. Previously known peers cached locally
2. Hard-coded bootstrap peer list (long-lived volunteer nodes, including `agora-server` instances)
3. DNS-based bootstrap: `_agora-bootstrap._udp.agora.example TXT` records
4. LAN mDNS: if any LAN peer is already in the DHT, use them as entry point

Any peer with a stable public address can volunteer as a bootstrap node by running `agora relay --bootstrap`.

---

### Layer 4: Room Protocol — Merkle Event DAG

The most significant protocol change from the current architecture.

**Today**: the server imposes a total ordering on events (a linear timeline). If the server is gone, there is no canonical order.

**In AMP**: a room is a **Merkle DAG** of signed events — the same structure as a git commit graph. Events reference their causal parents by hash. Peers can create events concurrently. The DAG captures the true causal structure of the conversation.

**SignedEvent**:

```
SignedEvent {
    // Identity
    event_id:   Blake3,          // BLAKE3(canonical CBOR of event body)
    agent_id:   AgentId,         // sender

    // Causal structure
    parents:    Vec<Blake3>,     // hashes of causally preceding events
                                 // (empty only for room genesis event)
    room_id:    Blake3,          // BLAKE3(room genesis event)

    // Content
    event_type: String,          // "m.room.message", "m.room.member", etc.
    content:    CborValue,       // encrypted or plaintext event body

    // Ordering
    lamport:    u64,             // Lamport clock — max(parents) + 1
    wall_clock: u64,             // wall time hint (untrusted, display only)

    // Auth
    sigchain_seqno: u64,         // sequence number in sender's sigchain
    signature:  Ed25519Sig,      // sig over event_id by agent_id private key
}
```

**Canonical display ordering** for clients rendering a room:
1. Topological sort of the DAG (parents always before children)
2. Ties broken by `lamport` clock
3. Remaining ties broken by `event_id` lexicographic order (deterministic)

This means every client that has the same set of events displays them in the same order — no server required to impose order.

**State events** (membership, room name, power levels, pinned events) use a **CRDT merge function** rather than last-write-wins. For each state key (`event_type` + `state_key`), the authoritative value is determined by:
1. The event in the DAG with the greatest `lamport` clock
2. Ties broken by `event_id` (deterministic, requires no coordination)

This makes state conflict resolution fully deterministic and local — any peer with the full DAG computes identical state.

**Room genesis event**: The first event in a room's DAG. Its hash becomes the `room_id`. It contains:
```
RoomGenesis {
    creator:        AgentId,
    room_name:      String,
    initial_members: Vec<AgentId>,
    room_key:       Ed25519PublicKey,  // room authority key for admin ops
    encryption:     Option<EncryptionConfig>,
    timestamp:      u64,
}
```
The creator signs the genesis event with their AgentID key. The `room_key`'s private counterpart authorizes membership changes and moderator actions (kick, ban, power level changes).

---

### Layer 5: Replication

Events spread through the network via a combination of push (gossip) and pull (request):

**Gossip fanout**: When a peer creates or receives a new event, it notifies its k-closest peers in the room's membership set. They pull any events they don't have. This is epidemic replication — events propagate without central coordination.

**Selective sync**: A peer joining a room requests the room's full DAG tip hashes from any known room member. It then requests only the events it is missing (content-addressed pull). It does not need to trust or rely on any single peer — it can verify every event's signature independently.

**Media / file content**: Stored separately from events. An `m.room.message` with `msgtype: m.image` contains a BLAKE3 content hash rather than (or in addition to) an mxc:// URI. Any peer who has seen the image can serve it. The DHT maps content hashes to peer lists who hold the data.

**Anti-entropy**: Peers periodically exchange their tip hashes for shared rooms and request any missing events. This heals network partitions — if two groups of peers were disconnected, when they reconnect they reconcile their DAGs automatically.

---

### Layer 6: Relay Protocol — Offline Delivery

The hardest problem in P2P messaging: what happens when the recipient is offline?

**Relay nodes** are peers that volunteer to store encrypted message payloads for offline recipients. They know nothing about the content — only the recipient AgentID, an encrypted blob, and a TTL.

**Relay payload**:
```
RelayPayload {
    recipient:  AgentId,              // who this is for (DHT lookup key)
    ciphertext: Bytes,                // X25519-encrypted event batch
                                      // (recipient's identity public key)
    nonce:      [u8; 24],             // XChaCha20-Poly1305 nonce
    ttl:        u32,                  // seconds until relay may discard
    depositor:  AgentId,              // who deposited this (for spam control)
    deposit_sig: Ed25519Sig,          // signed by depositor (anti-spam)
    relay_receipt: Ed25519Sig,        // relay's sig — proof of acceptance
}
```

**Relay discovery**: Relays advertise capacity in the DHT under `"relay:" + their AgentID`. A sender discovers relays by looking up the recipient's IdentityDocument (which lists their preferred relays) and falling back to a DHT query for available relay capacity.

**Delivery flow**:
1. Sender creates and signs events normally
2. If recipient is offline (no DHT presence or connection fails), sender encrypts the event batch to recipient's X25519 key and deposits it at one or more relays
3. When recipient comes online, they fetch any pending relay payloads keyed to their AgentID, decrypt, and merge events into their local DAGs
4. Relay deletes the payload after delivery or TTL expiry

**Spam/abuse control**: Relay nodes decide their own policies. Default policy: accept deposits from AgentIDs the operator or any of their contacts have interacted with (web-of-trust). Deposits require a valid Ed25519 signature from the depositor. Rate limits per depositor AgentID.

**Running a relay**: `agora relay` — any peer with a stable public address and available storage can volunteer. Relays gain no special privileges in the network. They are compensated by community goodwill; future versions may implement a lightweight credit system for relay capacity.

---

### Layer 7: Room Authority Without a Server

In the current architecture, the server enforces room permissions. In AMP, authority is distributed:

**Room key**: The room genesis event contains an Ed25519 public key (`room_key`). The corresponding private key is held by the room creator (and can be transferred). Operations that require authority — invite, kick, ban, promote to admin, change room settings — must be signed by the room key, not merely by a member's AgentID.

**Power level events** are standard state events (`m.room.power_levels`) signed by the room key. Clients validate power levels locally before applying state changes. An event purporting to change power levels without the room key's signature is rejected.

**Membership transitions**:
- **Invite**: room key holder signs an `m.room.member` event with `membership: invite` targeting a recipient AgentID. The recipient's client receives this (via relay if offline), and the user can accept or decline.
- **Join**: the invited user signs their own `m.room.member { membership: join }` event. Valid only if a corresponding invite exists in the DAG.
- **Kick/ban**: room key signs `m.room.member { membership: leave/ban }` for the target user. Other peers' clients treat events from that AgentID as excluded from the room.

**Room key delegation**: A room creator can rotate the room key by publishing a `m.room.key_rotation` state event signed by the old key containing the new public key. The sigchain of key rotations is itself a Merkle chain, auditable by any peer.

---

### Migration Path from Current Architecture

The transition is additive — the client-server protocol does not go away, it becomes a compatibility layer.

**Phase 0 (today)**: Client-server, single homeserver, SQLite. ← *we are here*

**Phase 1 (LAN Mesh)**: mDNS peer discovery + QUIC direct transport. No DHT, no internet P2P. Two users on the same LAN or VPN (Tailscale/ZeroTier) can communicate without running `agora-server`. Server remains supported as the internet connectivity option. Users get: `agora connect --local` for same-LAN use, no server required.

**Phase 2 (Identity Sovereignty)**: AgentID becomes the canonical user identity independent of any server. Clients can import/export identity keys. IdentityDocuments published to DHT. Existing `agora-server` users can claim their AgentID locally. User IDs gain a `@display-name` local representation backed by AgentID, decoupled from `:servername` suffix.

**Phase 3 (Internet P2P)**: Kademlia DHT, NAT traversal, full peer discovery. Users can communicate directly over the internet without any server, so long as NAT traversal succeeds. Relay protocol launches for offline delivery. Server nodes become optional relay/indexer super-peers.

**Phase 4 (Event DAG)**: Room history migrated from server-linear to Merkle DAG format. Servers that want to remain compatible adopt the DAG storage format. Existing rooms can be exported as DAGs and imported by peers. CRDT state resolution replaces server-authoritative state.

**Phase 5 (Full Mesh)**: `agora-server` becomes `agora-node` — a configuration of the same binary that enables relay, indexing, and DHT participation. There is no meaningful distinction between a "client" and a "server" at the protocol level. The network is fully peer-to-peer.

---

### What Existing Agora Servers Become

`agora-server` operators are not left behind. Their nodes become valuable participants:

| Role | Description |
|------|-------------|
| **Super-peer** | High-bandwidth, always-online DHT node — improves routing for the whole network |
| **Relay node** | Stores encrypted payloads for offline peers; earns trust reputation |
| **Bootstrap node** | Entry point for new peers joining the DHT |
| **Archive node** | Stores full room DAGs for rooms their users participate in; serves history to peers that missed events |
| **Compatibility gateway** | Bridges AMP ↔ Matrix federation for users still on standard Matrix homeservers |
| **Legacy C2S host** | Still serves the Matrix C2S API for older Agora clients and Matrix-compatible clients during transition |

Operators who run a server today gain all of these roles automatically as phases roll out, with no migration required on their part.

---

### Implementation Plan

#### Phase 1: LAN Mesh — "zero-server local"
- Add `agora-p2p` crate with `quinn` (QUIC) transport
- Add `mdns-sd` crate for local peer discovery
- Implement `AmpMessage` wire protocol (CBOR over QUIC)
- Direct P2P room event exchange between LAN peers
- `agora connect --local` discovers and connects without a server
- Client falls back to server if no LAN peers found
- **Verification**: two machines on same LAN exchange messages with no server running

#### Phase 2: Identity Sovereignty
- AgentID key export/import (`agora identity export/import`)
- `IdentityDocument` type in `agora-crypto`
- DHT stub (in-memory for now, seeded by known peers)
- Decouple user display from `:servername` in UI
- Settings panel shows full AgentID, not just server-scoped device ID
- **Verification**: user migrates identity from server to P2P client, messages verified by AgentID

#### Phase 3: Internet P2P
- Kademlia DHT implementation (or integrate `libp2p-kad` via `libp2p`)
- STUN integration for NAT address discovery (`stun` crate)
- QUIC hole punching via coordinator peer
- Circuit relay fallback (any peer with public IP can relay)
- Bootstrap peer list hardcoded + DNS SRV discovery
- `agora relay` subcommand — volunteer as relay/bootstrap node
- **Verification**: two users on different networks with NAT communicate directly

#### Phase 4: Event DAG and CRDT State
- `SignedEvent` with parent hashes replaces server-ordered event sequence in `agora-crypto`
- Topological sort + Lamport ordering for display
- CRDT merge for state events (BTreeMap of `(event_type, state_key) → SignedEvent`)
- Room genesis event and `room_key` authority model
- Membership transitions validated locally without server
- Server-side storage migrated to DAG format; existing rooms exported/imported as DAGs
- **Verification**: room created in P2P mode, two peers independently compute identical state

#### Phase 5: Relay Network
- `RelayPayload` type and relay protocol in `agora-p2p`
- Relay DHT advertisement and discovery
- Offline delivery: deposit at relay on send failure, fetch on reconnect
- Relay storage backend (SQLite, same pattern as `agora-server`)
- `agora node` unified binary: client + relay + DHT participation all configurable
- **Verification**: message delivered to offline peer within 30s of them coming online

---

### Prior Art and Design References

- **Kademlia** — Maymounkov & Mazières, 2002. The DHT algorithm underlying most P2P systems (BitTorrent, IPFS, Ethereum).
- **Secure Scuttlebutt (SSB)** — append-only log per identity, gossip replication, offline-first. Proof that social communication can work without servers.
- **Briar** — Tor + Bluetooth + WiFi direct, full P2P messaging. Proof that NAT traversal and offline delivery are solvable.
- **libp2p** — modular P2P networking stack used by IPFS and Ethereum. Rust implementation (`rust-libp2p`) is production-grade. AMP can either use libp2p directly or implement the same concepts with leaner dependencies.
- **Signal Protocol** — Double Ratchet + X3DH, which Agora already implements. Designed explicitly for P2P key establishment without a server-trusted key server.
- **CRDT literature** — Shapiro et al., "A Comprehensive Study of Convergent and Commutative Replicated Data Types", 2011. State-based CRDTs for room state; operation-based CRDTs for event DAG merge.
- **Matrix DAG** — Matrix's room model is already a DAG at the protocol level; the homeserver just linearizes it for clients. AMP exposes the DAG directly.

---

### Open Questions

1. **Incentives for relay operators**: Community goodwill works at small scale. At larger scale, relay nodes bear real storage and bandwidth costs. A lightweight credit system (signed receipts redeemable for relay capacity) could balance this without requiring a blockchain or token.

2. **Sybil resistance in the DHT**: Any peer can join the DHT. Without a cost to creating identities, an adversary can flood the routing table. Mitigations: proof-of-work for DHT participation, web-of-trust routing (prefer peers vouched for by known agents), or simple rate limiting.

3. **Key recovery**: If a user loses their AgentID private key, their identity is gone. No server can recover it. Social key recovery (m-of-n friends co-sign a key rotation) is the decentralized answer, but UX is hard.

4. **Moderation in serverless rooms**: Room key holders can kick/ban, but they cannot delete events already replicated to other peers. Deleted events remain in peers' local stores. This is a fundamental property of append-only P2P systems. Content moderation at scale requires out-of-band mechanisms (blocklists, reputation systems).

5. **Long-term archive availability**: Events exist as long as at least one peer holds them. If all members of a room go offline permanently, the room history is lost. Archive nodes (well-resourced super-peers) mitigate this, but there is no guarantee equivalent to a server with a backup strategy.

6. **Protocol ossification**: Once the event DAG format is deployed, changing it requires a hard fork of the network. The genesis event should include a protocol version; migration paths between versions must be designed carefully before Phase 4 ships.

---

## Remaining Items for Element/Standard Client Compatibility

The following features would be needed for full compatibility with standard Matrix clients like Element, FluffyChat, etc. These are lower priority given the AMP direction above but remain valuable for interoperability during the transition period.

### Filters

- `POST /user/{userId}/filter` — create server-side filter
- `GET /user/{userId}/filter/{filterId}` — retrieve filter
- `filter` parameter on `/sync` — apply filter to sync response
- `filter` parameter on `/messages` — apply RoomEventFilter

### Room Moderation

- `POST /rooms/{roomId}/kick` — kick a user from a room
- `POST /rooms/{roomId}/ban` — ban a user from a room
- `POST /rooms/{roomId}/unban` — unban a user
- Power level enforcement on all state-changing operations
- `m.room.power_levels` state event processing and validation

### Presence

- `PUT /presence/{userId}/status` — set presence status (online/offline/unavailable)
- `GET /presence/{userId}/status` — get presence status
- `set_presence` query parameter on `/sync`
- `presence` section in sync response with `m.presence` events
- Automatic idle/offline detection based on inactivity

### Read Receipts

- `POST /rooms/{roomId}/receipt/{receiptType}/{eventId}` — send read receipt
- Include receipts in `ephemeral.events` section of sync response
- `m.read` and `m.read.private` receipt types
- Fully-read markers (`m.fully_read` account data)

### Account Data

- `PUT /user/{userId}/account_data/{type}` — set global account data
- `GET /user/{userId}/account_data/{type}` — get global account data
- `PUT /user/{userId}/rooms/{roomId}/account_data/{type}` — per-room account data
- `GET /user/{userId}/rooms/{roomId}/account_data/{type}` — get per-room account data
- Include `account_data` section in sync response (global and per-room)

### Room Versioning

- Room version field in `m.room.create` content
- `POST /rooms/{roomId}/upgrade` — upgrade room to a new version
- Proper handling of room version capabilities

### E2E Encryption Enhancements

- `device_lists` section in sync response (changed/left device tracking)
- Device list change notifications when users join/leave rooms
- Cross-signing (master/self-signing/user-signing keys)
- Key backup (`/room_keys/` endpoints)
- Key sharing (`m.room_key_request`, `m.forwarded_room_key`)
- Encrypted attachments (AES-CTR file encryption)
- Verification (SAS, QR code)

### Matrix Federation (Server-to-Server API)

- `/_matrix/federation/v1/` endpoints
- Server key management and signing
- Event authorization and state resolution
- Backfill from remote servers
- Room joins via federation
- *Note: in the AMP context, existing `agora-server` nodes can serve as AMP ↔ Matrix bridges*

### Authentication Enhancements

- User-Interactive Authentication (UIAA) for `/register` and sensitive operations
- `m.login.token` login type
- SSO/OAuth2 login flow
- Refresh tokens (`refresh_token` in login response)
- Password change (`POST /account/password`)

### Push Notifications

- `GET /pushers` — list push notification targets
- `POST /set/pusher` — configure push notifications
- `GET /pushrules/` — get push rules
- `PUT /pushrules/` — set push rules
- Push gateway integration

### Content Repository Enhancements

- `GET /media/v3/thumbnail/{serverName}/{mediaId}` — thumbnailing
- Content-Type validation
- Media quarantine
- URL previews (`GET /media/v3/preview_url`)

### Room Alias Resolution in /join

- Resolve `#alias:server` to room_id in the `/join/{roomIdOrAlias}` endpoint
- Support `server_name` parameter for joining via alias

### Full-Text Search

- `POST /search` — full-text message search
- Search by room, sender, and content

### Third-Party Networks

- `GET /thirdparty/protocols` — list bridged protocols
- Application service (bridge) registration

---

## Agora-Native Feature Backlog

Features beyond Matrix compatibility — Agora-specific enhancements on the near and medium horizon.

### Near-Term
- Emoji reactions (Discord-style, per-message)
- User avatars and viewable profiles (click username to view)
- User status and presence indicators (online/idle/away/custom)
- Video inline playback (upload already works; player UI pending)
- Per-space custom uploadable reactions

### Medium-Term
- Voice chat (WebRTC or equivalent)
- Sigchain verification UI — click ⛓ badge to inspect and verify the full action chain
- Thread / reply support (Matrix `m.relates_to` with `m.thread`)
- Markdown rendering in messages
- Notification badges on room list (unread counts)
- Message editing and deletion (redaction)

### Longer-Term (post-AMP Phase 1+)
- Sigchain-attested agent actions: LLM agents that act on behalf of users with full auditability via sigchain
- Social key recovery: m-of-n trusted contacts co-sign identity key rotation
- Mobile clients (iOS/Android via Tauri Mobile)
- Reputation and web-of-trust graph: vouch for other AgentIDs, inform relay and DHT routing policy
- AMP ↔ Matrix bridge: `agora-node` instances that bridge the P2P network to the wider Matrix federation

---

## Feature Priorities

1. **Immediate** — Dogfood blockers: room creation fixes (stream_ordering, timestamp sequence) just landed, image rendering verification, E2E limitation documentation
2. **High** — AMP Phase 1 (LAN mesh, zero-server local), read receipts, moderation, account data
3. **Medium** — AMP Phase 2 (identity sovereignty), presence, emoji reactions, user profiles/avatars, E2E enhancements
4. **Low** — AMP Phase 3+ (internet P2P, DAG, relay), Matrix federation, push notifications, SSO, full-text search

---

## The World Tree — Long-Range Architecture Vision

> Canonical specification: `SOVEREIGN/.sovereign/docs/grand-plan-distributed-resource-economy.md`
> Near-term plan: `docs/plans/2026-03-05-kos-grand-plan.md`

### The Core Insight

`agora-crypto` derives `AgentId = BLAKE3(Ed25519 verifying_key)`. Yggdrasil network derives its globally-unique IPv6 node address from that same Ed25519 public key. **Same seed. Same key. Two addresses.** Agora's identity system is natively Yggdrasil-compatible — no refactor, just one derivation function.

Yggdrasil is not a future phase. **It is Phase 1.** Introduced alongside (and largely replacing) the raw QUIC + custom TLS cert pinning machinery. The existing QUIC code was a stepping stone. Yggdrasil is the World Tree.

### Revised Transport Hierarchy

```
mDNS          — zero-config, LAN, always available, zero deps
Yggdrasil     — primary WAN, E2EE at network layer, no port config, no CA
DHT (Phase 6) — fallback WAN for nodes without Yggdrasil daemon
agora-server  — optional Matrix compat bridge, not in critical path
```

### What Yggdrasil Eliminates From `agora-p2p`

| Current burden | Yggdrasil handles it |
|---|---|
| Custom TLS cert generation (`rcgen`) | Network-layer E2EE, no per-connection TLS |
| `FingerprintVerifier` / `FingerprintStore` | Address IS identity — derived from pubkey |
| NAT traversal (unsolved) | Path-agnostic routing through mesh nodes |
| Relay peer coordination (unbuilt) | Free relay via other Yggdrasil participants |
| DHT for WAN discovery (unbuilt) | Address derivable directly from pubkey |

### Agora Deliverables by Phase

**Phase 1 — Yggdrasil Transport (THE WORLD TREE)**
- [ ] `agora-p2p/src/transport/yggdrasil.rs` — bind to Yggdrasil IPv6 interface
- [ ] `agora-p2p/src/identity/yggdrasil.rs` — `yggdrasil_addr_from_pubkey(VerifyingKey) -> Ipv6Addr`
- [ ] `TransportMode` enum: `{ Quic(QuicConfig), Yggdrasil(YggdrasilConfig), Auto }`
- [ ] Remove `FingerprintVerifier`, `FingerprintStore` (superseded by Yggdrasil auth)
- [ ] `P2pNode::start()` probes for Yggdrasil daemon; graceful fallback to QUIC on LAN
- [ ] Integration test: two Yggdrasil nodes discover and exchange `AmpMessage`

**Phase 3 — Agent Collaboration**
- [ ] `AmpMessage::CollaborationRequest { block_id, content, from, correlation_path }`
- [ ] `AmpMessage::CollaborationResponse { block_id, content, agent_id, proof: Option<ZkProof> }`

**Phase 4 — Fuel Sharing**
- [ ] `AmpMessage::FuelOffer { space_id, from_agent, fuel_tokens, provider, model_hint, expires_at }`
- [ ] `AmpMessage::FuelClaim { offer_id, claiming_agent, tokens_claimed, sigchain_action_id }`
- [ ] `AmpMessage::FuelReceipt { claim_id, accepted, tokens_granted }`

**Phase 6 — DHT Fallback (non-Yggdrasil nodes)**
- [ ] `agora-p2p/src/discovery/dht.rs` — Kademlia, Agora homeservers as bootstrap
- [x] `AmpMessage::PeerAnnounce { agent_id, addresses, ttl }`
- [ ] `P2pConfig::wan_discovery: WanDiscoveryMode`

**Phase 7 — Dispute Game (RFC-005)**
- [ ] `AmpMessage::DisputeOpen`, `DisputeEvidence`, `DisputeVerdict`
- [ ] `agora-crypto`: `Sigchain::export_range(from, to) -> Vec<SignedEntry>`
- [ ] `agora-crypto`: `Sigchain::verify_segment(entries) -> Result<(), SigchainError>`
- [ ] `SigchainBody::DisputeRefusal` link type

**Phase 8 — On-Chain Anchoring (RFC-007)**
- [ ] `AmpMessage::StakeProof { checkpoint, on_chain_tx }`
- [ ] `agora-crypto`: `Checkpoint::as_on_chain_anchor() -> AnchorPayload`

**Phase 9 — ZK Execution Proofs (RFC-006)**
- [ ] `AmpMessage::ExecutionProof { action_id, zk_proof }`
- [ ] ZK proof field in `SigchainBody::Action`

**Phase 10 — Compute Credit Economy**
- [ ] `AmpMessage::CreditTransfer { amount, from, to, signed_by }`
- [ ] `AmpMessage::CheckpointAnchor { merkle_root, on_chain_tx_id }`

### On `agora-server`

The homeserver remains available as an optional compatibility bridge:
- Users who need standard Matrix client interop (unencrypted rooms)
- Offline message queuing for mobile / intermittent connectivity
- Historical migration from existing Matrix deployments

It is not required for P2P operation. Not in the critical path. Feature flags keep it independent.

---

## Feature Priorities (Updated)

1. **Immediate** — Phase 0: Fix 4 P2P bugs (double-accept, stream loop, zero peer identity, stale test)
2. **High** — Phase 1: Yggdrasil transport adapter — this is the World Tree, this is the point
3. **High** — Phase 2: Atelier embeds agora-p2p; `surface.rs:45` wired; mesh node live
4. **Medium** — Phase 3–4: Agent collaboration, fuel sharing; dogfood blockers in parallel
5. **Medium** — Phase 5: SOVEREIGN daemon identity delegation
6. **Low (Near)** — Phase 6: DHT fallback for non-Yggdrasil nodes
7. **Low (Long)** — Phase 7–10: Dispute game, anchoring, ZK proofs, compute economy
