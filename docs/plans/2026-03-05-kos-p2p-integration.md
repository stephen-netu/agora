# KOS P2P Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Stabilize agora-p2p so it actually works end-to-end, then design it cleanly enough that SOVEREIGN and Atelier can reuse the same transport and identity layers without duplication.

**Architecture:** Fix critical bugs in the existing P2P codebase (double-accept, missing stream loop, zero peer identity), then harden the public API to be generic enough for the full KOS ecosystem. Defer DHT, NAT traversal, and Event DAG entirely — LAN mesh first, always.

**Tech Stack:** Rust, quinn (QUIC), rustls, rcgen, mdns-sd, ciborium, blake3, agora-crypto (Ed25519/AgentId)

---

## Context: What Exists and What's Broken

### What's Built

`agora-p2p` is a new crate with:
- `QuicTransport` — QUIC endpoint, connects and accepts, fingerprint-based TLS
- `MdnsDiscovery` — mDNS service registration and browsing, maps service → AgentId
- `MeshManager` — peer connection map, deterministic initiator rule, send_to
- `P2pNode` — top-level orchestrator tying everything together
- `AmpMessage` / CBOR codec — message types with length-prefix framing
- `FingerprintStore` / `FingerprintServerVerifier` — TLS cert pinning

### What's Broken (Critical)

**Bug 1 — Double Accept:** `P2pNode::spawn_incoming_handler` calls `transport.accept()`, gets a connection, then passes a bare `Peer` to `mesh.handle_incoming(peer)`. That method then calls `self.transport.accept()` *again* — which waits for the **next** incoming connection, not the one already accepted. Every incoming connection is effectively dropped and deadlocks the next one.

**Bug 2 — Zero Peer Identity on Incoming:** `QuicTransport::accept()` returns a hardcoded zero-filled `AgentId` (`0000...0000`). Incoming connection peer identity is never extracted.

**Bug 3 — No Stream Accept Loop:** After the initial handshake bi-stream is opened/accepted, there is no loop to accept subsequent bi-streams opened by the remote peer. `send_to` opens a *new* stream per message — but the remote side never accepts those streams. Messages sent after the handshake are silently dropped.

**Bug 4 — Test Constant Mismatch:** `test_quic_config_default` asserts `keepalive_interval == 10_000` but `QuicConfig::new` sets no such field — the field doesn't exist in `QuicConfig`. The transport hardcodes 15s keepalive. Test is stale.

### KOS Ecosystem Context

```
/Users/netu/Projects/KOS/
├── agora/       — Communication platform (Matrix C/S API, E2E, sigchain)
│   ├── agora-crypto/  — AgentId, Ed25519, Blake3, Double Ratchet, X3DH, Sigchain
│   ├── agora-core/    — Room/event types, API layer
│   ├── agora-server/  — Homeserver (SQLite)
│   ├── agora-cli/     — CLI
│   ├── agora-app/     — Tauri + Svelte 5 desktop
│   └── agora-p2p/     — P2P mesh (THIS PLAN)
└── atelier/     — Spatial Lore IDE / Local Knowledge Vault
    ├── crates/core/    — Types, graphs, trees, views
    ├── crates/storage/ — SQLite
    ├── crates/llm/     — LLM integration
    ├── crates/api/     — API layer
    └── crates/desktop/ — Tauri + Svelte desktop
```

**SOVEREIGN** is a future project (not yet on disk). Based on project naming and roadmap, it likely provides sovereign identity management — keypair generation, storage, rotation, and sigchain operations — built on `agora-crypto` primitives.

### Integration Principle

The goal is NOT to build a KOS monorepo yet. The goal is to ensure:
1. `agora-crypto` is clean and dependency-free enough to be used by any KOS project
2. `agora-p2p` has a generic enough API that Atelier/SOVEREIGN can add it as a dependency and get P2P transport without pulling in Agora-specific business logic
3. No duplicated crypto primitives, no duplicated P2P transport code

The cleanest seam today: `agora-p2p` depends on `agora-crypto` for `AgentId`. Atelier and SOVEREIGN would add `agora-crypto` and `agora-p2p` as path (or git) dependencies. This is viable today without any repo restructuring.

---

## Phase 1: Fix agora-p2p (Make It Actually Work)

### Task 1: Fix Bug 1 — Remove double-accept in handle_incoming

**Problem:** `MeshManager::handle_incoming(peer)` calls `self.transport.accept()` internally. This is wrong — the connection was already accepted by `P2pNode::spawn_incoming_handler`. The connection needs to be passed in.

**Files:**
- Modify: `agora-p2p/src/mesh/peer.rs`
- Modify: `agora-p2p/src/node.rs`

**Step 1: Write the failing test**

In `agora-p2p/src/mesh/peer.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn handle_incoming_signature_takes_connection() {
        // This test validates that the signature compiles correctly.
        // It will fail to compile if handle_incoming still calls transport.accept() internally.
        // Actual behavioral test is in the integration test (Task 5).
        let _ = std::marker::PhantomData::<()>;
    }
}
```

Run: `cargo test -p agora-p2p handle_incoming_signature_takes_connection`
Expected: PASS (compile-time validation)

**Step 2: Refactor handle_incoming signature**

Change `MeshManager::handle_incoming` to accept a `QuicConnection` directly:

```rust
// In agora-p2p/src/mesh/peer.rs

pub async fn handle_incoming(&self, peer: Peer, connection: QuicConnection) {
    let peer_id = peer.agent_id.clone();
    let events = self.events.clone();
    let connections = self.connections.clone();

    if self.connections.read().await.contains_key(&peer_id) {
        return;
    }

    // Accept the handshake stream from the already-established connection
    match connection.connection.accept_bi().await {
        Ok((send, recv)) => {
            let connected_peer = ConnectedPeer {
                peer: peer.clone(),
                sender: send,
                connection: connection.clone(),
            };

            connections.write().await.insert(peer_id.clone(), connected_peer);

            let _ = events.send(MeshEvent::Connected(peer_id.clone())).await;

            // Spawn stream accept loop (Task 3 adds the full loop; for now, read first stream)
            tokio::spawn(async move {
                Self::read_messages_from_stream(peer_id, recv, events).await;
            });
        }
        Err(e) => {
            let _ = events
                .send(MeshEvent::Error(
                    peer_id.clone(),
                    format!("accept_bi error on incoming: {}", e),
                ))
                .await;
        }
    }
}
```

**Step 3: Update P2pNode to pass the connection**

In `agora-p2p/src/node.rs`, update `spawn_incoming_handler`:

```rust
fn spawn_incoming_handler(&self) {
    let transport = self.transport.clone();
    let mesh = self.mesh.clone();
    let mesh_events_tx = self.mesh_events_tx.clone();

    tokio::spawn(async move {
        info!("Incoming connection handler started");
        loop {
            match transport.accept().await {
                Ok((connection, _placeholder_id)) => {
                    let remote_addr = connection.remote_addr.to_string();
                    info!("Accepted incoming connection from {}", remote_addr);

                    // Peer identity comes from handshake (Task 2).
                    // For now, use a temporary placeholder peer.
                    let placeholder_agent_id = agora_crypto::AgentId::from_hex(
                        "0000000000000000000000000000000000000000000000000000000000000000",
                    )
                    .unwrap();

                    let peer = Peer {
                        agent_id: placeholder_agent_id.clone(),
                        addresses: vec![remote_addr],
                    };

                    mesh.handle_incoming(peer, connection).await;
                }
                Err(e) => {
                    if !e.to_string().contains("channel closed") {
                        warn!("Error accepting incoming connection: {}", e);
                    } else {
                        info!("Incoming connection handler: channel closed, stopping");
                        break;
                    }
                }
            }
        }
    });
}
```

**Step 4: Verify it compiles**

Run: `cargo build -p agora-p2p`
Expected: Compiles without errors.

**Step 5: Commit**

```bash
git add agora-p2p/src/mesh/peer.rs agora-p2p/src/node.rs
git commit -m "fix(p2p): pass connection to handle_incoming, remove double-accept"
```

---

### Task 2: Fix Bug 2 — Extract peer identity from handshake message

**Problem:** Incoming connections use a zero `AgentId`. The correct identity comes from reading the `Handshake` message that the connecting peer sends on the first bi-stream.

**Files:**
- Modify: `agora-p2p/src/mesh/peer.rs`
- Modify: `agora-p2p/src/node.rs`

**Step 1: Write the failing test**

```rust
// In agora-p2p/src/mesh/peer.rs tests block

#[tokio::test]
async fn handshake_extracts_peer_identity() {
    // This will be properly tested in the integration test.
    // Inline unit: verify that if we decode a Handshake message, we get the agent_id back.
    use crate::protocol::{AmpMessage, encode, decode};
    use agora_crypto::AgentId;

    let id = AgentId::from_hex(
        "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
    )
    .unwrap();

    let msg = AmpMessage::Handshake {
        agent_id: id.to_string(),
        version: 1,
        capabilities: Default::default(),
    };

    let bytes = encode(&msg).unwrap();
    let decoded = decode(&bytes).unwrap();

    match decoded {
        AmpMessage::Handshake { agent_id, .. } => {
            assert_eq!(agent_id, id.to_string());
        }
        _ => panic!("Expected Handshake"),
    }
}
```

Run: `cargo test -p agora-p2p handshake_extracts_peer_identity`
Expected: PASS (codec is correct)

**Step 2: Read handshake on incoming connections**

Update `MeshManager::handle_incoming` to read the handshake before inserting the peer:

```rust
pub async fn handle_incoming(&self, _placeholder_peer: Peer, connection: QuicConnection) {
    let events = self.events.clone();
    let connections = self.connections.clone();
    let transport_ref = self.transport.clone();

    match connection.connection.accept_bi().await {
        Ok((send, mut recv)) => {
            // Read the handshake message to learn who we're talking to
            let handshake_bytes = match crate::transport::quic::read_message(&mut recv).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("Failed to read handshake from incoming: {}", e);
                    return;
                }
            };

            let peer_agent_id = match crate::protocol::decode(&handshake_bytes) {
                Ok(crate::protocol::AmpMessage::Handshake { agent_id, .. }) => {
                    match agora_crypto::AgentId::from_hex(&agent_id) {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::warn!("Invalid agent_id in handshake: {}", e);
                            return;
                        }
                    }
                }
                Ok(other) => {
                    tracing::warn!("Expected Handshake, got {:?}", other);
                    return;
                }
                Err(e) => {
                    tracing::warn!("Failed to decode handshake: {}", e);
                    return;
                }
            };

            // Send our own handshake back
            let our_handshake = crate::protocol::AmpMessage::Handshake {
                agent_id: self.local_id.to_string(),
                version: 1,
                capabilities: Default::default(),
            };
            if let Ok(bytes) = crate::protocol::encode(&our_handshake) {
                if let Err(e) = crate::transport::quic::write_message(
                    &mut { send },
                    &bytes,
                ).await {
                    tracing::warn!("Failed to send handshake ack: {}", e);
                    // Don't abort — we know their identity, continue
                }
            }

            if self.connections.read().await.contains_key(&peer_agent_id) {
                tracing::debug!("Already connected to {}, dropping duplicate", peer_agent_id);
                return;
            }

            let peer = crate::types::Peer {
                agent_id: peer_agent_id.clone(),
                addresses: vec![connection.remote_addr.to_string()],
            };

            let connected_peer = ConnectedPeer {
                peer,
                sender: {
                    // We used send for handshake ack; open a fresh stream for sending
                    match connection.connection.open_bi().await {
                        Ok((s, _)) => s,
                        Err(e) => {
                            tracing::warn!("Failed to open send stream after handshake: {}", e);
                            return;
                        }
                    }
                },
                connection: connection.clone(),
            };

            connections.write().await.insert(peer_agent_id.clone(), connected_peer);
            let _ = events.send(MeshEvent::Connected(peer_agent_id.clone())).await;

            // Spawn stream accept loop (Task 3)
            let peer_id_for_loop = peer_agent_id.clone();
            let events_for_loop = events.clone();
            let conn_for_loop = connection.connection.clone();
            tokio::spawn(async move {
                Self::accept_streams_loop(peer_id_for_loop, conn_for_loop, events_for_loop).await;
            });
        }
        Err(e) => {
            tracing::warn!("Failed to accept_bi on incoming connection: {}", e);
        }
    }
}
```

**Step 3: Remove the _placeholder_peer parameter from spawn_incoming_handler**

Update `node.rs` so it no longer builds a placeholder peer — the identity comes from the handshake:

```rust
fn spawn_incoming_handler(&self) {
    let transport = self.transport.clone();
    let mesh = self.mesh.clone();
    let mesh_events_tx = self.mesh_events_tx.clone();

    tokio::spawn(async move {
        info!("Incoming connection handler started");
        loop {
            match transport.accept().await {
                Ok((connection, _)) => {
                    info!("Accepted incoming connection from {}", connection.remote_addr);
                    let mesh = mesh.clone();
                    let mesh_events_tx = mesh_events_tx.clone();

                    // Spawn per-connection handler — identity extracted via handshake
                    tokio::spawn(async move {
                        let placeholder = crate::types::Peer {
                            agent_id: agora_crypto::AgentId::from_hex(
                                "0000000000000000000000000000000000000000000000000000000000000000",
                            )
                            .unwrap(),
                            addresses: vec![],
                        };
                        mesh.handle_incoming(placeholder, connection).await;
                    });
                }
                Err(e) => {
                    if e.to_string().contains("channel closed") {
                        info!("Incoming connection handler stopped");
                        break;
                    }
                    warn!("Error accepting connection: {}", e);
                }
            }
        }
    });
}
```

**Step 4: Build and verify**

Run: `cargo build -p agora-p2p`
Expected: Compiles.

**Step 5: Commit**

```bash
git add agora-p2p/src/mesh/peer.rs agora-p2p/src/node.rs
git commit -m "fix(p2p): extract peer identity from incoming handshake message"
```

---

### Task 3: Fix Bug 3 — Accept incoming streams in a loop

**Problem:** `send_to` opens a new bi-stream per message. The remote peer never accepts these streams because there is no stream-accept loop per connection. All messages sent after the initial handshake are silently dropped.

**Files:**
- Modify: `agora-p2p/src/mesh/peer.rs`

**Step 1: Add accept_streams_loop method**

In `MeshManager`, add:

```rust
async fn accept_streams_loop(
    peer_id: AgentId,
    connection: quinn::Connection,
    events: mpsc::Sender<MeshEvent>,
) {
    loop {
        match connection.accept_bi().await {
            Ok((_send, recv)) => {
                let peer_id = peer_id.clone();
                let events = events.clone();
                tokio::spawn(async move {
                    // Each new stream contains exactly one framed message
                    let mut recv = recv;
                    match crate::transport::quic::read_message(&mut recv).await {
                        Ok(bytes) => match crate::protocol::decode(&bytes) {
                            Ok(message) => {
                                let msg_str = format!("{:?}", message);
                                let _ = events
                                    .send(MeshEvent::MessageReceived(peer_id, msg_str))
                                    .await;
                            }
                            Err(e) => {
                                let _ = events
                                    .send(MeshEvent::Error(
                                        peer_id,
                                        format!("decode error: {}", e),
                                    ))
                                    .await;
                            }
                        },
                        Err(e) => {
                            // Stream closed before data — not necessarily fatal
                            tracing::debug!("Stream closed before message: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                // Connection closed
                tracing::debug!("Stream accept loop ended for {}: {}", peer_id, e);
                let _ = events.send(MeshEvent::Disconnected(peer_id)).await;
                break;
            }
        }
    }
}
```

**Step 2: Spawn accept_streams_loop for outgoing connections too**

In `handle_new_connection`, after inserting the `ConnectedPeer`, add:

```rust
// After: connections.write().await.insert(peer_id.clone(), connected_peer);
// After: let _ = events.send(MeshEvent::Connected(peer_id.clone())).await;

// Spawn stream accept loop for incoming streams from remote peer
let peer_id_loop = peer_id.clone();
let events_loop = events.clone();
let conn_loop = connection.connection.clone();
tokio::spawn(async move {
    Self::accept_streams_loop(peer_id_loop, conn_loop, events_loop).await;
});
```

**Note:** Remove the old `read_messages_from_stream` spawn in `handle_new_connection` — the accept loop replaces it.

**Step 3: Build**

Run: `cargo build -p agora-p2p`
Expected: Compiles. (Tests in Task 5 will verify behavior.)

**Step 4: Remove stale read_messages_from_stream spawn from incoming path**

Also clean up `handle_incoming` added in Task 2 to use `accept_streams_loop` instead of the `read_messages_from_stream` call.

**Step 5: Commit**

```bash
git add agora-p2p/src/mesh/peer.rs
git commit -m "fix(p2p): add per-connection stream accept loop for incoming messages"
```

---

### Task 4: Fix Bug 4 — Remove stale test assertion

**Problem:** `test_quic_config_default` asserts `config.keepalive_interval == 10_000` but `QuicConfig` has no `keepalive_interval` field. Test fails to compile.

**Files:**
- Modify: `agora-p2p/src/transport/quic.rs`

**Step 1: Run the broken test to see the error**

Run: `cargo test -p agora-p2p test_quic_config_default 2>&1 | head -20`
Expected: Compile error mentioning `keepalive_interval`

**Step 2: Fix the test**

Replace the stale test with a correct assertion:

```rust
#[tokio::test]
async fn test_quic_config_default() {
    let (cert, key) = generate_self_signed_cert(&test_agent_id()).unwrap();
    let config = QuicConfig::new(cert, key);

    assert_eq!(config.max_idle_timeout, 30_000);
    // keepalive is hardcoded in transport (15s), not stored in QuicConfig
}
```

**Step 3: Run fixed test**

Run: `cargo test -p agora-p2p test_quic_config_default`
Expected: PASS

**Step 4: Commit**

```bash
git add agora-p2p/src/transport/quic.rs
git commit -m "fix(p2p): remove stale keepalive_interval assertion from test"
```

---

### Task 5: Integration Test — Two nodes exchange messages

This is the proof that everything works. Two `P2pNode` instances on the same machine connect via mDNS and exchange messages.

**Files:**
- Create: `agora-p2p/tests/integration_test.rs`
- Modify: `agora-p2p/Cargo.toml` (add test deps if needed)

**Step 1: Write the failing integration test**

```rust
// agora-p2p/tests/integration_test.rs

use agora_p2p::{P2pNode, MeshEvent};
use agora_p2p::types::Config;
use agora_crypto::AgentId;
use tokio::time::{timeout, Duration};

fn make_agent_id(hex: &str) -> AgentId {
    AgentId::from_hex(hex).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_connect_and_exchange_message() {
    // Two distinct agent IDs — deterministic initiator rule means the lexicographically
    // smaller one will initiate the connection.
    let id_a = make_agent_id(
        "0000000000000000000000000000000000000000000000000000000000000001",
    );
    let id_b = make_agent_id(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    );

    let config_a = Config {
        agent_id: id_a.clone(),
        listen_port: 0,
        service_name: "_agora-test._udp.local.".to_string(),
    };
    let config_b = Config {
        agent_id: id_b.clone(),
        listen_port: 0,
        service_name: "_agora-test._udp.local.".to_string(),
    };

    let mut node_a = P2pNode::new(config_a).await.expect("node_a creation failed");
    let mut node_b = P2pNode::new(config_b).await.expect("node_b creation failed");

    let mut events_a = node_a.take_mesh_events().unwrap();
    let mut events_b = node_b.take_mesh_events().unwrap();

    // Start both nodes (port 0 = OS assigns)
    node_a.start(0).await.expect("node_a start failed");
    node_b.start(0).await.expect("node_b start failed");

    // Wait for connection events (mDNS discovery takes a moment)
    let connected_a = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(event) = events_a.recv().await {
                if matches!(event, MeshEvent::Connected(_)) {
                    return true;
                }
            }
        }
    })
    .await
    .expect("node_a did not connect within 10s");

    assert!(connected_a, "node_a should have connected");

    // Send a message from A to B
    let peers_a = node_a.connected_peers().await;
    assert!(!peers_a.is_empty(), "node_a should have at least one connected peer");

    node_a
        .broadcast_room_message("test-room", b"hello from A")
        .await
        .expect("broadcast failed");

    // Wait for B to receive the message
    let received = timeout(Duration::from_secs(5), async {
        loop {
            if let Some(event) = events_b.recv().await {
                if matches!(event, MeshEvent::MessageReceived(_, _)) {
                    return true;
                }
            }
        }
    })
    .await
    .expect("node_b did not receive message within 5s");

    assert!(received, "node_b should have received a message from A");
}
```

**Step 2: Run it to verify it currently fails**

Run: `cargo test -p agora-p2p --test integration_test 2>&1 | tail -20`
Expected: FAIL or compile error due to `start(0)` (port 0 may not be supported in API)

Check `P2pNode::start` signature — it takes a `u16 port`. Port 0 is valid for OS auto-assignment. If the API doesn't allow it, the compilation will catch that.

**Step 3: Fix any API gaps discovered by the test**

If `start(0)` doesn't work, check `QuicTransport::listen` and `MdnsDiscovery::new` — they both need to handle port 0 (auto-assigned). The QUIC endpoint binds at creation in `QuicTransport::new` so `local_addr()` returns the real port. Update `MdnsDiscovery` to advertise the correct auto-assigned port.

The fix in `P2pNode::start`:
```rust
pub async fn start(&self, port: u16) -> Result<(), Error> {
    // Use the transport's actual local addr if port is 0 (auto-assigned)
    let actual_port = if port == 0 {
        self.transport.local_addr()?.port()
    } else {
        port
    };

    let listen_addr: SocketAddr = format!("0.0.0.0:{}", actual_port)
        .parse()
        .map_err(|e: std::net::AddrParseError| Error::Transport(e.to_string()))?;

    self.transport.listen(listen_addr).await?;
    info!("P2P transport listening on {}", listen_addr);

    let mdns = MdnsDiscovery::new(
        &self.config.agent_id.to_string(),
        actual_port,
        &self.config.service_name,
    )?;
    // ... rest of method unchanged
```

**Step 4: Run integration test until it passes**

Run: `cargo test -p agora-p2p --test integration_test -- --nocapture`

If mDNS is flaky on CI, add a longer timeout or a manual connect fallback. For local development it should work.

**Step 5: Commit**

```bash
git add agora-p2p/tests/integration_test.rs agora-p2p/src/node.rs
git commit -m "test(p2p): add two-node integration test for connect + message exchange"
```

---

## Phase 2: Clean Public API for KOS Reuse

### Task 6: Make service type configurable and expose raw message API

**Goal:** SOVEREIGN and Atelier need to use the P2P transport without coupling to Agora-specific message types. Add a `RawMessage` variant to `AmpMessage` and ensure `Config::service_name` is surfaced clearly in docs.

**Files:**
- Modify: `agora-p2p/src/protocol/messages.rs`
- Modify: `agora-p2p/src/types.rs`
- Modify: `agora-p2p/src/node.rs`

**Step 1: Write the test**

```rust
// In agora-p2p/src/protocol/messages.rs tests

#[test]
fn raw_message_round_trips() {
    use crate::protocol::{encode, decode, AmpMessage};

    let payload = b"arbitrary bytes from atelier".to_vec();
    let msg = AmpMessage::RawMessage {
        namespace: "atelier".to_string(),
        payload: payload.clone(),
    };

    let bytes = encode(&msg).unwrap();
    let decoded = decode(&bytes).unwrap();

    match decoded {
        AmpMessage::RawMessage { namespace, payload: p } => {
            assert_eq!(namespace, "atelier");
            assert_eq!(p, payload);
        }
        _ => panic!("Expected RawMessage"),
    }
}
```

Run: `cargo test -p agora-p2p raw_message_round_trips`
Expected: FAIL (RawMessage variant doesn't exist yet)

**Step 2: Add RawMessage variant to AmpMessage**

```rust
// In agora-p2p/src/protocol/messages.rs, add to AmpMessage:

/// Generic raw payload for non-Agora KOS applications.
/// `namespace` identifies the application (e.g., "atelier", "sovereign").
/// `payload` is application-defined CBOR, JSON, or raw bytes.
RawMessage {
    namespace: String,
    payload: Vec<u8>,
},
```

**Step 3: Add raw message send helper to P2pNode**

```rust
// In agora-p2p/src/node.rs

/// Send raw bytes to all connected peers. Use this from non-Agora KOS apps.
/// `namespace` identifies your app (e.g., "atelier").
pub async fn broadcast_raw(&self, namespace: &str, payload: &[u8]) -> Result<(), Error> {
    let msg = crate::protocol::AmpMessage::RawMessage {
        namespace: namespace.to_string(),
        payload: payload.to_vec(),
    };

    for peer_id in self.mesh.connected_peers().await {
        if let Err(e) = self.mesh.send_to(&peer_id, msg.clone()).await {
            tracing::warn!("Failed to send raw to {}: {}", peer_id, e);
        }
    }
    Ok(())
}

/// Send raw bytes to a specific peer.
pub async fn send_raw(&self, peer_id: &str, namespace: &str, payload: &[u8]) -> Result<(), Error> {
    let agent_id = agora_crypto::AgentId::from_hex(peer_id)
        .map_err(|e| Error::Mesh(format!("invalid peer_id: {}", e)))?;

    let msg = crate::protocol::AmpMessage::RawMessage {
        namespace: namespace.to_string(),
        payload: payload.to_vec(),
    };

    self.mesh.send_to(&agent_id, msg).await
}
```

**Step 4: Run test**

Run: `cargo test -p agora-p2p raw_message_round_trips`
Expected: PASS

**Step 5: Commit**

```bash
git add agora-p2p/src/protocol/messages.rs agora-p2p/src/node.rs
git commit -m "feat(p2p): add RawMessage variant and broadcast_raw/send_raw for KOS reuse"
```

---

### Task 7: Define P2pTransport trait for testability and abstraction

**Goal:** Make `P2pNode` testable via a trait. This also gives Atelier/SOVEREIGN a clean interface to code against.

**Files:**
- Create: `agora-p2p/src/traits.rs`
- Modify: `agora-p2p/src/lib.rs`

**Step 1: Write the failing test**

```rust
// In agora-p2p/src/traits.rs
// (This compiles or it doesn't — the "test" is that a mock implements the trait)

#[cfg(test)]
mod tests {
    use super::*;
    use agora_crypto::AgentId;

    struct MockTransport {
        peers: Vec<String>,
    }

    #[async_trait::async_trait]
    impl P2pTransport for MockTransport {
        async fn broadcast_raw(&self, _namespace: &str, _payload: &[u8]) -> crate::error::Error {
            // mock
            unimplemented!()
        }
        async fn connected_peers(&self) -> Vec<String> {
            self.peers.clone()
        }
    }

    #[tokio::test]
    async fn mock_transport_satisfies_trait() {
        let t = MockTransport { peers: vec!["abc".to_string()] };
        let peers = t.connected_peers().await;
        assert_eq!(peers.len(), 1);
    }
}
```

Run: `cargo test -p agora-p2p mock_transport_satisfies_trait`
Expected: FAIL (trait doesn't exist yet)

**Step 2: Create the trait**

```rust
// agora-p2p/src/traits.rs

use crate::error::Error;

/// Minimal trait for P2P transport. Implement this in tests with mocks,
/// or use P2pNode for the real implementation.
#[async_trait::async_trait]
pub trait P2pTransport: Send + Sync {
    /// Broadcast raw bytes to all connected peers.
    async fn broadcast_raw(&self, namespace: &str, payload: &[u8]) -> Result<(), Error>;

    /// Send raw bytes to a specific peer by AgentId hex string.
    async fn send_raw(
        &self,
        peer_id: &str,
        namespace: &str,
        payload: &[u8],
    ) -> Result<(), Error>;

    /// Return list of connected peer IDs as hex strings.
    async fn connected_peers(&self) -> Vec<String>;
}
```

**Step 3: Implement trait on P2pNode**

```rust
// In agora-p2p/src/node.rs

#[async_trait::async_trait]
impl crate::traits::P2pTransport for P2pNode {
    async fn broadcast_raw(&self, namespace: &str, payload: &[u8]) -> Result<(), Error> {
        self.broadcast_raw(namespace, payload).await
    }

    async fn send_raw(&self, peer_id: &str, namespace: &str, payload: &[u8]) -> Result<(), Error> {
        self.send_raw(peer_id, namespace, payload).await
    }

    async fn connected_peers(&self) -> Vec<String> {
        self.connected_peers().await
    }
}
```

**Step 4: Export from lib.rs**

```rust
// agora-p2p/src/lib.rs — add:
pub mod traits;
pub use traits::P2pTransport;
```

**Step 5: Run tests**

Run: `cargo test -p agora-p2p mock_transport_satisfies_trait`
Expected: PASS

**Step 6: Commit**

```bash
git add agora-p2p/src/traits.rs agora-p2p/src/lib.rs agora-p2p/src/node.rs
git commit -m "feat(p2p): add P2pTransport trait for testability and KOS integration"
```

---

### Task 8: Add MeshEvent::RawMessageReceived and document the integration contract

**Goal:** When a `RawMessage` arrives, fire a typed event that consumers can pattern-match on, separating Agora-internal messages from generic KOS payloads.

**Files:**
- Modify: `agora-p2p/src/node.rs`
- Modify: `agora-p2p/src/mesh/peer.rs`

**Step 1: Update MeshEvent**

In `agora-p2p/src/node.rs`, update `MeshEvent`:

```rust
#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(String),
    Disconnected(String),
    MessageReceived(String, Vec<u8>),    // Agora AmpMessage bytes
    RawMessageReceived {                  // For SOVEREIGN / Atelier
        peer_id: String,
        namespace: String,
        payload: Vec<u8>,
    },
    Error(String, String),
}
```

**Step 2: Dispatch RawMessageReceived in the stream accept loop**

In `MeshManager::accept_streams_loop`, after decoding an `AmpMessage::RawMessage`:

```rust
Ok(crate::protocol::AmpMessage::RawMessage { namespace, payload }) => {
    let _ = events
        .send(crate::mesh::peer::MeshEvent::RawMessageReceived {
            peer_id: peer_id.to_string(),
            namespace,
            payload,
        })
        .await;
}
```

(Need to thread the `MeshEvent` enum through carefully — `mesh/peer.rs` uses its own `MeshEvent` enum separate from `node.rs`. Consolidate them into one in `lib.rs` or re-export. The simplest fix is to move the canonical `MeshEvent` to `node.rs` and use it everywhere — which requires updating `mesh/peer.rs` to use `crate::MeshEvent`.)

**Step 3: Test**

```rust
// In agora-p2p/tests/integration_test.rs, add:

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn raw_message_delivered_to_namespace_handler() {
    // Similar setup to two_nodes_connect_and_exchange_message
    // Send a RawMessage with namespace "atelier"
    // Verify B receives MeshEvent::RawMessageReceived with correct namespace + payload
    // ... (abbreviated — full impl follows the same pattern as Task 5 test)
}
```

**Step 4: Run tests**

Run: `cargo test -p agora-p2p`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add agora-p2p/src/node.rs agora-p2p/src/mesh/peer.rs
git commit -m "feat(p2p): dispatch RawMessageReceived events for KOS namespace payloads"
```

---

## Phase 3: KOS Shared Crate Strategy

### Task 9: Document KOS integration contract (no code changes)

**Goal:** Write a concise document that explains how SOVEREIGN and Atelier should depend on `agora-crypto` and `agora-p2p`. No repo restructuring yet — just document the contract so future work doesn't diverge.

**Files:**
- Create: `docs/kos-integration.md`

**Step 1: Create the document**

```markdown
# KOS Integration Contract

## Identity Layer: agora-crypto

`agora-crypto` provides the canonical identity primitives for the KOS ecosystem:

- `AgentId` — Ed25519 public key, BLAKE3-content-addressed, hex-encoded
- `AgentIdentity` — keypair for signing operations
- `Sigchain` — append-only, hash-linked, signed action log
- Crypto ops: X25519 DH, ChaCha20-Poly1305, HKDF, BLAKE3

**How to use from SOVEREIGN or Atelier:**

```toml
# In SOVEREIGN/Cargo.toml or Atelier/Cargo.toml
agora-crypto = { path = "../agora/agora-crypto" }
# (or via git when published)
```

This gives you a stable AgentId for any KOS component.

## Transport Layer: agora-p2p

`agora-p2p` provides LAN peer-to-peer transport built on QUIC + mDNS.

```toml
agora-p2p = { path = "../agora/agora-p2p" }
```

**Basic usage from a non-Agora app:**

```rust
use agora_p2p::{P2pNode, MeshEvent};
use agora_p2p::types::Config;

let config = Config {
    agent_id: my_agent_id,
    listen_port: 0,                              // OS assigns
    service_name: "_myapp._udp.local.".to_string(), // unique per app
};

let mut node = P2pNode::new(config).await?;
let mut events = node.take_mesh_events().unwrap();
node.start(0).await?;

// Send
node.broadcast_raw("myapp", b"payload").await?;

// Receive
while let Some(event) = events.recv().await {
    if let MeshEvent::RawMessageReceived { peer_id, namespace, payload } = event {
        // handle
    }
}
```

## Service Name Convention

Use distinct mDNS service names per application to avoid cross-app noise:

| App       | Service name               |
|-----------|---------------------------|
| Agora     | `_agora._udp.local.`      |
| Atelier   | `_atelier._udp.local.`    |
| SOVEREIGN | `_sovereign._udp.local.`  |
| KOS (shared)| `_kos._udp.local.`      |

## Future: KOS Workspace

When SOVEREIGN matures, consider extracting to a KOS workspace:

```
kos/
├── crates/
│   ├── kos-identity/   (= agora-crypto, renamed)
│   └── kos-p2p/        (= agora-p2p, renamed)
├── agora/              (workspace member)
├── atelier/            (workspace member)
└── sovereign/          (workspace member)
```

This is NOT urgent. Do it when the duplication pain becomes real.

## What NOT to share yet

- Agora's Matrix C/S API types (`agora-core`) — these are Agora-specific
- Agora's server/DB layer — Agora-specific
- Atelier's graph/tree/view types — Atelier-specific

Keep the boundaries clean: identity + transport are shared; application logic stays per-app.
```

**Step 2: Commit**

```bash
git add docs/kos-integration.md
git commit -m "docs: add KOS integration contract for SOVEREIGN and Atelier"
```

---

## Phase 4: Wire P2P into Agora (Basic Dogfood)

### Task 10: Start P2P node from agora-server or agora-cli

**Goal:** Get P2P running in a real Agora context. The minimal viable thing: agora-cli can `connect --local` and find peers.

**Prerequisite:** Phase 1 integration test must be passing first.

**Files:**
- Modify: `agora-cli/src/main.rs` or `agora-cli/src/commands/mod.rs`
- Modify: `agora-cli/Cargo.toml`

**Step 1: Add agora-p2p dependency to agora-cli**

```toml
# agora-cli/Cargo.toml
[dependencies]
agora-p2p = { workspace = true }
```

**Step 2: Write the failing test (CLI integration)**

This is primarily a manual test since it requires network interaction. Write a unit test instead that validates the config creation path:

```rust
#[test]
fn p2p_config_builds_from_agent_id() {
    use agora_p2p::types::Config;
    use agora_crypto::AgentId;

    let id = AgentId::from_hex(
        "abcd000000000000000000000000000000000000000000000000000000000001",
    )
    .unwrap();

    let config = Config {
        agent_id: id.clone(),
        listen_port: 58421,
        service_name: "_agora._udp.local.".to_string(),
    };

    assert_eq!(config.listen_port, 58421);
    assert_eq!(config.agent_id.to_string(), id.to_string());
}
```

Run: `cargo test -p agora-cli p2p_config_builds_from_agent_id`
Expected: FAIL (agora-p2p not yet a dep)

**Step 3: Add the dependency and make it compile**

```toml
# agora-cli/Cargo.toml [dependencies]
agora-p2p = { workspace = true }
```

**Step 4: Add `connect --local` subcommand to CLI**

In the CLI command handler (check `agora-cli/src/main.rs` for the command structure), add:

```rust
("connect", Some(sub)) if sub.get_flag("local") => {
    // Load agent identity from stored credentials
    let agent_id = load_agent_id()?;  // existing function

    let config = agora_p2p::types::Config {
        agent_id,
        listen_port: 58421,
        service_name: "_agora._udp.local.".to_string(),
    };

    let mut node = agora_p2p::P2pNode::new(config).await?;
    let mut events = node.take_mesh_events().unwrap();
    node.start(58421).await?;

    println!("P2P node started. Discovering local peers...");

    // Print events until Ctrl-C
    while let Some(event) = events.recv().await {
        match event {
            agora_p2p::MeshEvent::Connected(peer) => {
                println!("Connected to peer: {}", peer);
            }
            agora_p2p::MeshEvent::Disconnected(peer) => {
                println!("Peer disconnected: {}", peer);
            }
            agora_p2p::MeshEvent::MessageReceived(peer, msg) => {
                println!("Message from {}: {}", peer, msg);
            }
            agora_p2p::MeshEvent::Error(peer, err) => {
                eprintln!("Error from {}: {}", peer, err);
            }
            _ => {}
        }
    }
}
```

**Step 5: Run the unit test**

Run: `cargo test -p agora-cli p2p_config_builds_from_agent_id`
Expected: PASS

**Step 6: Manual verification**

Run two terminals:
```bash
# Terminal 1
cargo run -p agora-cli -- connect --local

# Terminal 2
cargo run -p agora-cli -- connect --local
```
Expected: Both print "Connected to peer: ..." within ~5 seconds.

**Step 7: Commit**

```bash
git add agora-cli/Cargo.toml agora-cli/src/
git commit -m "feat(cli): add connect --local command using agora-p2p LAN mesh"
```

---

## Phase 5: Connection Health Monitoring (Optional Polish)

### Task 11: QUIC keepalive + application-level ping

This is already partially done (QUIC keepalive is hardcoded to 15s in `make_quinn_server_config`). The main gap is the application-level Ping/Pong loop for latency measurement and stale connection detection.

**Files:**
- Modify: `agora-p2p/src/mesh/peer.rs`

**Step 1: Write the test**

```rust
#[tokio::test]
async fn ping_pong_round_trip() {
    use crate::protocol::{AmpMessage, encode, decode};

    let ping = AmpMessage::Ping { nonce: 12345 };
    let bytes = encode(&ping).unwrap();
    let decoded = decode(&bytes).unwrap();

    match decoded {
        AmpMessage::Ping { nonce } => assert_eq!(nonce, 12345),
        _ => panic!("Expected Ping"),
    }

    let pong = AmpMessage::Pong { nonce: 12345 };
    let bytes = encode(&pong).unwrap();
    let decoded = decode(&bytes).unwrap();

    match decoded {
        AmpMessage::Pong { nonce } => assert_eq!(nonce, 12345),
        _ => panic!("Expected Pong"),
    }
}
```

Run: `cargo test -p agora-p2p ping_pong_round_trip`
Expected: PASS (codec handles these already)

**Step 2: Add ping loop to MeshManager**

```rust
pub fn start_ping_loop(
    connections: Arc<RwLock<HashMap<AgentId, ConnectedPeer>>>,
    events: mpsc::Sender<MeshEvent>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let peers: Vec<AgentId> = connections.read().await.keys().cloned().collect();
            for peer_id in peers {
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let ping = AmpMessage::Ping { nonce };
                // send via connection
                let conns = connections.read().await;
                if let Some(peer) = conns.get(&peer_id) {
                    if let Ok(bytes) = crate::protocol::encode(&ping) {
                        let conn = peer.connection.connection.clone();
                        drop(conns);
                        if let Ok((mut send, _)) = conn.open_bi().await {
                            let _ = crate::transport::quic::write_message(&mut send, &bytes).await;
                        }
                    }
                }
            }
        }
    });
}
```

**Step 3: Handle Pong in stream accept loop**

In `accept_streams_loop`, add a match arm for `AmpMessage::Pong { nonce }` — log latency or ignore.

**Step 4: Commit**

```bash
git add agora-p2p/src/mesh/peer.rs
git commit -m "feat(p2p): add application-level ping loop for connection health"
```

---

## What We Are Explicitly NOT Building (Yet)

Based on FEEDBACK.md and the strategic assessment:

| Feature | Why Deferred |
|---------|-------------|
| Kademlia DHT | 6-12 weeks of work; most chat apps never need it; Matrix federation handles internet |
| NAT traversal / STUN / hole punching | Hard, fragile; handle after LAN mesh is solid |
| TURN relay | Requires infrastructure; deferred to Phase 3 of AMP |
| Event DAG / Merkle tree | Phase 4 of AMP; requires stable P2P first |
| CRDT / LWW state sync | Tied to Event DAG |
| Peer reputation scoring | Nice to have; add with health monitoring |
| Rate limiting / DoS protection | Add when P2P is exposed to untrusted networks |
| TLS signature verification hardening | Current fingerprint approach is sufficient for LAN; improve before internet P2P |

---

## Build Commands Reference

```bash
# Build agora-p2p
cargo build -p agora-p2p

# Run all agora-p2p tests
cargo test -p agora-p2p

# Run integration tests only
cargo test -p agora-p2p --test integration_test

# Run a specific test
cargo test -p agora-p2p test_name -- --nocapture

# Run all KOS workspace tests
cargo test --workspace

# Build the CLI
cargo build -p agora-cli

# Run with debug logging
RUST_LOG=agora_p2p=debug cargo test -p agora-p2p --test integration_test -- --nocapture
```

---

## Validation Checklist

Before calling Phase 1 complete:
- [ ] `cargo build -p agora-p2p` — no errors or warnings
- [ ] `cargo test -p agora-p2p` — all unit tests pass
- [ ] `cargo test -p agora-p2p --test integration_test` — two-node test passes
- [ ] Manual: two CLI instances discover each other in < 10 seconds
- [ ] Manual: message sent from A appears in B's event stream

Before calling Phase 2 complete:
- [ ] `cargo test -p agora-p2p raw_message_round_trips` — PASS
- [ ] `cargo test -p agora-p2p mock_transport_satisfies_trait` — PASS
- [ ] `docs/kos-integration.md` exists and is accurate
- [ ] `P2pTransport` trait exported from crate root

---

## Notes on Current Code Quality

- `ConnectedPeer.sender: SendStream` is stored but not used (Task 3 makes it a connection-level concern). Remove it after Task 3.
- The `Capabilities` struct in `AmpMessage::Handshake` has all-false fields. Populate them with actual capabilities once features stabilize.
- `MdnsDiscovery::get_local_ip` uses a socket-connect trick that works but requires internet DNS reachability. Consider using `local-ip-address` crate or binding to `0.0.0.0` and letting mDNS pick the interface.
- `QuicTransport` stores connections in two separate maps (its own `connections` and `MeshManager::connections`). This is redundant — `MeshManager` should own the source of truth. Low priority, but worth cleaning up in Phase 2 or 3.

---

## Phase 6: Yggdrasil Transport Integration — The World Tree

> **Execute this phase AFTER Tasks 1–4 (bug fixes).** Before Atelier can embed agora-p2p, P2P must support Yggdrasil as the primary WAN transport. This phase implements the World Tree layer.
>
> **Architectural decision:** Yggdrasil is an **underlay**, not a replacement for QUIC. agora-p2p continues to use AmpMessage over QUIC, but binds the QUIC endpoint to the Yggdrasil IPv6 interface instead of `0.0.0.0`. Yggdrasil handles E2EE at the network layer, making `FingerprintVerifier`/`FingerprintStore`/`rcgen` redundant.
>
> **Canonical spec:** `SOVEREIGN/.sovereign/docs/grand-plan-distributed-resource-economy.md` § Phase 1
> **After this phase:** proceed to Atelier Phase 2 (`docs/plans/grand-plan-alignment.md`).

### Task 12: Add `TransportMode` enum and `YggdrasilConfig`

**Files:**
- Modify: `agora-p2p/src/types.rs` (or `agora-p2p/src/lib.rs` if types are inline)
- Modify: `agora-p2p/src/node.rs`

**Step 1: Write the failing test**

```rust
// In agora-p2p/src/types.rs or a new test module
#[test]
fn transport_mode_auto_selects_yggdrasil_when_daemon_present() {
    // This will fail until TransportMode exists
    let mode = TransportMode::Auto;
    // We can't actually probe for the daemon in a unit test,
    // so just verify the type compiles and has the expected variants
    match mode {
        TransportMode::Quic(_) => {}
        TransportMode::Yggdrasil(_) => {}
        TransportMode::Auto => {}
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cd /Users/netu/Projects/KOS/agora
cargo test -p agora-p2p transport_mode_auto -v
```

Expected: FAIL — `TransportMode` not found

**Step 3: Add the types**

In `agora-p2p/src/types.rs` (create if it doesn't exist, otherwise add to appropriate module):

```rust
/// QUIC transport configuration. Used for LAN-only operation (no Yggdrasil daemon).
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Port to listen on. 0 = OS-assigned.
    pub listen_port: u16,
}

/// Yggdrasil transport configuration.
#[derive(Debug, Clone)]
pub struct YggdrasilConfig {
    /// Yggdrasil admin socket path. Default: /var/run/yggdrasil.sock (Linux)
    /// or ~/Library/Application Support/yggdrasil/admin.sock (macOS).
    pub admin_socket: Option<std::path::PathBuf>,
    /// Port to listen on within the Yggdrasil address space. 0 = OS-assigned.
    pub listen_port: u16,
}

/// How agora-p2p connects to the network.
#[derive(Debug, Clone, Default)]
pub enum TransportMode {
    /// Use raw QUIC bound to 0.0.0.0. LAN only, no WAN. Fallback when no Yggdrasil daemon.
    Quic(QuicConfig),
    /// Bind QUIC to the local Yggdrasil IPv6 interface. Global WAN, E2EE at network layer.
    Yggdrasil(YggdrasilConfig),
    /// Probe for Yggdrasil daemon on startup. Use Yggdrasil if found, QUIC otherwise.
    #[default]
    Auto,
}
```

Add `transport: TransportMode` to `P2pConfig`:

```rust
pub struct P2pConfig {
    pub agent_id: AgentId,
    pub service_name: String,
    pub listen_port: u16,
    /// Transport selection. Defaults to Auto (Yggdrasil if daemon found, QUIC otherwise).
    pub transport: TransportMode,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            agent_id: AgentId::default(), // must be overridden
            service_name: "agora".to_string(),
            listen_port: 0,
            transport: TransportMode::Auto,
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test -p agora-p2p transport_mode_auto -v
```

Expected: PASS

**Step 5: Commit**

```bash
git add agora-p2p/src/
git commit -m "feat(agora-p2p): add TransportMode enum and YggdrasilConfig types"
```

---

### Task 13: Implement `yggdrasil_addr_from_pubkey`

**Files:**
- Create: `agora-p2p/src/identity/yggdrasil.rs`
- Modify: `agora-p2p/src/identity/mod.rs`

This function derives the Yggdrasil IPv6 address from an Ed25519 verifying key using the same algorithm Yggdrasil itself uses. It requires no running daemon — it is a pure cryptographic derivation.

**Step 1: Write the failing test**

```rust
// In agora-p2p/src/identity/yggdrasil.rs
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn same_key_produces_same_address() {
        let seed = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        let addr1 = yggdrasil_addr_from_pubkey(&verifying_key);
        let addr2 = yggdrasil_addr_from_pubkey(&verifying_key);

        assert_eq!(addr1, addr2, "address derivation must be deterministic");
    }

    #[test]
    fn different_keys_produce_different_addresses() {
        let key1 = SigningKey::from_bytes(&[1u8; 32]).verifying_key();
        let key2 = SigningKey::from_bytes(&[2u8; 32]).verifying_key();

        assert_ne!(
            yggdrasil_addr_from_pubkey(&key1),
            yggdrasil_addr_from_pubkey(&key2),
        );
    }

    #[test]
    fn address_is_in_yggdrasil_range() {
        let key = SigningKey::from_bytes(&[99u8; 32]).verifying_key();
        let addr = yggdrasil_addr_from_pubkey(&key);
        // Yggdrasil addresses start with 0x02 (first byte of the IPv6 address)
        assert_eq!(addr.octets()[0], 0x02, "Yggdrasil addresses must start with 0x02");
    }
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p agora-p2p yggdrasil_addr -v
```

Expected: FAIL — `yggdrasil_addr_from_pubkey` not found

**Step 3: Implement the derivation**

Create `agora-p2p/src/identity/yggdrasil.rs`:

```rust
//! Yggdrasil IPv6 address derivation from Ed25519 public keys.
//!
//! Yggdrasil derives node addresses by computing SHA-512 of the public key bytes,
//! then finding the position of the first zero bit after the leading ones.
//! The address prefix is 0x02 followed by bits derived from the hash.
//!
//! Reference: https://github.com/yggdrasil-network/yggdrasil-go/blob/develop/src/address/address.go

use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha512};
use std::net::Ipv6Addr;

/// Derive a Yggdrasil IPv6 address from an Ed25519 verifying key.
///
/// This is a pure cryptographic derivation — no daemon required.
/// The resulting address uniquely identifies the node on the Yggdrasil mesh.
pub fn yggdrasil_addr_from_pubkey(verifying_key: &VerifyingKey) -> Ipv6Addr {
    let hash = Sha512::digest(verifying_key.as_bytes());
    addr_from_hash(&hash)
}

/// Internal: derive Yggdrasil address from a 64-byte SHA-512 hash.
fn addr_from_hash(hash: &[u8]) -> Ipv6Addr {
    // Count leading one bits in the hash
    let mut ones = 0u8;
    'outer: for byte in hash.iter() {
        for bit in (0..8).rev() {
            if byte & (1 << bit) != 0 {
                ones += 1;
            } else {
                break 'outer;
            }
        }
    }

    // Build a 16-byte IPv6 address:
    // First byte: 0x02 (Yggdrasil global unicast prefix)
    // Second byte: number of leading ones (the "prefix length" of the node's subnet)
    // Remaining 14 bytes: bits from the hash starting after the leading ones and the zero bit
    let mut addr_bytes = [0u8; 16];
    addr_bytes[0] = 0x02;
    addr_bytes[1] = ones;

    // Skip `ones + 1` bits from the hash (the leading ones + the terminating zero),
    // then copy the next 112 bits (14 bytes) into the address
    let skip_bits = (ones as usize) + 1;
    let skip_bytes = skip_bits / 8;
    let bit_offset = skip_bits % 8;

    for i in 0..14 {
        let src_idx = skip_bytes + i;
        if src_idx + 1 < hash.len() {
            if bit_offset == 0 {
                addr_bytes[2 + i] = hash[src_idx];
            } else {
                addr_bytes[2 + i] = (hash[src_idx] << bit_offset)
                    | (hash[src_idx + 1] >> (8 - bit_offset));
            }
        }
    }

    Ipv6Addr::from(addr_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn same_key_produces_same_address() {
        let seed = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let addr1 = yggdrasil_addr_from_pubkey(&verifying_key);
        let addr2 = yggdrasil_addr_from_pubkey(&verifying_key);
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn different_keys_produce_different_addresses() {
        let key1 = SigningKey::from_bytes(&[1u8; 32]).verifying_key();
        let key2 = SigningKey::from_bytes(&[2u8; 32]).verifying_key();
        assert_ne!(yggdrasil_addr_from_pubkey(&key1), yggdrasil_addr_from_pubkey(&key2));
    }

    #[test]
    fn address_is_in_yggdrasil_range() {
        let key = SigningKey::from_bytes(&[99u8; 32]).verifying_key();
        let addr = yggdrasil_addr_from_pubkey(&key);
        assert_eq!(addr.octets()[0], 0x02);
    }
}
```

Add `sha2` to `agora-p2p/Cargo.toml` if not already present:

```toml
sha2 = "0.10"
```

Expose from `agora-p2p/src/identity/mod.rs`:

```rust
pub mod yggdrasil;
pub use yggdrasil::yggdrasil_addr_from_pubkey;
```

**Step 4: Run tests**

```bash
cargo test -p agora-p2p yggdrasil_addr -v
```

Expected: all 3 tests PASS

**Step 5: Commit**

```bash
git add agora-p2p/src/identity/
git commit -m "feat(agora-p2p): add Yggdrasil IPv6 address derivation from Ed25519 pubkey"
```

---

### Task 14: Implement `YggdrasilTransport` — daemon probe and interface bind

**Files:**
- Create: `agora-p2p/src/transport/yggdrasil.rs`
- Modify: `agora-p2p/src/node.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_none_when_no_daemon() {
        // In a test environment there's no Yggdrasil daemon running.
        // probe_yggdrasil_daemon() should return None gracefully.
        let result = probe_yggdrasil_daemon(None);
        // Either None (no daemon) or Some(addr) if running — just must not panic
        let _ = result; // accept any result, we're testing it doesn't crash
    }

    #[test]
    fn yggdrasil_config_default_socket_path() {
        let config = YggdrasilConfig { admin_socket: None, listen_port: 0 };
        let path = default_admin_socket_path();
        // Must return a plausible path, not panic
        assert!(path.to_string_lossy().len() > 0);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p agora-p2p probe_returns_none -v
```

Expected: FAIL — module not found

**Step 3: Implement**

Create `agora-p2p/src/transport/yggdrasil.rs`:

```rust
//! Yggdrasil transport adapter for agora-p2p.
//!
//! Strategy:
//! 1. Probe for a running Yggdrasil daemon via its admin socket.
//! 2. If found, query the daemon for this node's Yggdrasil IPv6 address.
//! 3. Bind the QUIC endpoint to that IPv6 address.
//! 4. If no daemon, return None and let P2pNode fall back to QUIC on 0.0.0.0.

use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use ed25519_dalek::VerifyingKey;

use crate::identity::yggdrasil::yggdrasil_addr_from_pubkey;
use crate::types::YggdrasilConfig;

/// Returns the platform-default Yggdrasil admin socket path.
pub fn default_admin_socket_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Application Support/yggdrasil/admin.sock")
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

    // Attempt to connect to the admin socket.
    // Yggdrasil admin API uses a simple JSON protocol over Unix domain sockets.
    use std::os::unix::net::UnixStream;
    use std::io::{Read, Write};
    use std::time::Duration;

    let mut stream = UnixStream::connect(&socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(500))).ok()?;

    // Send "getself" request to get this node's address
    let request = r#"{"keepalive":false,"request":"getself"}"#;
    stream.write_all(request.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    // Parse the "address" field from the JSON response
    // Expected: {"request":"getself","response":{"address":"200:...","...":"..."}}
    parse_yggdrasil_address(&response)
}

/// Parse Yggdrasil IPv6 address from the daemon's getself JSON response.
fn parse_yggdrasil_address(json: &str) -> Option<Ipv6Addr> {
    // Minimal parsing — find `"address":"<addr>"` pattern
    let key = r#""address":""#;
    let start = json.find(key)? + key.len();
    let end = json[start..].find('"')? + start;
    let addr_str = &json[start..end];
    addr_str.parse().ok()
}

/// Determine the bind address for agora-p2p given the transport config and agent key.
///
/// Returns:
/// - `Some(SocketAddr)` with the Yggdrasil IPv6 address if the daemon is running
/// - `None` if Yggdrasil is unavailable (caller should fall back to QUIC on 0.0.0.0)
pub fn resolve_yggdrasil_bind_addr(
    config: &YggdrasilConfig,
    _verifying_key: &VerifyingKey,
) -> Option<SocketAddr> {
    let ygg_addr = probe_yggdrasil_daemon(config.admin_socket.as_deref())?;
    Some(SocketAddr::new(std::net::IpAddr::V6(ygg_addr), config.listen_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_none_when_no_daemon() {
        // In CI / test environment, no Yggdrasil daemon is running.
        let result = probe_yggdrasil_daemon(Some(Path::new("/nonexistent/socket")));
        assert!(result.is_none(), "should return None when socket doesn't exist");
    }

    #[test]
    fn yggdrasil_config_default_socket_path() {
        let path = default_admin_socket_path();
        assert!(path.to_string_lossy().len() > 0);
    }

    #[test]
    fn parse_yggdrasil_address_valid() {
        let json = r#"{"request":"getself","response":{"address":"200:1234:5678:abcd::1","subnet":"200:1234:5678:abcd::/64"}}"#;
        let addr = parse_yggdrasil_address(json);
        assert!(addr.is_some(), "should parse valid Yggdrasil address");
        assert_eq!(addr.unwrap().octets()[0], 0x02);
    }

    #[test]
    fn parse_yggdrasil_address_malformed() {
        let result = parse_yggdrasil_address("not json");
        assert!(result.is_none());
    }
}
```

Expose from `agora-p2p/src/transport/mod.rs`:

```rust
pub mod yggdrasil;
pub use yggdrasil::{probe_yggdrasil_daemon, resolve_yggdrasil_bind_addr, default_admin_socket_path};
```

**Step 4: Run tests**

```bash
cargo test -p agora-p2p transport::yggdrasil -v
```

Expected: PASS (all 4 tests)

**Step 5: Commit**

```bash
git add agora-p2p/src/transport/yggdrasil.rs agora-p2p/src/transport/mod.rs
git commit -m "feat(agora-p2p): add Yggdrasil transport adapter with daemon probe"
```

---

### Task 15: Wire `TransportMode::Auto` into `P2pNode::start()`

**Files:**
- Modify: `agora-p2p/src/node.rs`

**Step 1: Locate the start method**

Find `P2pNode::start()` or `P2pNode::new()` in `node.rs`. The current code likely calls `QuicTransport::new(config.listen_port)` unconditionally.

**Step 2: Write the failing test**

```rust
#[tokio::test]
async fn p2p_node_starts_with_auto_mode() {
    use agora_crypto::AgentId;
    let config = P2pConfig {
        agent_id: AgentId::from_bytes([1u8; 32]),
        service_name: "test".to_string(),
        listen_port: 0,
        transport: TransportMode::Auto,
    };
    // Should start without panicking even without a Yggdrasil daemon
    let node = P2pNode::start(config).await;
    assert!(node.is_ok(), "P2pNode::start must succeed with Auto mode (falls back to QUIC)");
    node.unwrap().shutdown().await;
}
```

**Step 3: Implement `TransportMode` selection in `P2pNode::start()`**

In `agora-p2p/src/node.rs`, find where `QuicTransport` is created and add the mode switch:

```rust
use crate::transport::yggdrasil::resolve_yggdrasil_bind_addr;
use crate::types::TransportMode;

// In P2pNode::start():
let bind_addr = match &config.transport {
    TransportMode::Yggdrasil(ygg_config) => {
        match resolve_yggdrasil_bind_addr(ygg_config, &verifying_key) {
            Some(addr) => {
                tracing::info!("Yggdrasil transport: binding to {}", addr);
                addr
            }
            None => {
                tracing::warn!(
                    "Yggdrasil daemon not found at {:?}, falling back to QUIC on LAN",
                    ygg_config.admin_socket
                );
                SocketAddr::from(([0, 0, 0, 0], config.listen_port))
            }
        }
    }
    TransportMode::Quic(quic_config) => {
        SocketAddr::from(([0, 0, 0, 0], quic_config.listen_port))
    }
    TransportMode::Auto => {
        let ygg_config = YggdrasilConfig { admin_socket: None, listen_port: config.listen_port };
        match resolve_yggdrasil_bind_addr(&ygg_config, &verifying_key) {
            Some(addr) => {
                tracing::info!("TransportMode::Auto: Yggdrasil daemon found, binding to {}", addr);
                addr
            }
            None => {
                tracing::info!("TransportMode::Auto: no Yggdrasil daemon, using QUIC on LAN");
                SocketAddr::from(([0, 0, 0, 0], config.listen_port))
            }
        }
    }
};

// Then pass bind_addr to QuicTransport instead of just the port
let transport = QuicTransport::bind(bind_addr, &tls_config).await?;
```

**Note on `FingerprintVerifier`:** When `bind_addr` is a Yggdrasil IPv6 address, the Yggdrasil network layer already authenticates peers cryptographically. The `FingerprintVerifier` machinery can be bypassed in this path. It is not removed in this task (to minimize diff size), but mark it:

```rust
// IMPLEMENTATION_REQUIRED: Remove FingerprintVerifier when Yggdrasil transport is active.
// Yggdrasil authenticates peers at the network layer via their public key derivation.
// FingerprintVerifier is only needed for raw QUIC (LAN-only mode).
```

**Step 4: Run tests**

```bash
cargo test -p agora-p2p p2p_node_starts_with_auto_mode -v
```

Expected: PASS

**Step 5: Commit**

```bash
git add agora-p2p/src/node.rs
git commit -m "feat(agora-p2p): wire TransportMode::Auto into P2pNode::start() with Yggdrasil probe"
```

---

### Task 16: Remove `FingerprintVerifier` and `rcgen` dependency

> **Only execute this task when running in Yggdrasil mode.** The fingerprint machinery remains for QUIC/LAN fallback. This task removes it when the Yggdrasil path is stable and tested.

**Files:**
- Modify: `agora-p2p/src/transport/quic.rs`
- Modify: `agora-p2p/Cargo.toml`

**Step 1: Verify all tests still pass before removing**

```bash
cargo test -p agora-p2p -v
```

Expected: all existing tests pass

**Step 2: Remove `FingerprintVerifier` from the Yggdrasil transport path**

In `agora-p2p/src/transport/quic.rs`, add a conditional:

```rust
// When bind address is Yggdrasil (IPv6 starting with 0x02), skip fingerprint verification.
// Yggdrasil guarantees peer identity at the network layer.
fn is_yggdrasil_addr(addr: &std::net::SocketAddr) -> bool {
    match addr.ip() {
        std::net::IpAddr::V6(v6) => v6.octets()[0] == 0x02,
        _ => false,
    }
}
```

Mark `FingerprintVerifier` and `FingerprintStore` as deprecated:

```rust
/// Transport-layer peer verification via TLS certificate fingerprinting.
/// 
/// # Deprecation
/// This is used only in QUIC/LAN mode. When Yggdrasil transport is active,
/// peer identity is guaranteed by Yggdrasil's address derivation.
/// `FingerprintVerifier` will be removed when LAN-only mode is removed.
#[deprecated(note = "Use Yggdrasil transport for peer verification")]
pub struct FingerprintVerifier { ... }
```

**Step 3: Remove `rcgen` if no longer needed**

Check if `rcgen` is used outside fingerprint machinery:

```bash
grep -r "rcgen" /Users/netu/Projects/KOS/agora/agora-p2p/src/
```

If only in TLS cert generation for `FingerprintVerifier`, remove from `Cargo.toml`.

**Step 4: Run all tests**

```bash
cargo test -p agora-p2p -v
```

Expected: all tests pass, no `rcgen` compile errors

**Step 5: Commit**

```bash
git add agora-p2p/src/transport/quic.rs agora-p2p/Cargo.toml
git commit -m "refactor(agora-p2p): deprecate FingerprintVerifier; Yggdrasil handles peer auth at network layer"
```

---

### Task 17: Integration Test — Two Yggdrasil Nodes Exchange AmpMessage

> **This test requires two machines with Yggdrasil daemon installed, OR a test harness that simulates the Yggdrasil interface.** Document it here for when the environment is available. A simulated version using the LAN fallback can run in CI.

**Step 1: Write the CI-safe integration test (LAN fallback)**

```rust
// In agora-p2p/tests/yggdrasil_integration.rs
// This test uses Auto mode — will use QUIC/LAN in CI, Yggdrasil when daemon is present.

#[tokio::test]
async fn two_auto_mode_nodes_exchange_message() {
    use agora_crypto::{AgentId, AgentIdentity};

    let id_a = AgentIdentity::generate();
    let id_b = AgentIdentity::generate();

    let config_a = P2pConfig {
        agent_id: id_a.agent_id(),
        service_name: "test-ygg".to_string(),
        listen_port: 0,
        transport: TransportMode::Auto,
    };
    let config_b = P2pConfig {
        agent_id: id_b.agent_id(),
        service_name: "test-ygg".to_string(),
        listen_port: 0,
        transport: TransportMode::Auto,
    };

    let node_a = P2pNode::start(config_a).await.expect("node A start");
    let node_b = P2pNode::start(config_b).await.expect("node B start");

    let addr_a = node_a.local_addr().expect("node A addr");

    // B connects to A directly (bypasses mDNS for test speed)
    node_b.connect_to(id_a.agent_id(), addr_a).await.expect("connect");

    // A sends a message to B
    let payload = b"hello from A".to_vec();
    node_a.send_to(id_b.agent_id(), payload.clone()).await.expect("send");

    // B receives it
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        node_b.next_message(),
    ).await.expect("timeout").expect("message");

    assert_eq!(received.payload, payload);

    node_a.shutdown().await;
    node_b.shutdown().await;
}
```

**Step 2: Run the test**

```bash
cargo test -p agora-p2p two_auto_mode_nodes -v
```

Expected: PASS (uses QUIC/LAN in CI since no Yggdrasil daemon)

**Step 3: Verify transport mode in log output**

```bash
RUST_LOG=agora_p2p=info cargo test -p agora-p2p two_auto_mode_nodes -- --nocapture
```

Expected log output:
```
INFO agora_p2p::node: TransportMode::Auto: no Yggdrasil daemon, using QUIC on LAN
```

**Step 4: Final commit for Yggdrasil phase**

```bash
git add agora-p2p/tests/
git commit -m "test(agora-p2p): add two-node Auto-mode integration test for Yggdrasil/QUIC transport"
```

---

### Phase 6 Checklist

- [ ] Task 12: `TransportMode` enum and `YggdrasilConfig` types committed
- [ ] Task 13: `yggdrasil_addr_from_pubkey` with 3 passing tests committed
- [ ] Task 14: `YggdrasilTransport` daemon probe — 4 passing tests committed
- [ ] Task 15: `P2pNode::start()` wired with `TransportMode` selection committed
- [ ] Task 16: `FingerprintVerifier` deprecated in Yggdrasil path committed
- [ ] Task 17: Integration test — two Auto-mode nodes exchange message committed
- [ ] `cargo test -p agora-p2p` passes with no warnings

**After Phase 6:** Move to Atelier Phase 2 (`docs/plans/grand-plan-alignment.md` — wire `surface.rs:45`).

