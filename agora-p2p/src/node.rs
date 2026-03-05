//! Main P2P Node that ties everything together

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::error::Error;
use crate::types::Config;
use crate::transport::quic::{QuicTransport, QuicConfig, generate_self_signed_cert};
use crate::discovery::mdns::{MdnsDiscovery, MdnsPeerEvent};
use crate::protocol::{AmpMessage, SerializedEvent};

pub struct P2pNode {
    config: Config,
    transport: Arc<QuicTransport>,
    discovery: Arc<RwLock<Option<MdnsDiscovery>>>,
    mesh_events_tx: mpsc::Sender<MeshEvent>,
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(String),
    Disconnected(String),
    MessageReceived(String, Vec<u8>),
    Error(String, String),
}

impl P2pNode {
    pub async fn new(config: Config) -> Result<Self, Error> {
        let (cert, key) = generate_self_signed_cert(&config.agent_id)?;
        
        let quic_config = QuicConfig::new(cert, key);
        
        let transport = QuicTransport::new(quic_config, config.agent_id.clone()).await?;
        let transport = Arc::new(transport);
        
        let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(100);
        
        Ok(Self {
            config,
            transport,
            discovery: Arc::new(RwLock::new(None)),
            mesh_events_tx,
        })
    }
    
    pub async fn start(&self, port: u16) -> Result<(), Error> {
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
        
        let (discovery, mut discovery_events) = mdns;
        discovery.start_browse().await?;
        
        *self.discovery.write().await = Some(discovery);
        
        info!("mDNS discovery started for {}", self.config.agent_id);
        
        self.spawn_event_handlers(&mut discovery_events).await;
        
        Ok(())
    }
    
    async fn spawn_event_handlers(
        &self,
        discovery_events: &mut mpsc::Receiver<MdnsPeerEvent>,
    ) {
        let transport = self.transport.clone();
        let mesh_events_tx = self.mesh_events_tx.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = discovery_events.recv() => {
                        match event {
                            MdnsPeerEvent::PeerDiscovered(peer) => {
                                info!("Discovered peer: {}", peer.agent_id);
                                if let Some(addr_str) = peer.addresses.first() {
                                    if let Ok(addr) = addr_str.parse::<SocketAddr>() {
                                        match transport.connect(addr).await {
                                            Ok(_) => {
                                                let _ = mesh_events_tx.send(MeshEvent::Connected(peer.agent_id.to_string())).await;
                                            }
                                            Err(e) => {
                                                let err_str = e.to_string();
                                                let _ = mesh_events_tx.send(MeshEvent::Error(peer.agent_id.to_string(), err_str)).await;
                                            }
                                        }
                                    }
                                }
                            }
                            MdnsPeerEvent::PeerRemoved(agent_id) => {
                                info!("Peer removed: {}", agent_id);
                                let _ = mesh_events_tx.send(MeshEvent::Disconnected(agent_id.clone())).await;
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
        let event_id = format!("{}-{}", room_id, std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis());
        
        let msg = AmpMessage::EventPush {
            room_id: room_id.to_string(),
            events: vec![SerializedEvent {
                event_id,
                event_type: "m.room.message".to_string(),
                content: message.to_vec(),
                origin_server_ts: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            }],
        };
        
        let encoded = crate::protocol::encode(&msg)
            .map_err(|e| Error::Protocol(e.to_string()))?;
        
        info!("Broadcasting message to room {} ({} bytes)", room_id, encoded.len());
        
        Ok(())
    }
    
    pub async fn connected_peers(&self) -> Vec<String> {
        let addrs = self.transport.connections.read().await;
        let peers: Vec<String> = addrs.keys().map(|k| k.to_string()).collect();
        peers
    }
}
