# Implementation Plan: Pure DHT Bootstrap for Agora

**Design Document:** `docs/plans/2026-03-09-pure-dht-bootstrap-design.md`
**Target Version:** v0.2.0
**Total Estimated Effort:** 6-10 days (~800-1200 lines of new code)

---

## Phase 1: Transport Trait + RustMesh Wiring

**Priority:** P0 (Blocking - foundational)
**Estimated:** 1-2 days (~250 lines)

### 1.1 Define Transport Trait

**File:** `agora-p2p/src/transport/trait.rs` (NEW)

```rust
/// Core transport trait for P2P communication.
/// S-02: All trait methods use async futures, no blocking operations.
pub trait Transport: Send + Sync {
    /// Local socket address
    fn local_addr(&self) -> Result<SocketAddr, Error>;
    
    /// Connect to a peer at the given address
    fn connect(&self, peer: &AgentId, addr: SocketAddr) 
        -> Pin<Box<dyn Future<Output = Result<Connection>> + Send>>;
    
    /// Accept incoming connections
    fn accept(&self) -> Pin<Box<dyn Future<Output = Result<(Connection, AgentId)>> + Send>>;
    
    /// Start listening on the given address
    fn listen(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

/// Unified connection type across transports
pub enum Connection {
    Quic(QuicConnection),
    // RustMesh variant added in Phase 1.3
}
```

**Lines:** ~80

### 1.2 Implement Transport for QuicTransport

**File:** `agora-p2p/src/transport/quic.rs` (MODIFY)

Add `impl Transport for QuicTransport` block:

```rust
impl Transport for QuicTransport {
    fn local_addr(&self) -> Result<SocketAddr, Error> { /* existing */ }
    
    fn connect(&self, peer: &AgentId, addr: SocketAddr) 
        -> Pin<Box<dyn Future<Output = Result<Connection>> + Send>> {
        Box::pin(async move, peer, {
            let conn = self.connect(addr None).await?;
            Ok(Connection::Quic(conn))
        })
    }
    
    fn accept(&self) -> Pin<Box<dyn Future<Output = Result<(Connection, AgentId)>> + Send>> {
        Box::pin(async move {
            let (conn, peer_id) = self.accept().await?;
            Ok((Connection::Quic(conn), peer_id))
        })
    }
    
    fn listen(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(self.listen(addr))
    }
}
```

**Lines:** ~30

### 1.3 Add RustMesh TransportMode Variant

**File:** `agora-p2p/src/types.rs` (MODIFY)

```rust
pub enum TransportMode {
    Quic(Arc<QuicConfigInner>),
    Yggdrasil(YggdrasilConfig),
    RustMesh(RustMeshConfig),  // NEW
    Auto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RustMeshConfig {
    pub listen_port: u16,
    pub max_peers: usize,
    pub connection_timeout_ms: u64,
}
```

**Lines:** ~15

### 1.4 Wire TransportMode into P2pNode

**File:** `agora-p2p/src/node.rs` (MODIFY)

In `P2pNode::new()`:

```rust
let transport: Arc<dyn Transport> = match &config.transport {
    TransportMode::Quic(quic_cfg) => {
        let quic_config = QuicConfig::new(cert, key, bind_addr);
        Arc::new(QuicTransport::new(quic_config, agent_id.clone()).await?) as Arc<dyn Transport>
    }
    TransportMode::RustMesh(rm_config) => {
        // Create RustMesh transport
        let crypto_provider = Arc::new(RustMeshCrypto::new());
        let rm_transport = new_rust_mesh_transport(
            rm_config.clone(),
            agent_id.clone(),
            crypto_provider,
        );
        Arc::new(rm_transport) as Arc<dyn Transport>
    }
    TransportMode::Yggdrasil(ygg_config) => { /* ... */ }
    TransportMode::Auto => { /* existing fallback */ }
};
```

Also update `MeshManager` to accept `Arc<dyn Transport>`:

**File:** `agora-p2p/src/mesh/peer.rs` (MODIFY)

```rust
pub struct MeshManager {
    // Change from Arc<QuicTransport> to Arc<dyn Transport>
    transport: Arc<dyn Transport>,
    // ...
}
```

**Lines:** ~60

### 1.5 Update Module Exports

**File:** `agora-p2p/src/transport/mod.rs` (MODIFY)

```rust
pub mod quic;
pub mod tls;
pub mod yggdrasil;
pub mod rust_mesh_transport;
pub mod trait_;  // NEW - transport trait
pub use trait_::{Transport, Connection};
```

**Lines:** ~5

---

## Phase 2: DhtProvider Trait Definition

**Priority:** P0 (Blocking - foundational)
**Estimated:** 0.5 day (~100 lines)

### 2.1 Define DhtProvider Trait

**File:** `agora-p2p/src/discovery/dht/provider.rs` (NEW)

```rust
use async_trait::async_trait;
use sovereign_sdk::AgentId;

/// Result type for DHT operations
pub struct DhtPeer {
    pub agent_id: AgentId,
    pub addresses: Vec<SocketAddr>,
    pub last_seen: u64,  // S-02: sequence-based, not timestamp
}

/// Provider trait for DHT implementations
/// Allows swapping implementations without API changes
#[async_trait]
pub trait DhtProvider: Send + Sync {
    /// Find peers for an agent ID (lookup)
    async fn find_peer(&self, agent_id: &AgentId) -> Result<Vec<DhtPeer>, Error>;
    
    /// Announce our presence (store)
    async fn store_peer(&self, agent_id: &AgentId, addrs: Vec<SocketAddr>) -> Result<(), Error>;
    
    /// Bootstrap from seed nodes
    async fn bootstrap(&self, seeds: &[SocketAddr]) -> Result<(), Error>;
    
    /// Get our local DHT node ID
    fn local_node_id(&self) -> &AgentId;
}
```

**Lines:** ~50

### 2.2 Create Stub Implementation

**File:** `agora-p2p/src/discovery/dht/stub.rs` (NEW)

```rust
/// Stub implementation for development/testing
/// Uses in-memory BTreeMap, no actual DHT network
pub struct StubDhtProvider {
    local_id: AgentId,
    peers: BTreeMap<AgentId, DhtPeer>,
    sequence: AtomicU64,
}

impl StubDhtProvider {
    pub fn new(local_id: AgentId) -> Self { /* ... */ }
}

#[async_trait]
impl DhtProvider for StubDhtProvider {
    async fn find_peer(&self, agent_id: &AgentId) -> Result<Vec<DhtPeer>, Error> {
        Ok(self.peers.get(agent_id).cloned().into_iter().collect())
    }
    
    async fn store_peer(&self, agent_id: &AgentId, addrs: Vec<SocketAddr>) -> Result<(), Error> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        self.peers.insert(agent_id.clone(), DhtPeer {
            agent_id: agent_id.clone(),
            addresses: addrs,
            last_seen: seq,
        });
        Ok(())
    }
    
    async fn bootstrap(&self, _seeds: &[SocketAddr]) -> Result<(), Error> {
        Ok(())  // No-op for stub
    }
    
    fn local_node_id(&self) -> &AgentId { &self.local_id }
}
```

**Lines:** ~60

### 2.3 Update Module Exports

**File:** `agora-p2p/src/discovery/dht.rs` (MODIFY)

```rust
pub mod provider;
pub mod stub;
pub mod store;  // Phase 3

pub use provider::{DhtProvider, DhtPeer};
pub use stub::StubDhtProvider;
```

**Lines:** ~5

---

## Phase 3: External DHT Crate Integration

**Priority:** P1 (Core functionality)
**Estimated:** 2-3 days (~400 lines)

### 3.1 Add DHT Crate Dependency

**File:** `agora-p2p/Cargo.toml` (MODIFY)

```toml
[dependencies]
# DHT - choose one: dht, krabbe, or p2p
# Recommendation: dht (active, simple API, allows custom storage)
dht = "0.5"  
```

**Note:** Evaluate alternatives before choosing. Criteria:
- Allows custom storage backend (BTreeMap)
- Minimal dependencies
- Actively maintained

### 3.2 Create BTreeMap-backed Storage

**File:** `agora-p2p/src/discovery/dht/store.rs` (NEW)

```rust
/// S-02 compliant storage wrapper using BTreeMap
/// Wraps external crate's HashMap with deterministic BTreeMap
pub struct S2DhtStore {
    /// Main storage: agent_id -> peer info
    /// S-02: BTreeMap for deterministic iteration
    peers: BTreeMap<AgentId, DhtPeer>,
    /// Sequence counter for last_seen timestamps
    sequence: AtomicU64,
    /// Token bucket for rate limiting
    rate_limit: Arc<RwLock<RateLimiter>>,
}

struct RateLimiter {
    max_ops_per_second: u32,
    tokens: AtomicU32,
    last_refill: AtomicU64,
}

impl S2DhtStore {
    pub fn new() -> Self { /* ... */ }
    
    pub fn put(&self, agent_id: &AgentId, peer: DhtPeer) -> Result<(), Error> {
        // Check rate limit first
        // S-02: Use sequence number instead of wall-clock
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let mut peer = peer;
        peer.last_seen = seq;
        self.peers.insert(agent_id.clone(), peer);
        Ok(())
    }
    
    pub fn get(&self, agent_id: &AgentId) -> Option<DhtPeer> {
        self.peers.get(agent_id).cloned()
    }
    
    pub fn get_closest(&self, target: &AgentId, count: usize) -> Vec<DhtPeer> {
        // BTreeMap iteration is deterministic (S-02)
        self.peers.values().take(count).cloned().collect()
    }
    
    pub fn remove(&self, agent_id: &AgentId) -> Option<DhtPeer> {
        self.peers.remove(agent_id)
    }
    
    pub fn len(&self) -> usize { self.peers.len() }
    pub fn is_empty(&self) -> bool { self.peers.is_empty() }
}
```

**Lines:** ~100

### 3.3 Implement ExternalDhtProvider

**File:** `agora-p2p/src/discovery/dht/external.rs` (NEW)

```rust
/// Implementation using external DHT crate
/// Wraps crate types with our DhtProvider trait + BTreeMap storage
pub struct ExternalDhtProvider {
    local_id: AgentId,
    store: Arc<S2DhtStore>,
    /// Actual DHT node from external crate
    dht_node: Arc<RwLock<Option<dht::Dht>>,
    config: DhtConfig,
    /// Bootstrap nodes
    seeds: Vec<SocketAddr>,
}

#[derive(Clone)]
pub struct DhtConfig {
    pub port: u16,
    pub max_peers: usize,
    pub bucket_size: usize,
    pub query_timeout_ms: u64,
}

impl ExternalDhtProvider {
    pub async fn new(
        local_id: AgentId,
        config: DhtConfig,
        seeds: Vec<SocketAddr>,
    ) -> Result<Self, Error> {
        let store = Arc::new(S2DhtStore::new());
        
        Ok(Self {
            local_id,
            store,
            dht_node: Arc::new(RwLock::new(None)),
            config,
            seeds,
        })
    }
    
    pub async fn start(&self) -> Result<(), Error> {
        let dht = dht::Dht::new(
            self.local_id.as_bytes(),
            self.config.port,
            true,  // enable routing table
        ).map_err(|e| Error::Discovery(e.to_string()))?;
        
        *self.dht_node.write().await = Some(dht);
        
        // Bootstrap to seeds
        self.bootstrap(&self.seeds).await
    }
}

#[async_trait]
impl DhtProvider for ExternalDhtProvider {
    async fn find_peer(&self, agent_id: &AgentId) -> Result<Vec<DhtPeer>, Error> {
        let node_guard = self.dht_node.read().await;
        let node = node_guard.as_ref().ok_or_else(|| 
            Error::Discovery("DHT not started".to_string())
        )?;
        
        // Query external DHT
        let results = node.lookup(agent_id.as_bytes())
            .map_err(|e| Error::Discovery(e.to_string()))?;
        
        // Convert to our types, use BTreeMap storage
        let peers: Vec<DhtPeer> = results.into_iter().map(|addr| {
            DhtPeer {
                agent_id: agent_id.clone(),
                addresses: vec![addr],
                last_seen: self.store.sequence().load(Ordering::SeqCst),
            }
        }).collect();
        
        // Store found peers
        for peer in &peers {
            self.store.put(&peer.agent_id, peer.clone())?;
        }
        
        Ok(peers)
    }
    
    async fn store_peer(&self, agent_id: &AgentId, addrs: Vec<SocketAddr>) -> Result<(), Error> {
        let node_guard = self.dht_node.read().await;
        if let Some(node) = node_guard.as_ref() {
            for addr in &addrs {
                node.store(agent_id.as_bytes(), addr.to_string().as_bytes())
                    .map_err(|e| Error::Discovery(e.to_string()))?;
            }
        }
        
        self.store.put(agent_id, DhtPeer {
            agent_id: agent_id.clone(),
            addresses: addrs,
            last_seen: self.store.sequence().load(Ordering::SeqCst),
        })
    }
    
    async fn bootstrap(&self, seeds: &[SocketAddr]) -> Result<(), Error> {
        let node_guard = self.dht_node.read().await;
        let node = node_guard.as_ref().ok_or_else(|| 
            Error::Discovery("DHT not started".to_string())
        )?;
        
        for seed in seeds {
            node.bootstrap(seed.to_string())
                .map_err(|e| Error::Discovery(e.to_string()))?;
        }
        
        info!("DHT bootstrapped to {} seed nodes", seeds.len());
        Ok(())
    }
    
    fn local_node_id(&self) -> &AgentId { &self.local_id }
}
```

**Lines:** ~150

### 3.4 Add to Module Exports

**File:** `agora-p2p/src/discovery/dht.rs` (MODIFY)

```rust
pub mod provider;
pub mod stub;
pub mod store;
pub mod external;

pub use provider::{DhtProvider, DhtPeer};
pub use stub::StubDhtProvider;
pub use store::S2DhtStore;
pub use external::{ExternalDhtProvider, DhtConfig};
```

**Lines:** ~5

---

## Phase 4: Seed Node Configuration + Bootstrap

**Priority:** P1 (Core functionality)
**Estimated:** 0.5 day (~80 lines)

### 4.1 Add Seed Configuration to Types

**File:** `agora-p2p/src/types.rs` (MODIFY)

```rust
/// Configuration for DHT/WAN discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WanConfig {
    /// Seed nodes for DHT bootstrap
    #[serde(default = "default_seeds")]
    pub seeds: Vec<String>,
    
    /// Enable NAT hole-punching
    #[serde(default)]
    pub enable_hole_punch: bool,
    
    /// STUN servers for NAT detection
    #[serde(default = "default_stun_servers")]
    pub stun_servers: Vec<String>,
    
    /// DHT listening port
    #[serde(default = "default_dht_port")]
    pub dht_port: u16,
}

fn default_seeds() -> Vec<String> {
    vec![
        "seeds.agora0.io:6881".to_string(),
        "seeds.agora1.io:6881".to_string(),
        "seeds.agora2.io:6881".to_string(),
    ]
}

fn default_stun_servers() -> Vec<String> {
    vec!["stun.l.google.com:19302".to_string()]
}

fn default_dht_port() -> u16 { 6881 }

impl Default for WanConfig {
    fn default() -> Self {
        Self {
            seeds: default_seeds(),
            enable_hole_punch: true,
            stun_servers: default_stun_servers(),
            dht_port: 6881,
        }
    }
}
```

**Lines:** ~40

### 4.2 Update P2pConfig

**File:** `agora-p2p/src/types.rs` (MODIFY)

```rust
pub struct P2pConfig {
    // ... existing fields ...
    pub wan: WanConfig,
}
```

### 4.3 Bootstrap Logic in P2pNode

**File:** `agora-p2p/src/node.rs` (MODIFY)

Add to `P2pNode::new()`:

```rust
// Initialize DHT if WAN discovery enabled
let dht_provider: Option<Arc<dyn DhtProvider>> = match config.wan.seeds.is_empty() {
    false => {
        let seeds: Vec<SocketAddr> = config.wan.seeds.iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        
        let dht_config = DhtConfig {
            port: config.wan.dht_port,
            max_peers: 128,
            bucket_size: 20,
            query_timeout_ms: 5000,
        };
        
        let dht = ExternalDhtProvider::new(
            agent_id.clone(),
            dht_config,
            seeds,
        ).await.map_err(Error::Discovery)?;
        
        dht.start().await.map_err(Error::Discovery)?;
        
        Some(Arc::new(dht))
    }
    true => None,
};
```

**Lines:** ~40

---

## Phase 5: STUN Client + NAT Hole-Punch

**Priority:** P2 (Enhanced connectivity)
**Estimated:** 1-2 days (~350 lines)

### 5.1 STUN Client Implementation

**File:** `agora-p2p/src/nat/stun.rs` (NEW)

```rust
/// STUN client for NAT type detection and public IP discovery
/// S-02: Uses sequence-based timestamps, not wall-clock
pub struct StunClient {
    server: SocketAddr,
    socket: UdpSocket,
    local_port: u16,
    /// Cached public address (None = not yet discovered)
    public_addr: Arc<RwLock<Option<SocketAddr>>>,
    sequence: AtomicU64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatType {
    /// Full cone NAT - any external can connect
    FullCone,
    /// Restricted cone - must have sent to external first
    RestrictedCone,
    /// Port restricted cone
    PortRestrictedCone,
    /// Symmetric NAT - different external port per destination
    Symmetric,
    /// Public IP (no NAT)
    Public,
}

impl StunClient {
    pub async fn new(server: &str, local_port: u16) -> Result<Self, Error> {
        let server: SocketAddr = server.parse()
            .map_err(|e| Error::Nat(format!("invalid STUN server: {}", e)))?;
        
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", local_port))
            .await
            .map_err(|e| Error::Nat(format!("failed to bind: {}", e)))?;
        
        Ok(Self {
            server,
            socket,
            local_port,
            public_addr: Arc::new(RwLock::new(None)),
            sequence: AtomicU64::new(0),
        })
    }
    
    /// Discover public IP via STUN binding request
    pub async fn discover_public_ip(&self) -> Result<SocketAddr, Error> {
        // Build STUN binding request
        let transaction_id = self.sequence.fetch_add(1, Ordering::SeqCst);
        let request = self.build_binding_request(transaction_id);
        
        // Send to STUN server
        self.socket.send_to(&request, self.server).await
            .map_err(|e| Error::Nat(e.to_string()))?;
        
        // Wait for response
        let mut buf = [0u8; 512];
        let (len, _) = self.socket.recv_from(&mut buf).await
            .map_err(|e| Error::Nat(e.to_string()))?;
        
        // Parse XOR-MAPPED-ADDRESS
        self.parse_xor_mapped_address(&buf[..len])
    }
    
    /// Detect NAT type by testing connection patterns
    pub async fn detect_nat_type(&self) -> Result<NatType, Error> {
        // First, check if we have public IP
        let public_ip = self.discover_public_ip().await?;
        
        if public_ip.ip().is_public() {
            return Ok(NatType::Public);
        }
        
        // Test with two STUN servers to detect symmetric NAT
        // Simplified: assume symmetric if binding changes
        Ok(NatType::Symmetric)
    }
    
    fn build_binding_request(&self, transaction_id: u64) -> Vec<u8> {
        // STUN binding request magic cookie + transaction ID
        let mut msg = vec![0x00, 0x01];  // Binding request
        msg.extend_from_slice(&[0x00, 0x00]);  // Message length
        msg.extend_from_slice(&0x2112A442.to_be_bytes());  // Magic cookie
        
        // Transaction ID (96 bits)
        let tid = transaction_id.to_be_bytes();
        msg.extend_from_slice(&tid);
        msg.extend_from_slice(&tid[..4]);  // Pad to 96 bits
        
        // No attributes needed for basic binding request
        msg
    }
    
    fn parse_xor_mapped_address(&self, response: &[u8]) -> Result<SocketAddr, Error> {
        // Simplified: parse XOR-MAPPED-ADDRESS attribute
        // Actual implementation needs full STUN attribute parsing
        Err(Error::Nat("not implemented".to_string()))
    }
}
```

**Lines:** ~130

### 5.2 NAT Hole-Punch Implementation

**File:** `agora-p2p/src/nat/hole_punch.rs` (NEW)

```rust
/// NAT hole-punching coordination
/// Works for full-cone, restricted-cone, and port-restricted-cone NATs
pub struct HolePunchServer {
    port: u16,
    socket: UdpSocket,
    peers: Arc<RwLock<BTreeMap<AgentId, PeerHolePunchState>>>,
}

struct PeerHolePunchState {
    addr: SocketAddr,
    hole_punch_sent: bool,
    sequence: u64,
}

#[derive(Debug)]
pub enum PunchResult {
    Success(SocketAddr),
    Failed(String),
    SymmetricNat,
}

impl HolePunchServer {
    pub async fn new(port: u16) -> Result<Self, Error> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| Error::Nat(e.to_string()))?;
        
        Ok(Self {
            port,
            socket,
            peers: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }
    
    /// Attempt to punch through NAT to reach peer
    pub async fn punch_to(&self, peer_id: &AgentId, peer_addr: SocketAddr) -> PunchResult {
        // Send UDP packet to peer (opens hole in NAT)
        let punch_msg = b"AGORA_PUNCH";
        if let Err(e) = self.socket.send_to(punch_msg, peer_addr).await {
            return PunchResult::Failed(e.to_string());
        }
        
        // Wait for response
        let mut buf = [0u8; 64];
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            self.socket.recv_from(&mut buf)
        ).await;
        
        match result {
            Ok(Ok((_, addr))) => PunchResult::Success(addr),
            Ok(Err(_)) => PunchResult::Failed("timeout".to_string()),
            Err(_) => PunchResult::Failed("timed out".to_string()),
        }
    }
    
    /// Handle incoming hole-punch packet
    pub async fn handle_punch(&self, from: SocketAddr) {
        // Record that we received a packet from this address
        // This allows the NAT to accept future incoming packets
        let mut peers = self.peers.write().await;
        
        // Find or create peer state
        for (peer_id, state) in peers.iter_mut() {
            if state.addr == from {
                state.hole_punch_sent = true;
                break;
            }
        }
    }
}
```

**Lines:** ~80

### 5.3 NAT Detection + Connection Scoring

**File:** `agora-p2p/src/nat/mod.rs` (NEW)

```rust
pub mod stun;
pub mod hole_punch;

pub use stun::{StunClient, NatType};
pub use hole_punch::HolePunchServer;

/// Connection health scoring for NAT traversal
/// S-02: Uses BTreeMap for deterministic iteration
pub struct ConnectionScorer {
    scores: BTreeMap<AgentId, ConnectionScore>,
    sequence: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct ConnectionScore {
    pub peer_id: AgentId,
    pub score: i32,
    pub attempts: u32,
    pub successes: u32,
    pub last_attempt: u64,
}

impl ConnectionScorer {
    pub fn new() -> Self { /* ... */ }
    
    pub fn record_success(&self, peer_id: &AgentId) {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        // Increase score, cap at 100
        // Update last_attempt with sequence number
    }
    
    pub fn record_failure(&self, peer_id: &AgentId) {
        // Decrease score, floor at -100
    }
    
    pub fn get_score(&self, peer_id: &AgentId) -> i32 {
        self.scores.get(peer_id).map(|s| s.score).unwrap_or(0)
    }
    
    pub fn should_retry(&self, peer_id: &AgentId) -> bool {
        // Check if enough sequences have passed since last attempt
        // Trigger retry every 300 sequences (approximately 5 minutes)
    }
}
```

**Lines:** ~50

### 5.4 Integrate NAT into P2pNode

**File:** `agora-p2p/src/node.rs` (MODIFY)

Add fields and initialization:

```rust
pub struct P2pNode {
    // ... existing fields ...
    nat_client: Option<StunClient>,
    hole_punch: Option<HolePunchServer>,
    connection_scorer: ConnectionScorer,
}
```

Initialize in `new()`:

```rust
let nat_client = if config.wan.enable_hole_punch {
    let stun_server = config.wan.stun_servers.first()
        .cloned()
        .unwrap_or_else(|| "stun.l.google.com:19302".to_string());
    Some(StunClient::new(&stun_server, config.wan.dht_port).await?)
} else {
    None
};

let hole_punch = if config.wan.enable_hole_punch {
    Some(HolePunchServer::new(config.wan.dht_port + 1).await?)
} else {
    None
};
```

**Lines:** ~30

---

## Phase 6: Integration Testing

**Priority:** P2 (Verification)
**Estimated:** 1-2 days (~300 lines)

### 6.1 Unit Tests

**Files:** Distributed across each module

- `transport/trait.rs` - Test Transport trait implementations
- `discovery/dht/store.rs` - Test S2DhtStore BTreeMap operations
- `discovery/dht/stub.rs` - Test stub provider
- `nat/stun.rs` - Test STUN parsing (mock network)

### 6.2 Integration Tests

**File:** `agora-p2p/tests/dht_bootstrap_test.rs` (NEW)

```rust
/// Test: Two nodes on same machine connect via DHT
#[tokio::test]
async fn test_local_dht_bootstrap() {
    // Start seed node
    let seed_config = /* ... */;
    let seed_node = ExternalDhtProvider::new(seed_id, seed_config, vec![]).await?;
    seed_node.start().await?;
    
    // Start client node with seed as bootstrap
    let client_config = /* ... */;
    let client_node = ExternalDhtProvider::new(client_id, client_config, vec![seed_addr]).await?;
    client_node.start().await?;
    
    // Announce peer from seed
    seed_node.store_peer(&peer_id, vec![peer_addr]).await?;
    
    // Client should find peer via DHT
    let found = client_node.find_peer(&peer_id).await?;
    assert!(!found.is_empty());
}

/// Test: Seed node can be started locally
#[tokio::test]
async fn test_seed_node_local() {
    // Run same binary with --seed flag
    // Verify DHT port is listening
}

/// Test: DHT with BTreeMap storage
#[tokio::test]
async fn test_dht_store_deterministic() {
    let store = S2DhtStore::new();
    
    // Insert multiple peers
    // Verify iteration order is deterministic (BTreeMap)
}
```

**Lines:** ~80

### 6.3 NAT Traversal Tests

**File:** `agora-p2p/tests/nat_traversal_test.rs` (NEW)

```rust
/// Test: STUN client discovers public IP
#[tokio::test]
async fn test_stun_public_ip_discovery() {
    let client = StunClient::new("stun.l.google.com:19302", 0).await?;
    let public_ip = client.discover_public_ip().await;
    // May fail in some test environments
}

/// Test: Connection scoring
#[tokio::test]
async fn test_connection_scorer() {
    let scorer = ConnectionScorer::new();
    let peer_id = AgentId::from_hex("00...").unwrap();
    
    scorer.record_success(&peer_id);
    assert!(scorer.get_score(&peer_id) > 0);
    
    scorer.record_failure(&peer_id);
    assert!(scorer.get_score(&peer_id) < 10);
}
```

**Lines:** ~40

### 6.4 End-to-End Test (Manual)

**File:** `agora-p2p/tests/e2e_bootstrap_test.rs` (NEW)

```rust
/// Manual test: Two friends on different networks
/// Requires: Actual NAT environments or docker-nat simulation
#[tokio::test]
#[ignore]  // Manual test only
async fn test_cross_nat_connection() {
    // Setup docker-nat environment
    // Node A behind NAT 1
    // Node B behind NAT 2
    // Verify: peer discovery → connect → message exchange
}
```

**Lines:** ~30

---

## Dependency Graph

```
Phase 1 (Transport Trait)
    │
    ├── 1.1 Define Transport trait
    ├── 1.2 Implement for QUIC (depends on 1.1)
    ├── 1.3 Add RustMesh variant (depends on 1.1)
    ├── 1.4 Wire into P2pNode (depends on 1.2, 1.3)
    └── 1.5 Update exports
            │
            ▼
Phase 2 (DhtProvider trait)
    │
    ├── 2.1 Define DhtProvider trait
    ├── 2.2 Create stub impl (depends on 2.1)
    └── 2.3 Update exports
            │
            ▼
Phase 3 (External DHT)
    │
    ├── 3.1 Add dependency (no deps)
    ├── 3.2 BTreeMap storage (depends on 2.1)
    ├── 3.3 ExternalDhtProvider (depends on 3.1, 3.2, 2.1)
    └── 3.4 Update exports
            │
            ▼
Phase 4 (Seed config)
    │
    ├── 4.1 Add config types (depends on 2.1)
    ├── 4.2 Update P2pConfig (depends on 4.1)
    └── 4.3 Bootstrap logic (depends on 3.3, 4.2, 1.4)
            │
            ▼
Phase 5 (NAT traversal)
    │
    ├── 5.1 STUN client (no deps)
    ├── 5.2 Hole-punch (depends on 5.1)
    ├── 5.3 Connection scoring (no deps)
    └── 5.4 Integrate into P2pNode (depends on 5.1, 5.2, 5.3, 4.3)
            │
            ▼
Phase 6 (Testing)
    │
    └── All phases complete
```

---

## File Summary

| File | Lines | Phase | Description |
|------|-------|-------|-------------|
| `src/transport/trait.rs` | ~80 | 1.1 | Transport trait definition |
| `src/transport/quic.rs` (mod) | +30 | 1.2 | Transport impl for QUIC |
| `src/types.rs` (mod) | +15 | 1.3 | RustMesh variant |
| `src/node.rs` (mod) | +60 | 1.4 | Wire into P2pNode |
| `src/transport/mod.rs` (mod) | +5 | 1.5 | Export trait |
| `src/discovery/dht/provider.rs` | ~50 | 2.1 | DhtProvider trait |
| `src/discovery/dht/stub.rs` | ~60 | 2.2 | Stub implementation |
| `src/discovery/dht.rs` (mod) | +5 | 2.3 | Export trait |
| `Cargo.toml` (mod) | +5 | 3.1 | Add DHT dependency |
| `src/discovery/dht/store.rs` | ~100 | 3.2 | BTreeMap storage |
| `src/discovery/dht/external.rs` | ~150 | 3.3 | External DHT impl |
| `src/discovery/dht.rs` (mod) | +5 | 3.4 | Export |
| `src/types.rs` (mod) | +40 | 4.1 | WAN config types |
| `src/node.rs` (mod) | +40 | 4.3 | Bootstrap logic |
| `src/nat/stun.rs` | ~130 | 5.1 | STUN client |
| `src/nat/hole_punch.rs` | ~80 | 5.2 | Hole-punch server |
| `src/nat/mod.rs` | ~50 | 5.3 | Connection scoring |
| `src/node.rs` (mod) | +30 | 5.4 | NAT integration |
| `tests/dht_bootstrap_test.rs` | ~80 | 6.2 | DHT integration tests |
| `tests/nat_traversal_test.rs` | ~40 | 6.3 | NAT tests |
| `tests/e2e_bootstrap_test.rs` | ~30 | 6.4 | E2E test stub |

**Total: ~950 lines** (within estimated 800-1200 range)

---

## Verification Commands

After completing each phase, run:

```bash
# Phase 1-3 verification
cd agora-p2p
cargo check
cargo clippy -- -D warnings

# Phase 4+ verification
cargo check -p agora-p2p
cargo clippy -p agora-p2p -- -D warnings

# Run tests
cargo test --lib
cargo test --test dht_bootstrap_test
cargo test --test nat_traversal_test
```

---

## Open Questions (for implementation)

1. **DHT crate selection:** Finalize which crate to use (dht, krabbe, p2p)
2. **Seed hosting:** Who runs default seeds?
3. **IPv6 preference:** Should we prefer IPv6 over IPv4?
4. **Testing infrastructure:** Docker-nat for automated NAT tests?
