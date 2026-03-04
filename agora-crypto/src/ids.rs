//! BLAKE3 content-addressed ID generation.
//!
//! Replaces all `Uuid::new_v4()` and `EventId::new()` calls with deterministic,
//! content-derived identifiers. Same inputs always produce the same ID.
//!
//! # S-02 Compliance
//! All functions take explicit inputs — no `SystemTime`, `OsRng`, or `thread_rng`.
//! Callers must supply a timestamp from a `TimestampProvider`.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

/// Separator byte between hash input fields.
const SEP: u8 = 0x00;

/// Generate a deterministic Matrix event ID.
///
/// Format: `$blake3:<base64url(hash[..20])>`
///
/// # Arguments
/// - `room_id` — the room the event belongs to
/// - `sender` — the sender's user ID string
/// - `event_type` — Matrix event type string (e.g. `m.room.message`)
/// - `content_bytes` — canonical JSON serialization of the event content
/// - `timestamp` — monotonic sequence timestamp from `TimestampProvider`
pub fn event_id(
    room_id: &str,
    sender: &str,
    event_type: &str,
    content_bytes: &[u8],
    timestamp: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"agora:event_id:v1");
    hasher.update(&[SEP]);
    hasher.update(room_id.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(sender.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(event_type.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(content_bytes);
    hasher.update(&[SEP]);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    format!("$blake3:{}", URL_SAFE_NO_PAD.encode(&hash.as_bytes()[..20]))
}

/// Generate a deterministic Matrix room ID.
///
/// Format: `!blake3:<base64url(hash[..12])>:<domain>`
///
/// # Arguments
/// - `creator` — user ID of the room creator
/// - `room_name` — initial room name (may be empty)
/// - `timestamp` — monotonic sequence timestamp
/// - `domain` — server name (e.g. `localhost`)
pub fn room_id(creator: &str, room_name: &str, timestamp: u64, domain: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"agora:room_id:v1");
    hasher.update(&[SEP]);
    hasher.update(creator.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(room_name.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    format!(
        "!blake3:{}:{}",
        URL_SAFE_NO_PAD.encode(&hash.as_bytes()[..12]),
        domain
    )
}

/// Generate a deterministic media ID.
///
/// Format: `<base64url(hash[..16])>`
///
/// # Arguments
/// - `uploader` — user ID of the uploader
/// - `content_hash` — BLAKE3 hash of the media content itself
/// - `timestamp` — monotonic sequence timestamp
pub fn media_id(uploader: &str, content_hash: &[u8; 32], timestamp: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"agora:media_id:v1");
    hasher.update(&[SEP]);
    hasher.update(uploader.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(content_hash);
    hasher.update(&[SEP]);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(&hash.as_bytes()[..16])
}

/// Generate a deterministic access token.
///
/// Format: `agora_<base64url(hash[..24])>`
///
/// # Arguments
/// - `user_id` — the authenticated user's ID
/// - `device_id` — the device being registered
/// - `timestamp` — monotonic sequence timestamp
pub fn access_token(user_id: &str, device_id: &str, timestamp: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"agora:access_token:v1");
    hasher.update(&[SEP]);
    hasher.update(user_id.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(device_id.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    format!("agora_{}", URL_SAFE_NO_PAD.encode(&hash.as_bytes()[..24]))
}

/// Generate a deterministic device ID.
///
/// Format: `<uppercase-hex(hash[..5])>` — 10 hex chars, matches existing format
///
/// # Arguments
/// - `user_id` — the user this device belongs to
/// - `timestamp` — monotonic sequence timestamp
pub fn device_id(user_id: &str, timestamp: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"agora:device_id:v1");
    hasher.update(&[SEP]);
    hasher.update(user_id.as_bytes());
    hasher.update(&[SEP]);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    hex_upper(&hash.as_bytes()[..5])
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_deterministic() {
        let id1 = event_id("!room:localhost", "@alice:localhost", "m.room.message", b"{}", 42);
        let id2 = event_id("!room:localhost", "@alice:localhost", "m.room.message", b"{}", 42);
        assert_eq!(id1, id2);
        assert!(id1.starts_with("$blake3:"));
    }

    #[test]
    fn event_id_differs_on_timestamp() {
        let id1 = event_id("!r:s", "@u:s", "m.t", b"{}", 1);
        let id2 = event_id("!r:s", "@u:s", "m.t", b"{}", 2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn room_id_deterministic() {
        let id1 = room_id("@alice:localhost", "general", 0, "localhost");
        let id2 = room_id("@alice:localhost", "general", 0, "localhost");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("!blake3:"));
        assert!(id1.ends_with(":localhost"));
    }

    #[test]
    fn media_id_deterministic() {
        let content_hash = blake3::hash(b"test content").into();
        let id1 = media_id("@alice:localhost", &content_hash, 5);
        let id2 = media_id("@alice:localhost", &content_hash, 5);
        assert_eq!(id1, id2);
    }

    #[test]
    fn device_id_format() {
        let id = device_id("@alice:localhost", 0);
        assert_eq!(id.len(), 10);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && (c.is_uppercase() || c.is_ascii_digit())));
    }

    #[test]
    fn no_uuid_v4_usage() {
        // All IDs are deterministic — no randomness involved.
        let ts = [0u64, 1, 2, 100, u64::MAX - 1];
        for t in ts {
            let id = event_id("!r:s", "@u:s", "m.t", b"x", t);
            assert!(id.starts_with("$blake3:"), "unexpected format: {id}");
        }
    }
}
