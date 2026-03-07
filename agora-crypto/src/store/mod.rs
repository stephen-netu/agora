//! Abstract crypto persistence trait and in-memory implementation.
//!
//! All implementations must use deterministic data structures (`BTreeMap`,
//! not `HashMap`) to satisfy the S-02 invariant.

use std::collections::BTreeMap;

use crate::{identity::AgentId, CryptoError};

/// Abstract storage backend for cryptographic state.
///
/// Every method returns `Result<_, CryptoError>` so that implementations can
/// propagate I/O or serialization failures through the standard error path.
pub trait CryptoStore: Send + Sync {
    /// Persist the serialized Double Ratchet session state for a remote peer.
    fn store_ratchet_state(
        &mut self,
        remote_id: &AgentId,
        state_bytes: &[u8],
    ) -> Result<(), CryptoError>;

    /// Load the serialized Double Ratchet session state for a remote peer.
    ///
    /// Returns `Ok(None)` if no session exists for that peer.
    fn load_ratchet_state(&self, remote_id: &AgentId) -> Result<Option<Vec<u8>>, CryptoError>;

    /// Store a signed pre-key bundle identified by a 32-byte key ID.
    fn store_prekey(&mut self, key_id: &[u8; 32], prekey_bytes: &[u8]) -> Result<(), CryptoError>;

    /// Retrieve and remove a pre-key bundle (one-time use).
    ///
    /// Returns `Ok(None)` if the key ID is not present.
    fn consume_prekey(&mut self, key_id: &[u8; 32]) -> Result<Option<Vec<u8>>, CryptoError>;

    /// Append a serialized sigchain link to the persistent log.
    fn append_sigchain(&mut self, link_bytes: &[u8]) -> Result<(), CryptoError>;

    /// Load all serialized sigchain links for the given agent as a single
    /// concatenated `Vec<u8>`, or `Ok(None)` if no chain has been recorded.
    fn load_sigchain(&self, agent_id: &AgentId) -> Result<Option<Vec<u8>>, CryptoError>;
}

// ── MemoryStore ───────────────────────────────────────────────────────────────

/// Volatile in-memory `CryptoStore` backed by `BTreeMap`.
///
/// Suitable for unit tests and ephemeral sessions.  All state is lost when
/// the struct is dropped.
pub struct MemoryStore {
    ratchet_states: BTreeMap<[u8; 32], Vec<u8>>,
    prekeys: BTreeMap<[u8; 32], Vec<u8>>,
    /// Each element is one serialized `SigchainLink` as returned by
    /// `rmp_serde::to_vec_named`.
    sigchain_links: Vec<Vec<u8>>,
    /// The `AgentId` bytes of the chain owner, set on first `append_sigchain`.
    sigchain_owner: Option<[u8; 32]>,
}

impl MemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            ratchet_states: BTreeMap::new(),
            prekeys: BTreeMap::new(),
            sigchain_links: Vec::new(),
            sigchain_owner: None,
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CryptoStore for MemoryStore {
    fn store_ratchet_state(
        &mut self,
        remote_id: &AgentId,
        state_bytes: &[u8],
    ) -> Result<(), CryptoError> {
        self.ratchet_states
            .insert(*remote_id.as_bytes(), state_bytes.to_vec());
        Ok(())
    }

    fn load_ratchet_state(&self, remote_id: &AgentId) -> Result<Option<Vec<u8>>, CryptoError> {
        Ok(self.ratchet_states.get(remote_id.as_bytes()).cloned())
    }

    fn store_prekey(&mut self, key_id: &[u8; 32], prekey_bytes: &[u8]) -> Result<(), CryptoError> {
        self.prekeys.insert(*key_id, prekey_bytes.to_vec());
        Ok(())
    }

    fn consume_prekey(&mut self, key_id: &[u8; 32]) -> Result<Option<Vec<u8>>, CryptoError> {
        Ok(self.prekeys.remove(key_id))
    }

    fn append_sigchain(&mut self, link_bytes: &[u8]) -> Result<(), CryptoError> {
        self.sigchain_links.push(link_bytes.to_vec());
        Ok(())
    }

    fn load_sigchain(&self, agent_id: &AgentId) -> Result<Option<Vec<u8>>, CryptoError> {
        // If a specific owner has been set and it doesn't match, return None.
        if let Some(owner) = self.sigchain_owner {
            if owner != *agent_id.as_bytes() {
                return Ok(None);
            }
        }

        if self.sigchain_links.is_empty() {
            return Ok(None);
        }

        // Concatenate all link blobs.  The caller is responsible for knowing
        // the framing format (e.g. length-prefixed MessagePack) and splitting
        // them back out.
        let total: usize = self.sigchain_links.iter().map(|b| b.len()).sum();
        let mut combined = Vec::with_capacity(total);
        for link in &self.sigchain_links {
            combined.extend_from_slice(link);
        }
        Ok(Some(combined))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{AgentId, AgentIdentity};

    fn make_agent_id(seed_byte: u8) -> AgentId {
        AgentIdentity::from_seed(&[seed_byte; 32]).agent_id
    }

    #[test]
    fn test_ratchet_state_store_and_load() {
        let mut store = MemoryStore::new();
        let id = make_agent_id(0x01);
        let state = b"ratchet session bytes";

        store.store_ratchet_state(&id, state).expect("store failed");

        let loaded = store.load_ratchet_state(&id).expect("load failed");

        assert_eq!(loaded, Some(state.to_vec()));
    }

    #[test]
    fn test_ratchet_state_missing_returns_none() {
        let store = MemoryStore::new();
        let id = make_agent_id(0x02);
        let loaded = store.load_ratchet_state(&id).expect("load failed");
        assert_eq!(loaded, None);
    }

    #[test]
    fn test_ratchet_state_overwrite() {
        let mut store = MemoryStore::new();
        let id = make_agent_id(0x03);

        store
            .store_ratchet_state(&id, b"v1")
            .expect("store v1 failed");
        store
            .store_ratchet_state(&id, b"v2")
            .expect("store v2 failed");

        let loaded = store.load_ratchet_state(&id).expect("load failed");
        assert_eq!(loaded, Some(b"v2".to_vec()));
    }

    #[test]
    fn test_different_agents_are_isolated() {
        let mut store = MemoryStore::new();
        let id_a = make_agent_id(0x0A);
        let id_b = make_agent_id(0x0B);

        store
            .store_ratchet_state(&id_a, b"state-a")
            .expect("store a failed");

        let loaded_b = store.load_ratchet_state(&id_b).expect("load b failed");
        assert_eq!(loaded_b, None);
    }

    #[test]
    fn test_prekey_store_and_consume() {
        let mut store = MemoryStore::new();
        let key_id = [0x10u8; 32];
        let prekey = b"prekey bundle bytes";

        store
            .store_prekey(&key_id, prekey)
            .expect("store prekey failed");

        // First consume returns the value.
        let first = store.consume_prekey(&key_id).expect("consume failed");
        assert_eq!(first, Some(prekey.to_vec()));

        // Second consume returns None (one-time use).
        let second = store
            .consume_prekey(&key_id)
            .expect("second consume failed");
        assert_eq!(second, None);
    }

    #[test]
    fn test_prekey_consume_missing_returns_none() {
        let mut store = MemoryStore::new();
        let key_id = [0x20u8; 32];
        let result = store.consume_prekey(&key_id).expect("consume failed");
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_prekeys_independent() {
        let mut store = MemoryStore::new();
        let key_id_a = [0x31u8; 32];
        let key_id_b = [0x32u8; 32];

        store.store_prekey(&key_id_a, b"a").expect("store a failed");
        store.store_prekey(&key_id_b, b"b").expect("store b failed");

        let result_a = store.consume_prekey(&key_id_a).expect("consume a failed");
        assert_eq!(result_a, Some(b"a".to_vec()));

        // b is untouched.
        let result_b = store.consume_prekey(&key_id_b).expect("consume b failed");
        assert_eq!(result_b, Some(b"b".to_vec()));
    }

    #[test]
    fn test_sigchain_append_and_load() {
        let mut store = MemoryStore::new();
        let id = make_agent_id(0x40);

        // Initially no chain.
        let empty = store.load_sigchain(&id).expect("initial load failed");
        assert_eq!(empty, None);

        store.append_sigchain(b"link-0").expect("append failed");
        store.append_sigchain(b"link-1").expect("append failed");

        let loaded = store.load_sigchain(&id).expect("load after append failed");
        // The two blobs are concatenated.
        assert_eq!(loaded, Some(b"link-0link-1".to_vec()));
    }

    #[test]
    fn test_sigchain_empty_store_returns_none() {
        let store = MemoryStore::new();
        let id = make_agent_id(0x50);
        let result = store.load_sigchain(&id).expect("load failed");
        assert_eq!(result, None);
    }

    #[test]
    fn test_default_produces_empty_store() {
        let store = MemoryStore::default();
        let id = make_agent_id(0x60);
        assert_eq!(store.load_ratchet_state(&id).expect("load failed"), None);
    }

    #[test]
    fn test_btreemap_ordering_is_deterministic() {
        // Verify that iterating over two identically-populated stores
        // yields the same key order (BTreeMap sorts by key).
        let ids: Vec<AgentId> = (0u8..5).map(make_agent_id).collect();
        let states: Vec<&[u8]> = vec![b"s0", b"s1", b"s2", b"s3", b"s4"];

        let mut store_a = MemoryStore::new();
        let mut store_b = MemoryStore::new();

        for (id, state) in ids.iter().zip(states.iter()) {
            store_a
                .store_ratchet_state(id, state)
                .expect("store a failed");
        }
        // Insert in reverse order into store_b.
        for (id, state) in ids.iter().zip(states.iter()).rev() {
            store_b
                .store_ratchet_state(id, state)
                .expect("store b failed");
        }

        let keys_a: Vec<[u8; 32]> = store_a.ratchet_states.keys().copied().collect();
        let keys_b: Vec<[u8; 32]> = store_b.ratchet_states.keys().copied().collect();
        assert_eq!(keys_a, keys_b, "BTreeMap must yield consistent ordering");
    }
}
