//! Session serialization helpers for E2EE (agora-crypto wrappers).

use agora_crypto::account::PairwiseSession;
use agora_crypto::group::{InboundGroupSession, OutboundGroupSession};

/// Serialize an outbound group session.
pub fn pickle_outbound_group(session: &OutboundGroupSession) -> Result<String, String> {
    session.to_snapshot().map_err(|e| format!("pickle outbound group: {e}"))
}

/// Restore an outbound group session.
pub fn unpickle_outbound_group(s: &str) -> Result<OutboundGroupSession, String> {
    OutboundGroupSession::from_snapshot(s).map_err(|e| format!("unpickle outbound group: {e}"))
}

/// Serialize an inbound group session.
pub fn pickle_inbound_group(session: &InboundGroupSession) -> Result<String, String> {
    session.to_snapshot().map_err(|e| format!("pickle inbound group: {e}"))
}

/// Restore an inbound group session.
pub fn unpickle_inbound_group(s: &str) -> Result<InboundGroupSession, String> {
    InboundGroupSession::from_snapshot(s).map_err(|e| format!("unpickle inbound group: {e}"))
}

/// Serialize a pairwise session.
pub fn pickle_pairwise_session(session: &PairwiseSession) -> Result<String, String> {
    session.to_snapshot().map_err(|e| format!("pickle pairwise session: {e}"))
}

/// Restore a pairwise session.
pub fn unpickle_pairwise_session(s: &str) -> Result<PairwiseSession, String> {
    PairwiseSession::from_snapshot(s).map_err(|e| format!("unpickle pairwise session: {e}"))
}
