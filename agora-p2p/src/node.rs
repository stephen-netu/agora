//! Main P2P Node that ties everything together

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use agora_crypto::AgentId;
use crate::error::Error;
use crate::types::{Config, Peer};
use crate::transport::quic::{QuicTransport, QuicConfig, generate_self_signed_cert};
use crate::discovery::mdns::{MdnsDiscovery, MdnsPeerEvent};
use crate::protocol::{AmpMessage, SerializedEvent};
use crate::mesh::peer::MeshManager;

pub struct P2pNode {
    config: Config,
    transport: Arc<QuicTransport>,
    discovery: Arc<RwLock<Option<MdnsDiscovery>>>,
    mesh: Arc<MeshManager>,
    mesh_events_tx: mpsc::Sender<MeshEvent>,
    mesh_events_rx: Option<mpsc::Receiver<MeshEvent>>,
    mesh_internal_rx: Option<mpsc::Receiver<crate::mesh::peer::MeshEvent>>,
    /// Sequence counter for deterministic event timestamps (S-02 compliant)
    sequence_counter: AtomicU64,
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(String),
    Disconnected(String),
    MessageReceived(String, crate::protocol::AmpMessage),
    Error(String, String),
}

impl P2pNode {
    pub async fn new(config: Config) -> Result<Self, Error> {
        let (cert, key) = generate_self_signed_cert(&config.agent_id)?;
        
        let quic_config = QuicConfig::new(cert, key);
        
        let transport = QuicTransport::new(quic_config, config.agent_id.clone()).await?;
        let transport = Arc::new(transport);
        
        let (mesh_events_tx, mesh_events_rx) = mpsc::channel(100);
        
        let (mesh_internal_tx, mesh_internal_rx) = mpsc::channel(100);
        
        let mesh = Arc::new(MeshManager::new(
            config.agent_id.clone(),
            transport.clone(),
            mesh_internal_tx,
        ));
        
        Ok(Self {
            config,
            transport,
            discovery: Arc::new(RwLock::new(None)),
            mesh,
            mesh_events_tx,
            mesh_events_rx: Some(mesh_events_rx),
            mesh_internal_rx: Some(mesh_internal_rx),
            sequence_counter: AtomicU64::new(0),
        })
    }
    
    pub async fn start(&mut self, port: u16) -> Result<(), Error> {
        let listen_addr: SocketAddr = format!("0.0.0.0:{}", port)
            .parse()
            .map_err(|e: std::net::AddrParseError| Error::Transport(e.to_string()))?;
        
        self.transport.listen(listen_addr).await?;
        info!("P2P transport listening on {}", listen_addr);
        
        let mdns = MdnsDiscovery::new(
            &self.config.agent_id.to_string(),
            port,
            &self.config.service_name,
        )?;
        
        let (discovery, discovery_events) = mdns;
        discovery.start_browse().await?;
        
        *self.discovery.write().await = Some(discovery);
        
        info!("mDNS discovery started for {}", self.config.agent_id);
        
        self.spawn_incoming_handler();
        
        self.spawn_event_handlers(discovery_events).await;
        
        if let Some(mut internal_rx) = self.mesh_internal_rx.take() {
            let events_tx = self.mesh_events_tx.clone();
            tokio::spawn(async move {
                while let Some(event) = internal_rx.recv().await {
                    let public_event = match event {
                        crate::mesh::peer::MeshEvent::Connected(id) => MeshEvent::Connected(id.to_string()),
                        crate::mesh::peer::MeshEvent::Disconnected(id) => MeshEvent::Disconnected(id.to_string()),
                        crate::mesh::peer::MeshEvent::MessageReceived(id, msg) => MeshEvent::MessageReceived(id.to_string(), msg),
                        crate::mesh::peer::MeshEvent::Error(id, err) => MeshEvent::Error(id.to_string(), err),
                    };
                    let _ = events_tx.send(public_event).await;
                }
            });
        }
        
        Ok(())
    }
    
    fn spawn_incoming_handler(&self) {
        let transport = self.transport.clone();
        let mesh = self.mesh.clone();
        
        tokio::spawn(async move {
            info!("Incoming connection handler started");
            loop {
                match transport.accept().await {
                    Ok((connection, peer_id)) => {
                        info!("Accepted incoming connection from {}", connection.remote_addr);
                        
                        let mesh_clone = mesh.clone();
                        tokio::spawn(async move {
                            mesh_clone.handle_incoming(connection, Some(peer_id)).await;
                        });
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
    
    async fn spawn_event_handlers(
        &self,
        mut discovery_events: mpsc::Receiver<MdnsPeerEvent>,
    ) {
        let mesh = self.mesh.clone();
        let mesh_events_tx = self.mesh_events_tx.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = discovery_events.recv() => {
                        match event {
                            MdnsPeerEvent::PeerDiscovered(peer) => {
                                info!("Discovered peer: {}", peer.agent_id);
                                
                                if let Err(e) = mesh.try_connect(&peer).await {
                                    let err_str = e.to_string();
                                    let _ = mesh_events_tx.send(MeshEvent::Error(peer.agent_id.to_string(), err_str)).await;
                                }
                            }
                            MdnsPeerEvent::PeerRemoved(agent_id) => {
                                info!("Peer removed: {}", agent_id);
                                let peer_id = AgentId::from_hex(&agent_id)
                                    .unwrap_or_else(|_| {
                                        AgentId::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap()
                                    });
                                mesh.disconnect(&peer_id).await;
                            }
                            MdnsPeerEvent::PeerUpdated(peer) => {
                                info!("Peer updated: {}", peer.agent_id);
                            }
                        }
                    }
                    else => break,
                }
            }
        });
    }
    
    pub async fn broadcast_room_message(&self, room_id: &str, message: &[u8]) -> Result<(), Error> {
        // S-02 compliant: use deterministic sequence number instead of wall-clock time
        let sequence = self.sequence_counter.fetch_add(1, Ordering::SeqCst);
        
        let event_id = format!("{}-{}", room_id, sequence);
        
        let msg = AmpMessage::EventPush {
            room_id: room_id.to_string(),
            events: vec![SerializedEvent {
                event_id,
                event_type: "m.room.message".to_string(),
                content: message.to_vec(),
                origin_server_ts: sequence,
            }],
        };
        
        let peers = self.mesh.connected_peers().await;
        let peer_count = peers.len();
        
        for peer_id in peers {
            if let Err(e) = self.mesh.send_to(&peer_id, msg.clone()).await {
                info!("Failed to send message to peer {}: {}", peer_id, e);
            }
        }
        
        info!("Broadcasting message to room {} ({} peers)", room_id, peer_count);
        
        Ok(())
    }
    
    pub async fn connected_peers(&self) -> Vec<String> {
        let peers = self.mesh.connected_peers().await;
        peers.iter().map(|k| k.to_string()).collect()
    }
    
    pub async fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.transport.local_addr()
    }
    
    pub fn take_mesh_events(&mut self) -> Option<mpsc::Receiver<MeshEvent>> {
        self.mesh_events_rx.take()
    }
}
