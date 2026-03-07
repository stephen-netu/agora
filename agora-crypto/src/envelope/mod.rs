//! Saltpack-inspired MessagePack envelope format.
//!
//! Provides multi-recipient encryption using X25519 key agreement and
//! ChaCha20-Poly1305 AEAD, with Ed25519 attached signing.

use blake3;
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit,
    aead::Aead,
};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::CryptoError;

/// The complete encrypted/signed envelope format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignEnvelope {
    /// Always "sovereign-envelope".
    pub format: String,
    /// Semver as two bytes: [1, 0].
    pub version: [u8; 2],
    /// 0 = encrypt, 1 = sign-attached.
    pub mode: u8,
    /// Ephemeral X25519 public key (32 bytes).
    pub ephemeral_public: [u8; 32],
    /// Sender identity encrypted with ChaCha20-Poly1305 (empty for anonymous).
    pub sender_secretbox: Vec<u8>,
    /// One entry per recipient.
    pub recipients: Vec<RecipientEntry>,
    /// Encrypted payload, split into 1 MiB chunks.
    pub payload_chunks: Vec<EncryptedChunk>,
}

/// Per-recipient key-encapsulation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientEntry {
    /// BLAKE3 hash of the recipient's X25519 public key bytes.
    pub recipient_id: [u8; 32],
    /// Payload key encrypted with ChaCha20-Poly1305 for this recipient.
    pub encrypted_payload_key: Vec<u8>,
}

/// One authenticated ciphertext chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedChunk {
    /// Sequence number of this chunk (0-indexed).
    pub sequence: u32,
    /// Whether this is the final chunk.
    pub is_final: bool,
    /// Encrypted chunk payload.
    pub ciphertext: Vec<u8>,
    /// Nonce used for ChaCha20-Poly1305 encryption.
    pub nonce: [u8; 12],
}

// ── Internal signed-envelope wire format (for sign_attached / verify_attached) ──

#[derive(Debug, Serialize, Deserialize)]
struct SignedEnvelopeWire {
    format: String,
    version: [u8; 2],
    verifying_key: [u8; 32],
    signature: Vec<u8>,
    payload: Vec<u8>,
}

// ── Chunk size ────────────────────────────────────────────────────────────────

const CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB

// ── Public API ────────────────────────────────────────────────────────────────

/// Serialize an envelope to MessagePack.
pub fn encode(envelope: &SovereignEnvelope) -> Result<Vec<u8>, CryptoError> {
    rmp_serde::to_vec_named(envelope)
        .map_err(|e| CryptoError::Encoding(e.to_string()))
}

/// Deserialize an envelope from MessagePack.
pub fn decode(bytes: &[u8]) -> Result<SovereignEnvelope, CryptoError> {
    rmp_serde::from_slice(bytes)
        .map_err(|e| CryptoError::Decoding(e.to_string()))
}

/// Encrypt `plaintext` to one or more X25519 recipients.
///
/// The caller supplies the ephemeral `StaticSecret` so that key generation
/// remains deterministic (S-02).  In production the caller generates it from
/// a CSPRNG seed; in tests a fixed seed can be used.
pub fn encrypt(
    plaintext: &[u8],
    recipients: &[x25519_dalek::PublicKey],
    ephemeral_secret: x25519_dalek::StaticSecret,
) -> Result<SovereignEnvelope, CryptoError> {
    if recipients.is_empty() {
        return Err(CryptoError::Encryption(
            "at least one recipient required".into(),
        ));
    }

    let ephemeral_public_key = x25519_dalek::PublicKey::from(&ephemeral_secret);
    let ephemeral_public: [u8; 32] = *ephemeral_public_key.as_bytes();

    // Snapshot the secret bytes so we can reconstruct the secret for each recipient DH.
    let ephemeral_secret_bytes: [u8; 32] = ephemeral_secret.to_bytes();

    // Derive a 32-byte payload key from the DH with the first recipient.
    // We use the ephemeral_public bytes (not secret bytes) as salt so the decryptor
    // can reproduce the same value using only the envelope header.
    let dh_first = {
        let s = x25519_dalek::StaticSecret::from(ephemeral_secret_bytes);
        *s.diffie_hellman(&recipients[0]).as_bytes()
    };
    let mut payload_key_input = [0u8; 64];
    payload_key_input[..32].copy_from_slice(&dh_first);
    payload_key_input[32..].copy_from_slice(&ephemeral_public);
    let payload_key: [u8; 32] = *blake3::hash(&payload_key_input).as_bytes();

    // Build recipient entries — each gets the payload_key wrapped for them.
    let mut recipient_entries: Vec<RecipientEntry> = Vec::with_capacity(recipients.len());

    for recipient_pub in recipients {
        let recipient_id: [u8; 32] =
            *blake3::hash(recipient_pub.as_bytes()).as_bytes();

        // Nonce for wrapping the payload key: BLAKE3(recipient_pubkey || "payload_key_nonce")
        let mut nonce_input = Vec::with_capacity(32 + 16);
        nonce_input.extend_from_slice(recipient_pub.as_bytes());
        nonce_input.extend_from_slice(b"payload_key_nonce");
        let nonce_bytes: [u8; 12] = blake3::hash(&nonce_input).as_bytes()[..12]
            .try_into()
            .map_err(|_| CryptoError::Encryption("nonce slice error".into()))?;

        // Wrap key: BLAKE3(DH(ephemeral_secret, recipient_pub) || ephemeral_public).
        // The decryptor computes DH(recipient_secret, ephemeral_pub) which equals
        // DH(ephemeral_secret, recipient_pub) by ECDH symmetry.
        let eph_secret_copy = x25519_dalek::StaticSecret::from(ephemeral_secret_bytes);
        let dh_bytes = *eph_secret_copy.diffie_hellman(recipient_pub).as_bytes();
        let mut wrap_key_input = [0u8; 64];
        wrap_key_input[..32].copy_from_slice(&dh_bytes);
        wrap_key_input[32..].copy_from_slice(&ephemeral_public);
        let wrap_key: [u8; 32] = *blake3::hash(&wrap_key_input).as_bytes();

        let cipher = ChaCha20Poly1305::new_from_slice(&wrap_key)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;
        let nonce = chacha20poly1305::Nonce::from(nonce_bytes);
        let encrypted_payload_key = cipher
            .encrypt(&nonce, payload_key.as_ref())
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        recipient_entries.push(RecipientEntry {
            recipient_id,
            encrypted_payload_key,
        });
    }

    // Chunk and encrypt plaintext with the payload key.
    let payload_cipher = ChaCha20Poly1305::new_from_slice(&payload_key)
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let chunks_raw: Vec<&[u8]> = if plaintext.is_empty() {
        vec![&[]]
    } else {
        plaintext.chunks(CHUNK_SIZE).collect()
    };

    let total = chunks_raw.len();
    let mut payload_chunks: Vec<EncryptedChunk> = Vec::with_capacity(total);

    for (idx, chunk) in chunks_raw.iter().enumerate() {
        let sequence = idx as u32;
        let is_final = idx == total - 1;

        // Nonce: first 4 bytes = sequence (big-endian), remaining 8 bytes = zeros.
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&sequence.to_be_bytes());

        let nonce = chacha20poly1305::Nonce::from(nonce_bytes);
        let ciphertext = payload_cipher
            .encrypt(&nonce, *chunk)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        payload_chunks.push(EncryptedChunk {
            sequence,
            is_final,
            ciphertext,
            nonce: nonce_bytes,
        });
    }

    Ok(SovereignEnvelope {
        format: "sovereign-envelope".to_string(),
        version: [1, 0],
        mode: 0,
        ephemeral_public,
        sender_secretbox: Vec::new(),
        recipients: recipient_entries,
        payload_chunks,
    })
}

/// Decrypt an envelope using the recipient's static X25519 key pair.
pub fn decrypt(
    envelope: &SovereignEnvelope,
    recipient_key: &x25519_dalek::StaticSecret,
    recipient_pub: &x25519_dalek::PublicKey,
) -> Result<Vec<u8>, CryptoError> {
    if envelope.format != "sovereign-envelope" {
        return Err(CryptoError::Decryption(format!(
            "unexpected format: {}",
            envelope.format
        )));
    }

    let my_id: [u8; 32] = *blake3::hash(recipient_pub.as_bytes()).as_bytes();

    // Locate our recipient entry.
    let entry = envelope
        .recipients
        .iter()
        .find(|r| r.recipient_id == my_id)
        .ok_or_else(|| CryptoError::Decryption("recipient not found in envelope".into()))?;

    // Reconstruct the wrap key: DH(my_key, ephemeral_pub) mixed with ephemeral_pub bytes.
    let ephemeral_pub = x25519_dalek::PublicKey::from(envelope.ephemeral_public);
    let dh_bytes = *recipient_key.diffie_hellman(&ephemeral_pub).as_bytes();
    let mut wrap_key_input = [0u8; 64];
    wrap_key_input[..32].copy_from_slice(&dh_bytes);
    wrap_key_input[32..].copy_from_slice(&envelope.ephemeral_public);
    let wrap_key: [u8; 32] = *blake3::hash(&wrap_key_input).as_bytes();

    // Reconstruct the nonce used to wrap the payload key.
    let mut nonce_input = Vec::with_capacity(32 + 16);
    nonce_input.extend_from_slice(recipient_pub.as_bytes());
    nonce_input.extend_from_slice(b"payload_key_nonce");
    let nonce_bytes: [u8; 12] = blake3::hash(&nonce_input).as_bytes()[..12]
        .try_into()
        .map_err(|_| CryptoError::Decryption("nonce slice error".into()))?;

    let wrap_cipher = ChaCha20Poly1305::new_from_slice(&wrap_key)
        .map_err(|e| CryptoError::Decryption(e.to_string()))?;
    let nonce = chacha20poly1305::Nonce::from(nonce_bytes);
    let payload_key_bytes = wrap_cipher
        .decrypt(&nonce, entry.encrypted_payload_key.as_ref())
        .map_err(|e| CryptoError::Decryption(format!("payload key decrypt failed: {e}")))?;

    if payload_key_bytes.len() != 32 {
        return Err(CryptoError::Decryption(
            "decrypted payload key has wrong length".into(),
        ));
    }
    let mut payload_key = [0u8; 32];
    payload_key.copy_from_slice(&payload_key_bytes);

    // Decrypt chunks in order.
    let payload_cipher = ChaCha20Poly1305::new_from_slice(&payload_key)
        .map_err(|e| CryptoError::Decryption(e.to_string()))?;

    let mut plaintext = Vec::new();

    // Sort chunks by sequence number — the envelope spec keeps them ordered,
    // but defensive ordering is free.
    let mut sorted_chunks: Vec<&EncryptedChunk> = envelope.payload_chunks.iter().collect();
    sorted_chunks.sort_by_key(|c| c.sequence);

    for chunk in &sorted_chunks {
        let chunk_nonce = chacha20poly1305::Nonce::from(chunk.nonce);
        let chunk_plain = payload_cipher
            .decrypt(&chunk_nonce, chunk.ciphertext.as_ref())
            .map_err(|e| {
                CryptoError::Decryption(format!("chunk {} decrypt failed: {e}", chunk.sequence))
            })?;
        plaintext.extend_from_slice(&chunk_plain);
    }

    Ok(plaintext)
}

/// Create a signed (non-encrypted) envelope containing `payload` signed by `signer`.
pub fn sign_attached(
    signer: &ed25519_dalek::SigningKey,
    payload: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let signature: Signature = signer.sign(payload);
    let verifying_key: [u8; 32] = signer.verifying_key().to_bytes();

    let wire = SignedEnvelopeWire {
        format: "sovereign-signed".to_string(),
        version: [1, 0],
        verifying_key,
        signature: signature.to_bytes().to_vec(),
        payload: payload.to_vec(),
    };

    rmp_serde::to_vec_named(&wire)
        .map_err(|e| CryptoError::Encoding(e.to_string()))
}

/// Verify a signed envelope and return the signer's verifying key plus the payload.
pub fn verify_attached(
    signed_bytes: &[u8],
) -> Result<(VerifyingKey, Vec<u8>), CryptoError> {
    let wire: SignedEnvelopeWire = rmp_serde::from_slice(signed_bytes)
        .map_err(|e| CryptoError::Decoding(e.to_string()))?;

    if wire.format != "sovereign-signed" {
        return Err(CryptoError::InvalidSignature(format!(
            "unexpected format: {}",
            wire.format
        )));
    }

    let verifying_key = VerifyingKey::from_bytes(&wire.verifying_key)
        .map_err(|e| CryptoError::InvalidKey(e.to_string()))?;

    let sig_bytes: [u8; 64] = wire
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| CryptoError::InvalidSignature("signature must be 64 bytes".into()))?;
    let signature = Signature::from_bytes(&sig_bytes);

    verifying_key
        .verify(&wire.payload, &signature)
        .map_err(|e| CryptoError::InvalidSignature(e.to_string()))?;

    Ok((verifying_key, wire.payload))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use x25519_dalek::StaticSecret;

    fn make_recipient() -> (StaticSecret, x25519_dalek::PublicKey) {
        let secret = StaticSecret::from([0x42u8; 32]);
        let public = x25519_dalek::PublicKey::from(&secret);
        (secret, public)
    }

    fn make_ephemeral() -> StaticSecret {
        StaticSecret::from([0xEFu8; 32])
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let envelope = SovereignEnvelope {
            format: "sovereign-envelope".to_string(),
            version: [1, 0],
            mode: 0,
            ephemeral_public: [0u8; 32],
            sender_secretbox: vec![],
            recipients: vec![],
            payload_chunks: vec![],
        };
        let bytes = encode(&envelope).expect("encode failed");
        let decoded = decode(&bytes).expect("decode failed");
        assert_eq!(decoded.format, "sovereign-envelope");
        assert_eq!(decoded.version, [1, 0]);
    }

    #[test]
    fn test_encrypt_decrypt_single_recipient() {
        let (recipient_secret, recipient_pub) = make_recipient();
        let ephemeral = make_ephemeral();
        let plaintext = b"hello, sovereign world!";

        let envelope = encrypt(plaintext, &[recipient_pub], ephemeral)
            .expect("encrypt failed");

        assert_eq!(envelope.format, "sovereign-envelope");
        assert_eq!(envelope.version, [1, 0]);
        assert_eq!(envelope.recipients.len(), 1);
        assert!(!envelope.payload_chunks.is_empty());

        let recovered = decrypt(&envelope, &recipient_secret, &recipient_pub)
            .expect("decrypt failed");

        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_multi_recipient() {
        let (secret_a, pub_a) = (StaticSecret::from([0x11u8; 32]), {
            let s = StaticSecret::from([0x11u8; 32]);
            x25519_dalek::PublicKey::from(&s)
        });
        let (secret_b, pub_b) = (StaticSecret::from([0x22u8; 32]), {
            let s = StaticSecret::from([0x22u8; 32]);
            x25519_dalek::PublicKey::from(&s)
        });
        let ephemeral = StaticSecret::from([0xABu8; 32]);
        let plaintext = b"multi-recipient test";

        let envelope = encrypt(plaintext, &[pub_a, pub_b], ephemeral)
            .expect("encrypt failed");

        assert_eq!(envelope.recipients.len(), 2);

        let recovered_a = decrypt(&envelope, &secret_a, &pub_a).expect("decrypt A failed");
        let recovered_b = decrypt(&envelope, &secret_b, &pub_b).expect("decrypt B failed");

        assert_eq!(recovered_a, plaintext);
        assert_eq!(recovered_b, plaintext);
    }

    #[test]
    fn test_encrypt_large_payload_chunked() {
        let (recipient_secret, recipient_pub) = make_recipient();
        let ephemeral = make_ephemeral();
        // 2.5 MiB — forces three chunks.
        let plaintext: Vec<u8> = (0u8..=255).cycle().take(2 * 1024 * 1024 + 512 * 1024).collect();

        let envelope = encrypt(&plaintext, &[recipient_pub], ephemeral)
            .expect("encrypt failed");

        assert_eq!(envelope.payload_chunks.len(), 3);

        let recovered = decrypt(&envelope, &recipient_secret, &recipient_pub)
            .expect("decrypt failed");

        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_recipient_fails() {
        let (_secret_a, pub_a) = make_recipient();
        let ephemeral = make_ephemeral();
        let (secret_b, pub_b) = (StaticSecret::from([0x99u8; 32]), {
            let s = StaticSecret::from([0x99u8; 32]);
            x25519_dalek::PublicKey::from(&s)
        });

        let envelope = encrypt(b"secret", &[pub_a], ephemeral).expect("encrypt failed");
        let result = decrypt(&envelope, &secret_b, &pub_b);
        assert!(result.is_err(), "decrypt by wrong recipient should fail");
    }

    #[test]
    fn test_encrypt_empty_recipients_fails() {
        let ephemeral = make_ephemeral();
        let result = encrypt(b"data", &[], ephemeral);
        assert!(result.is_err());
    }

    #[test]
    fn test_sign_and_verify_attached() {
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[0xBBu8; 32]);
        let payload = b"sovereignty payload";

        let signed = sign_attached(&signing_key, payload).expect("sign failed");
        let (vk, recovered) = verify_attached(&signed).expect("verify failed");

        assert_eq!(recovered, payload);
        assert_eq!(vk.to_bytes(), signing_key.verifying_key().to_bytes());
    }

    #[test]
    fn test_verify_tampered_payload_fails() {
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[0xCCu8; 32]);
        let payload = b"original";

        let mut signed = sign_attached(&signing_key, payload).expect("sign failed");
        // Flip the last byte to tamper with the serialized payload.
        if let Some(last) = signed.last_mut() {
            *last ^= 0xFF;
        }

        let result = verify_attached(&signed);
        assert!(result.is_err(), "tampered envelope should fail verification");
    }

    #[test]
    fn test_envelope_encode_decode_with_data() {
        let (recipient_secret, recipient_pub) = make_recipient();
        let ephemeral = make_ephemeral();
        let plaintext = b"roundtrip via msgpack";

        let envelope = encrypt(plaintext, &[recipient_pub], ephemeral)
            .expect("encrypt failed");

        let bytes = encode(&envelope).expect("encode failed");
        let decoded = decode(&bytes).expect("decode failed");
        let recovered = decrypt(&decoded, &recipient_secret, &recipient_pub)
            .expect("decrypt failed");

        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let (recipient_secret, recipient_pub) = make_recipient();
        let ephemeral = make_ephemeral();

        let envelope = encrypt(&[], &[recipient_pub], ephemeral).expect("encrypt failed");
        let recovered = decrypt(&envelope, &recipient_secret, &recipient_pub)
            .expect("decrypt failed");

        assert_eq!(recovered, b"");
    }
}
