use std::collections::HashMap;
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
    MessageReceived(AgentId, String),
    Error(AgentId, String),
}

pub struct MeshManager {
    local_id: AgentId,
    transport: Arc<QuicTransport>,
    connections: Arc<RwLock<HashMap<AgentId, ConnectedPeer>>>,
    pending: Arc<RwLock<HashMap<AgentId, bool>>>,
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
            connections: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
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
                    Self::read_messages_from_stream(peer_id, recv, events).await;
                });
            }
            Err(e) => {
                let _ = events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
            }
        }
    }

    async fn read_messages_from_stream(peer_id: AgentId, mut recv: RecvStream, events: mpsc::Sender<MeshEvent>) {
        loop {
            match read_message(&mut recv).await {
                Ok(bytes) => {
                    match decode(&bytes) {
                        Ok(message) => {
                            let msg_str = format!("{:?}", message);
                            if events.send(MeshEvent::MessageReceived(peer_id.clone(), msg_str)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = events.send(MeshEvent::Error(peer_id.clone(), format!("decode error: {}", e))).await;
                        }
                    }
                }
                Err(e) => {
                    let _ = events.send(MeshEvent::Error(peer_id.clone(), format!("read error: {}", e))).await;
                    break;
                }
            }
        }
    }

    pub async fn handle_incoming(&self, peer: Peer) {
        let peer_id = peer.agent_id.clone();
        let events = self.events.clone();
        let connections = self.connections.clone();

        if self.connections.read().await.contains_key(&peer_id) {
            return;
        }

        match self.transport.accept().await {
            Ok((connection, _)) => {
                match connection.connection.accept_bi().await {
                    Ok((send, recv)) => {
                        let connected_peer = ConnectedPeer {
                            peer: peer.clone(),
                            sender: send,
                            connection: connection.clone(),
                        };

                        connections.write().await.insert(peer_id.clone(), connected_peer);

                        let _ = events.send(MeshEvent::Connected(peer_id.clone())).await;

                        tokio::spawn(async move {
                            Self::read_messages_from_stream(peer_id, recv, events).await;
                        });
                    }
                    Err(e) => {
                        let _ = events.send(MeshEvent::Error(peer_id.clone(), format!("accept bi error: {}", e))).await;
                    }
                }
            }
            Err(e) => {
                let _ = events.send(MeshEvent::Error(peer_id.clone(), format!("accept error: {}", e))).await;
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
        self.connections.write().await.remove(peer_id);
        let _ = self.events.send(MeshEvent::Disconnected(peer_id.clone())).await;
    }
}
