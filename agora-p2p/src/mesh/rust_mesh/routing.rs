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
use tracing::{error, info, trace, warn};

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
    sequence_counter: u64,
}

impl RoutingTable {
    pub fn new() -> Self {
        info!(target: "mesh", "Initialized new routing table");
        Self {
            entries: BTreeMap::new(),
            sequence_counter: 0,
        }
    }

    fn next_sequence(&mut self) -> u64 {
        self.sequence_counter += 1;
        self.sequence_counter
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
        self.sequence_counter
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

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

// IMPLEMENTATION_REQUIRED: DHT-based route discovery
// Currently uses only local table. Need to integrate with DHT for
// distributed route advertisement and discovery.
impl RoutingTable {
    pub fn advertise_to_dht(&self, _agent_id: &AgentId) -> Result<(), &'static str> {
        error!("DHT advertisement not implemented");
        Err("DHT integration requires implementation")
    }

    #[allow(dead_code)]
    pub fn discover_from_dht(
        &self,
        _agent_id: &AgentId,
    ) -> Result<Vec<RoutingEntry>, &'static str> {
        error!("DHT discovery not implemented");
        Err("DHT integration requires implementation")
    }
}

// IMPLEMENTATION_REQUIRED: Path metric calculation
// Current implementation uses simple metric. Need to implement
// proper path selection based on latency, bandwidth, etc.
impl RoutingEntry {
    #[allow(dead_code)]
    pub fn calculate_metric(&self, _rtt_ms: u32, _bandwidth_mbps: u32) -> u32 {
        self.path_metric
    }
}

// IMPLEMENTATION_REQUIRED: IPv6 subnet routing
// Yggdrasil uses IPv6 subnet routing for reachability.
// This implementation needs to handle /64 subnet announcements.

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
