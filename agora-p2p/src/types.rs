//! Core types for agora-p2p

use std::sync::Arc;

use agora_crypto::AgentId;
use serde::{Deserialize, Serialize};

use crate::transport::quic::QuicConfig as QuicConfigInner;

/// A peer in the P2P mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub agent_id: AgentId,
    pub addresses: Vec<String>,
}

/// Local peer configuration
#[derive(Debug, Clone)]
pub struct P2pConfig {
    /// Agent identity for this peer
    pub agent_id: AgentId,
    /// Port to listen on for QUIC
    pub listen_port: u16,
    /// Service name for mDNS advertisement
    pub service_name: String,
    /// Transport mode for P2P communication
    pub transport: TransportMode,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            agent_id: AgentId::from_hex(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            listen_port: 0,
            service_name: "_agora._udp.local.".to_string(),
            transport: TransportMode::Auto,
        }
    }
}

/// Configuration for Yggdrasil transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YggdrasilConfig {
    /// Admin socket path for Yggdrasil daemon
    /// If None, uses platform default
    pub admin_socket: Option<String>,
    /// Port to listen on for QUIC over Yggdrasil
    pub listen_port: u16,
}

impl Default for YggdrasilConfig {
    fn default() -> Self {
        Self {
            admin_socket: None,
            listen_port: 0,
        }
    }
}

/// Transport mode for P2P communication
#[derive(Clone)]
pub enum TransportMode {
    /// QUIC transport with custom configuration
    Quic(Arc<QuicConfigInner>),
    /// Yggdrasil mesh transport with custom configuration
    Yggdrasil(YggdrasilConfig),
    /// Automatically select transport based on availability
    Auto,
}

impl Default for TransportMode {
    fn default() -> Self {
        TransportMode::Auto
    }
}

impl std::fmt::Debug for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Quic(_) => write!(f, "TransportMode::Quic(...)"),
            Self::Yggdrasil(config) => write!(f, "TransportMode::Yggdrasil({:?})", config),
            Self::Auto => write!(f, "TransportMode::Auto"),
        }
    }
}
