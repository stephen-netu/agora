//! Tree-based routing table for mesh networking
//!
//! Provides deterministic routing table implementation using BTreeMap
//! for deterministic iteration order (S-02 compliance).
//!
//! # S-02 Compliance
//! - Uses BTreeMap for deterministic iteration
//! - No HashMap or non-deterministic collections
//! - Sequence-based timestamps where needed

use sovereign_sdk::AgentId;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{error, info, trace, warn};

use crate::discovery::dht::DhtDiscovery;

#[derive(Debug, Clone)]
pub struct RoutingEntry {
    pub agent_id: AgentId,
    pub socket_addr: SocketAddr,
    pub sequence: u64,
    pub path_metric: u32,
    pub is_connected: bool,
}

pub struct RoutingTable {
    entries: BTreeMap<AgentId, RoutingEntry>,
    sequence_counter: AtomicU64,
    dht: Option<Arc<DhtDiscovery>>,
}

impl RoutingTable {
    pub fn new() -> Self {
        info!(target: "mesh", "Initialized new routing table");
        Self {
            entries: BTreeMap::new(),
            sequence_counter: AtomicU64::new(0),
            dht: None,
        }
    }

    pub fn with_dht(dht: Arc<DhtDiscovery>) -> Self {
        info!(target: "mesh", "Initialized routing table with DHT integration");
        Self {
            entries: BTreeMap::new(),
            sequence_counter: AtomicU64::new(0),
            dht: Some(dht),
        }
    }

    fn next_sequence(&mut self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert(&mut self, agent_id: AgentId, socket_addr: SocketAddr) -> Option<RoutingEntry> {
        trace!(target: "mesh", "Inserting route for agent: {}", agent_id);

        let entry = RoutingEntry {
            agent_id: agent_id.clone(),
            socket_addr,
            sequence: self.next_sequence(),
            path_metric: 0,
            is_connected: true,
        };

        self.entries.insert(agent_id, entry)
    }

    pub fn insert_with_metric(
        &mut self,
        agent_id: AgentId,
        socket_addr: SocketAddr,
        metric: u32,
    ) -> Option<RoutingEntry> {
        trace!(target: "mesh", "Inserting route for {} with metric {}", agent_id, metric);

        let entry = RoutingEntry {
            agent_id: agent_id.clone(),
            socket_addr,
            sequence: self.next_sequence(),
            path_metric: metric,
            is_connected: true,
        };

        self.entries.insert(agent_id, entry)
    }

    pub fn lookup(&self, agent_id: &AgentId) -> Option<&RoutingEntry> {
        let entry = self.entries.get(agent_id);
        if entry.is_some() {
            trace!(target: "mesh", "Found route for agent: {}", agent_id);
        }
        entry
    }

    pub fn lookup_mut(&mut self, agent_id: &AgentId) -> Option<&mut RoutingEntry> {
        self.entries.get_mut(agent_id)
    }

    pub fn remove(&mut self, agent_id: &AgentId) -> Option<RoutingEntry> {
        trace!(target: "mesh", "Removing route for agent: {}", agent_id);
        self.entries.remove(agent_id)
    }

    pub fn update_socket(&mut self, agent_id: &AgentId, socket_addr: SocketAddr) -> bool {
        let new_sequence = self.next_sequence();
        if let Some(entry) = self.entries.get_mut(agent_id) {
            entry.socket_addr = socket_addr;
            entry.sequence = new_sequence;
            trace!(target: "mesh", "Updated socket for {}: {}", agent_id, socket_addr);
            true
        } else {
            warn!(target: "mesh", "Attempted to update non-existent route for {}", agent_id);
            false
        }
    }

    pub fn update_metric(&mut self, agent_id: &AgentId, metric: u32) -> bool {
        let new_sequence = self.next_sequence();
        if let Some(entry) = self.entries.get_mut(agent_id) {
            entry.path_metric = metric;
            entry.sequence = new_sequence;
            trace!(target: "mesh", "Updated metric for {}: {}", agent_id, metric);
            true
        } else {
            false
        }
    }

    pub fn set_connected(&mut self, agent_id: &AgentId, connected: bool) -> bool {
        let new_sequence = self.next_sequence();
        if let Some(entry) = self.entries.get_mut(agent_id) {
            entry.is_connected = connected;
            entry.sequence = new_sequence;
            trace!(target: "mesh", "Set connected status for {}: {}", agent_id, connected);
            true
        } else {
            false
        }
    }

    pub fn contains(&self, agent_id: &AgentId) -> bool {
        self.entries.contains_key(agent_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AgentId, &RoutingEntry)> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&AgentId, &mut RoutingEntry)> {
        self.entries.iter_mut()
    }

    pub fn connected_peers(&self) -> impl Iterator<Item = (&AgentId, &RoutingEntry)> {
        self.entries.iter().filter(|(_, e)| e.is_connected)
    }

    pub fn routes_by_metric(&self) -> Vec<(&AgentId, &RoutingEntry)> {
        let mut routes: Vec<_> = self.entries.iter().collect();
        routes.sort_by_key(|(_, e)| e.path_metric);
        routes
    }

    pub fn clear(&mut self) {
        let count = self.entries.len();
        self.entries.clear();
        info!(target: "mesh", "Cleared {} routes from routing table", count);
    }

    pub fn sequence(&self) -> u64 {
        self.sequence_counter.load(Ordering::SeqCst)
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

// IMPLEMENTATION_REQUIRED: Iterator for routing table traversal - used in Phase 3 DHT integration
#[allow(dead_code)]
pub struct RoutingTableIterator {
    entries: std::collections::btree_map::Iter<'static, AgentId, RoutingEntry>,
}

impl RoutingTable {
    pub fn drain_connected(&mut self) -> impl Iterator<Item = (AgentId, RoutingEntry)> + '_ {
        self.entries
            .iter()
            .filter(|(_, e)| e.is_connected)
            .map(|(k, v)| (k.clone(), v.clone()))
    }
}

// DHT-based route discovery (Phase 2-3)
// Integrates with DHT module for distributed route advertisement and discovery.
impl RoutingTable {
    pub async fn advertise_to_dht(&self, agent_id: &AgentId) -> Result<(), String> {
        let seq = self.next_sequence_for_dht();

        let dht = self.dht.as_ref().ok_or_else(|| {
            error!(target: "mesh", sequence = seq, "DHT not configured for routing table");
            "DHT not configured".to_string()
        })?;

        let addresses: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.is_connected)
            .map(|(_, e)| format!("{}", e.socket_addr))
            .collect();

        info!(target: "mesh", sequence = seq, agent_id = %agent_id, address_count = addresses.len(), "DHT advertisement: announcing routes");

        if addresses.is_empty() {
            warn!(target: "mesh", sequence = seq, "No connected routes to advertise to DHT");
            return Ok(());
        }

        match dht.announce_peer(addresses, 3600).await {
            Ok(_) => {
                info!(target: "mesh", sequence = seq, agent_id = %agent_id, "DHT advertisement: success");
                Ok(())
            }
            Err(e) => {
                error!(target: "mesh", sequence = seq, agent_id = %agent_id, error = %e, "DHT advertisement: failed");
                Err(format!("DHT advertisement failed: {}", e))
            }
        }
    }

    pub async fn discover_from_dht(&mut self, agent_id: &AgentId) -> Result<Vec<RoutingEntry>, String> {
        let seq = self.next_sequence_for_dht();

        let dht = self.dht.as_ref().ok_or_else(|| {
            error!(target: "mesh", sequence = seq, "DHT not configured for routing table");
            "DHT not configured".to_string()
        })?;

        let agent_id_hex = agent_id.to_hex();
        info!(target: "mesh", sequence = seq, target_agent_id = %agent_id, "DHT discovery: looking up peer");

        match dht.lookup_peer(&agent_id_hex).await {
            Ok(Some(peer)) => {
                info!(target: "mesh", sequence = seq, target_agent_id = %agent_id, address_count = peer.addresses.len(), "DHT discovery: peer found");

                let next_seq = self.next_sequence();
                let entries: Vec<RoutingEntry> = peer
                    .addresses
                    .iter()
                    .filter_map(|addr| {
                        addr.parse::<SocketAddr>().ok().map(|socket_addr| {
                            RoutingEntry {
                                agent_id: agent_id.clone(),
                                socket_addr,
                                sequence: next_seq,
                                path_metric: 0,
                                is_connected: false,
                            }
                        })
                    })
                    .collect();

                info!(target: "mesh", sequence = seq, target_agent_id = %agent_id, entry_count = entries.len(), "DHT discovery: converted to routing entries");
                Ok(entries)
            }
            Ok(None) => {
                info!(target: "mesh", sequence = seq, target_agent_id = %agent_id, "DHT discovery: peer not found");
                Ok(Vec::new())
            }
            Err(e) => {
                error!(target: "mesh", sequence = seq, target_agent_id = %agent_id, error = %e, "DHT discovery: lookup failed");
                Err(format!("DHT lookup failed: {}", e))
            }
        }
    }

    fn next_sequence_for_dht(&self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }
}

// Path metric calculation (Phase 3)
// Implements weighted path selection based on RTT and bandwidth.
impl RoutingEntry {
    pub fn calculate_metric(&self, rtt_ms: u32, bandwidth_mbps: u32) -> u32 {
        const RTT_WEIGHT: u32 = 10;
        const BW_WEIGHT: u32 = 1;
        const BASE_METRIC: u32 = 100;

        let metric = BASE_METRIC + (rtt_ms * RTT_WEIGHT) + (bandwidth_mbps * BW_WEIGHT);

        trace!(target: "mesh", rtt_ms = rtt_ms, bandwidth_mbps = bandwidth_mbps, metric = metric, "Calculated path metric");

        metric
    }
}

// IPv6 subnet routing (Phase 3)
// Yggdrasil uses IPv6 subnet routing for reachability.
// This handles /64 subnet announcements for route delegation.
impl RoutingEntry {
    pub fn derive_subnet_prefix(&self) -> Option<String> {
        // DEVELOPMENT_BLOCKER: S-01 — Yggdrasil subnet derivation requires
        // integration with kernel's Yggdrasil address handling
        // ARCHITECTURE_PENDING: Implement /64 subnet prefix derivation from agent's Yggdrasil address
        None
    }

    pub fn advertise_subnet(&self, _prefix: &str) -> Result<(), String> {
        // ARCHITECTURE_PENDING: Implement subnet advertisement via Yggdrasil
        Err("Yggdrasil subnet advertisement not yet implemented".to_string())
    }

    pub fn lookup_subnet_routes(&self, _subnet: &str) -> Result<Vec<AgentId>, String> {
        // ARCHITECTURE_PENDING: Implement subnet route lookup
        Err("Yggdrasil subnet lookup not yet implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddrV4;
    use std::str::FromStr;

    #[test]
    fn test_insert_and_lookup() {
        let mut table = RoutingTable::new();
        let agent_id = AgentId::from_bytes([0u8; 32]);
        let addr = SocketAddr::V4(SocketAddrV4::from_str("192.168.1.1:1234").unwrap());

        table.insert(agent_id.clone(), addr);

        let entry = table.lookup(&agent_id).unwrap();
        assert_eq!(entry.socket_addr, addr);
    }

    #[test]
    fn test_deterministic_iteration() {
        let mut table = RoutingTable::new();

        for i in (0..10).rev() {
            let mut id = [0u8; 32];
            id[0] = i;
            let agent_id = AgentId::from_bytes(id);
            let addr = SocketAddr::V4(SocketAddrV4::from_str("192.168.1.1:1234").unwrap());
            table.insert(agent_id, addr);
        }

        let keys: Vec<_> = table.iter().map(|(k, _)| k.as_bytes()[0]).collect();
        assert_eq!(keys, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_remove() {
        let mut table = RoutingTable::new();
        let agent_id = AgentId::from_bytes([1u8; 32]);
        let addr = SocketAddr::V4(SocketAddrV4::from_str("192.168.1.1:1234").unwrap());

        table.insert(agent_id.clone(), addr);
        let removed = table.remove(&agent_id);
        assert!(removed.is_some());
        assert!(table.lookup(&agent_id).is_none());
    }
}
