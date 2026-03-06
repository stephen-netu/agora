//! Core types for agora-p2p

use agora_crypto::AgentId;
use serde::{Deserialize, Serialize};

/// A peer in the P2P mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub agent_id: AgentId,
    pub addresses: Vec<String>,
}

/// Local peer configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Agent identity for this peer
    pub agent_id: AgentId,
    /// Port to listen on for QUIC
    pub listen_port: u16,
    /// Service name for mDNS advertisement
    pub service_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agent_id: AgentId::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            listen_port: 0,
            service_name: "_agora._udp.local.".to_string(),
        }
    }
}
