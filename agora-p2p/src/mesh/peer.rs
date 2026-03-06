use std::collections::BTreeMap;
use std::sync::Arc;

use agora_crypto::AgentId;
use quinn::{RecvStream, SendStream};
use tokio::sync::{mpsc, RwLock};

use crate::error::Error;
use crate::types::Peer;
use crate::transport::quic::{read_message, QuicConnection, QuicTransport};
use crate::protocol::{decode, AmpMessage};

pub struct ConnectedPeer {
    pub peer: Peer,
    pub sender: SendStream,
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
        }
    }

    pub fn local_id(&self) -> &AgentId {
        &self.local_id
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

        match connection.connection.open_bi().await {
            Ok((mut send, recv)) => {
                let handshake = AmpMessage::Handshake {
                    agent_id: local_id.to_string(),
                    version: 1,
                    capabilities: Default::default(),
                };

                if let Ok(bytes) = crate::protocol::encode(&handshake) {
                    if let Err(e) = crate::transport::quic::write_message(&mut send, &bytes).await {
                        let _ = events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
                        return;
                    }
                }

                let connected_peer = ConnectedPeer {
                    peer: peer.clone(),
                    sender: send,
                    connection: connection.clone(),
                };

                connections.write().await.insert(peer_id.clone(), connected_peer);

                let _ = events.send(MeshEvent::Connected(peer_id.clone())).await;

                tokio::spawn(async move {
                    Self::accept_streams_loop(peer_id, connection.connection, events).await;
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
    ) {
        loop {
            match connection.accept_bi().await {
                Ok((_send, recv)) => {
                    let peer_id = peer_id.clone();
                    let events = events.clone();
                    tokio::spawn(async move {
                        let mut recv = recv;
                        match read_message(&mut recv).await {
                            Ok(bytes) => match decode(&bytes) {
                                Ok(message) => {
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

        match connection.connection.accept_bi().await {
            Ok((mut send, mut recv)) => {
                let handshake_bytes = match read_message(&mut recv).await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("Failed to read handshake from incoming: {}", e);
                        return;
                    }
                };

                let peer_agent_id = match decode(&handshake_bytes) {
                    Ok(AmpMessage::Handshake { agent_id, .. }) => {
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

                let our_handshake = AmpMessage::Handshake {
                    agent_id: self.local_id.to_string(),
                    version: 1,
                    capabilities: Default::default(),
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

                let peer = crate::types::Peer {
                    agent_id: peer_agent_id.clone(),
                    addresses: vec![connection.remote_addr.to_string()],
                };

                let mut connection = connection;
                connection.peer_id = peer_agent_id.clone();

                let connected_peer = ConnectedPeer {
                    peer,
                    sender: send,
                    connection: connection.clone(),
                };

                connections.write().await.insert(peer_agent_id.clone(), connected_peer);

                tracing::info!("Incoming connection established with peer {} at {}", peer_agent_id, remote_addr);
                
                let _ = events.send(MeshEvent::Connected(peer_agent_id.clone())).await;

                tokio::spawn(async move {
                    Self::accept_streams_loop(peer_agent_id, connection.connection, events).await;
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

    pub async fn is_connected(&self, peer_id: &AgentId) -> bool {
        self.connections.read().await.contains_key(peer_id)
    }

    pub async fn disconnect(&self, peer_id: &AgentId) {
        let was_connected = self.connections.write().await.remove(peer_id).is_some();
        if was_connected {
            let _ = self.events.send(MeshEvent::Disconnected(peer_id.clone())).await;
        }
    }
}
