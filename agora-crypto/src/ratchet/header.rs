//! Double Ratchet message header.
//!
//! Every encrypted message includes a header containing:
//! - The sender's current DH ratchet public key
//! - `PN`: previous sending chain message count
//! - `N`: current message number in the sending chain

use serde::{Deserialize, Serialize};

/// Message header transmitted with each Double Ratchet ciphertext.
///
/// Associated data for the AEAD encryption (not encrypted, but authenticated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Sender's current DH ratchet public key (X25519).
    pub dh_public: [u8; 32],
    /// Number of messages in the previous sending chain.
    pub prev_chain_length: u32,
    /// Message number in the current sending chain.
    pub message_number: u32,
}

impl MessageHeader {
    pub fn new(dh_public: [u8; 32], prev_chain_length: u32, message_number: u32) -> Self {
        Self {
            dh_public,
            prev_chain_length,
            message_number,
        }
    }

    /// Encode header to bytes for use as AEAD associated data.
    ///
    /// Uses a fixed layout for determinism:
    /// [dh_public (32)] [prev_chain_length (4 LE)] [message_number (4 LE)]
    pub fn to_associated_data(&self) -> [u8; 40] {
        let mut out = [0u8; 40];
        out[..32].copy_from_slice(&self.dh_public);
        out[32..36].copy_from_slice(&self.prev_chain_length.to_le_bytes());
        out[36..40].copy_from_slice(&self.message_number.to_le_bytes());
        out
    }

    /// Encode to MessagePack bytes for wire transmission.
    pub fn encode(&self) -> Vec<u8> {
        rmp_serde::to_vec_named(self).expect("MessageHeader serialization is infallible")
    }

    /// Decode from MessagePack bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        rmp_serde::from_slice(bytes).map_err(|e| format!("header decode: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associated_data_layout() {
        let dh = [0x01u8; 32];
        let hdr = MessageHeader::new(dh, 5, 10);
        let ad = hdr.to_associated_data();
        assert_eq!(&ad[..32], &dh);
        assert_eq!(&ad[32..36], &5u32.to_le_bytes());
        assert_eq!(&ad[36..40], &10u32.to_le_bytes());
    }

    #[test]
    fn encode_decode_roundtrip() {
        let hdr = MessageHeader::new([0xABu8; 32], 3, 7);
        let bytes = hdr.encode();
        let decoded = MessageHeader::decode(&bytes).unwrap();
        assert_eq!(hdr, decoded);
    }
}
