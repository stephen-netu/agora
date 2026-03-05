use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AmpMessage {
    Handshake {
        agent_id: String,
        version: u32,
        capabilities: Capabilities,
    },

    HandshakeAck {
        agent_id: String,
        version: u32,
        capabilities: Capabilities,
    },

    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },

    EventPush {
        room_id: String,
        events: Vec<SerializedEvent>,
    },

    EventRequest {
        event_hashes: Vec<String>,
    },

    EventResponse {
        events: Vec<SerializedEvent>,
    },

    StateRequest {
        room_id: String,
        since_hash: Option<String>,
    },

    StateResponse {
        room_id: String,
        state_events: Vec<SerializedEvent>,
    },

    RelayStore {
        recipient_agent_id: String,
        ciphertext: Vec<u8>,
        ttl_seconds: u32,
    },

    RelayFetch {
        since: u64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub events: bool,
    pub relay: bool,
    pub state_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    pub event_id: String,
    pub event_type: String,
    pub content: Vec<u8>,
    pub origin_server_ts: u64,
}
