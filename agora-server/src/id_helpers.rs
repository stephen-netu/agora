//! Temporary ID generation helpers for agora-server.
//!
//! These bridge the removal of `EventId::new()` / `RoomId::new()` from
//! agora-core (PR #2) until agora-server migrates to BLAKE3 content-addressed
//! IDs via agora-crypto (PR #3 — feat/agora-crypto).

use agora_core::identifiers::{EventId, RoomId};

/// Generate a temporary random event ID.
///
/// Uses `uuid::Uuid::new_v4()` on this branch.
/// Will be replaced with `agora_crypto::ids::event_id(...)` in PR #3.
pub(crate) fn new_event_id() -> EventId {
    EventId::parse(&format!("${}", uuid::Uuid::new_v4().simple()))
        .expect("uuid simple format always produces a valid event id")
}

/// Generate a temporary random room ID for the given server.
///
/// Uses `uuid::Uuid::new_v4()` on this branch.
/// Will be replaced with `agora_crypto::ids::room_id(...)` in PR #3.
pub(crate) fn new_room_id(server_name: &str) -> RoomId {
    RoomId::parse(&format!("!{}:{}", uuid::Uuid::new_v4().simple(), server_name))
        .expect("uuid simple format always produces a valid room id")
}
