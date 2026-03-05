# Agora P2P Mesh Networking Implementation Plan

## Critical Fixes Applied

The following critical fixes have been identified and documented based on code review feedback. These must be applied to the implementation to ensure correctness and security.

### 1. TLS Implementation Fix

**Problem**: The original code incorrectly uses `rustls::TlsServer` and `rustls::TlsClient` types which don't exist in the expected form.

**Solution**: Use `rustls::ServerConfig` and `rustls::ClientConfig` directly with Quinn's integration:

```rust
// Correct approach
use rustls::{ServerConfig, ClientConfig};
use quinn::ServerConfig as QuinnServerConfig;

pub fn create_server_tls_config() -> Result<ServerConfig, QuicError> {
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)?;
    Ok(server_config)
}

// For Quinn integration
let quinn_server_config = QuinnServerConfig::with_crypto(Arc::new(rustls_config));
```

### 2. Security Fix - Replace InsecureVerifier

**Problem**: Using `dangerous() with_custom_certificate_verifier(InsecureVerifier)` accepts any certificate, which is insecure even for local networks.

**Solution**: Implement fingerprint-based certificate verification with AgentId binding:

```rust
pub struct FingerprintVerifier {
    // FIXED: Map AgentId -> Fingerprint for bound verification
    trusted_fingerprints: Arc<RwLock<HashMap<AgentId, [u8; 32]>>>,
}

impl FingerprintVerifier {
    pub fn new() -> Self {
        Self {
            trusted_fingerprints: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    // Register expected fingerprint for a specific agent
    pub async fn register_agent(&self, agent_id: AgentId, fingerprint: [u8; 32]) {
        self.trusted_fingerprints.write().await.insert(agent_id, fingerprint);
    }
    
    // Get fingerprint to verify against during handshake (after agent_id is known)
    pub async fn get_expected_fingerprint(&self, agent_id: &AgentId) -> Option<[u8; 32]> {
        self.trusted_fingerprints.read().await.get(agent_id).copied()
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _now: SystemTime,
        _dns_names: &[&str],
        _ip_addrs: &[IpAddr],
    ) -> Result<ServerCertVerified, rustls::Error> {
        // Extract certificate fingerprint = blake3(public_key)
        let cert_fingerprint = blake3::hash(end_entity.as_ref()).into();
        
        // Store fingerprint for later verification after handshake reveals agent_id
        // The actual agent_id binding happens during handshake verification
        self.pending_fingerprints.write().await.insert(cert_fingerprint, Instant::now());
        
        Ok(ServerCertVerified::assertion())
    }
}

// IMPROVED ARCHITECTURE: AgentId = blake3(public_key)
// This makes fingerprint verification trivial!

/*
 * Verification flow:
 * 
 * 1. TLS handshake completes
 * 2. Extract certificate fingerprint = blake3(public_key)
 * 3. Handshake message sends agent_id
 * 4. Verify: blake3(public_key) == AgentId  (trivial equality check!)
 * 
 * This eliminates the need for explicit fingerprint mapping.
 * The certificate's public key hash IS the AgentId.
 */
```

### 3. mDNS Removal Logic Fix

**Problem**: The current peer removal logic is broken - `fullname` from `ServiceRemoved` event looks like `agora-xxxx._agora._udp.local.` not the AgentId, so the comparison will never match.

**Solution**: Maintain a `service_instance → agent_id` mapping:

```rust
pub struct DiscoveryManager {
    mdns: MdnsDiscovery,
    peers: HashMap<AgentId, PeerInfo>,
    service_to_agent: HashMap<String, AgentId>,  // Track service instance → agent_id
    event_receiver: Option<Receiver<ServiceEvent>>,
}

pub struct PeerInfo {
    pub agent_id: AgentId,
    pub addr: SocketAddr,
    pub last_seen: std::time::Instant,
}

impl DiscoveryManager {
    pub async fn handle_events(&mut self) {
        // ... existing code ...
        
        Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
            // Extract instance name from fullname: "agora-xxxx._agora._udp.local."
            if let Some(agent_id) = self.service_to_agent.get(&fullname) {
                self.peers.remove(agent_id);
                self.service_to_agent.remove(&fullname);
            }
        }
        
        Ok(ServiceEvent::ServiceResolved(info)) => {
            // Track the mapping
            let instance_name = info.get_fullname().to_string();
            if let Some(agent_id_str) = info.get_property_val_str("agent_id") {
                if let Ok(agent_id) = AgentId::from_hex(agent_id_str) {
                    self.service_to_agent.insert(instance_name, agent_id.clone());
                    // ... rest of existing code
                }
            }
        }
    }
}
```

**Additional Improvement**: mDNS instance names are **NOT guaranteed unique across restarts**. Track full mapping including address to avoid stale removal events removing wrong peer:

```rust
pub struct DiscoveryManager {
    mdns: MdnsDiscovery,
    peers: HashMap<AgentId, PeerInfo>,
    // IMPROVED: Track service_instance -> (agent_id, addr) to handle restarts
    service_instances: HashMap<String, (AgentId, SocketAddr)>,
    event_receiver: Option<Receiver<ServiceEvent>>,
}

impl DiscoveryManager {
    pub async fn handle_events(&mut self) {
        loop {
            match receiver.recv().await {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let addr = SocketAddr::new(
                        info.get_addresses()[0],
                        info.get_port(),
                    );
                    
                    if let Some(agent_id_str) = info.get_property_val_str("agent_id") {
                        if let Ok(agent_id) = AgentId::from_hex(agent_id_str) {
                            let instance_name = info.get_fullname().to_string();
                            
                            // Update mapping: service_instance -> (agent_id, addr)
                            self.service_instances.insert(instance_name, (agent_id.clone(), addr));
                            
                            // Update or insert peer
                            self.peers.insert(agent_id.clone(), PeerInfo {
                                agent_id,
                                addr,
                                last_seen: std::time::Instant::now(),
                            });
                        }
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    // Look up by service instance name - may have updated agent_id
                    if let Some((agent_id, _addr)) = self.service_instances.get(&fullname) {
                        self.peers.remove(agent_id);
                        self.service_instances.remove(&fullname);
                    }
                }
                Ok(_) => {}
                Err(mpsc::error::RecvError) => break,
            }
        }
    }
}
```

### 4. Peer Connection Race Condition Fix

**Problem**: When two peers discover each other via mDNS, both may attempt to connect simultaneously, resulting in duplicate connections and wasted resources.

**Solution**: Add deterministic initiator rule based on AgentId comparison and collapse duplicates:

```rust
impl PeerManager {
    pub async fn maybe_connect_to_peer(&self, peer_agent_id: AgentId, addr: SocketAddr) {
        // Deterministic initiator: always connect if local_id < peer_id
        if self.local_agent_id < peer_agent_id {
            // Initiate connection
            if let Err(e) = self.connect_to_peer(addr, peer_agent_id).await {
                tracing::debug!("Connection attempt failed: {}", e);
            }
        }
        // Otherwise, wait for incoming connection from peer
    }
}

// DUPLICATE CONNECTION COLLAPSE:
// Even with deterministic initiator rule, may temporarily get both incoming + outgoing connection
// Add logic to collapse duplicates

impl PeerManager {
    pub async fn handle_incoming_connection(&self, conn: Connection, remote_agent_id: AgentId) {
        let mut connections = self.connections.write().await;
        
        // Check if we already have a connection to this peer
        if let Some(existing) = connections.get(&remote_agent_id) {
            // Determine which connection to keep using initiator rule
            // We are the responder in this case
            let we_should_keep = self.local_agent_id < remote_agent_id;
            
            if we_should_keep {
                // We're the designated initiator, keep incoming, close outgoing if exists
                tracing::debug!("Keeping incoming connection from {} (we are initiator)", remote_agent_id);
                // Note: outgoing connection would be closed by its owner when it sees this connection
                connections.insert(remote_agent_id.clone(), PeerConnection {
                    agent_id: remote_agent_id,
                    connection: conn,
                    connected_at: std::time::Instant::now(),
                });
            } else {
                // We're not the initiator, reject this incoming connection
                tracing::debug!("Rejecting incoming from {} (peer is initiator)", remote_agent_id);
                // Close the incoming connection - peer should have outgoing
                conn.close(0u8.into(), b"not initiator");
                return;
            }
        } else {
            // No existing connection, insert new one
            connections.insert(remote_agent_id.clone(), PeerConnection {
                agent_id: remote_agent_id,
                connection: conn,
                connected_at: std::time::Instant::now(),
            });
        }
    }
}

// In incoming connection handler, verify the remote AgentId matches expected
impl QuicServer {
    pub async fn handle_incoming(&self, incoming: Incoming) -> Result<Connection, QuicError> {
        let conn = incoming.await?;
        // Connection established - handshake will verify AgentId
        Ok(conn)
    }
}
```

### 5. Message Framing Fix

**Problem**: QUIC streams are byte streams, not message-framed. Without delimiters, the receiver cannot determine message boundaries.

**Solution**: Add length-prefix framing:

```rust
// Frame format: [message_length: u32][cbor_bytes]

pub struct MessageCodec;

impl MessageCodec {
    pub fn encode(message: &P2pMessage) -> Result<Vec<u8>, ProtocolError> {
        let mut buffer = Vec::new();
        
        // Serialize message directly to bytes
        ciborium::ser::into_writer(message, &mut buffer)
            .map_err(|e| ProtocolError::Encoding(e.to_string()))?;
        
        // Prepend length prefix
        let length = buffer.len() as u32;
        let mut frame = length.to_be_bytes().to_vec();
        frame.extend(buffer);
        
        Ok(frame)
    }
    
    // CORRECT: Read length prefix first, then read that many bytes, then deserialize
    pub async fn read_message(recv: &mut RecvStream) -> Result<P2pMessage, ProtocolError> {
        let mut length_buf = [0u8; 4];
        recv.read_exact(&mut length_buf).await?;
        let length = u32::from_be_bytes(length_buf) as usize;
        
        let mut buffer = vec![0u8; length];
        recv.read_exact(&mut buffer).await?;
        
        // Deserialize directly from buffer (no length prefix in buffer)
        ciborium::de::from_reader(&buffer[..])
            .map_err(|e| ProtocolError::Decoding(e.to_string()))
    }
}
```

### 6. CBOR Encoding Fix

**Problem**: Using `Value = message.into()` pattern is incorrect - this goes through serde transmutation which can lose type information.

**Solution**: Serialize/deserialize directly with ciborium:

```rust
pub struct MessageCodec;

impl MessageCodec {
    pub fn encode(message: &P2pMessage) -> Result<Vec<u8>, ProtocolError> {
        let mut buffer = Vec::new();
        
        // Direct serialization - no Value intermediate
        ciborium::ser::into_writer(message, &mut buffer)
            .map_err(|e| ProtocolError::Encoding(e.to_string()))?;
        
        Ok(buffer)
    }
    
    pub fn decode(data: &[u8]) -> Result<P2pMessage, ProtocolError> {
        // Direct deserialization - no Value intermediate
        ciborium::de::from_reader(data)
            .map_err(|e| ProtocolError::Decoding(e.to_string()))
    }
}
```

### 7. Add Missing Components

#### 7.1 Peer Authentication (Handshake)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum P2pMessage {
    Handshake {
        agent_id: AgentId,
        version: String,
        // NEW: Add public key and signature for authentication
        public_key: [u8; 32],
        signature: Vec<u8>,  // IMPROVED: Sign(transcript_hash + nonce)
        nonce: u64,          // For replay protection
    },
    // ... existing messages
}

impl PeerManager {
    pub async fn complete_handshake(
        &self,
        conn: &Connection,
        remote_agent_id: AgentId,
        remote_public_key: [u8; 32],
        signature: &[u8],
        nonce: u64,
    ) -> Result<(), P2pError> {
        // IMPROVED: Use transcript_hash instead of simple conn_id + agent_id
        // Standard pattern:
        //   ed25519_sign(
        //       blake3(
        //           tls_session_id
        //           + agent_id
        //           + public_key
        //       )
        //   )
        
        // Get TLS session ID for the transcript
        let tls_session_id = conn.peer_certificate()
            .ok_or(P2pError::Auth("No peer certificate".into()))?;
        
        // Build challenge: blake3(tls_session_id + agent_id + public_key + nonce)
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(tls_session_id.as_ref());
        hasher.update(remote_agent_id.as_bytes());
        hasher.update(&remote_public_key);
        hasher.update(&nonce.to_be_bytes());
        let challenge_hash = hasher.finalize();
        
        // Verify signature over challenge
        use ed25519_dalek::Verifier;
        
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&remote_public_key)
            .map_err(|_| P2pError::Auth("Invalid public key".into()))?;
        
        let ed_signature = ed25519_dalek::Signature::from_slice(signature)
            .map_err(|_| P2pError::Auth("Invalid signature format".into()))?;
        
        verifying_key.verify(challenge_hash.as_bytes(), &ed_signature)
            .map_err(|_| P2pError::Auth("Signature verification failed".into()))?;
        
        // IMPROVED ARCHITECTURE: Verify AgentId = blake3(public_key)
        // If AgentId is derived from public key, verification is trivial
        let expected_agent_id = AgentId::from_bytes(blake3::hash(&remote_public_key).as_bytes());
        if remote_agent_id != expected_agent_id {
            return Err(P2pError::Auth("AgentId doesn't match public key".into()));
        }
        
        // Check nonce for replay protection (store recent nonces)
        if self.is_nonce_used(nonce).await {
            return Err(P2pError::Auth("Nonce replay detected".into()));
        }
        self.mark_nonce_used(nonce).await;
        
        Ok(())
    }
    
    // Nonce tracking for replay protection
    async fn is_nonce_used(&self, nonce: u64) -> bool {
        let nonces = self.used_nonces.read().await;
        nonces.contains(&nonce)
    }
    
    async fn mark_nonce_used(&self, nonce: u64) {
        let mut nonces = self.used_nonces.write().await;
        nonces.insert(nonce);
        // Keep only recent nonces (last 1000)
        while nonces.len() > 1000 {
            if let Some(min) = nonces.iter().min().copied() {
                nonces.remove(&min);
            }
        }
    }
}
```

#### 7.2 Connection Health Monitoring

```rust
pub struct PeerConnection {
    pub agent_id: AgentId,
    pub connection: Connection,
    pub connected_at: std::time::Instant,
    // NEW: Health tracking
    pub last_ping: std::time::Instant,
    pub latency_ms: Option<u64>,
    pub consecutive_failures: u32,
}

// IMPROVED: Use BOTH QUIC built-in keepalive AND application pings

pub fn create_quic_server_config() -> ServerConfig {
    let mut transport_config = TransportConfig::default();
    
    // QUIC built-in keepalive for transport-level health
    transport_config.keep_alive_interval(Some(Duration::from_secs(15)));
    
    let mut server_config = ServerConfig::with_crypto(Arc::new(rustls_config));
    server_config.transport_config(Arc::new(transport_config));
    
    server_config
}

pub fn create_quic_client_config() -> ClientConfig {
    let mut transport_config = TransportConfig::default();
    
    // QUIC built-in keepalive for transport-level health
    transport_config.keep_alive_interval(Some(Duration::from_secs(15)));
    
    let mut client_config = ClientConfig::new(Arc::new(rustls_config));
    client_config.transport_config(Arc::new(transport_config));
    
    client_config
}

impl PeerManager {
    pub async fn start_health_checker(&self) {
        let connections = self.connections.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                
                let mut conns = connections.write().await;
                for (agent_id, peer) in conns.iter_mut() {
                    // Application-level ping for latency measurement
                    let ping = P2pMessage::Ping {
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                    };
                    
                    let start = std::time::Instant::now();
                    
                    if let Err(e) = Self::send_message(&peer.connection, &ping).await {
                        peer.consecutive_failures += 1;
                        if peer.consecutive_failures >= 3 {
                            tracing::warn!("Peer {} failed health check, marking for removal", agent_id);
                            // Trigger reconnection or removal
                        }
                    } else {
                        peer.consecutive_failures = 0;
                        peer.latency_ms = Some(start.elapsed().as_millis() as u64);
                    }
                    
                    peer.last_ping = std::time::Instant::now();
                }
            }
        });
    }
}
```

**Health Monitoring Strategy**:
- **QUIC keepalive** (15s): Handles transport-level health, detects network path failures
- **Application ping** (30s): Measures latency, detects application-level stalls
- Both together provide comprehensive health monitoring

#### 7.3 Backpressure Handling

// IMPROVED: One long-lived stream per peer with message queue → writer task pattern
// Opening a new stream per message is expensive

pub struct PeerConnection {
    pub agent_id: AgentId,
    pub connection: Connection,
    pub connected_at: std::time::Instant,
    // IMPROVED: Long-lived stream with dedicated writer task
    pub send_queue: Arc<tokio::sync::mpsc::Sender<Vec<u8>>>,
    writer_task: Option<tokio::task::JoinHandle<()>>,
}

impl PeerConnection {
    pub fn new(agent_id: AgentId, connection: Connection) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1000); // Buffer 1000 messages
        
        let connection_clone = connection.clone();
        let writer_task = tokio::spawn(async move {
            Self::writer_task_fn(connection_clone, rx).await;
        });
        
        Self {
            agent_id,
            connection,
            connected_at: std::time::Instant::now(),
            send_queue: Arc::new(tx),
            writer_task: Some(writer_task),
        }
    }
    
    async fn writer_task_fn(mut connection: Connection, mut rx: tokio::sync::mpsc::Receiver<Vec<u8>>) {
        // Open one bidirectional stream that lives for the connection lifetime
        let mut bi_stream = match connection.open_bi().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to open bidirectional stream: {}", e);
                return;
            }
        };
        
        while let Some(data) = rx.recv().await {
            // Write length prefix + data
            let length = (data.len() as u32).to_be_bytes();
            if let Err(e) = bi_stream.write_all(&length).await {
                tracing::error!("Failed to write length: {}", e);
                continue;
            }
            if let Err(e) = bi_stream.write_all(&data).await {
                tracing::error!("Failed to write data: {}", e);
                continue;
            }
        }
        
        // Clean up stream when channel closes
        let _ = bi_stream.finish().await;
    }
    
    pub async fn send(&self, message: P2pMessage) -> Result<(), P2pError> {
        let data = MessageCodec::encode(&message)?;
        
        // Non-blocking send to writer task
        self.send_queue.send(data)
            .await
            .map_err(|_| P2pError::Backpressure("Send queue full".into()))
    }
}

impl PeerManager {
    pub async fn send_with_backpressure(
        &self,
        peer_id: &AgentId,
        message: P2pMessage,
    ) -> Result<(), P2pError> {
        let connections = self.connections.read().await;
        let peer = connections.get(peer_id)
            .ok_or(P2pError::PeerNotFound(peer_id.to_string()))?;
        
        // Send to writer task (non-blocking with queue)
        peer.send(message).await
    }
    
    // For batching multiple messages - also uses the long-lived stream
    pub async fn batch_send(
        &self,
        peer_id: &AgentId,
        messages: Vec<P2pMessage>,
    ) -> Result<(), P2pError> {
        let connections = self.connections.read().await;
        let peer = connections.get(peer_id)
            .ok_or(P2pError::PeerNotFound(peer_id.to_string()))?;
        
        // Queue all messages to the writer task
        for message in messages {
            peer.send(message).await?;
        }
        
        Ok(())
    }
}

// Alternative: Simple semaphore-based approach (less efficient but simpler)

pub struct PeerConnectionSimple {
    pub agent_id: AgentId,
    pub connection: Connection,
    pub connected_at: std::time::Instant,
    pub pending_sends: Arc<tokio::sync::Semaphore>,
}

impl PeerManager {
    pub async fn send_with_backpressure_simple(
        &self,
        peer_id: &AgentId,
        message: P2pMessage,
    ) -> Result<(), P2pError> {
        let connections = self.connections.read().await;
        let peer = connections.get(peer_id)
            .ok_or(P2pError::PeerNotFound(peer_id.to_string()))?;
        
        // Acquire permit with timeout
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            peer.pending_sends.acquire(),
        ).await
        .map_err(|_| P2pError::Backpressure("Timeout waiting for send permit".into()))?
        .map_err(|_| P2pError::Backpressure("Semaphore closed".into()))?;
        
        let data = MessageCodec::encode(&message)?;
        
        // Send with flow control awareness
        let mut send = peer.connection.open_bi().await?;
        
        // Check connection's flow control window
        if send.send_window() < data.len() as u64 {
            // Wait for flow control credit
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        
        send.write_all(&data).await?;
        send.finish().await?;
        
        drop(permit);  // Release permit
        Ok(())
    }
}

### 8. Additional Missing Components

The following components were identified as missing from the original plan:

#### 8.1 NAT Traversal Limitation

**Important**: STUN alone cannot solve symmetric NAT. If you implement internet P2P, you will also need:

- **TURN relay**: A relay server that forwards traffic when direct connection fails
- **Hole punching coordinator**: A signaling server to coordinate NAT traversal between peers

```rust
pub enum ConnectionMethod {
    Direct(SocketAddrV4),
    HolePunched(SocketAddrV4),
    Relayed(SocketAddrV4),  // Via TURN relay
}

impl PeerConnection {
    pub async fn establish(
        local_id: AgentId,
        remote_id: AgentId,
        remote_addr: Option<SocketAddrV4>,
        relay_server: Option<SocketAddrV4>,
    ) -> Result<ConnectionMethod, NatError> {
        // 1. Try direct connection
        if let Some(addr) = remote_addr {
            if Self::test_direct_connect(addr).await {
                return Ok(ConnectionMethod::Direct(addr));
            }
        }
        
        // 2. Try hole punching
        if let Some(addr) = remote_addr {
            if Self::attempt_hole_punch(addr).await {
                return Ok(ConnectionMethod::HolePunched(addr));
            }
        }
        
        // 3. Fall back to TURN relay
        if let Some(relay) = relay_server {
            return Ok(ConnectionMethod::Relayed(relay));
        }
        
        Err(NatError::ConnectionFailed)
    }
}
```

#### 8.2 Peer Scoring for Reputation

Add basic reputation tracking to help with routing decisions:

```rust
#[derive(Debug, Clone, Default)]
pub struct PeerScore {
    pub latency_ms: u64,           // Average latency
    pub failures: u32,              // Consecutive failures
    pub disconnects: u32,           // Total disconnects
    pub last_success: std::time::Instant,
    pub last_failure: std::time::Instant,
}

impl PeerScore {
    pub fn record_success(&mut self, latency: std::time::Duration) {
        self.latency_ms = (self.latency_ms + latency.as_millis() as u64) / 2;
        self.failures = 0;
        self.last_success = std::time::Instant::now();
    }
    
    pub fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = std::time::Instant::now();
    }
    
    pub fn record_disconnect(&mut self) {
        self.disconnects += 1;
    }
    
    pub fn is_healthy(&self) -> bool {
        self.failures < 3 && self.latency_ms < 5000
    }
}

pub struct PeerManager {
    // ... existing fields
    pub peer_scores: Arc<RwLock<HashMap<AgentId, PeerScore>>>,
}
```

#### 8.3 Rate Limiting

Prevent DoS attacks with per-peer rate limiting:

```rust
pub struct RateLimiter {
    message_limits: Arc<RwLock<HashMap<AgentId, TokenBucket>>>,
    connection_limits: Arc<RwLock<HashMap<AgentId, TokenBucket>>>,
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: std::time::Instant,
}

impl TokenBucket {
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
    
    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            message_limits: Arc::new(RwLock::new(HashMap::new())),
            connection_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    // 100 messages per second per peer
    pub async fn check_message_rate(&self, agent_id: &AgentId) -> bool {
        let mut limits = self.message_limits.write().await;
        let bucket = limits.entry(agent_id.clone()).or_insert_with(|| TokenBucket {
            tokens: 100.0,
            max_tokens: 100.0,
            refill_rate: 100.0,
            last_refill: std::time::Instant::now(),
        });
        bucket.try_consume(1.0)
    }
    
    // 10 connection attempts per minute per peer
    pub async fn check_connection_rate(&self, agent_id: &AgentId) -> bool {
        let mut limits = self.connection_limits.write().await;
        let bucket = limits.entry(agent_id.clone()).or_insert_with(|| TokenBucket {
            tokens: 10.0,
            max_tokens: 10.0,
            refill_rate: 10.0 / 60.0,  // 10 per minute
            last_refill: std::time::Instant::now(),
        });
        bucket.try_consume(1.0)
    }
}
```

#### 8.4 Stream Limits

Configure QUIC stream limits to prevent resource exhaustion:

```rust
pub fn create_server_tls_config() -> Result<ServerConfig, QuicError> {
    let mut server_config = ServerConfig::with_crypto(Arc::new(rustls_config));
    
    // Configure transport parameters
    let mut transport_config = TransportConfig::default();
    
    // Limit concurrent streams to prevent resource exhaustion
    transport_config.max_concurrent_bidi_streams(VarInt::from_u64(16).unwrap());
    transport_config.max_concurrent_uni_streams(VarInt::from_u64(16).unwrap());
    
    // Set keepalive for connection health
    transport_config.keep_alive_interval(Some(Duration::from_secs(15)));
    
    // Set idle timeout
    transport_config.max_idle_timeout(Some(VarInt::from_u64(30_000).unwrap()));
    
    server_config.transport_config(Arc::new(transport_config));
    
    Ok(server_config)
}
```

### 9. Strategic Adjustment - Consider Delaying DHT

**Note**: Most chat networks never actually need a DHT. Matrix Federation already handles internet connectivity effectively. Consider this strategic adjustment:

> **Consider delaying DHT entirely** - Most chat networks (Signal, Telegram, WhatsApp) use centralized or federated servers rather than pure P2P. Matrix Federation can handle internet connectivity without DHT.
> 
> The DHT adds significant complexity for marginal benefit in a chat application. The primary use case (local network P2P) is fully addressed by Phase 1 (mDNS + QUIC).
> 
> Only implement DHT if:
> - True decentralization is a hard requirement
> - You're willing to maintain DHT bootstrap nodes
> - You've validated that federation doesn't meet the use case

---

## Overview

This document outlines the implementation strategy for adding peer-to-peer mesh networking capabilities to Agora. The architecture employs a dual-path communication model: P2P for local area networks (LAN) and Matrix Federation for internet-based communication. This approach maximizes privacy and reduces server dependency for local deployments while maintaining internet connectivity through established federation protocols.

The P2P implementation uses a custom stack built from individual crates rather than monolithic libraries like libp2p. This provides finer control over dependencies, reduced attack surface, and more direct integration with Agora's existing cryptographic primitives.

## Architecture Overview

### Communication Paths

```
┌─────────────────────────────────────────────────────────────────────┐
│                          Agora Node                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐     ┌──────────────────┐     ┌──────────────┐ │
│  │   App Layer  │────▶│   P2P Manager    │────▶│  Federation  │ │
│  └──────────────┘     └──────────────────┘     │   (Matrix)   │ │
│                              │                 └──────────────┘ │
│                              ▼                                    │
│                     ┌──────────────────┐                          │
│                     │  Network Router  │                          │
│                     └──────────────────┘                          │
│                              │                                     │
│              ┌───────────────┴───────────────┐                    │
│              ▼                               ▼                    │
│     ┌─────────────────┐            ┌─────────────────┐           │
│     │   LAN Mesh      │            │  Internet P2P   │           │
│     │ (mDNS + QUIC)  │            │  (DHT + STUN)   │           │
│     └─────────────────┘            └─────────────────┘           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Design Principles

The P2P stack follows several core principles. First, **minimal dependencies** means using individual crates rather than comprehensive frameworks, which reduces the attack surface and simplifies auditing. Second, **graceful degradation** ensures that if P2P fails (due to NAT, firewalls, or no peers), the system automatically falls back to Matrix Federation. Third, **crypto integration** leverages Agora's existing identity, signing, and sigchain infrastructure rather than implementing redundant cryptographic operations. Fourth, **incremental deployment** allows Phase 1 (LAN) to function independently, with subsequent phases building on that foundation.

### New Crate Structure

A new crate `agora-p2p` will be added to the workspace with the following internal structure:

```
agora-p2p/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── types.rs
    ├── transport/
    │   ├── mod.rs
    │   ├── quic.rs
    │   └── tls.rs
    ├── discovery/
    │   ├── mod.rs
    │   └── mdns.rs
    ├── protocol/
    │   ├── mod.rs
    │   ├── messages.rs
    │   └── codec.rs
    ├── mesh/
    │   ├── mod.rs
    │   ├── peer.rs
    │   └── room.rs
    ├── dht/
    │   ├── mod.rs
    │   ├── bucket.rs
    │   └── routing.rs
    ├── nat/
    │   ├── mod.rs
    │   ├── stun.rs
    │   └── hole_punch.rs
    └── dag/
        ├── mod.rs
        ├── signed_event.rs
        └── crdt.rs
```

### Workspace Integration

Add `agora-p2p` to the workspace `Cargo.toml`:

```toml
[workspace]
members = [
    "agora-core",
    "agora-crypto",
    "agora-server",
    "agora-cli",
    "agora-app",
    "agora-p2p",
]
```

The crate will depend on `agora-crypto` for identity management, signing, and event ID generation, while also depending on `agora-core` for room and event types.

---

## Phase 1: LAN Mesh (Priority)

Phase 1 implements local area network peer discovery and direct communication. This phase delivers immediate value by enabling peer-to-peer communication on local networks without any server infrastructure. The implementation prioritizes reliability and simplicity over feature completeness.

### 1.1 QUIC Transport Layer

The QUIC transport provides the foundation for reliable, encrypted, bidirectional communication between peers. We use the `quinn` crate which implements QUIC protocol in pure Rust.

#### Dependencies

```toml
# agora-p2p/Cargo.toml
[dependencies]
quinn = "0.11"
rustls = "0.23"
rcgen = "0.13"        # for self-signed certificate generation
```

#### Server Endpoint

The server endpoint listens for incoming connections from other peers on the local network.

```rust
// src/transport/quic.rs

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

use quinn::{
    Endpoint, EndpointConfig, ServerConfig, Connection, 
    Incoming, RecvStream, SendStream, VarInt,
};

pub struct QuicServer {
    endpoint: Endpoint,
    local_addr: SocketAddr,
}

impl QuicServer {
    pub async fn new(port: u16) -> Result<Self, QuicError> {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let socket = UdpSocket::bind(addr).await?;
        
        let tls_server = create_server_tls_config()?;
        let mut server_config = ServerConfig::with_crypto(Arc::new(tls_server));
        
        // Configure for local network
        server_config.max_idle_timeout(Some VarInt::from_u64(30_000).unwrap()));
        server_config.max_concurrent_uni_streams(VarInt::from_u64(16).unwrap());
        
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            socket,
            Arc::new(quinn::default_runtime()),
        )?;
        
        let local_addr = endpoint.local_addr()?;
        
        Ok(Self { endpoint, local_addr })
    }
    
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    pub fn accept(&self) -> Incoming {
        self.endpoint.accept()
    }
}
```

#### Client Endpoint

The client endpoint initiates connections to discovered peers.

```rust
pub struct QuicClient {
    endpoint: Endpoint,
}

impl QuicClient {
    pub async fn new() -> Result<Self, QuicError> {
        let tls_client = create_client_tls_config()?;
        let client_config = ClientConfig::new(Arc::new(tls_client));
        
        let endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        endpoint.set_default_client_config(client_config);
        
        Ok(Self { endpoint })
    }
    
    pub async fn connect(
        &self, 
        addr: SocketAddr, 
        server_name: &str,
    ) -> Result<Connection, QuicError> {
        self.endpoint.connect(addr, server_name)?.await
    }
}
```

#### TLS Configuration

For LAN communication, we generate self-signed certificates. These certificates are not trusted by default but provide encryption for local traffic.

```rust
// src/transport/tls.rs

use rcgen::{
    CertifiedKey, CertificateParams, DistinguishedName, DnType, 
    KeyPair, SanType,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    RootStore, ServerConfig, ClientConfig,
};
use std::sync::Arc;

pub fn create_server_tls_config() -> Result<rustls::TlsServer, QuicError> {
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "agora-p2p-local");
    
    let params = CertificateParams::default()
        .distinguished_name(distinguished_name)
        .self_signed();
    
    let key_pair = KeyPair::from_pkcs8(
        &params.serialize_private_key()?,
        &params.serialize_public_key()?,
    ).map_err(|e| QuicError::Tls(format!("key pair: {e}")))?;
    
    let certified_key = params.self_signed_signed(&key_pair)
        .map_err(|e| QuicError::Tls(format!("signing: {e}")))?;
    
    let cert = CertificateDer::from(certified_key.cert);
    let key = PrivateKeyDer::from(key_pair.serialize_private_key_der());
    
    let mut tls = rustls::TlsServer::new(std::sync::RwLock::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)?
    ));
    
    Ok(tls)
}

pub fn create_client_tls_config() -> Result<rustls::TlsClient, QuicError> {
    let mut root_store = RootStore::empty();
    // In production, add known self-signed certs here
    
    let tls = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(
            InsecureVerifier // ⚠️ FOR LOCAL NETWORK ONLY
        ))
        .with_no_client_auth();
    
    Ok(tls)
}

// ⚠️ DANGER: Only for local network testing
struct InsecureVerifier;
impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self, 
        _end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _now: std::time::SystemTime,
        _dns_names: &[&str],
        _ip_addrs: &[std::net::IpAddr],
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    
    fn verify_tls12_signature(
        &self, 
        _message: &[u8], 
        _cert: &CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    
    fn verify_tls13_signature(
        &self, 
        _message: &[u8], 
        _cert: &CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
}
```

#### Bidirectional Streams

QUIC provides bidirectional streams that we use for request-response patterns:

```rust
pub async fn open_bidirectional(
    conn: &Connection,
) -> Result<(SendStream, RecvStream), QuicError> {
    Ok(conn.open_bi().await?)
}

pub async fn read_message(
    recv: &mut RecvStream,
) -> Result<Vec<u8>, QuicError> {
    let mut buf = vec![0u8; 65536];
    let n = recv.read(&mut buf).await?;
    buf.truncate(n);
    Ok(buf)
}

pub async fn write_message(
    send: &mut SendStream,
    data: &[u8],
) -> Result<(), QuicError> {
    send.write_all(data).await?;
    send.finish().await?;
    Ok(())
}
```

### 1.2 mDNS Discovery

The mdns-sd crate provides service discovery on local networks using the multicast DNS (mDNS) protocol. This enables automatic peer discovery without manual configuration.

#### Dependencies

```toml
mdns-sd = "0.12"
```

#### Service Advertisement

Each peer advertises its presence on the local network:

```rust
// src/discovery/mdns.rs

use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};
use std::net::IpAddr;
use std::time::Duration;

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_fullname: String,
}

impl MdnsDiscovery {
    pub fn new() -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new()?;
        Ok(Self { daemon, service_fullname: String::new() })
    }
    
    pub fn advertise(
        &mut self,
        agent_id: &AgentId,
        quic_port: u16,
        local_ip: IpAddr,
    ) -> Result<(), DiscoveryError> {
        let service_type = "_agora._udp.local.";
        let instance_name = format!("agora-{}", agent_id.to_hex()[..8]);
        let hostname = format!("{}.local.", instance_name);
        
        let mut properties = serde_json::Map::new();
        properties.insert("agent_id".to_string(), serde_json::Value::String(agent_id.to_hex()));
        properties.insert("version".to_string(), serde_json::Value::String("1.0.0".to_string()));
        
        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &hostname,
            local_ip,
            quic_port,
            properties,
        )?.enable_addr_auto()?;
        
        self.service_fullname = self.daemon.register(service_info)?;
        Ok(())
    }
    
    pub fn browse(&self) -> Result<Receiver<ServiceEvent>, DiscoveryError> {
        self.daemon.browse("_agora._udp.local.")
    }
}
```

#### Peer Discovery

The discovery manager handles service events and maintains a list of discovered peers:

```rust
// src/discovery/mod.rs

use std::collections::HashMap;
use std::net::SocketAddr;
use mdns_sd::ServiceEvent;
use tokio::sync::mpsc;
use agora_crypto::AgentId;

pub struct PeerInfo {
    pub agent_id: AgentId,
    pub addr: SocketAddr,
    pub last_seen: std::time::Instant,
}

pub struct DiscoveryManager {
    mdns: MdnsDiscovery,
    peers: HashMap<AgentId, PeerInfo>,
    event_receiver: Option<Receiver<ServiceEvent>>,
}

impl DiscoveryManager {
    pub async fn start(
        agent_id: AgentId,
        quic_port: u16,
        local_ip: std::net::IpAddr,
    ) -> Result<Self, DiscoveryError> {
        let mut mdns = MdnsDiscovery::new()?;
        mdns.advertise(&agent_id, quic_port, local_ip)?;
        
        let event_receiver = mdns.browse()?;
        
        Ok(Self {
            mdns,
            peers: HashMap::new(),
            event_receiver: Some(event_receiver),
        })
    }
    
    pub async fn handle_events(&mut self) {
        let Some(mut receiver) = self.event_receiver.take() else {
            return;
        };
        
        loop {
            match receiver.recv().await {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let addr = SocketAddr::new(
                        info.get_addresses()[0],
                        info.get_port(),
                    );
                    
                    if let Some(agent_id_str) = info.get_property_val_str("agent_id") {
                        if let Ok(agent_id) = AgentId::from_hex(agent_id_str) {
                            self.peers.insert(agent_id.clone(), PeerInfo {
                                agent_id,
                                addr,
                                last_seen: std::time::Instant::now(),
                            });
                        }
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    // Remove peer
                    self.peers.retain(|_, p| p.agent_id.to_string() != fullname);
                }
                Ok(_) => {}
                Err(mpsc::error::RecvError) => break,
            }
        }
    }
    
    pub fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }
}
```

### 1.3 Wire Protocol

The wire protocol defines the message format for communication between peers. We use CBOR (Concise Binary Object Representation) via the `ciborium` crate for efficient binary serialization.

#### Dependencies

```toml
ciborium = "0.2"
```

#### Message Types

```rust
// src/protocol/messages.rs

use serde::{Deserialize, Serialize};
use agora_crypto::AgentId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum P2pMessage {
    /// Initial handshake: exchange agent IDs and capabilities
    Handshake {
        agent_id: AgentId,
        version: String,
    },
    
    /// Keepalive response
    Pong {
        timestamp: u64,
    },
    
    /// Keepalive request
    Ping {
        timestamp: u64,
    },
    
    /// Push an event to a peer (for relay or direct delivery)
    EventPush {
        event: SignedEventPayload,
    },
    
    /// Request events from a peer (for sync)
    EventRequest {
        room_id: String,
        since: Option<u64>,
        limit: u32,
    },
    
    /// Response to EventRequest
    EventResponse {
        room_id: String,
        events: Vec<SignedEventPayload>,
    },
    
    /// Request full room state
    StateRequest {
        room_id: String,
    },
    
    /// Response to StateRequest  
    StateResponse {
        room_id: String,
        state: RoomState,
    },
    
    /// Notify peers about room membership changes
    MemberUpdate {
        room_id: String,
        members: Vec<AgentId>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEventPayload {
    pub event_id: String,
    pub sender: AgentId,
    pub room_id: String,
    pub event_type: String,
    pub content: Vec<u8>,
    pub origin_server_ts: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomState {
    pub room_id: String,
    pub members: Vec<AgentId>,
    pub events: Vec<SignedEventPayload>,
}
```

#### Codec Implementation

The codec handles serialization and deserialization of messages:

```rust
// src/protocol/codec.rs

use ciborium::Value;
use std::io::Cursor;

use super::messages::P2pMessage;

pub struct MessageCodec;

impl MessageCodec {
    pub fn encode(message: &P2pMessage) -> Result<Vec<u8>, ProtocolError> {
        let mut buffer = Vec::new();
        let value: Value = message.into();
        ciborium::ser::into_writer(&value, &mut buffer)
            .map_err(|e| ProtocolError::Encoding(e.to_string()))?;
        Ok(buffer)
    }
    
    pub fn decode(data: &[u8]) -> Result<P2pMessage, ProtocolError> {
        let value: Value = ciborium::de::from_reader(Cursor::new(data))
            .map_err(|e| ProtocolError::Decoding(e.to_string()))?;
        
        let message: P2pMessage = value.try_into()
            .map_err(|e: serde::de::value::Error| 
                ProtocolError::Decoding(e.to_string()))?;
        
        Ok(message)
    }
}

// Manual Serialize/Deserialize implementations would be added here
// using ciborium's Value mapping or custom derive macros
```

### 1.4 Basic P2P Messaging

This section implements the peer connection management and event exchange system.

#### Peer Connection Management

```rust
// src/mesh/peer.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use quinn::Connection;
use agora_crypto::AgentId;

use crate::transport::quic::{QuicClient, QuicServer};
use crate::protocol::messages::P2pMessage;

pub struct PeerConnection {
    pub agent_id: AgentId,
    pub connection: Connection,
    pub connected_at: std::time::Instant,
}

pub struct PeerManager {
    local_agent_id: AgentId,
    quic_client: QuicClient,
    connections: Arc<RwLock<HashMap<AgentId, PeerConnection>>>,
}

impl PeerManager {
    pub fn new(local_agent_id: AgentId) -> Self {
        Self {
            local_agent_id,
            quic_client: QuicClient::new().expect("QUIC client init"),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn connect_to_peer(
        &self,
        addr: std::net::SocketAddr,
        peer_agent_id: AgentId,
    ) -> Result<(), P2pError> {
        let conn = self.quic_client
            .connect(addr, "agora-p2p")
            .await?;
        
        // Send handshake
        let handshake = P2pMessage::Handshake {
            agent_id: self.local_agent_id.clone(),
            version: "1.0.0".to_string(),
        };
        
        // ... send message via connection ...
        
        let connection = PeerConnection {
            agent_id: peer_agent_id,
            connection: conn,
            connected_at: std::time::Instant::now(),
        };
        
        self.connections.write().await
            .insert(peer_agent_id, connection);
        
        Ok(())
    }
    
    pub async fn send_to_peer(
        &self,
        peer_id: &AgentId,
        message: P2pMessage,
    ) -> Result<(), P2pError> {
        let connections = self.connections.read().await;
        let conn = connections.get(peer_id)
            .ok_or(P2pError::PeerNotFound)?;
        
        // Open bidirectional stream and send message
        let (mut send, mut recv) = conn.connection.open_bi().await?;
        
        let data = crate::protocol::codec::MessageCodec::encode(&message)?;
        
        tokio::io::AsyncWriteExt::write_all(&mut send, &data).await?;
        tokio::io::AsyncWriteExt::flush(&mut send).await?;
        
        Ok(())
    }
    
    pub async fn broadcast(
        &self,
        message: P2pMessage,
    ) -> Result<(), P2pError> {
        let connections = self.connections.read().await;
        
        for (peer_id, conn) in connections.iter() {
            if let Err(e) = self.send_to_peer(peer_id, message.clone()).await {
                tracing::warn!("Failed to send to {}: {}", peer_id, e);
            }
        }
        
        Ok(())
    }
}
```

#### Room Membership Sync

```rust
// src/mesh/room.rs

use std::collections::HashSet;
use agora_crypto::AgentId;

pub struct MeshRoom {
    pub room_id: String,
    pub members: HashSet<AgentId>,
    pub local_member: AgentId,
}

impl MeshRoom {
    pub fn new(room_id: String, local_agent_id: AgentId) -> Self {
        let mut members = HashSet::new();
        members.insert(local_agent_id.clone());
        
        Self {
            room_id,
            members,
            local_member: local_agent_id,
        }
    }
    
    pub fn add_member(&mut self, agent_id: AgentId) -> bool {
        self.members.insert(agent_id)
    }
    
    pub fn remove_member(&mut self, agent_id: &AgentId) -> bool {
        self.members.remove(agent_id)
    }
    
    pub fn member_count(&self) -> usize {
        self.members.len()
    }
    
    pub fn is_member(&self, agent_id: &AgentId) -> bool {
        self.members.contains(agent_id)
    }
}
```

### Phase 1 Integration Points

Integration with the existing Agora codebase requires modifications to `agora-core` to enable P2P message routing:

1. **Add P2P trait to agora-core**: Define a `P2pTransport` trait that abstracts P2P capabilities, allowing the core to send and receive messages via P2P.

2. **Event routing**: Modify the event handler in `agora-core` to attempt P2P delivery first, falling back to Matrix Federation if P2P fails.

3. **Configuration**: Add P2P configuration to the Agora configuration system, enabling users to disable P2P if needed.

### Phase 1 Testing Approach

Unit tests verify individual components in isolation. The QUIC transport tests verify connection establishment and data transfer. The mDNS discovery tests verify service advertisement and peer detection. The protocol tests verify message encoding and decoding. Integration tests verify end-to-end peer communication on a local network with multiple nodes.

---

## Phase 2: Internet P2P

Phase 2 extends P2P capabilities beyond local networks to enable peer-to-peer communication over the internet. This requires addressing the challenges of NAT traversal and distributed peer discovery.

### 2.1 Kademlia DHT

The Kademlia distributed hash table enables peer discovery over the internet without centralized servers. We implement a custom DHT using the `buckets` crate for bucket management.

#### Dependencies

```toml
buckets = "0.4"       # For DHT bucket management
xor feed = "0.3"      # For Kademlia distance calculation
```

#### Peer Routing Table

```rust
// src/dht/routing.rs

use buckets::KBucket;
use xor_feed::{XorMetric, XorBucket, XorItem};
use std::collections::HashMap;
use agora_crypto::AgentId;

const K: usize = 20;  // K-bucket size
const B: u8 = 8;      // Number of bits in node ID

pub struct DhtPeer {
    pub id: AgentId,
    pub addr: std::net::SocketAddrV4,
    pub last_seen: std::time::Instant,
}

impl XorItem for DhtPeer {
    type ID = AgentId;
    
    fn id(&self) -> &AgentId {
        &self.id
    }
}

pub struct RoutingTable {
    buckets: HashMap<u8, XorBucket<DhtPeer, K>>,
    local_id: AgentId,
}

impl RoutingTable {
    pub fn new(local_id: AgentId) -> Self {
        Self {
            buckets: HashMap::new(),
            local_id,
        }
    }
    
    pub fn add_peer(&mut self, peer: DhtPeer) -> Option<DhtPeer> {
        let distance = self.distance(&peer.id);
        let bucket_index = self.bucket_index(&distance);
        
        let bucket = self.buckets.entry(bucket_index)
            .or_insert_with(|| XorBucket::new(bucket_index));
        
        bucket.insert(peer)
    }
    
    pub fn find_closest(&self, target: &AgentId, count: usize) -> Vec<DhtPeer> {
        let mut all_peers: Vec<_> = self.buckets.values()
            .flat_map(|b| b.iter())
            .cloned()
            .collect();
        
        all_peers.sort_by(|a, b| {
            let da = xor_feed::distance(&a.id, target);
            let db = xor_feed::distance(&b.id, target);
            da.cmp(&db)
        });
        
        all_peers.truncate(count);
        all_peers
    }
    
    fn distance(&self, other: &AgentId) -> [u8; 32] {
        let mut dist = [0u8; 32];
        for (i, (a, b)) in self.local_id.as_bytes().iter()
            .zip(other.as_bytes().iter())
            .enumerate() {
            dist[i] = a ^ b;
        }
        dist
    }
    
    fn bucket_index(&self, distance: &[u8; 32]) -> u8 {
        for (i, &byte) in distance.iter().enumerate() {
            if byte != 0 {
                return i as u8 * 8 + byte.leading_zeros() as u8;
            }
        }
        B * 32 - 1
    }
}
```

#### DHT Put/Get Operations

```rust
// src/dht/mod.rs

use std::net::SocketAddrV4;
use crate::dht::routing::{RoutingTable, DhtPeer};

pub struct DhtClient {
    routing_table: RoutingTable,
    bootstrap_nodes: Vec<SocketAddrV4>,
}

impl DhtClient {
    pub fn new(local_id: AgentId, bootstrap: Vec<SocketAddrV4>) -> Self {
        Self {
            routing_table: RoutingTable::new(local_id),
            bootstrap_nodes: bootstrap,
        }
    }
    
    pub async fn bootstrap(&mut self) -> Result<(), DhtError> {
        // Connect to bootstrap nodes and request their peer lists
        for addr in &self.bootstrap_nodes {
            // TODO: Connect and exchange peer information
        }
        Ok(())
    }
    
    pub async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), DhtError> {
        // Find k closest peers to key and store value
        let key_id = AgentId::from_bytes(key);
        let closest = self.routing_table.find_closest(&key_id, K);
        
        for peer in closest {
            // TODO: Send store request to peer
        }
        
        Ok(())
    }
    
    pub async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, DhtError> {
        let key_id = AgentId::from_bytes(key);
        let closest = self.routing_table.find_closest(&key_id, K);
        
        for peer in closest {
            // TODO: Query peer for value
        }
        
        Ok(None)
    }
}

impl AgentId {
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes[..32]);
        AgentId(arr)
    }
}
```

### 2.2 NAT Traversal

NAT traversal enables peers behind network address translators to establish direct connections. We implement STUN for external IP detection and UDP hole punching for connection establishment.

#### STUN Client

```rust
// src/nat/stun.rs

use std::net::SocketAddrV4;

const STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
];

pub struct StunClient {
    local_port: u16,
}

impl StunClient {
    pub fn new(local_port: u16) -> Self {
        Self { local_port }
    }
    
    pub async fn discover_external_ip(&self) -> Result<SocketAddrV4, NatError> {
        // Create UDP socket
        let socket = tokio::net::UdpSocket::bind(format!("0.0.0.0:{}", self.local_port))
            .await?;
        
        for stun_server in STUN_SERVERS {
            if let Ok(addr) = stun_server.parse::<SocketAddrV4>() {
                if let Ok(mapped) = self.query_stun(socket, addr).await {
                    return Ok(mapped);
                }
            }
        }
        
        Err(NatError::StunFailed)
    }
    
    async fn query_stun(
        &self, 
        socket: tokio::net::UdpSocket,
        stun_addr: SocketAddrV4,
    ) -> Result<SocketAddrV4, NatError> {
        // Send STUN binding request (RFC 5389)
        let request = self.build_binding_request();
        socket.send_to(&request, stun_addr).await?;
        
        let mut buf = [0u8; 128];
        let (len, _) = socket.recv_from(&mut buf).await?;
        
        // Parse STUN response to get XOR-mapped address
        self.parse_binding_response(&buf[..len])
    }
    
    fn build_binding_request(&self) -> Vec<u8> {
        // RFC 5389 STUN binding request
        let mut msg = vec![0u8; 20];
        // Magic cookie
        msg[4..8].copy_from_slice(&0x2112A442.to_be_bytes());
        // Transaction ID
        // ... (random 12 bytes)
        msg
    }
    
    fn parse_binding_response(&self, data: &[u8]) -> Result<SocketAddrV4, NatError> {
        // Parse STUN attributes for XOR-MAPPED-ADDRESS (0x0020)
        // This is a simplified implementation
        Err(NatError::ParseError)
    }
}
```

#### UDP Hole Punching

```rust
// src/nat/hole_punch.rs

use std::net::SocketAddrV4;

pub struct HolePuncher {
    local_port: u16,
}

impl HolePuncher {
    pub fn new(local_port: u16) -> Self {
        Self { local_port }
    }
    
    pub async fn punch(
        &self,
        peer_addr: SocketAddrV4,
    ) -> Result<SocketAddrV4, NatError> {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        
        // Send outgoing packet to create outbound mapping
        socket.send_to(b"agora-hole-punch", peer_addr).await?;
        
        // In a full implementation, coordinate with peer via relay server
        // to synchronize packet exchange
        
        Ok(peer_addr)
    }
}
```

#### Relay Fallback

When hole punching fails, we use agora-server as a relay:

```rust
pub enum ConnectionMethod {
    Direct(SocketAddrV4),
    HolePunched(SocketAddrV4),
    Relayed(SocketAddrV4),
}

impl PeerConnection {
    pub async fn establish(
        local_id: AgentId,
        remote_id: AgentId,
        remote_addr: Option<SocketAddrV4>,
        relay_server: Option<SocketAddrV4>,
    ) -> Result<ConnectionMethod, NatError> {
        // 1. Try direct connection
        if let Some(addr) = remote_addr {
            if Self::test_direct_connect(addr).await {
                return Ok(ConnectionMethod::Direct(addr));
            }
        }
        
        // 2. Try hole punching
        if let Some(addr) = remote_addr {
            if Self::attempt_hole_punch(addr).await {
                return Ok(ConnectionMethod::HolePunched(addr));
            }
        }
        
        // 3. Fall back to relay
        if let Some(relay) = relay_server {
            return Ok(ConnectionMethod::Relayed(relay));
        }
        
        Err(NatError::ConnectionFailed)
    }
}
```

### Phase 2 Integration Points

1. **DHT bootstrapping**: Configure agora-server instances as DHT bootstrap nodes, maintaining a list of known stable peers.

2. **NAT configuration**: Add STUN server configuration and relay server addresses to the Agora configuration system.

3. **Connection manager**: Extend the peer manager to attempt multiple connection methods in order of preference.

### Phase 2 Testing Approach

Testing requires simulating NAT scenarios. Unit tests verify DHT bucket operations and routing. Integration tests verify DHT peer exchange between multiple nodes. Network simulation tests verify NAT traversal behavior using network namespace manipulation.

---

## Phase 3: Event DAG (Future)

Phase 3 implements the full event graph structure that provides cryptographically secure, distributed state synchronization. This phase builds on Phases 1 and 2 to enable fully decentralized room state management.

### 3.1 SignedEvent Structure

```rust
// src/dag/signed_event.rs

use serde::{Deserialize, Serialize};
use agora_crypto::{AgentId, CryptoError};
use crate::protocol::messages::P2pMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEvent {
    /// Event ID (BLAKE3 hash of content)
    pub event_id: String,
    
    /// Room this event belongs to
    pub room_id: String,
    
    /// Sender's agent ID
    pub sender: AgentId,
    
    /// Event type (e.g., "m.room.message")
    pub event_type: String,
    
    /// Event content (CBOR-encoded)
    pub content: Vec<u8>,
    
    /// Wall clock timestamp (origin_server_ts)
    pub origin_server_ts: u64,
    
    /// Lamport logical timestamp
    pub lamport_ts: u64,
    
    /// Parent event IDs (for DAG structure)
    pub parents: Vec<String>,
    
    /// Ed25519 signature over canonical encoding
    pub signature: Vec<u8>,
    
    /// Signing public key
    pub sender_key: [u8; 32],
}

impl SignedEvent {
    pub fn new(
        room_id: String,
        sender: AgentId,
        event_type: String,
        content: Vec<u8>,
        origin_server_ts: u64,
        lamport_ts: u64,
        parents: Vec<String>,
    ) -> Result<Self, CryptoError> {
        let event_id = Self::compute_event_id(
            &room_id, 
            &sender, 
            &event_type, 
            &content, 
            origin_server_ts,
        );
        
        let canonical = Self::canonical_bytes(
            &event_id,
            &room_id,
            &sender,
            &event_type,
            &content,
            origin_server_ts,
            lamport_ts,
            &parents,
        );
        
        // Signature would be added by the caller using their AgentIdentity
        Ok(Self {
            event_id,
            room_id,
            sender,
            event_type,
            content,
            origin_server_ts,
            lamport_ts,
            parents,
            signature: vec![],
            sender_key: [0u8; 32],
        })
    }
    
    fn compute_event_id(
        room_id: &str,
        sender: &AgentId,
        event_type: &str,
        content: &[u8],
        origin_server_ts: u64,
    ) -> String {
        use agora_crypto::ids::event_id;
        event_id(room_id, &sender.to_string(), event_type, content, origin_server_ts)
    }
    
    fn canonical_bytes(
        event_id: &str,
        room_id: &str,
        sender: &AgentId,
        event_type: &str,
        content: &[u8],
        origin_server_ts: u64,
        lamport_ts: u64,
        parents: &[String],
    ) -> Vec<u8> {
        // Serialize for signing
        let data = (
            event_id,
            room_id,
            sender.as_bytes(),
            event_type,
            content,
            origin_server_ts,
            lamport_ts,
            parents,
        );
        rmp_serde::to_vec_named(&data).unwrap()
    }
    
    pub fn verify_signature(&self) -> Result<(), CryptoError> {
        let canonical = Self::canonical_bytes(
            &self.event_id,
            &self.room_id,
            &self.sender,
            &self.event_type,
            &self.content,
            self.origin_server_ts,
            self.lamport_ts,
            &self.parents,
        );
        
        use ed25519_dalek::Verifier;
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&self.sender_key)
            .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;
        
        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|_| CryptoError::InvalidSignature("Invalid signature bytes".into()))?;
        
        verifying_key.verify(&canonical, &signature)
            .map_err(|e| CryptoError::InvalidSignature(e.to_string()))
    }
}
```

### 3.2 Merkle DAG

```rust
// src/dag/mod.rs

use std::collections::{HashMap, HashSet};
use super::signed_event::SignedEvent;

pub struct EventDag {
    events: HashMap<String, SignedEvent>,
    // Cache of Merkle roots per depth
    roots: HashMap<u64, [u8; 32]>,
}

impl EventDag {
    pub fn new() -> Self {
        Self {
            events: HashMap::new(),
            roots: HashMap::new(),
        }
    }
    
    pub fn add_event(&mut self, event: SignedEvent) -> Result<(), DagError> {
        // Verify parents exist
        for parent_id in &event.parents {
            if !self.events.contains_key(parent_id) {
                return Err(DagError::MissingParent(parent_id.clone()));
            }
        }
        
        // Verify signature
        event.verify_signature()
            .map_err(|e| DagError::InvalidSignature(e.to_string()))?;
        
        self.events.insert(event.event_id.clone(), event);
        Ok(())
    }
    
    pub fn get_event(&self, event_id: &str) -> Option<&SignedEvent> {
        self.events.get(event_id)
    }
    
    pub fn get_tips(&self) -> Vec<String> {
        let mut children: HashSet<String> = HashSet::new();
        
        for event in self.events.values() {
            for parent in &event.parents {
                children.insert(parent.clone());
            }
        }
        
        self.events.keys()
            .filter(|id| !children.contains(*id))
            .cloned()
            .collect()
    }
    
    pub fn compute_merkle_root(&self, depth: u64) -> [u8; 32] {
        // Group events by lamport timestamp
        let events_at_depth: Vec<_> = self.events.values()
            .filter(|e| e.lamport_ts == depth)
            .collect();
        
        if events_at_depth.is_empty() {
            return *blake3::hash(b"agora:merkle:empty").as_bytes();
        }
        
        // Compute Merkle tree root
        let mut hashes: Vec<[u8; 32]> = events_at_depth.iter()
            .map(|e| {
                let id = e.event_id.as_bytes();
                *blake3::hash(id).as_bytes()
            })
            .collect();
        
        while hashes.len() > 1 {
            let mut next = Vec::new();
            for pair in hashes.chunks(2) {
                let left = &pair[0];
                let right = pair.get(1).unwrap_or(left);
                
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"agora:merkle:node");
                hasher.update(left);
                hasher.update(right);
                next.push(*hasher.finalize().as_bytes());
            }
            hashes = next;
        }
        
        hashes[0]
    }
}
```

### 3.3 CRDT for State Sync

For conflict-free replicated data types, we evaluate both the `yrs` crate and a custom Last-Writer-Wins Register implementation.

#### Option A: Custom LWWRegister

For simple state like room membership and configuration:

```rust
// src/dag/crdt.rs

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LwwRegister<T> {
    value: T,
    timestamp: u64,
    writer_id: Vec<u8>,
}

impl<T: Clone> LwwRegister<T> {
    pub fn new(initial: T) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            value: initial,
            timestamp,
            writer_id: vec![],
        }
    }
    
    pub fn set(&mut self, value: T, writer_id: Vec<u8>, timestamp: u64) {
        if timestamp > self.timestamp || 
           (timestamp == self.timestamp && writer_id > self.writer_id) {
            self.value = value;
            self.timestamp = timestamp;
            self.writer_id = writer_id;
        }
    }
    
    pub fn get(&self) -> &T {
        &self.value
    }
}

pub struct RoomState {
    pub name: LwwRegister<String>,
    pub topic: LwwRegister<String>,
    pub members: LwwRegister<HashMap<String, MemberInfo>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemberInfo {
    pub display_name: Option<String>,
    pub join_ts: u64,
}

impl RoomState {
    pub fn new(room_id: String) -> Self {
        Self {
            name: LwwRegister::new(room_id),
            topic: LwwRegister::new(String::new()),
            members: LwwRegister::new(HashMap::new()),
        }
    }
}
```

#### Option B: Yrs Integration

For complex collaborative state, integrate the `yrs` crate:

```toml
yrs = "0.19"
```

```rust
use yrs::{Doc, Transact, Observer};
use yrs::sync::SyncMessage;

pub struct YrsState {
    doc: Doc,
}

impl YrsState {
    pub fn new() -> Self {
        Self { doc: Doc::new() }
    }
    
    pub fn apply_update(&mut self, update: &[u8]) {
        let mut txn = self.doc.transact();
        txn.apply_update(update);
    }
    
    pub fn get_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update()
    }
}
```

### 3.4 Lamport Clock

```rust
// src/dag/lamport.rs

use std::sync::atomic::{AtomicU64, Ordering};

pub struct LamportClock {
    clock: AtomicU64,
}

impl LamportClock {
    pub fn new(initial: u64) -> Self {
        Self {
            clock: AtomicU64::new(initial),
        }
    }
    
    pub fn tick(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::SeqCst)
    }
    
    pub fn observe(&self, received: u64) -> u64 {
        loop {
            let current = self.clock.load(Ordering::SeqCst);
            let new = std::cmp::max(current, received) + 1;
            
            if self.clock.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return new;
            }
        }
    }
    
    pub fn now(&self) -> u64 {
        self.clock.load(Ordering::SeqCst)
    }
}
```

### Phase 3 Integration Points

1. **Event persistence**: Integrate EventDag with agora-core's storage layer for persistent event history.

2. **State resolution**: Implement CRDT merge logic in the room state manager.

3. **History sync**: Add state sync protocol messages to enable new peers to catch up on room history.

### Phase 3 Testing Approach

Unit tests verify signature verification, DAG consistency, and CRDT merge behavior. Fuzz tests verify robustness against malicious or corrupted events. Simulation tests verify distributed consensus under various network conditions.

---

## Implementation Details

### Key Types and Traits

```rust
// src/types.rs

use agora_crypto::AgentId;
use async_trait::async_trait;
use std::net::SocketAddr;

pub type Result<T> = std::result::Result<T, P2pError>;

#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("transport error: {0}")]
    Transport(String),
    
    #[error("discovery error: {0}")]
    Discovery(String),
    
    #[error("protocol error: {0}")]
    Protocol(String),
    
    #[error("peer not found: {0}")]
    PeerNotFound(String),
    
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("DHT error: {0}")]
    Dht(String),
    
    #[error("NAT error: {0}")]
    Nat(String),
}

#[async_trait]
pub trait P2pTransport: Send + Sync {
    async fn send_to(&self, peer: &AgentId, message: Vec<u8>) -> Result<()>;
    async fn broadcast(&self, message: Vec<u8>) -> Result<()>;
    async fn receive(&self) -> Result<(AgentId, Vec<u8>)>;
    
    fn local_agent_id(&self) -> &AgentId;
    fn peer_addresses(&self) -> Vec<(AgentId, SocketAddr)>;
}

pub struct P2pConfig {
    pub enabled: bool,
    pub quic_port: u16,
    pub mdns_enabled: bool,
    pub dht_enabled: bool,
    pub dht_bootstrap_nodes: Vec<String>,
    pub stun_servers: Vec<String>,
    pub relay_server: Option<String>,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            quic_port: 58421,
            mdns_enabled: true,
            dht_enabled: false,
            dht_bootstrap_nodes: vec![],
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
            ],
            relay_server: None,
        }
    }
}
```

### Integration with agora-crypto

The P2P implementation integrates with existing agora-crypto types:

| Agora-crypto Type | P2P Usage |
|------------------|-----------|
| `AgentId` | Peer identification, DHT keys |
| `AgentIdentity` | Signing events, handshake authentication |
| `Sigchain` | Verifying peer identity trust level |
| `event_id()` | Generating deterministic event IDs |

### Testing Approach

Each phase includes unit tests for individual components and integration tests that verify end-to-end functionality. Network simulation using tools like `comcast` or network namespaces can test behavior under various network conditions (latency, packet loss, partitions).

---

## Research Summary

### Crate Selection Rationale

| Component | Crate | Version | Rationale |
|-----------|-------|---------|-----------|
| QUIC Transport | `quinn` | 0.11 | Mature, pure Rust, async-native |
| TLS | `rustls` | 0.23 | Memory-safe, modular |
| Certificate Gen | `rcgen` | 0.13 | Simple self-signed certs |
| mDNS | `mdns-sd` | 0.12 | Full mDNS/bonjour implementation |
| CBOR | `ciborium` | 0.2 | No-std compatible, safe |
| DHT Buckets | `buckets` | 0.4 | K-bucket management |
| XOR Distance | `xor-feed` | 0.3 | Kademlia distance metric |
| CRDT | `yrs` (optional) | 0.19 | Production-ready CRDTs |

### Security Considerations

1. **TLS certificates**: Self-signed certificates are appropriate for LAN but must be validated against known peer certificates in production.

2. **DHT security**: The DHT implementation should include verification that returned peers are actually reachable.

3. **NAT traversal**: UDP hole punching behavior varies significantly across NAT types; the relay fallback ensures connectivity at the cost of privacy.

4. **Event signing**: All events must be signed by the sender's Ed25519 key, with signatures verified before inclusion in the DAG.

---

## Implementation Order

1. **Week 1-2**: Set up agora-p2p crate, implement QUIC transport with TLS
2. **Week 3-4**: Implement mDNS discovery and peer management
3. **Week 5-6**: Implement wire protocol and message handling
4. **Week 7-8**: Basic P2P messaging and room sync
5. **Week 9-10**: DHT implementation and bootstrap
6. **Week 11-12**: NAT traversal and relay fallback
7. **Week 13-16**: Event DAG and CRDT (if time permits)

---

## Future Considerations

1. **Tor/I2P support**: Add optional anonymous routing for enhanced privacy
2. **Partial sync**: Implement state snapshots for faster room joining
3. **Offline support**: Queue messages for delivery when peers come online
4. **Rate limiting**: Prevent DoS from malicious peers
5. **Peer reputation**: Build trust scores based on behavior
