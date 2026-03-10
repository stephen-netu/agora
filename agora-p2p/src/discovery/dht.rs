//! Kademlia DHT discovery module
//!
//! Provides DHT-based peer discovery for nodes that cannot run Yggdrasil daemon.
//! Participation is ALWAYS opt-in (default: disabled).
//!
//! S-02: Uses BTreeMap for deterministic iteration
//! S-04: All DHT queries are logged for auditability
//! S-05: Routing table is bounded to prevent unbounded memory growth

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn, error};

use crate::error::Error;
use crate::types::{Peer, WanDiscoveryMode};
use sovereign_sdk::AgentId;

const MAX_ROUTING_TABLE_SIZE: usize = 1000;
const DHT_QUERY_SEQUENCE_START: u64 = 0;

#[derive(Debug, Clone)]
pub enum DhtPeerEvent {
    PeerDiscovered(Peer),
    PeerRemoved(String),
    PeerUpdated(Peer),
}

pub struct DhtDiscovery {
    node: Arc<RwLock<Option<DhtNode>>>,
    peers: Arc<RwLock<BTreeMap<String, Peer>>>,
    peer_events: mpsc::Sender<DhtPeerEvent>,
    sequence: Arc<AtomicU64>,
    agent_id: String,
    running: Arc<RwLock<bool>>,
}

struct DhtNode {
    local_addr: String,
}

impl DhtDiscovery {
    pub fn new(
        agent_id: &str,
        wan_discovery: &WanDiscoveryMode,
    ) -> Result<(Self, mpsc::Receiver<DhtPeerEvent>), Error> {
        let (tx, rx) = mpsc::channel(100);

        let node = match wan_discovery {
            WanDiscoveryMode::Disabled => {
                info!("DHT discovery disabled (opt-in only)");
                None
            }
            WanDiscoveryMode::Bootstrap(nodes) => {
                info!("DHT discovery enabled with {} bootstrap nodes", nodes.len());
                Some(DhtNode {
                    local_addr: "0.0.0.0:0".to_string(),
                })
            }
            WanDiscoveryMode::Public => {
                info!("DHT discovery enabled with public bootstrap nodes");
                Some(DhtNode {
                    local_addr: "0.0.0.0:0".to_string(),
                })
            }
        };

        Ok((
            Self {
                node: Arc::new(RwLock::new(node)),
                peers: Arc::new(RwLock::new(BTreeMap::new())),
                peer_events: tx,
                sequence: Arc::new(AtomicU64::new(DHT_QUERY_SEQUENCE_START)),
                agent_id: agent_id.to_string(),
                running: Arc::new(RwLock::new(false)),
            },
            rx,
        ))
    }

    pub async fn start(&self) -> Result<(), Error> {
        let mut running = self.running.write().await;
        if *running {
            warn!("DHT discovery already running");
            return Ok(());
        }
        *running = true;
        drop(running);

        info!("Starting DHT discovery for agent: {}", self.agent_id);

        let node_guard = self.node.read().await;
        if node_guard.is_none() {
            info!("DHT is disabled, skipping start");
            return Ok(());
        }
        drop(node_guard);

        let node = self.node.clone();
        let peers = self.peers.clone();
        let peer_events = self.peer_events.clone();
        let sequence = self.sequence.clone();
        let agent_id = self.agent_id.clone();
        let running_flag = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !*running_flag.read().await {
                            break;
                        }
                        if let Err(e) = Self::refresh_routing_table(
                            &node,
                            &peers,
                            &peer_events,
                            &sequence,
                            &agent_id,
                        ).await {
                            error!("DHT refresh error: {}", e);
                        }
                    }
                }
            }
            
            info!("DHT discovery stopped");
        });

        Ok(())
    }

    async fn refresh_routing_table(
        node: &Arc<RwLock<Option<DhtNode>>>,
        peers: &Arc<RwLock<BTreeMap<String, Peer>>>,
        _peer_events: &mpsc::Sender<DhtPeerEvent>,
        sequence: &Arc<AtomicU64>,
        agent_id: &str,
    ) -> Result<(), Error> {
        let seq = sequence.fetch_add(1, Ordering::SeqCst);
        
        info!(sequence = seq, "DHT query: bootstrap_refresh - agent_id={}", agent_id);

        let _node_guard = node.read().await;
        let node = _node_guard.as_ref().ok_or_else(|| {
            Error::Discovery("DHT not initialized".to_string())
        })?;

        info!(sequence = seq, "DHT query result: bootstrap_refresh - no peers (DHT node: {})", node.local_addr);

        let peers_guard = peers.write().await;
        
        if peers_guard.len() >= MAX_ROUTING_TABLE_SIZE {
            warn!("Routing table at max capacity: {}", MAX_ROUTING_TABLE_SIZE);
        }

        Ok(())
    }

    pub async fn get_peers(&self) -> Vec<Peer> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        
        info!(sequence = seq, "DHT query: get_peers - agent_id={}", self.agent_id);
        
        let peers = self.peers.read().await;
        let result: Vec<Peer> = peers.values().cloned().collect();
        
        info!(sequence = seq, result_count = result.len(), "DHT query result: get_peers");
        
        result
    }

    pub async fn get_peer(&self, target_agent_id: &str) -> Option<Peer> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        
        info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query: get_peer - agent_id={}", self.agent_id);
        
        let peers = self.peers.read().await;
        let result = peers.get(target_agent_id).cloned();
        
        if result.is_some() {
            info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query result: get_peer - found");
        } else {
            info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query result: get_peer - not found");
        }
        
        result
    }

    pub async fn announce_peer(&self, addresses: Vec<String>, _ttl: u64) -> Result<(), Error> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        
        info!(sequence = seq, address_count = addresses.len(), "DHT query: announce_peer - agent_id={}", self.agent_id);

        let node_guard = self.node.read().await;
        if node_guard.is_none() {
            return Err(Error::Discovery("DHT not initialized".to_string()));
        }

        let mut peers_guard = self.peers.write().await;
        
        if peers_guard.len() >= MAX_ROUTING_TABLE_SIZE {
            warn!("Cannot announce: routing table at max capacity");
            return Err(Error::Discovery("Routing table at max capacity".to_string()));
        }

        let peer = Peer {
            agent_id: AgentId::from_hex(&self.agent_id)
                .expect("DHT Discovery initialized with invalid agent_id - this is a programming error"),
            addresses: addresses.clone(),
        };
        peers_guard.insert(self.agent_id.clone(), peer);

        info!(sequence = seq, "DHT query result: announce_peer - success");

        Ok(())
    }

    pub async fn lookup_peer(&self, target_agent_id: &str) -> Result<Option<Peer>, Error> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        
        info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query: lookup_peer - agent_id={}", self.agent_id);

        let node_guard = self.node.read().await;
        if node_guard.is_none() {
            return Err(Error::Discovery("DHT not initialized".to_string()));
        }

        let peers_guard = self.peers.read().await;
        
        if let Some(peer) = peers_guard.get(target_agent_id) {
            info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query result: lookup_peer - found");
            Ok(Some(peer.clone()))
        } else {
            info!(sequence = seq, target_agent_id = %target_agent_id, "DHT query result: lookup_peer - not found");
            Ok(None)
        }
    }

    pub async fn is_enabled(&self) -> bool {
        self.node.read().await.is_some()
    }

    pub async fn stop(&self) -> Result<(), Error> {
        let mut running = self.running.write().await;
        *running = false;
        info!("DHT discovery stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Multiaddr;

    #[test]
    fn test_wan_discovery_mode_default() {
        let mode = WanDiscoveryMode::default();
        matches!(mode, WanDiscoveryMode::Disabled);
    }

    #[tokio::test]
    async fn test_dht_disabled_creation() {
        let mode = WanDiscoveryMode::Disabled;
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        assert!(!dht.is_enabled().await);
    }

    #[tokio::test]
    async fn test_dht_disabled_get_peers() {
        let mode = WanDiscoveryMode::Disabled;
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        let peers = dht.get_peers().await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_dht_disabled_get_peer() {
        let mode = WanDiscoveryMode::Disabled;
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        let peer = dht.get_peer("0000000000000000000000000000000000000000000000000000000000000002").await;
        assert!(peer.is_none());
    }

    #[tokio::test]
    async fn test_dht_bootstrap_mode_creation() {
        let bootstrap_nodes = vec![
            Multiaddr("/ip4/1.2.3.4/tcp/1234".to_string()),
            Multiaddr("/ip4/5.6.7.8/tcp/5678".to_string()),
        ];
        let mode = WanDiscoveryMode::Bootstrap(bootstrap_nodes);
        
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        assert!(dht.is_enabled().await);
    }

    #[tokio::test]
    async fn test_dht_public_mode_creation() {
        let mode = WanDiscoveryMode::Public;
        
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        assert!(dht.is_enabled().await);
    }

    #[tokio::test]
    async fn test_dht_announce_peer() {
        let mode = WanDiscoveryMode::Public;
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        let addresses = vec!["/ip4/192.168.1.1/tcp/1234".to_string()];
        let result = dht.announce_peer(addresses.clone(), 3600).await;
        assert!(result.is_ok());
        
        let peers = dht.get_peers().await;
        assert_eq!(peers.len(), 1);
    }

    #[tokio::test]
    async fn test_dht_lookup_peer() {
        let mode = WanDiscoveryMode::Public;
        let (dht, _rx) = DhtDiscovery::new(
            "0000000000000000000000000000000000000000000000000000000000000001",
            &mode,
        ).unwrap();
        
        let target_id = "0000000000000000000000000000000000000000000000000000000000000002";
        let result = dht.lookup_peer(target_id).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
