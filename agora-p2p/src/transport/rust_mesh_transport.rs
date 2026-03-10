//! Pure Rust mesh networking transport for agora-p2p.
//!
//! This module provides a pure Rust implementation of mesh networking
//! that can replace the Yggdrasil daemon dependency. It integrates with
//! the rust_mesh crate for address derivation, routing, and encryption.
//!
//! # Architecture
//!
//! - `RustMeshTransport`: Main transport struct managing the mesh networking stack
//! - `YggdrasilAddress`: Derives IPv6 addresses from Ed25519 public keys
//! - `RoutingTable`: Manages peer routing information
//! - `CryptoProvider`: Provides network-layer E2EE encryption
//!
//! # Integration with rust_mesh
//!
//! When the rust_mesh crate is available, replace the trait implementations
//! with the actual rust_mesh types:
//! - `rust_mesh::YggdrasilAddress` for address derivation
//! - `rust_mesh::RoutingTable` for peer routing
//! - `rust_mesh::CryptoProvider` for network-layer E2EE

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::VerifyingKey;
use tokio::sync::RwLock;
use tracing::{debug, info};

use sovereign_sdk::AgentId;

use crate::error::Error;
use crate::mesh::rust_mesh::{
    CryptoProvider as RustMeshCrypto, RoutingTable as RustMeshRoutingTable,
    YggdrasilAddress as RustMeshYggAddr,
};

/// Configuration for RustMesh transport
#[derive(Debug, Clone)]
pub struct RustMeshConfig {
    pub listen_port: u16,
    pub max_peers: usize,
    pub connection_timeout_ms: u64,
}

impl Default for RustMeshConfig {
    fn default() -> Self {
        Self {
            listen_port: 0,
            max_peers: 128,
            connection_timeout_ms: 5000,
        }
    }
}

/// A peer in the rust_mesh network
#[derive(Debug, Clone)]
pub struct MeshPeer {
    pub agent_id: AgentId,
    pub yggdrasil_addr: YggdrasilAddress,
    pub socket_addr: Option<SocketAddr>,
}

/// RustMesh transport for P2P mesh networking.
///
/// This transport provides a pure Rust alternative to the Yggdrasil daemon,
/// handling:
/// - Address derivation from Ed25519 keys
/// - Peer discovery and routing
/// - Network-layer encryption
#[derive(Clone)]
pub struct RustMeshTransport {
    config: RustMeshConfig,
    local_agent_id: AgentId,
    local_yggdrasil_addr: RustMeshYggAddr,
    routing_table: Arc<RwLock<RustMeshRoutingTable>>,
    crypto_provider: Arc<RustMeshCrypto>,
    peers: Arc<RwLock<BTreeMap<AgentId, MeshPeer>>>,
}

pub type YggdrasilAddr = RustMeshYggAddr;

pub type YggdrasilAddress = RustMeshYggAddr;

/// Trait for cryptographic operations (to be replaced by rust_mesh::CryptoProvider)
pub trait CryptoProvider: Send + Sync {
    fn encrypt(&self, key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String>;
    fn decrypt(&self, key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String>;
}

pub fn derive_address_from_bytes(input_bytes: &[u8; 32]) -> RustMeshYggAddr {
    RustMeshYggAddr::from_public_key(input_bytes)
}

pub fn derive_address_from_keypair(public_key: &VerifyingKey) -> RustMeshYggAddr {
    RustMeshYggAddr::from_public_key(public_key.as_bytes())
}

/// Create a new RustMesh transport instance.
///
/// # Arguments
///
/// * `config` - Transport configuration
/// * `agent_id` - The local agent identity (used as raw bytes for address derivation)
/// * `crypto_provider` - Cryptographic operations provider
///
/// # Returns
///
/// A new `RustMeshTransport` instance
///
/// # Note
///
/// The AgentId is derived from BLAKE3(Ed25519 verifying_key), so it cannot be
/// directly used as an Ed25519 point. This function derives the mesh address
/// using the same SHA-512 scheme as Yggdrasil, treating the AgentId bytes
/// as if they were the public key input.
pub fn new_rust_mesh_transport(
    config: RustMeshConfig,
    agent_id: AgentId,
    crypto_provider: Arc<RustMeshCrypto>,
) -> RustMeshTransport {
    let agent_id_bytes: &[u8; 32] = agent_id.as_bytes();
    let local_yggdrasil_addr = derive_address_from_bytes(agent_id_bytes);

    info!(
        "Created RustMesh transport with address: {}",
        local_yggdrasil_addr
    );

    RustMeshTransport {
        config,
        local_agent_id: agent_id,
        local_yggdrasil_addr,
        routing_table: Arc::new(RwLock::new(RustMeshRoutingTable::new())),
        crypto_provider,
        peers: Arc::new(RwLock::new(BTreeMap::new())),
    }
}

impl RustMeshTransport {
    /// Get the local Yggdrasil IPv6 address
    pub fn local_address(&self) -> RustMeshYggAddr {
        self.local_yggdrasil_addr.clone()
    }

    /// Get the local agent ID
    pub fn agent_id(&self) -> &AgentId {
        &self.local_agent_id
    }

    /// Get the socket address to bind to for QUIC
    pub fn bind_address(&self) -> Option<SocketAddr> {
        if self.local_yggdrasil_addr.is_global() {
            let addr_bytes = self.local_yggdrasil_addr.as_bytes();
            let ipv6 = std::net::Ipv6Addr::from(*addr_bytes);
            Some(SocketAddr::new(std::net::IpAddr::V6(ipv6), self.config.listen_port))
        } else {
            None
        }
    }

    /// Add a peer to the routing table
    pub async fn add_peer(&self, peer: MeshPeer) {
        debug!("Adding peer {} to routing table", peer.agent_id);

        let socket_addr = peer.socket_addr;
        let agent_id = peer.agent_id.clone();

        let mut peers = self.peers.write().await;
        peers.insert(peer.agent_id.clone(), peer);

        if let Some(addr) = socket_addr {
            let mut routing = self.routing_table.write().await;
            routing.insert(agent_id, addr);
        }
    }

    /// Remove a peer from the routing table
    pub async fn remove_peer(&self, agent_id: &AgentId) {
        debug!("Removing peer {} from routing table", agent_id);

        let mut peers = self.peers.write().await;
        peers.remove(agent_id);

        let mut routing = self.routing_table.write().await;
        routing.remove(agent_id);
    }

    /// Get a peer's socket address by agent ID
    pub async fn get_peer_address(&self, agent_id: &AgentId) -> Option<SocketAddr> {
        let routing = self.routing_table.read().await;
        routing.lookup(agent_id).map(|e| e.socket_addr)
    }

    /// Get the routing table
    pub async fn routing_table(&self) -> Arc<RwLock<RustMeshRoutingTable>> {
        self.routing_table.clone()
    }

    /// Get the crypto provider
    pub fn crypto(&self) -> Arc<RustMeshCrypto> {
        self.crypto_provider.clone()
    }

    /// Get all known peers
    pub async fn get_peers(&self) -> Vec<MeshPeer> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    /// Check if an address is a Yggdrasil address (in 200::/7 range)
    pub fn is_yggdrasil_addr(addr: &SocketAddr) -> bool {
        if let std::net::IpAddr::V6(ipv6) = addr.ip() {
            let octets = ipv6.octets();
            (octets[0] & 0xfe) == 0x02
        } else {
            false
        }
    }

    /// Encrypt a message for a specific peer
    pub async fn encrypt_for_peer(
        &self,
        peer_id: &AgentId,
        _plaintext: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let peers = self.peers.read().await;
        let _ = peers
            .get(peer_id)
            .ok_or_else(|| Error::Mesh("Peer not found".to_string()))?;

        Err(Error::Mesh("Encryption not implemented: peer-to-peer encryption uses placeholder crypto that provides no security. Wire to KeyPair for forward-secret session keys.".to_string()))
    }

    /// Decrypt a message from a specific peer
    pub async fn decrypt_from_peer(
        &self,
        peer_id: &AgentId,
        _ciphertext: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let peers = self.peers.read().await;
        let _ = peers
            .get(peer_id)
            .ok_or_else(|| Error::Mesh("Peer not found".to_string()))?;

        Err(Error::Mesh("Decryption not implemented: peer-to-peer encryption uses placeholder crypto that provides no security. Wire to KeyPair for forward-secret session keys.".to_string()))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_from_key() {
        let test_key_bytes: [u8; 32] = [
            0x9d, 0x61, 0xb1, 0x9e, 0x5c, 0x5a, 0xd7, 0x5f,
            0x1e, 0x7d, 0x89, 0x3a, 0x06, 0x8d, 0x83, 0xbd,
            0x68, 0x8e, 0xce, 0x02, 0x1a, 0x05, 0x07, 0xc3,
            0x75, 0x2e, 0xdf, 0x52, 0x87, 0x45, 0x2d, 0x1c,
        ];
        let addr = derive_address_from_bytes(&test_key_bytes);

        assert!(addr.is_global());
    }

    #[test]
    fn test_yggdrasil_addr_is_in_range() {
        let addr = YggdrasilAddr::from_bytes([
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        ]);
        assert!(addr.is_global());

        let addr_outside = RustMeshYggAddr::from_bytes([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        ]);
        assert!(!addr_outside.is_global());
    }

    #[test]
    fn test_routing_table() {
        
        let mut table = RustMeshRoutingTable::new();
        let agent_id = AgentId::from_hex("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20").unwrap();
        let socket_addr: SocketAddr = "[200::1]:5000".parse().unwrap();

        table.insert(agent_id.clone(), socket_addr);
        assert!(table.lookup(&agent_id).is_some());

        table.remove(&agent_id);
        assert!(table.lookup(&agent_id).is_none());
    }

    #[tokio::test]
    async fn test_transport_peer_management() {
        let config = RustMeshConfig::default();
        let agent_id = AgentId::from_hex("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20").unwrap();

        let crypto_provider = Arc::new(RustMeshCrypto::new());

        let transport = new_rust_mesh_transport(
            config,
            agent_id.clone(),
            crypto_provider,
        );

        let peer_agent_id = AgentId::from_hex("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap();
        let peer_ygg_addr = RustMeshYggAddr::from_bytes([
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
        ]);

        let peer = MeshPeer {
            agent_id: peer_agent_id.clone(),
            yggdrasil_addr: peer_ygg_addr,
            socket_addr: Some("[200::2]:5000".parse().unwrap()),
        };

        transport.add_peer(peer).await;

        let peers = transport.get_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].agent_id, peer_agent_id);

        let addr = transport.get_peer_address(&peer_agent_id).await;
        assert!(addr.is_some());
    }
}
