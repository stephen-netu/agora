//! Core types for agora-p2p

use std::sync::Arc;

use std::path::PathBuf;

use agora_crypto::AgentId;
use serde::{Deserialize, Serialize};

/// Source of identity keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IdentitySource {
    /// Read identity from a file (Phases 1–4)
    File(PathBuf),
    /// Delegate to sovereignd daemon socket (Phase 5)
    Daemon(PathBuf),
}

impl Default for IdentitySource {
    fn default() -> Self {
        let default_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agora")
            .join("identity.key");
        IdentitySource::File(default_path)
    }
}

impl IdentitySource {
    /// Check if the identity source is available.
    /// For File source, checks if the file exists.
    /// For Daemon source, checks if the socket is reachable.
    /// Returns true if identity can be resolved.
    pub async fn is_available(&self) -> bool {
        match self {
            IdentitySource::File(path) => tokio::fs::metadata(path).await.is_ok(),
            IdentitySource::Daemon(socket_path) => {
                tokio::net::UnixStream::connect(socket_path)
                    .await
                    .is_ok()
            }
        }
    }

    /// Get the resolved AgentId from this source.
    /// For File source, reads and derives the key.
    /// For Daemon source, queries the daemon.
    pub async fn resolve_agent_id(&self) -> Result<AgentId, String> {
        match self {
            IdentitySource::File(path) => {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| format!("failed to read identity file: {}", e))?;
                if bytes.len() != 32 {
                    return Err("identity file must be 32 bytes".to_string());
                }
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(&bytes);
                Ok(AgentId::from_bytes(&key_bytes)
                    .map_err(|e| format!("invalid identity key: {}", e))?)
            }
            IdentitySource::Daemon(socket_path) => {
                let _ = socket_path;
                Err("sovereignd daemon identity resolution not implemented yet".to_string())
            }
        }
    }
}

use crate::transport::quic::QuicConfig as QuicConfigInner;

/// A peer in the P2P mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub agent_id: AgentId,
    pub addresses: Vec<String>,
}

/// Local peer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Identity source for this peer
    pub identity_source: IdentitySource,
    /// AgentId (resolved from identity_source)
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
            identity_source: IdentitySource::default(),
            agent_id: AgentId::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
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
