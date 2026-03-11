//! Transport trait for P2P communication backends.
//!
//! Implement this trait to plug a new transport into P2pNode.
//! S-02: No wall-clock time in any trait method.

use std::net::SocketAddr;
use std::future::Future;
use std::pin::Pin;

use sovereign_sdk::AgentId;

use crate::error::Error;

/// Opaque send handle — transport fills this in when returning a PeerConnection.
pub enum ConnectionInner {
    Quic(crate::transport::quic::QuicConnection),
    /// Placeholder variant until RustMesh send path is wired.
    // IMPLEMENTATION_REQUIRED: wire RustMesh send path in wt-XXX
    RustMesh,
}

/// A live connection to a remote peer, returned by Transport::connect/accept.
pub struct PeerConnection {
    pub peer_id: AgentId,
    pub remote_addr: SocketAddr,
    /// Opaque send handle — transport fills this in.
    pub inner: ConnectionInner,
}

/// Core transport trait. All implementations must be Send + Sync.
pub trait Transport: Send + Sync {
    /// The local socket address this transport is listening on.
    fn local_addr(&self) -> Result<SocketAddr, Error>;

    /// Connect to a remote peer at the given socket address.
    fn connect(
        &self,
        peer_id: &AgentId,
        addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Result<PeerConnection, Error>> + Send + '_>>;

    /// Accept the next incoming connection. Loops until one arrives.
    fn accept(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PeerConnection, Error>> + Send + '_>>;
}
