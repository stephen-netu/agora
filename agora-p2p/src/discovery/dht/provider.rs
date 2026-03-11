//! DhtProvider trait — swappable DHT implementation.
//! S-02: last_seen is a sequence number, not a wall-clock timestamp.

use std::net::SocketAddr;
use std::future::Future;
use std::pin::Pin;

use sovereign_sdk::AgentId;

use crate::error::Error;

/// A peer entry in the DHT routing table
#[derive(Debug, Clone)]
pub struct DhtPeer {
    pub agent_id: AgentId,
    pub addresses: Vec<SocketAddr>,
    /// Sequence number of last seen event (S-02: not wall-clock)
    pub last_seen_seq: u64,
}

/// Swappable DHT backend trait.
pub trait DhtProvider: Send + Sync {
    /// Find peers for an agent ID.
    fn find_peer(
        &self,
        agent_id: &AgentId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<DhtPeer>, Error>> + Send + '_>>;

    /// Announce our own presence at these addresses.
    fn store_self(
        &self,
        agent_id: &AgentId,
        addrs: Vec<SocketAddr>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>>;

    /// Bootstrap from seed nodes.
    fn bootstrap(
        &self,
        seeds: &[SocketAddr],
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>>;

    /// Local DHT node identity.
    fn local_id(&self) -> &AgentId;
}
