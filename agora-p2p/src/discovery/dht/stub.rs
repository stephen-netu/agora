//! In-memory stub DhtProvider for development and testing.
//! No network I/O — stores peers in a BTreeMap.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;
use sovereign_sdk::AgentId;

use crate::error::Error;
use super::provider::{DhtProvider, DhtPeer};

pub struct StubDhtProvider {
    local_id: AgentId,
    peers: Arc<RwLock<BTreeMap<AgentId, DhtPeer>>>,
    sequence: Arc<AtomicU64>,
}

impl StubDhtProvider {
    pub fn new(local_id: AgentId) -> Self {
        Self {
            local_id,
            peers: Arc::new(RwLock::new(BTreeMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl DhtProvider for StubDhtProvider {
    fn find_peer(
        &self,
        agent_id: &AgentId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<DhtPeer>, Error>> + Send + '_>> {
        let agent_id = agent_id.clone();
        let peers = self.peers.clone();
        Box::pin(async move {
            let guard = peers.read().await;
            Ok(guard.get(&agent_id).cloned().into_iter().collect())
        })
    }

    fn store_self(
        &self,
        agent_id: &AgentId,
        addrs: Vec<SocketAddr>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        let agent_id = agent_id.clone();
        let peers = self.peers.clone();
        let sequence = self.sequence.clone();
        Box::pin(async move {
            let seq = sequence.fetch_add(1, Ordering::SeqCst);
            let mut guard = peers.write().await;
            guard.insert(agent_id.clone(), DhtPeer {
                agent_id,
                addresses: addrs,
                last_seen_seq: seq,
            });
            Ok(())
        })
    }

    fn bootstrap(
        &self,
        _seeds: &[SocketAddr],
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn local_id(&self) -> &AgentId {
        &self.local_id
    }
}
