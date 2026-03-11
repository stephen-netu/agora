//! Core types for agora-p2p

use std::sync::Arc;

use std::path::PathBuf;

use sovereign_sdk::AgentId;
use serde::{Deserialize, Serialize};

/// Source of identity keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IdentitySource {
    /// Read identity from a file (Phases 1–4)
    File(PathBuf),
    /// Delegate to sovereignd daemon socket (Phase 5)
    Daemon(PathBuf),
    /// Testing variant that directly provides the agent_id without file I/O
    Testing(AgentId),
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
    /// For Testing source, always returns true.
    /// Returns true if identity can be resolved.
    pub async fn is_available(&self) -> bool {
        match self {
            IdentitySource::File(path) => tokio::fs::metadata(path).await.is_ok(),
            IdentitySource::Daemon(socket_path) => {
                tokio::net::UnixStream::connect(socket_path)
                    .await
                    .is_ok()
            }
            IdentitySource::Testing(_) => true,
        }
    }

    /// Get the resolved AgentId from this source.
    /// For File source, reads and derives the key.
    /// For Daemon source, queries the daemon.
    /// For Testing source, returns the embedded agent_id directly.
    pub async fn resolve_agent_id(&self) -> Result<AgentId, String> {
        match self {
            IdentitySource::File(path) => {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| format!("failed to read identity file: {}", e))?;
                // Support both 32-byte (legacy) and 64-byte (sovereignd) formats
                // 64-byte format: secret key (32 bytes) + verifying key (32 bytes)
                let secret_key = if bytes.len() == 64 {
                    &bytes[0..32]
                } else if bytes.len() == 32 {
                    &bytes[..]
                } else {
                    return Err("identity file must be 32 or 64 bytes".to_string());
                };
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(secret_key);
                Ok(AgentId::from_bytes(key_bytes))
            }
            IdentitySource::Daemon(socket_path) => {
                let _ = socket_path;
                Err("sovereignd daemon identity resolution not implemented yet".to_string())
            }
            IdentitySource::Testing(agent_id) => Ok(agent_id.clone()),
        }
    }
}

use crate::transport::quic::QuicConfig as QuicConfigInner;

/// WAN discovery mode for P2P peers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WanDiscoveryMode {
    /// WAN discovery is disabled
    #[default]
    Disabled,
    /// Use specific bootstrap nodes for WAN discovery
    Bootstrap(Vec<Multiaddr>),
    /// Use public/default bootstrap nodes for WAN discovery
    Public,
}

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
    #[serde(skip)]
    pub transport: TransportMode,
    /// WAN discovery mode
    pub wan_discovery: WanDiscoveryMode,
    /// WAN discovery configuration (seeds, STUN servers, DHT port)
    pub wan_config: WanConfig,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            identity_source: IdentitySource::default(),
            agent_id: AgentId::from_bytes([0u8; 32]),
            listen_port: 0,
            service_name: "_agora._udp.local.".to_string(),
            transport: TransportMode::Auto,
            wan_discovery: WanDiscoveryMode::default(),
            wan_config: WanConfig::default(),
        }
    }
}

/// Configuration for Yggdrasil transport
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YggdrasilConfig {
    /// Admin socket path for Yggdrasil daemon
    /// If None, uses platform default
    pub admin_socket: Option<String>,
    /// Port to listen on for QUIC over Yggdrasil
    pub listen_port: u16,
}

/// Transport mode for P2P communication
#[derive(Clone, Default)]
pub enum TransportMode {
    /// QUIC transport with custom configuration
    Quic(Arc<QuicConfigInner>),
    /// Yggdrasil mesh transport with custom configuration
    Yggdrasil(YggdrasilConfig),
    /// Pure Rust mesh transport (Yggdrasil-compatible, no external daemon)
    RustMesh(crate::transport::rust_mesh_transport::RustMeshConfig),
    /// Automatically select transport based on availability
    #[default]
    Auto,
}

/// Multiaddress type for P2P connections (simple string wrapper)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Multiaddr(pub String);

/// WAN discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WanConfig {
    /// Seed nodes for DHT bootstrap (hostname:port or IP:port)
    #[serde(default = "default_wan_seeds")]
    pub seeds: Vec<String>,
    /// STUN servers for NAT detection
    #[serde(default = "default_wan_stun_servers")]
    pub stun_servers: Vec<String>,
    /// UDP port for DHT (separate from QUIC port)
    #[serde(default = "default_wan_dht_port")]
    pub dht_port: u16,
    /// Enable NAT hole-punching
    #[serde(default)]
    pub enable_hole_punch: bool,
}

fn default_wan_seeds() -> Vec<String> {
    vec![
        "seeds.agora0.io:6881".to_string(),
        "seeds.agora1.io:6881".to_string(),
        "seeds.agora2.io:6881".to_string(),
    ]
}

fn default_wan_stun_servers() -> Vec<String> {
    vec!["stun.l.google.com:19302".to_string()]
}

fn default_wan_dht_port() -> u16 {
    6881
}

impl Default for WanConfig {
    fn default() -> Self {
        Self {
            seeds: default_wan_seeds(),
            stun_servers: default_wan_stun_servers(),
            dht_port: default_wan_dht_port(),
            enable_hole_punch: false,
        }
    }
}

impl std::fmt::Debug for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Quic(_) => write!(f, "TransportMode::Quic(...)"),
            Self::Yggdrasil(config) => write!(f, "TransportMode::Yggdrasil({:?})", config),
            Self::RustMesh(cfg) => write!(f, "TransportMode::RustMesh({:?})", cfg),
            Self::Auto => write!(f, "TransportMode::Auto"),
        }
    }
}
