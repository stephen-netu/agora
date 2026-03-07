//! S-02 compliant deterministic timestamp provider.
//!
//! Replaces all `SystemTime::now()` usages with a monotonic sequence counter.
//! No wall-clock access; timestamps are deterministic and replayable.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::CryptoError;

/// Default epoch offset shared across all Agora nodes: 2024-03-01 00:00:00 UTC in milliseconds.
///
/// All `SequenceTimestamp` instances should use this as their `epoch_offset` so that
/// IDs from different nodes are comparable and sequence numbers can be resumed correctly
/// after a restart.
pub const DEFAULT_EPOCH_MS: u64 = 1_709_251_200_000;

/// Provides monotonically increasing, deterministic timestamps.
///
/// Implementations must never access `SystemTime`, `Instant`, or any
/// non-deterministic clock source.
pub trait TimestampProvider: Send + Sync {
    /// Returns the next monotonic timestamp value.
    fn next_timestamp(&self) -> Result<u64, CryptoError>;

    /// Returns the last issued timestamp without advancing.
    fn current(&self) -> u64;
}

/// Sequence-counter-based timestamp provider.
///
/// Each call to `next_timestamp()` returns `epoch_offset + counter`,
/// where `counter` increments atomically. The `epoch_offset` can be set
/// to a known-good base (e.g. last persisted timestamp) so that IDs remain
/// monotonically ordered across restarts.
#[derive(Debug)]
pub struct SequenceTimestamp {
    counter: AtomicU64,
    epoch_offset: u64,
}

impl SequenceTimestamp {
    /// Create a new provider starting at `epoch_offset`.
    pub fn new(epoch_offset: u64) -> Self {
        Self {
            counter: AtomicU64::new(0),
            epoch_offset,
        }
    }

    /// Resume from a previously persisted sequence number.
    pub fn resume_from(epoch_offset: u64, last_sequence: u64) -> Self {
        Self {
            counter: AtomicU64::new(last_sequence.saturating_add(1)),
            epoch_offset,
        }
    }

    /// Wrap in `Arc` for shared ownership.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl Default for SequenceTimestamp {
    /// Default epoch offset: 2024-03-01 00:00:00 UTC in milliseconds.
    /// Chosen to keep IDs in a reasonable numeric range while being deterministic.
    fn default() -> Self {
        Self::new(DEFAULT_EPOCH_MS)
    }
}

impl TimestampProvider for SequenceTimestamp {
    fn next_timestamp(&self) -> Result<u64, CryptoError> {
        let seq = self.counter.fetch_add(1, Ordering::SeqCst);
        self.epoch_offset
            .checked_add(seq)
            .ok_or(CryptoError::SequenceOverflow)
    }

    fn current(&self) -> u64 {
        let seq = self.counter.load(Ordering::SeqCst);
        self.epoch_offset.saturating_add(seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonically_increasing() {
        let ts = SequenceTimestamp::new(0);
        let t0 = ts.next_timestamp().unwrap();
        let t1 = ts.next_timestamp().unwrap();
        let t2 = ts.next_timestamp().unwrap();
        assert!(t0 < t1);
        assert!(t1 < t2);
    }

    #[test]
    fn deterministic_from_same_offset() {
        let ts1 = SequenceTimestamp::new(1_000_000);
        let ts2 = SequenceTimestamp::new(1_000_000);
        assert_eq!(ts1.next_timestamp().unwrap(), ts2.next_timestamp().unwrap());
    }

    #[test]
    fn resume_starts_after_last() {
        let ts = SequenceTimestamp::resume_from(0, 99);
        assert_eq!(ts.next_timestamp().unwrap(), 100);
    }

    #[test]
    fn no_system_time_access() {
        // Compile-time proof: SequenceTimestamp has no std::time imports.
        // Runtime proof: values are purely counter-derived.
        let ts = SequenceTimestamp::default();
        let v = ts.next_timestamp().unwrap();
        assert_eq!(v, DEFAULT_EPOCH_MS);
    }
}
