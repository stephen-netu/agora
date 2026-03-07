//! Replay protection for P2P messages.
//! 
//! Tracks sequence numbers per peer to prevent replay attacks.
//! Uses a sliding window approach to keep memory bounded.

use std::collections::{BTreeSet, BTreeMap};
use std::sync::Arc;

use agora_crypto::AgentId;
use tokio::sync::RwLock;

/// Maximum number of sequence numbers to track per peer.
const MAX_SEQUENCES_PER_PEER: usize = 1000;

/// Replay protection tracker for P2P connections.
///
/// This struct tracks used sequence numbers for each peer to prevent
/// replay attacks on Yggdrasil-routed messages.
#[derive(Clone)]
pub struct ReplayProtection {
    /// Map from peer AgentId to their used sequence numbers.
    sequences: Arc<RwLock<BTreeMap<AgentId, BTreeSet<u64>>>>,
}

impl ReplayProtection {
    /// Create a new ReplayProtection tracker.
    pub fn new() -> Self {
        Self {
            sequences: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Check if a sequence number has been used from a peer.
    ///
    /// Returns `true` if the sequence has been seen before (replay detected).
    /// Returns `false` if this is a new sequence.
    pub async fn is_sequence_used(&self, peer_id: &AgentId, sequence: u64) -> bool {
        let sequences = self.sequences.read().await;
        sequences
            .get(peer_id)
            .map(|set| set.contains(&sequence))
            .unwrap_or(false)
    }

    /// Mark a sequence number as used for a peer.
    ///
    /// If the number of tracked sequences exceeds `MAX_SEQUENCES_PER_PEER`,
    /// older sequences are removed to maintain the sliding window.
    pub async fn mark_sequence_used(&self, peer_id: &AgentId, sequence: u64) {
        let mut sequences = self.sequences.write().await;
        
        let peer_sequences = sequences.entry(peer_id.clone()).or_insert_with(BTreeSet::new);
        peer_sequences.insert(sequence);
        
        // Sliding window: keep only recent sequences
        while peer_sequences.len() >= MAX_SEQUENCES_PER_PEER {
            // BTreeSet::iter().next() is O(1) - more efficient than min() on HashSet
            if let Some(min) = peer_sequences.iter().next().copied() {
                peer_sequences.remove(&min);
            }
        }
    }

    /// Validate and mark a sequence number as used.
    ///
    /// Returns `Ok(())` if the sequence is new (valid).
    /// Returns `Err(())` if the sequence has been used before (replay detected).
    pub async fn validate_and_mark(&self, peer_id: &AgentId, sequence: u64) -> Result<(), ()> {
        if self.is_sequence_used(peer_id, sequence).await {
            tracing::warn!(
                "Replay attack detected: peer {} reused sequence {}",
                peer_id, sequence
            );
            Err(())
        } else {
            self.mark_sequence_used(peer_id, sequence).await;
            Ok(())
        }
    }

    /// Clear all tracked sequences for a peer.
    ///
    /// Should be called when a peer disconnects.
    pub async fn remove_peer(&self, peer_id: &AgentId) {
        self.sequences.write().await.remove(peer_id);
    }

    /// IMPLEMENTATION_REQUIRED: wired in future wt-XXX for replay protection diagnostics
    /// Get the count of tracked sequences for a peer.
    #[allow(dead_code)]
    pub async fn sequence_count(&self, peer_id: &AgentId) -> usize {
        self.sequences
            .read()
            .await
            .get(peer_id)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

impl Default for ReplayProtection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_crypto::AgentId;

    fn test_agent_id() -> AgentId {
        // Use a deterministic test AgentId
        let bytes = [0u8; 32];
        AgentId::from_bytes(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_new_sequence_is_valid() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        let result = replay.validate_and_mark(&peer_id, 1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reused_sequence_is_rejected() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        // First use should succeed
        assert!(replay.validate_and_mark(&peer_id, 1).await.is_ok());
        
        // Reuse should fail
        assert!(replay.validate_and_mark(&peer_id, 1).await.is_err());
    }

    #[tokio::test]
    async fn test_different_sequences_are_valid() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        assert!(replay.validate_and_mark(&peer_id, 1).await.is_ok());
        assert!(replay.validate_and_mark(&peer_id, 2).await.is_ok());
        assert!(replay.validate_and_mark(&peer_id, 100).await.is_ok());
    }

    #[tokio::test]
    async fn test_sliding_window_eviction() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        // Insert MAX_SEQUENCES_PER_PEER + 1 sequences
        for i in 0..(MAX_SEQUENCES_PER_PEER + 1) {
            replay.mark_sequence_used(&peer_id, i as u64).await;
        }
        
        // The oldest (0) should have been evicted
        assert!(!replay.is_sequence_used(&peer_id, 0).await);
        
        // The newest should still be tracked
        assert!(replay.is_sequence_used(&peer_id, MAX_SEQUENCES_PER_PEER as u64).await);
    }

    #[tokio::test]
    async fn test_remove_peer_clears_sequences() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        replay.mark_sequence_used(&peer_id, 1).await;
        assert!(replay.is_sequence_used(&peer_id, 1).await);
        
        replay.remove_peer(&peer_id).await;
        assert!(!replay.is_sequence_used(&peer_id, 1).await);
    }

    #[tokio::test]
    async fn test_sequence_count() {
        let replay = ReplayProtection::new();
        let peer_id = test_agent_id();
        
        assert_eq!(replay.sequence_count(&peer_id).await, 0);
        
        replay.mark_sequence_used(&peer_id, 1).await;
        assert_eq!(replay.sequence_count(&peer_id).await, 1);
        
        replay.mark_sequence_used(&peer_id, 2).await;
        assert_eq!(replay.sequence_count(&peer_id).await, 2);
    }
}
