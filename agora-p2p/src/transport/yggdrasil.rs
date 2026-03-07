//! Yggdrasil transport adapter for agora-p2p.
//!
//! Strategy:
//! 1. Probe for a running Yggdrasil daemon via its admin socket.
//! 2. If found, query the daemon for this node's Yggdrasil IPv6 address.
//! 3. Bind the QUIC endpoint to that IPv6 address.
//! 4. If no daemon, return None and let P2pNode fall back to QUIC on 0.0.0.0.

use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use crate::types::YggdrasilConfig;

/// Returns the platform-default Yggdrasil admin socket path.
pub fn default_admin_socket_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        // Default macOS Yggdrasil admin socket location
        PathBuf::from("/tmp/yggdrasil.sock")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/run/yggdrasil.sock")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from("/tmp/yggdrasil.sock")
    }
}

/// Probe for a running Yggdrasil daemon and return the local Yggdrasil address if found.
///
/// Returns `None` if the daemon is not running or not reachable.
/// This function must never panic — it is called at startup and failure
/// simply means we fall back to raw QUIC.
pub fn probe_yggdrasil_daemon(admin_socket: Option<&Path>) -> Option<Ipv6Addr> {
    let socket_path = admin_socket
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_admin_socket_path);

    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(&socket_path).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;

    let request = r#"{"keepalive":false,"request":"getself}"#;
    stream.write_all(request.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    parse_yggdrasil_address(&response)
}

/// Parse Yggdrasil IPv6 address from the daemon's getself JSON response.
fn parse_yggdrasil_address(json: &str) -> Option<Ipv6Addr> {
    let key = r#""address":""#;
    let start = json.find(key)? + key.len();
    let end = json[start..].find('"')? + start;
    let addr_str = &json[start..end];
    addr_str.parse().ok()
}

/// Determine the bind address for agora-p2p given the transport config.
///
/// Returns:
/// - `Some(SocketAddr)` with the Yggdrasil IPv6 address if the daemon is running
/// - `None` if Yggdrasil is unavailable (caller should fall back to QUIC on 0.0.0.0)
pub fn resolve_yggdrasil_bind_addr(config: &YggdrasilConfig) -> Option<SocketAddr> {
    let admin_socket = config.admin_socket.as_ref().map(|s| Path::new(s));
    let ygg_addr = probe_yggdrasil_daemon(admin_socket)?;
    Some(SocketAddr::new(
        std::net::IpAddr::V6(ygg_addr),
        config.listen_port,
    ))
}

/// Check if an address is a Yggdrasil address (in 200::/7 range)
// IMPLEMENTATION_REQUIRED: wired in future wt-XXX for Yggdrasil address detection
pub fn is_yggdrasil_addr(addr: &std::net::SocketAddr) -> bool {
    if let std::net::IpAddr::V6(ipv6) = addr.ip() {
        let octets = ipv6.octets();
        (octets[0] & 0xfe) == 0x02
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_socket_path() {
        let path = default_admin_socket_path();
        assert!(path.to_string_lossy().contains("yggdrasil"));
    }

    #[test]
    fn test_parse_yggdrasil_address_valid() {
        let json = r#"{"request":"getself","response":{"address":"200:dead:beef:cafe::1","nodeId":"dead:beef:cafe:dead:beef:cafe:dead:beef"}}"#;
        let addr = parse_yggdrasil_address(json);
        assert!(addr.is_some());
    }

    #[test]
    fn test_parse_yggdrasil_address_malformed() {
        let json = "not json";
        let result = parse_yggdrasil_address(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_is_yggdrasil_addr() {
        let ygg_addr: SocketAddr = "[200:dead:beef::1]:1234".parse().unwrap();
        assert!(is_yggdrasil_addr(&ygg_addr));

        let normal_addr: SocketAddr = "192.168.1.1:1234".parse().unwrap();
        assert!(!is_yggdrasil_addr(&normal_addr));
    }
}
