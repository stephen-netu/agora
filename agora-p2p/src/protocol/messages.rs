use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AmpMessage {
    Handshake {
        agent_id: String,
        version: u32,
        capabilities: Capabilities,
        sequence: u64,
    },

    HandshakeAck {
        agent_id: String,
        version: u32,
        capabilities: Capabilities,
        sequence: u64,
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

    CollaborationRequest {
        block_id: String,
        content: Vec<u8>,
        from: String,
        correlation_path: Vec<String>,
    },

    CollaborationResponse {
        block_id: String,
        content: Vec<u8>,
        agent_id: String,
        proof: Option<Vec<u8>>,
    },

    CollaborationRefusal {
        block_id: String,
        from: String,
        reason: String,
        correlation_path_snapshot: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub events: bool,
    pub relay: bool,
    pub state_sync: bool,
    pub collaboration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    pub event_id: String,
    pub event_type: String,
    pub content: Vec<u8>,
    pub origin_server_ts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_cbor::to_vec as cbor_encode;
    use serde_cbor::from_slice as cbor_decode;

    #[test]
    fn test_collaboration_request_roundtrip() {
        let msg = AmpMessage::CollaborationRequest {
            block_id: "block123".to_string(),
            content: b"test content".to_vec(),
            from: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            correlation_path: vec![
                "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
                "0000000000000000000000000000000000000000000000000000000000000003".to_string(),
            ],
        };
        
        let encoded = cbor_encode(&msg).expect("encode failed");
        let decoded: AmpMessage = cbor_decode(&encoded).expect("decode failed");
        
        match decoded {
            AmpMessage::CollaborationRequest { block_id, content, from, correlation_path } => {
                assert_eq!(block_id, "block123");
                assert_eq!(content, b"test content");
                assert_eq!(from, "0000000000000000000000000000000000000000000000000000000000000001");
                assert_eq!(correlation_path.len(), 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_collaboration_response_roundtrip() {
        let msg = AmpMessage::CollaborationResponse {
            block_id: "block123".to_string(),
            content: b"response content".to_vec(),
            agent_id: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            proof: Some(b"proof data".to_vec()),
        };
        
        let encoded = cbor_encode(&msg).expect("encode failed");
        let decoded: AmpMessage = cbor_decode(&encoded).expect("decode failed");
        
        match decoded {
            AmpMessage::CollaborationResponse { block_id, content, agent_id, proof } => {
                assert_eq!(block_id, "block123");
                assert_eq!(content, b"response content");
                assert_eq!(agent_id, "0000000000000000000000000000000000000000000000000000000000000001");
                assert!(proof.is_some());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_collaboration_refusal_roundtrip() {
        let msg = AmpMessage::CollaborationRefusal {
            block_id: "block123".to_string(),
            from: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
            reason: "loop detected".to_string(),
            correlation_path_snapshot: vec![
                "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            ],
        };
        
        let encoded = cbor_encode(&msg).expect("encode failed");
        let decoded: AmpMessage = cbor_decode(&encoded).expect("decode failed");
        
        match decoded {
            AmpMessage::CollaborationRefusal { block_id, from, reason, correlation_path_snapshot } => {
                assert_eq!(block_id, "block123");
                assert_eq!(from, "0000000000000000000000000000000000000000000000000000000000000002");
                assert_eq!(reason, "loop detected");
                assert_eq!(correlation_path_snapshot.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_capabilities_with_collaboration() {
        let caps = Capabilities {
            events: true,
            relay: true,
            state_sync: false,
            collaboration: true,
        };
        
        let encoded = cbor_encode(&caps).expect("encode failed");
        let decoded: Capabilities = cbor_decode(&encoded).expect("decode failed");
        
        assert!(decoded.events);
        assert!(decoded.relay);
        assert!(!decoded.state_sync);
        assert!(decoded.collaboration);
    }
}
