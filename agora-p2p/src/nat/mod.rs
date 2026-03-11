//! NAT traversal support.
//!
//! ConnectionScorer: tracks peer reliability for retry decisions.
//! STUN + hole-punch: IMPLEMENTATION_REQUIRED — needs external crate selection.
//!
//! S-02: All timestamps are sequence numbers (AtomicU64), never wall-clock.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;
use sovereign_sdk::AgentId;

const MAX_SCORE: i32 = 100;
const MIN_SCORE: i32 = -100;
/// Retry after this many sequence ticks have elapsed since last attempt
const RETRY_AFTER_TICKS: u64 = 300;

#[derive(Debug, Clone)]
pub struct ConnectionScore {
    pub peer_id: AgentId,
    pub score: i32,
    pub attempts: u32,
    pub successes: u32,
    pub last_attempt_seq: u64,
}

pub struct ConnectionScorer {
    scores: Arc<RwLock<BTreeMap<AgentId, ConnectionScore>>>,
    sequence: Arc<AtomicU64>,
}

impl ConnectionScorer {
    pub fn new() -> Self {
        Self {
            scores: Arc::new(RwLock::new(BTreeMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn record_success(&self, peer_id: &AgentId) {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let mut scores = self.scores.write().await;
        let entry = scores.entry(peer_id.clone()).or_insert_with(|| ConnectionScore {
            peer_id: peer_id.clone(),
            score: 0,
            attempts: 0,
            successes: 0,
            last_attempt_seq: seq,
        });
        entry.score = (entry.score + 10).min(MAX_SCORE);
        entry.attempts += 1;
        entry.successes += 1;
        entry.last_attempt_seq = seq;
    }

    pub async fn record_failure(&self, peer_id: &AgentId) {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let mut scores = self.scores.write().await;
        let entry = scores.entry(peer_id.clone()).or_insert_with(|| ConnectionScore {
            peer_id: peer_id.clone(),
            score: 0,
            attempts: 0,
            successes: 0,
            last_attempt_seq: seq,
        });
        entry.score = (entry.score - 15).max(MIN_SCORE);
        entry.attempts += 1;
        entry.last_attempt_seq = seq;
    }

    pub async fn get_score(&self, peer_id: &AgentId) -> i32 {
        self.scores.read().await.get(peer_id).map(|s| s.score).unwrap_or(0)
    }

    pub async fn should_retry(&self, peer_id: &AgentId) -> bool {
        let current_seq = self.sequence.load(Ordering::SeqCst);
        let scores = self.scores.read().await;
        match scores.get(peer_id) {
            None => true,
            Some(entry) => {
                entry.score >= MIN_SCORE + 50
                    && (current_seq - entry.last_attempt_seq) >= RETRY_AFTER_TICKS
            }
        }
    }
}

impl Default for ConnectionScorer {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer() -> AgentId {
        AgentId::from_hex(
            "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
        ).unwrap()
    }

    #[tokio::test]
    async fn test_record_success_increases_score() {
        let scorer = ConnectionScorer::new();
        let peer = test_peer();
        scorer.record_success(&peer).await;
        assert!(scorer.get_score(&peer).await > 0);
    }

    #[tokio::test]
    async fn test_record_failure_decreases_score() {
        let scorer = ConnectionScorer::new();
        let peer = test_peer();
        scorer.record_failure(&peer).await;
        assert!(scorer.get_score(&peer).await < 0);
    }

    #[tokio::test]
    async fn test_score_bounded() {
        let scorer = ConnectionScorer::new();
        let peer = test_peer();
        for _ in 0..20 {
            scorer.record_success(&peer).await;
        }
        assert_eq!(scorer.get_score(&peer).await, 100);
        for _ in 0..15 {
            scorer.record_failure(&peer).await;
        }
        assert_eq!(scorer.get_score(&peer).await, -100);
    }

    #[tokio::test]
    async fn test_unknown_peer_score_is_zero() {
        let scorer = ConnectionScorer::new();
        let peer = test_peer();
        assert_eq!(scorer.get_score(&peer).await, 0);
    }
}
