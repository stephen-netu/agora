use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sovereign_sdk::TrustState;
use sovereign_sdk::AgentId;
use quinn;
use tokio::sync::{mpsc, RwLock};

use crate::error::Error;
use crate::types::Peer;
use crate::transport::quic::{read_message, QuicConnection, QuicTransport};
use crate::protocol::{decode, AmpMessage, Capabilities};
use super::replay::ReplayProtection;

pub struct ConnectedPeer {
    pub connection: QuicConnection,
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(AgentId),
    Disconnected(AgentId),
    MessageReceived(AgentId, AmpMessage),
    Error(AgentId, String),
}

pub struct MeshManager {
    local_id: AgentId,
    transport: Arc<QuicTransport>,
    connections: Arc<RwLock<BTreeMap<AgentId, ConnectedPeer>>>,
    pending: Arc<RwLock<BTreeMap<AgentId, bool>>>,
    events: mpsc::Sender<MeshEvent>,
    /// Sequence number generator for outgoing messages
    sequence_counter: AtomicU64,
    /// Replay protection for incoming messages
    replay_protection: ReplayProtection,
    /// Trust registry for peer trust state
    trust_registry: Arc<RwLock<BTreeMap<AgentId, TrustState>>>,
}

impl MeshManager {
    pub fn new(
        local_id: AgentId,
        transport: Arc<QuicTransport>,
        events: mpsc::Sender<MeshEvent>,
    ) -> Self {
        Self {
            local_id,
            transport,
            connections: Arc::new(RwLock::new(BTreeMap::new())),
            pending: Arc::new(RwLock::new(BTreeMap::new())),
            events,
            sequence_counter: AtomicU64::new(0),
            replay_protection: ReplayProtection::new(),
            trust_registry: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Generate the next sequence number for outgoing messages.
    fn next_sequence(&self) -> u64 {
        self.sequence_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn should_initiate(&self, peer_id: &AgentId) -> bool {
        self.local_id.to_string() < peer_id.to_string()
    }

    pub async fn try_connect(&self, peer: &Peer) -> Result<(), Error> {
        let peer_id = &peer.agent_id;

        if !self.should_initiate(peer_id).await {
            return Ok(());
        }

        if self.connections.read().await.contains_key(peer_id) {
            return Ok(());
        }

        if self.pending.read().await.contains_key(peer_id) {
            return Ok(());
        }

        self.pending.write().await.insert(peer_id.clone(), true);

        if let Some(addr) = peer.addresses.first() {
            if let Ok(socket_addr) = addr.parse() {
                match self.transport.connect(socket_addr, peer_id, None).await {
                    Ok(conn) => {
                        self.handle_new_connection(peer.clone(), conn).await;
                    }
                    Err(e) => {
                        self.pending.write().await.remove(peer_id);
                        return Err(Error::Mesh(e.to_string()));
                    }
                }
            }
        }

        self.pending.write().await.remove(peer_id);

        Ok(())
    }

    async fn handle_new_connection(&self, peer: Peer, connection: QuicConnection) {
        let peer_id = peer.agent_id.clone();
        let events = self.events.clone();
        let connections = self.connections.clone();
        let local_id = self.local_id.clone();
        let sequence = self.next_sequence();

        match connection.connection.open_bi().await {
            Ok((mut send, _recv)) => {
                let handshake = AmpMessage::Handshake {
                    agent_id: local_id.to_string(),
                    version: 1,
                    capabilities: Capabilities {
                        events: true,
                        relay: true,
                        state_sync: false,
                        collaboration: true,
                        fuel: true,
                        dispute: true,
                    },
                    sequence,
                };

                if let Ok(bytes) = crate::protocol::encode(&handshake) {
                    if let Err(e) = crate::transport::quic::write_message(&mut send, &bytes).await {
                        let _ = events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
                        return;
                    }
                }

                let connected_peer = ConnectedPeer {
                    connection: connection.clone(),
                };

                connections.write().await.insert(peer_id.clone(), connected_peer);

                let _ = events.send(MeshEvent::Connected(peer_id.clone())).await;

                let trust_registry = self.trust_registry.clone();
                tokio::spawn(async move {
                    Self::accept_streams_loop(peer_id, connection.connection, events, trust_registry).await;
                });
            }
            Err(e) => {
                let _ = events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
            }
        }
    }

    async fn accept_streams_loop(
        peer_id: AgentId,
        connection: quinn::Connection,
        events: mpsc::Sender<MeshEvent>,
        trust_registry: Arc<RwLock<BTreeMap<AgentId, TrustState>>>,
    ) {
        loop {
            match connection.accept_bi().await {
                Ok((_send, recv)) => {
                    let peer_id = peer_id.clone();
                    let events = events.clone();
                    let trust_registry = trust_registry.clone();
                    tokio::spawn(async move {
                        let mut recv = recv;
                        match read_message(&mut recv).await {
                            Ok(bytes) => match decode(&bytes) {
                                Ok(message) => {
                                    // Trust gate: reject FuelClaim from untrusted peers
                                    if let AmpMessage::FuelClaim { .. } = &message {
                                        let trust_state = trust_registry
                                            .read()
                                            .await
                                            .get(&peer_id)
                                            .cloned()
                                            .unwrap_or(TrustState::Untrusted);
                                        
                                        if trust_state != TrustState::Trusted {
                                            tracing::debug!("Rejected FuelClaim from untrusted peer: {}", peer_id);
                                            let _ = events.send(MeshEvent::Error(
                                                peer_id.clone(), 
                                                "rejected FuelClaim: peer not trusted".to_string()
                                            )).await;
                                            return;
                                        }
                                    }
                                    let _ = events.send(MeshEvent::MessageReceived(peer_id, message)).await;
                                }
                                Err(e) => {
                                    let _ = events.send(MeshEvent::Error(peer_id, format!("decode error: {}", e))).await;
                                }
                            },
                            Err(e) => {
                                tracing::debug!("Stream closed before message: {}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::debug!("Stream accept loop ended for {}: {}", peer_id, e);
                    let _ = events.send(MeshEvent::Disconnected(peer_id)).await;
                    break;
                }
            }
        }
    }

    pub async fn handle_incoming(&self, connection: QuicConnection, cert_peer_id: Option<AgentId>) {
        let remote_addr = connection.remote_addr;
        tracing::debug!("Handling incoming connection from {}", remote_addr);
        
        let events = self.events.clone();
        let connections = self.connections.clone();
        let replay_protection = self.replay_protection.clone();

        match connection.connection.accept_bi().await {
            Ok((mut send, mut recv)) => {
                let handshake_bytes = match read_message(&mut recv).await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("Failed to read handshake from incoming: {}", e);
                        return;
                    }
                };

                let (peer_agent_id, handshake_sequence) = match decode(&handshake_bytes) {
                    Ok(AmpMessage::Handshake { agent_id, sequence, .. }) => {
                        match sovereign_sdk::AgentId::from_hex(&agent_id) {
                            Ok(id) => (id, sequence),
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

                // Validate sequence number for replay protection
                if let Err(()) = replay_protection.validate_and_mark(&peer_agent_id, handshake_sequence).await {
                    tracing::warn!(
                        "Rejected connection from {}: sequence {} already used (replay attack?)",
                        peer_agent_id, handshake_sequence
                    );
                    return;
                }

                if let Some(ref cert_peer_id) = cert_peer_id {
                    if &peer_agent_id != cert_peer_id {
                        tracing::warn!(
                            "Identity mismatch: handshake peer_id {} != certificate peer_id {}",
                            peer_agent_id, cert_peer_id
                        );
                        return;
                    }
                    tracing::debug!("Identity verified: peer_id matches certificate");
                }

                let our_sequence = self.next_sequence();
                let our_handshake = AmpMessage::Handshake {
                    agent_id: self.local_id.to_string(),
                    version: 1,
                    capabilities: Capabilities {
                        events: true,
                        relay: true,
                        state_sync: false,
                        collaboration: true,
                        fuel: true,
                        dispute: true,
                    },
                    sequence: our_sequence,
                };
                if let Ok(bytes) = crate::protocol::encode(&our_handshake) {
                    if let Err(e) = crate::transport::quic::write_message(&mut send, &bytes).await {
                        tracing::warn!("Failed to send handshake ack: {}", e);
                    }
                }

                if connections.read().await.contains_key(&peer_agent_id) {
                    tracing::debug!("Already connected to {}, dropping duplicate", peer_agent_id);
                    return;
                }

                let mut connection = connection;
                connection.peer_id = peer_agent_id.clone();

                let connected_peer = ConnectedPeer {
                    connection: connection.clone(),
                };

                connections.write().await.insert(peer_agent_id.clone(), connected_peer);

                tracing::info!("Incoming connection established with peer {} at {}", peer_agent_id, remote_addr);
                
                let _ = events.send(MeshEvent::Connected(peer_agent_id.clone())).await;

                let trust_registry = self.trust_registry.clone();
                tokio::spawn(async move {
                    Self::accept_streams_loop(peer_agent_id, connection.connection, events, trust_registry).await;
                });
            }
            Err(e) => {
                tracing::warn!("Failed to accept_bi on incoming connection: {}", e);
            }
        }
    }

    pub async fn send_to(&self, peer_id: &AgentId, message: AmpMessage) -> Result<(), Error> {
        if !self.is_connected(peer_id).await {
            return Err(Error::Mesh("peer not connected".to_string()));
        }

        let connections = self.connections.read().await;
        let connected_peer = connections.get(peer_id)
            .ok_or_else(|| Error::Mesh("peer not found".to_string()))?;

        let (mut stream, _) = connected_peer.connection.connection.open_bi().await
            .map_err(|e| Error::Mesh(e.to_string()))?;

        let bytes = crate::protocol::encode(&message)
            .map_err(|e| Error::Protocol(e.to_string()))?;

        crate::transport::quic::write_message(&mut stream, &bytes).await
            .map_err(|e| Error::Mesh(e.to_string()))
    }

    pub async fn connected_peers(&self) -> Vec<AgentId> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn peer_addr(&self, peer_id: &str) -> Option<SocketAddr> {
        let connections = self.connections.read().await;
        let agent_id = AgentId::from_hex(peer_id).ok()?;
        connections.get(&agent_id).map(|p| p.connection.remote_addr)
    }

    pub async fn is_connected(&self, peer_id: &AgentId) -> bool {
        self.connections.read().await.contains_key(peer_id)
    }

    pub async fn disconnect(&self, peer_id: &AgentId) {
        let was_connected = self.connections.write().await.remove(peer_id).is_some();
        if was_connected {
            // Clean up replay protection state for this peer
            self.replay_protection.remove_peer(peer_id).await;
            let _ = self.events.send(MeshEvent::Disconnected(peer_id.clone())).await;
        }
    }
}
