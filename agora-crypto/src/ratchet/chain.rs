//! KDF chains for the Double Ratchet.
//!
//! Implements the Signal spec KDF chains:
//! - Root chain: KDF(root_key, dh_output) → (new_root_key, new_chain_key)
//! - Symmetric chain: KDF(chain_key) → (message_key, next_chain_key)
//!
//! HKDF-SHA256 is used for the root chain.
//! HMAC-SHA256 is used for the symmetric chain steps.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use hkdf::Hkdf;

use super::keys::{ChainKey, MessageKey, RootKey};

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 constant for advancing chain key.
const CHAIN_KEY_INPUT: &[u8] = &[0x02];
/// HMAC-SHA256 constant for deriving message key.
const MESSAGE_KEY_INPUT: &[u8] = &[0x01];

/// Root chain KDF. Takes (root_key, dh_output) → (new_root_key, new_chain_key).
///
/// Uses HKDF-SHA256 per Signal spec:
/// HKDF(IKM=dh_output, salt=root_key, info="WhisperRatchet") → 64 bytes split as (new_root_key, new_chain_key)
pub fn root_kdf(root_key: &RootKey, dh_output: &[u8; 32]) -> (RootKey, ChainKey) {
    let (_, hk) = Hkdf::<Sha256>::extract(Some(&root_key.0), dh_output);
    let mut okm = [0u8; 64];
    hk.expand(b"WhisperRatchet", &mut okm)
        .expect("HKDF expand: 64 bytes is valid for SHA-256");

    let mut new_root = [0u8; 32];
    let mut new_chain = [0u8; 32];
    new_root.copy_from_slice(&okm[..32]);
    new_chain.copy_from_slice(&okm[32..]);

    (RootKey(new_root), ChainKey(new_chain))
}

/// Symmetric chain step: derive message key from chain key.
///
/// HMAC-SHA256(chain_key, 0x01) → message_key
pub fn chain_message_key(chain_key: &ChainKey) -> MessageKey {
    let mut mac = HmacSha256::new_from_slice(&chain_key.0)
        .expect("HMAC: any key size is valid");
    mac.update(MESSAGE_KEY_INPUT);
    let result = mac.finalize().into_bytes();
    let mut mk = [0u8; 32];
    mk.copy_from_slice(&result[..32]);
    MessageKey(mk)
}

/// Symmetric chain step: advance chain key to next state.
///
/// HMAC-SHA256(chain_key, 0x02) → next_chain_key
pub fn chain_advance(chain_key: &ChainKey) -> ChainKey {
    let mut mac = HmacSha256::new_from_slice(&chain_key.0)
        .expect("HMAC: any key size is valid");
    mac.update(CHAIN_KEY_INPUT);
    let result = mac.finalize().into_bytes();
    let mut ck = [0u8; 32];
    ck.copy_from_slice(&result[..32]);
    ChainKey(ck)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_kdf_deterministic() {
        let rk = RootKey([0x42u8; 32]);
        let dh = [0x13u8; 32];
        let (new_rk1, new_ck1) = root_kdf(&rk, &dh);
        let (new_rk2, new_ck2) = root_kdf(&rk, &dh);
        assert_eq!(new_rk1.0, new_rk2.0);
        assert_eq!(new_ck1.0, new_ck2.0);
    }

    #[test]
    fn root_kdf_produces_different_rk_and_ck() {
        let rk = RootKey([0x01u8; 32]);
        let dh = [0x02u8; 32];
        let (new_rk, new_ck) = root_kdf(&rk, &dh);
        // Root key and chain key must differ
        assert_ne!(new_rk.0, new_ck.0);
        // They must differ from the input root key
        assert_ne!(new_rk.0, rk.0);
    }

    #[test]
    fn chain_advance_is_deterministic() {
        let ck = ChainKey([0xABu8; 32]);
        let ck2a = chain_advance(&ck);
        let ck2b = chain_advance(&ck);
        assert_eq!(ck2a.0, ck2b.0);
    }

    #[test]
    fn chain_message_key_differs_from_chain_advance() {
        let ck = ChainKey([0xFFu8; 32]);
        let mk = chain_message_key(&ck);
        let next_ck = chain_advance(&ck);
        assert_ne!(mk.0, next_ck.0);
    }

    #[test]
    fn forward_secrecy_from_chain() {
        // Advancing the chain should produce a different message key each time
        let ck1 = ChainKey([0x55u8; 32]);
        let mk1 = chain_message_key(&ck1);
        let ck2 = chain_advance(&ck1);
        let mk2 = chain_message_key(&ck2);
        assert_ne!(mk1.0, mk2.0);
    }
}
