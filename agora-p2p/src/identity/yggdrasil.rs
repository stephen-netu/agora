use std::net::Ipv6Addr;

use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha512};

// IMPLEMENTATION_REQUIRED: wired in wt-010 for Yggdrasil address generation
pub fn yggdrasil_addr_from_pubkey(verifying_key: &VerifyingKey) -> Ipv6Addr {
    let pubkey_bytes = verifying_key.as_bytes();

    let hash = Sha512::digest(pubkey_bytes);

    let mut addr_bytes = [0u8; 16];
    addr_bytes[0] = 0x02;
    addr_bytes[1..16].copy_from_slice(&hash[0..15]);

    Ipv6Addr::from(addr_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_yggdrasil_address_range() {
        let secret_bytes: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        let addr = yggdrasil_addr_from_pubkey(&verifying_key);
        let first_byte = addr.octets()[0];
        assert!(
            first_byte == 0x02 || first_byte == 0x03,
            "Expected 200::/7 range, got {:#04x}",
            first_byte
        );
    }
}
