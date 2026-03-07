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

    FuelOffer {
        offer_id: String,
        amount: u64,
        from: String,
        expiration_ts: u64,
    },

    FuelClaim {
        offer_id: String,
        amount: u64,
        claimant: String,
    },

    FuelReceipt {
        offer_id: String,
        claimed_by: String,
        amount: u64,
        signature: Vec<u8>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub events: bool,
    pub relay: bool,
    pub state_sync: bool,
    pub collaboration: bool,
    pub fuel: bool,
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
    use crate::protocol::codec::{encode, decode};

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
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
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
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
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
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
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
        // Verify collaboration field round-trips through the Handshake message
        let msg = AmpMessage::Handshake {
            agent_id: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            version: 1,
            capabilities: Capabilities {
                events: true,
                relay: true,
                state_sync: false,
                collaboration: true,
                fuel: false,
            },
            sequence: 1,
        };
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
        match decoded {
            AmpMessage::Handshake { capabilities, .. } => {
                assert!(capabilities.events);
                assert!(capabilities.relay);
                assert!(!capabilities.state_sync);
                assert!(capabilities.collaboration);
                assert!(!capabilities.fuel);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_fuel_offer_roundtrip() {
        let msg = AmpMessage::FuelOffer {
            offer_id: "offer_abc123".to_string(),
            amount: 1000,
            from: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            expiration_ts: 1700000000,
        };
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
        match decoded {
            AmpMessage::FuelOffer { offer_id, amount, from, expiration_ts } => {
                assert_eq!(offer_id, "offer_abc123");
                assert_eq!(amount, 1000);
                assert_eq!(from, "0000000000000000000000000000000000000000000000000000000000000001");
                assert_eq!(expiration_ts, 1700000000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_fuel_claim_roundtrip() {
        let msg = AmpMessage::FuelClaim {
            offer_id: "offer_abc123".to_string(),
            amount: 500,
            claimant: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
        };
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
        match decoded {
            AmpMessage::FuelClaim { offer_id, amount, claimant } => {
                assert_eq!(offer_id, "offer_abc123");
                assert_eq!(amount, 500);
                assert_eq!(claimant, "0000000000000000000000000000000000000000000000000000000000000002");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_fuel_receipt_roundtrip() {
        let msg = AmpMessage::FuelReceipt {
            offer_id: "offer_abc123".to_string(),
            claimed_by: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
            amount: 500,
            signature: b"zk_proof_placeholder".to_vec(),
        };
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
        match decoded {
            AmpMessage::FuelReceipt { offer_id, claimed_by, amount, signature } => {
                assert_eq!(offer_id, "offer_abc123");
                assert_eq!(claimed_by, "0000000000000000000000000000000000000000000000000000000000000002");
                assert_eq!(amount, 500);
                assert_eq!(signature, b"zk_proof_placeholder");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_capabilities_with_fuel() {
        // Verify fuel field round-trips through the Handshake message
        let msg = AmpMessage::Handshake {
            agent_id: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            version: 1,
            capabilities: Capabilities {
                events: true,
                relay: false,
                state_sync: true,
                collaboration: false,
                fuel: true,
            },
            sequence: 1,
        };
        let encoded = encode(&msg).expect("encode failed");
        let decoded = decode(&encoded).expect("decode failed");
        match decoded {
            AmpMessage::Handshake { capabilities, .. } => {
                assert!(capabilities.events);
                assert!(!capabilities.relay);
                assert!(capabilities.state_sync);
                assert!(!capabilities.collaboration);
                assert!(capabilities.fuel);
            }
            _ => panic!("wrong variant"),
        }
    }
}
