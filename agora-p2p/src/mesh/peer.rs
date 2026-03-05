use std::collections::HashMap;
use std::sync::Arc;

use agora_crypto::AgentId;
use tokio::sync::{mpsc, RwLock};

use crate::error::Error;
use crate::types::Peer;
use crate::transport::quic::QuicTransport;
use crate::protocol::AmpMessage;

pub struct ConnectedPeer {
    pub peer: Peer,
    pub sender: quinn::SendStream,
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

    async fn handle_new_connection(&self, peer: Peer, _connection: crate::transport::quic::QuicConnection) {
        let peer_id = peer.agent_id.clone();

        match self.transport.open_stream(&peer_id).await {
            Ok(mut stream) => {
                let handshake = AmpMessage::Handshake {
                    agent_id: self.local_id.to_string(),
                    version: 1,
                    capabilities: Default::default(),
                };

                if let Ok(bytes) = crate::protocol::encode(&handshake) {
                    if let Err(e) = crate::transport::quic::write_message(&mut stream, &bytes).await {
                        let _ = self.events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
                        return;
                    }
                }

                let connected = ConnectedPeer {
                    peer,
                    sender: stream,
                };

                self.connections.write().await.insert(peer_id.clone(), connected);

                let _ = self.events.send(MeshEvent::Connected(peer_id.clone())).await;
            }
            Err(e) => {
                let _ = self.events.send(MeshEvent::Error(peer_id.clone(), e.to_string())).await;
            }
        }
    }

    pub async fn handle_incoming(&self, peer: Peer) {
        let peer_id = peer.agent_id.clone();

        if self.connections.read().await.contains_key(&peer_id) {
            return;
        }

        let _ = self.events.send(MeshEvent::Connected(peer_id)).await;
    }

    pub async fn send_to(&self, peer_id: &AgentId, message: AmpMessage) -> Result<(), Error> {
        if !self.is_connected(peer_id).await {
            return Err(Error::Mesh("peer not connected".to_string()));
        }

        let mut stream = self.transport.open_stream(peer_id).await
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
