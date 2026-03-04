//! Key agreement protocols for establishing Double Ratchet sessions.
//!
//! Implements X3DH (Extended Triple Diffie-Hellman):
//! <https://signal.org/docs/specifications/x3dh/>
//!
//! The `KeyAgreement` trait abstracts the handshake so PQXDH can be
//! swapped in as Phase 1b without changing the ratchet or envelope layers.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha256;
use serde::{Deserialize, Serialize};

use crate::CryptoError;

/// Pre-key bundle published by a party to allow asynchronous session setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyBundle {
    /// Long-term identity X25519 public key.
    pub identity_key: [u8; 32],
    /// Signed prekey X25519 public key (rotate weekly).
    pub signed_prekey: [u8; 32],
    /// Ed25519 signature over the signed_prekey bytes.
    pub signed_prekey_signature: Vec<u8>,
    /// Ed25519 public key used to sign the prekey.
    pub identity_signing_key: [u8; 32],
    /// One-time prekeys (optional, consumed once each).
    pub one_time_prekeys: Vec<[u8; 32]>,
}

/// Context produced by the initiator, sent to the responder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiatorMessage {
    /// Initiator's long-term identity X25519 public key.
    pub identity_key: [u8; 32],
    /// Initiator's ephemeral X25519 public key.
    pub ephemeral_key: [u8; 32],
    /// Index of the one-time prekey used (None if no OTK available).
    pub one_time_prekey_index: Option<usize>,
    /// The one-time prekey public key used (for lookup by responder).
    pub one_time_prekey_used: Option<[u8; 32]>,
    /// Initial message ciphertext (optional, for "initial message" optimization).
    pub initial_ciphertext: Option<Vec<u8>>,
}

/// Result of a successful X3DH handshake.
pub struct HandshakeResult {
    /// 32-byte shared secret for Double Ratchet initialization.
    pub shared_secret: [u8; 32],
    /// The responder's initial DH ratchet public key
    /// (= signed_prekey, used by Alice in `init_alice`).
    pub bob_ratchet_key: [u8; 32],
}

/// Perform X3DH as the initiator (Alice).
///
/// # Arguments
/// - `alice_identity` — Alice's long-term X25519 static secret
/// - `alice_ephemeral` — Alice's ephemeral X25519 static secret (from deterministic seed)
/// - `bundle` — Bob's published pre-key bundle
///
/// # Returns
/// `(HandshakeResult, InitiatorMessage)` — the shared secret and the message to send Bob.
pub fn x3dh_initiate(
    alice_identity: &x25519_dalek::StaticSecret,
    alice_ephemeral: &x25519_dalek::StaticSecret,
    bundle: &PreKeyBundle,
) -> Result<(HandshakeResult, InitiatorMessage), CryptoError> {
    // Verify Bob's signed prekey signature.
    let bob_signing = VerifyingKey::from_bytes(&bundle.identity_signing_key)
        .map_err(|e| CryptoError::InvalidKey(format!("bob signing key: {e}")))?;
    let sig = Signature::from_slice(&bundle.signed_prekey_signature)
        .map_err(|e| CryptoError::InvalidSignature(format!("prekey sig: {e}")))?;
    bob_signing
        .verify(&bundle.signed_prekey, &sig)
        .map_err(|e| CryptoError::InvalidSignature(format!("prekey sig verify: {e}")))?;

    let bob_identity = x25519_dalek::PublicKey::from(bundle.identity_key);
    let bob_spk = x25519_dalek::PublicKey::from(bundle.signed_prekey);

    // DH1 = DH(IK_A, SPK_B)
    let dh1 = alice_identity.diffie_hellman(&bob_spk).to_bytes();
    // DH2 = DH(EK_A, IK_B)
    let dh2 = alice_ephemeral.diffie_hellman(&bob_identity).to_bytes();
    // DH3 = DH(EK_A, SPK_B)
    let dh3 = alice_ephemeral.diffie_hellman(&bob_spk).to_bytes();

    // DH4 = DH(EK_A, OPK_B) if one-time prekey available.
    let (dh4, opk_used, opk_index) = if let Some(opk_pub) = bundle.one_time_prekeys.first() {
        let opk = x25519_dalek::PublicKey::from(*opk_pub);
        let dh = alice_ephemeral.diffie_hellman(&opk).to_bytes();
        (Some(dh), Some(*opk_pub), Some(0usize))
    } else {
        (None, None, None)
    };

    // Concatenate DH outputs.
    let mut ikm = Vec::with_capacity(128);
    ikm.extend_from_slice(&[0xFFu8; 32]); // F padding per Signal spec
    ikm.extend_from_slice(&dh1);
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);
    if let Some(ref d4) = dh4 {
        ikm.extend_from_slice(d4);
    }

    let shared_secret = x3dh_kdf(&ikm)?;

    let alice_pub = x25519_dalek::PublicKey::from(alice_ephemeral);
    let msg = InitiatorMessage {
        identity_key: x25519_dalek::PublicKey::from(alice_identity).to_bytes(),
        ephemeral_key: alice_pub.to_bytes(),
        one_time_prekey_index: opk_index,
        one_time_prekey_used: opk_used,
        initial_ciphertext: None,
    };

    Ok((
        HandshakeResult {
            shared_secret,
            bob_ratchet_key: bundle.signed_prekey,
        },
        msg,
    ))
}

/// Perform X3DH as the responder (Bob).
///
/// # Arguments
/// - `bob_identity` — Bob's long-term X25519 static secret
/// - `bob_signed_prekey` — Bob's signed prekey static secret
/// - `bob_one_time_prekey` — Bob's OTK static secret (if Alice used one)
/// - `msg` — The `InitiatorMessage` received from Alice
pub fn x3dh_respond(
    bob_identity: &x25519_dalek::StaticSecret,
    bob_signed_prekey: &x25519_dalek::StaticSecret,
    bob_one_time_prekey: Option<&x25519_dalek::StaticSecret>,
    msg: &InitiatorMessage,
) -> Result<HandshakeResult, CryptoError> {
    let alice_identity = x25519_dalek::PublicKey::from(msg.identity_key);
    let alice_ephemeral = x25519_dalek::PublicKey::from(msg.ephemeral_key);

    // DH1 = DH(SPK_B, IK_A)
    let dh1 = bob_signed_prekey.diffie_hellman(&alice_identity).to_bytes();
    // DH2 = DH(IK_B, EK_A)
    let dh2 = bob_identity.diffie_hellman(&alice_ephemeral).to_bytes();
    // DH3 = DH(SPK_B, EK_A)
    let dh3 = bob_signed_prekey.diffie_hellman(&alice_ephemeral).to_bytes();

    let dh4 = bob_one_time_prekey.map(|opk| opk.diffie_hellman(&alice_ephemeral).to_bytes());

    let mut ikm = Vec::with_capacity(128);
    ikm.extend_from_slice(&[0xFFu8; 32]);
    ikm.extend_from_slice(&dh1);
    ikm.extend_from_slice(&dh2);
    ikm.extend_from_slice(&dh3);
    if let Some(ref d4) = dh4 {
        ikm.extend_from_slice(d4);
    }

    let shared_secret = x3dh_kdf(&ikm)?;

    let bob_spk_pub = x25519_dalek::PublicKey::from(bob_signed_prekey);

    Ok(HandshakeResult {
        shared_secret,
        bob_ratchet_key: bob_spk_pub.to_bytes(),
    })
}

/// Generate a pre-key bundle for publication.
///
/// # Arguments
/// - `identity_x25519` — X25519 identity static secret
/// - `identity_signing` — Ed25519 signing key (signs the SPK)
/// - `signed_prekey` — X25519 signed prekey static secret
/// - `one_time_prekeys` — list of OTK secrets (optional)
pub fn make_prekey_bundle(
    identity_x25519: &x25519_dalek::StaticSecret,
    identity_signing: &SigningKey,
    signed_prekey: &x25519_dalek::StaticSecret,
    one_time_prekeys: &[x25519_dalek::StaticSecret],
) -> PreKeyBundle {
    let spk_pub = x25519_dalek::PublicKey::from(signed_prekey);
    let sig = identity_signing.sign(spk_pub.as_bytes());

    let otks: Vec<[u8; 32]> = one_time_prekeys
        .iter()
        .map(|s| x25519_dalek::PublicKey::from(s).to_bytes())
        .collect();

    PreKeyBundle {
        identity_key: x25519_dalek::PublicKey::from(identity_x25519).to_bytes(),
        signed_prekey: spk_pub.to_bytes(),
        signed_prekey_signature: sig.to_bytes().to_vec(),
        identity_signing_key: identity_signing.verifying_key().to_bytes(),
        one_time_prekeys: otks,
    }
}

/// X3DH KDF: HKDF-SHA256 over concatenated DH outputs.
fn x3dh_kdf(ikm: &[u8]) -> Result<[u8; 32], CryptoError> {
    let (_, hk) = Hkdf::<Sha256>::extract(Some(b"WhisperText"), ikm);
    let mut okm = [0u8; 32];
    hk.expand(b"WhisperText", &mut okm)
        .map_err(|e| CryptoError::KeyGeneration(format!("X3DH KDF: {e}")))?;
    Ok(okm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity() -> (x25519_dalek::StaticSecret, SigningKey) {
        let x_secret = x25519_dalek::StaticSecret::from([0x11u8; 32]);
        let ed_key = SigningKey::from_bytes(&[0x22u8; 32]);
        (x_secret, ed_key)
    }

    #[test]
    fn x3dh_shared_secret_matches() {
        let (bob_identity, bob_signing) = make_identity();
        let bob_spk = x25519_dalek::StaticSecret::from([0x33u8; 32]);
        let bob_otk = x25519_dalek::StaticSecret::from([0x44u8; 32]);

        let bundle = make_prekey_bundle(&bob_identity, &bob_signing, &bob_spk, &[bob_otk.clone()]);

        let alice_identity = x25519_dalek::StaticSecret::from([0x55u8; 32]);
        let alice_ephemeral = x25519_dalek::StaticSecret::from([0x66u8; 32]);

        let (alice_result, msg) = x3dh_initiate(&alice_identity, &alice_ephemeral, &bundle).unwrap();
        let bob_result = x3dh_respond(
            &bob_identity,
            &bob_spk,
            Some(&bob_otk),
            &msg,
        )
        .unwrap();

        assert_eq!(alice_result.shared_secret, bob_result.shared_secret);
    }

    #[test]
    fn x3dh_without_otk_matches() {
        let (bob_identity, bob_signing) = make_identity();
        let bob_spk = x25519_dalek::StaticSecret::from([0x33u8; 32]);

        let bundle = make_prekey_bundle(&bob_identity, &bob_signing, &bob_spk, &[]);

        let alice_identity = x25519_dalek::StaticSecret::from([0x55u8; 32]);
        let alice_ephemeral = x25519_dalek::StaticSecret::from([0x66u8; 32]);

        let (alice_result, msg) = x3dh_initiate(&alice_identity, &alice_ephemeral, &bundle).unwrap();
        let bob_result = x3dh_respond(&bob_identity, &bob_spk, None, &msg).unwrap();

        assert_eq!(alice_result.shared_secret, bob_result.shared_secret);
    }

    #[test]
    fn invalid_prekey_signature_rejected() {
        let (bob_identity, bob_signing) = make_identity();
        let bob_spk = x25519_dalek::StaticSecret::from([0x33u8; 32]);

        let mut bundle = make_prekey_bundle(&bob_identity, &bob_signing, &bob_spk, &[]);
        // Corrupt the signature.
        bundle.signed_prekey_signature[0] ^= 0xFF;

        let alice_identity = x25519_dalek::StaticSecret::from([0x55u8; 32]);
        let alice_ephemeral = x25519_dalek::StaticSecret::from([0x66u8; 32]);

        let result = x3dh_initiate(&alice_identity, &alice_ephemeral, &bundle);
        assert!(result.is_err());
    }
}
