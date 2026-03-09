# Design: Pure DHT Bootstrap for Agora

## Overview

This design specifies how Agora achieves true internet-wide P2P connectivity without servers using DHT-based peer discovery and the rust_mesh transport.

## 1. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        P2pNode                               │
├─────────────────────────────────────────────────────────────┤
│  TransportMode enum:                                        │
│    - Quic (UDP QUIC, current)                               │
│    - Yggdrasil (external daemon)                            │
│    - RustMesh (NEW: pure Rust mesh transport)               │
│                                                              │
│  DiscoveryMode enum:                                         │
│    - Lan (mDNS, already works)                             │
│    - Wan(DhtProvider) ← NEW trait                          │
└─────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
   ┌────────────┐    ┌────────────────┐   ┌────────────┐
   │ RustMesh   │    │ External DHT   │   │  mDNS      │
   │ Transport  │    │ Provider       │   │  (existing)│
   │ (NEW)      │    │ (uses crate)   │   │            │
   └────────────┘    └────────────────┘   └────────────┘
                             │
                    ┌────────┴────────┐
                    ▼                 ▼
            ┌────────────┐    ┌────────────┐
            │ S2DhtStore │    │ Seed Nodes  │
            │ (BTreeMap) │    │ (hardcoded)│
            └────────────┘    └────────────┘
```

### Key Architectural Decisions

- `DhtProvider` trait allows swapping implementations without API changes
- Default implementation uses external crate, wraps storage with BTreeMap
- Seed nodes are just DHT bootstrap nodes — same protocol
- `TransportMode::RustMesh` added to enable pure Rust mesh networking

## 2. Bootstrap UX

### User Flow

```
User launches Agora
       │
       ▼
┌──────────────────┐
│ Connect to seeds │ ← 2-3 hardcoded seed addresses
│   (3 peers max)  │   - seeds.agora0.io:6881
└──────────────────┘   - seeds.agora1.io:6881
       │               - seeds.agora2.io:6881
       ▼
┌──────────────────┐
│ DHT: find_peer   │ ← Query seeds for known peers
│   "alice"        │
└──────────────────┘
       │
       ▼
┌──────────────────┐
│ Mesh: connect    │ ← Direct QUIC/RustMesh to peer
│   to peer        │   (hole-punch optional, E2EE always)
└──────────────────┘
```

### UX Details

1. App starts → connects to 3 seeds (background, non-blocking)
2. Seeds respond with known peers (from their DHT buckets)
3. P2pNode dials discovered peers → mesh grows
4. If seeds unreachable → continue with mDNS peers only (graceful degradation)

### Seed Configuration

- Default seeds: hardcoded in config
- Custom seeds: configurable via `agora.toml`
- Users can run their own seed nodes (documented)

## 3. NAT Traversal

### Strategy: STUN + Optional Hole-Punch

1. **STUN Client** (~50 lines)
   - Detect public IP via STUN server
   - Store: `public_ip: Option<SocketAddr>`

2. **Hole-Punch Attempt** (UDP only)
   - Both peers send UDP to each other simultaneously
   - Works for ~60-70% of NAT types (full-cone, restricted-cone)
   - Fails gracefully for symmetric NAT

3. **Connection Health Scoring**
   ```
   BTreeMap<AgentId, ConnectionScore>
   - score increases on successful connect
   - score decreases on failure
   - periodic retry every 5 minutes
   ```

### Graceful Degradation

- Both behind NAT → try hole-punch → if fails, peer marked "unreachable"
- One has public IP → direct connect works
- No peers reachable → app still works on LAN via mDNS

## 4. rust_mesh Integration

### Current State

- `TransportMode` enum exists in `types.rs`
- QUIC is hardcoded in `P2pNode::new()`
- `RustMeshTransport` exists but not wired in

### Changes Required

```rust
// types.rs
pub enum TransportMode {
    Quic,
    Yggdrasil,
    RustMesh,  // NEW
    Auto,
}
```

```rust
// In P2pNode::new():
let transport = match config.transport_mode {
    TransportMode::Quic => Arc::new(QuicTransport::new(...)?) as Arc<dyn Transport>,
    TransportMode::Yggdrasil => Arc::new(YggdrasilTransport::new(...)?) as Arc<dyn Transport>,
    TransportMode::RustMesh => Arc::new(RustMeshTransport::new(...)?) as Arc<dyn Transport>,
    TransportMode::Auto => /* existing fallback logic */,
};
```

### Transport Trait

Add minimal `Transport` trait:
```rust
trait Transport: Send + Sync {
    fn local_addr(&self) -> SocketAddr;
    fn connect(&self, peer: &AgentId, addr: SocketAddr) -> impl Future<Output = Result<Connection>>;
    fn accept(&self) -> impl Future<Output = Connection>;
}
```

## 5. DHT Implementation

### DhtProvider Trait

```rust
pub trait DhtProvider: Send + Sync {
    /// Find peers for an agent ID
    fn find_peer(&self, agent_id: &AgentId) -> impl Future<Output = Result<Vec<Peer>>>;
    
    /// Announce our presence
    fn store_peer(&self, agent_id: &AgentId, addr: SocketAddr) -> impl Future<Output = Result<()>>;
    
    /// Bootstrap from seed nodes
    fn bootstrap(&self, seeds: &[SocketAddr]) -> impl Future<Output = Result<()>>;
}
```

### Implementation Strategy

**Option: External crate + BTreeMap wrapper**

```
┌─────────────────────────────────────────┐
│           DhtProvider trait              │
├─────────────────────────────────────────┤
│ fn find_peer(agent_id: AgentId)         │
│ fn store_peer(agent_id, addr)          │
│ fn bootstrap(seeds: &[SocketAddr])      │
└─────────────────────────────────────────┘
           △                    △
           │                    │
┌──────────┴───────┐   ┌────────┴────────┐
│ ExternalDhtImpl  │   │ CustomImpl     │
│ (uses dht crate) │   │ (future)       │
├──────────────────┤   ├──────────────────┤
│ S2DhtStore       │   │ BTreeMap       │
│ (wraps HashMap)  │   │ directly       │
└──────────────────┘   └──────────────────┘
```

### What's Needed

| Component | Lines | Notes |
|-----------|-------|-------|
| DhtProvider trait | ~50 | Define interface |
| ExternalDhtProvider | ~200 | Uses crate |
| S2DhtStore wrapper | ~100 | BTreeMap conversion |
| Wire into P2pNode | ~50 | Integration |
| Seed config | ~10 | TOML config |

### DHT Crate Selection Criteria

- Allows custom storage backend
- Minimal dependencies
- Actively maintained
- Evaluate: dht, krabbe, p2p

## 6. Timeline Estimate

| Phase | Work | Estimate |
|-------|------|----------|
| 1. Transport trait + RustMesh wiring | Add trait, wire RustMeshTransport | 1-2 days |
| 2. DHT provider trait | Define interface | 0.5 day |
| 3. External DHT integration | Wrap crate, add BTreeMap storage | 2-3 days |
| 4. Seed config + bootstrap | Add toml config, connect logic | 0.5 day |
| 5. NAT traversal (STUN + hole-punch) | STUN client, punch logic | 1-2 days |
| 6. Integration testing | Two friends across NAT | 1-2 days |

**Total: ~6-10 days (1-2 weeks)**

## 7. Configuration

### agora.toml

```toml
[p2p]
# Transport mode: quic, yggdrasil, rustmesh, auto
transport = "rustmesh"

# Discovery mode: lan, wan, both
discovery = "both"

[p2p.wan]
# Seed nodes for DHT bootstrap (leave empty for defaults)
seeds = []

# Enable NAT hole-punching
enable_hole_punch = true

# STUN servers (leave empty for defaults)
stun_servers = ["stun.l.google.com:19302"]
```

### Default Values

```rust
const DEFAULT_SEEDS: &[&str] = &[
    "seeds.agora0.io:6881",
    "seeds.agora1.io:6881", 
    "seeds.agora2.io:6881",
];

const DEFAULT_STUN: &[&str] = &[
    "stun.l.google.com:19302",
];
```

## 8. Security Considerations

### Threat Model

- **Seeds are "honest but curious"** — they help find peers, E2EE prevents spying
- **DHT only knows:** "Peer X is at IP:port" — no message content
- **Transport layer:** ChaCha20-Poly1305 E2EE (already implemented in rust_mesh)

### Mitigations

1. **Multiple DHT backends** — architecture supports swapping implementations
2. **Seed diversity** — anyone can run seeds, not controlled by us
3. **Connection verification** — peers must prove identity via sigchain
4. **Local reputation** — track peer reliability via connection scoring

## 9. Testing Strategy

### Unit Tests

- DhtProvider trait implementations
- S2DhtStore BTreeMap wrapper
- Transport trait implementations

### Integration Tests

- Two nodes on same machine connect via DHT
- Seed node can be started locally
- NAT traversal works with docker-nat simulation

### Manual Testing

- Two friends on different networks (home WiFi, mobile hotspot)
- Verify: peer discovery → connect → message exchange

## 10. Open Questions

1. **Seed hosting:** Who runs default seeds? (Recommend: host 2-3 on small VPS)
2. **DHT crate final choice:** Which crate best fits criteria?
3. **IPv6 preference:** Should we prefer IPv6 over IPv4 for peer connections?

---

**Status:** Design approved
**Created:** 2026-03-09
**Target:** v0.2.0 (P2P with DHT bootstrap)
