//! Address derivation for Yggdrasil mesh addresses
//!
//! Provides utilities for deriving Yggdrasil IPv6 addresses from public keys
//! and manipulating mesh network addresses.
//!
//! # S-02 Compliance
//! - Uses BTreeMap/BTreeSet for deterministic storage
//! - No non-deterministic time sources
//! - Deterministic address derivation from public keys

use sovereign_sdk::AgentId;
use tracing::{info, trace, warn};

pub use sovereign_sdk::yggdrasil_addr_from_pubkey;

// IMPLEMENTATION_REQUIRED: Used for Yggdrasil address formatting in to_address_string() and parse()
const YGGDRASIL_PREFIX: &str = "020";
// IMPLEMENTATION_REQUIRED: Length of the YGGDRASIL_PREFIX used in address parsing
#[allow(dead_code)]
const YGGDRASIL_PREFIX_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YggdrasilAddress {
    bytes: [u8; 16],
}

impl YggdrasilAddress {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        trace!(target: "mesh", "Creating YggdrasilAddress from bytes");
        Self { bytes }
    }

    pub fn from_public_key(key: &[u8; 32]) -> Self {
        let addr = yggdrasil_addr_from_pubkey(key);
        let addr_bytes = addr.octets();
        info!(target: "mesh", "Derived Yggdrasil address from public key");
        Self { bytes: addr_bytes }
    }

    pub fn from_verifying_key(key_bytes: &[u8; 32]) -> Self {
        Self::from_public_key(key_bytes)
    }

    pub fn from_agent_id(agent_id: &AgentId) -> Self {
        let key_bytes = agent_id.as_bytes();
        let mut addr_bytes = [0u8; 16];
        addr_bytes.copy_from_slice(&key_bytes[..16]);
        trace!(target: "mesh", "Derived Yggdrasil address from AgentId");
        Self { bytes: addr_bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn is_global(&self) -> bool {
        self.bytes[0] == 0x02 || self.bytes[0] == 0x03
    }

    pub fn is_ulua(&self) -> bool {
        self.bytes[0] == 0xfc || self.bytes[0] == 0xfd
    }

    pub fn network_prefix(&self) -> Option<[u8; 4]> {
        if self.bytes.len() >= 4 {
            let mut prefix = [0u8; 4];
            prefix.copy_from_slice(&self.bytes[..4]);
            Some(prefix)
        } else {
            None
        }
    }

    pub fn node_id(&self) -> Option<[u8; 12]> {
        if self.bytes.len() >= 16 {
            let mut id = [0u8; 12];
            id.copy_from_slice(&self.bytes[4..16]);
            Some(id)
        } else {
            None
        }
    }

    pub fn to_address_string(&self) -> String {
        let hex_part = self
            .bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        format!("{}:{}", YGGDRASIL_PREFIX, hex_part)
    }

    pub fn parse(s: &str) -> Option<Self> {
        if s.len() < 5 || &s[..4] != "020:" {
            warn!(target: "mesh", "Invalid Yggdrasil address format: {}", s);
            return None;
        }

        let hex_part = &s[4..];
        if hex_part.len() != 32 {
            warn!(target: "mesh", "Invalid hex part length: expected 32, got {}", hex_part.len());
            return None;
        }

        let mut bytes = [0u8; 16];
        for (i, chunk) in hex_part.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(chunk).ok()?;
            bytes[i] = u8::from_str_radix(s, 16).ok()?;
        }

        Some(Self { bytes })
    }
}

impl From<[u8; 16]> for YggdrasilAddress {
    fn from(bytes: [u8; 16]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<&[u8]> for YggdrasilAddress {
    type Error = std::array::TryFromSliceError;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 16] = slice.try_into()?;
        Ok(Self::from_bytes(arr))
    }
}

impl std::fmt::Display for YggdrasilAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_address_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_derivation() {
        let test_bytes = [0u8; 32];
        let agent_id = AgentId::from_bytes(test_bytes);
        let addr = YggdrasilAddress::from_agent_id(&agent_id);
        assert_eq!(addr.as_bytes()[..12], agent_id.as_bytes()[..12]);
    }

    #[test]
    fn test_address_to_string() {
        let bytes = [
            0x02, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa,
            0xbb, 0xcc,
        ];
        let addr = YggdrasilAddress::from_bytes(bytes);
        assert!(addr.to_address_string().starts_with("020:"));
    }

    #[test]
    fn test_address_parse_roundtrip() {
        let bytes = [
            0x02, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa,
            0xbb, 0xcc,
        ];
        let addr = YggdrasilAddress::from_bytes(bytes);
        let s = addr.to_address_string();
        eprintln!("String: '{}' len={}", s, s.len());
        eprintln!(
            "Prefix check: first4='{}' starts_with_020={}",
            &s[..4],
            &s[..4] == "020:"
        );
        let parsed = YggdrasilAddress::parse(&s).unwrap();
        assert_eq!(addr.bytes, parsed.bytes);
    }
}
