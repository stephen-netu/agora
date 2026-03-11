#![deny(dead_code)]
#![forbid(unsafe_code)]
//! Agora P2P Mesh Networking
//! 
//! This crate provides peer-to-peer networking capabilities for Agora,
//! enabling direct communication between peers on local networks.

mod error;
pub mod types;
pub mod transport;
mod protocol;
mod discovery;
mod mesh;
mod node;
pub mod nat;

// Re-export only the public API
pub use sovereign_sdk::AgentId;
pub use node::{P2pNode, MeshEvent};
pub use types::{P2pConfig, TransportMode, Peer, YggdrasilConfig, WanDiscoveryMode, WanConfig, IdentitySource, Multiaddr};
pub use sovereign_sdk::yggdrasil_addr_from_pubkey;
pub use transport::quic::QuicConfig;
pub use protocol::{AmpMessage, SerializedEvent};
pub use discovery::dht::{DhtDiscovery, DhtPeerEvent, DhtProvider, DhtPeer, StubDhtProvider};
pub use error::Error;
