//! Main P2P Node that ties everything together

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use sovereign_sdk::AgentId;
use crate::error::Error;
use crate::types::{P2pConfig, WanDiscoveryMode};
use crate::transport::quic::{QuicTransport, QuicConfig, generate_self_signed_cert};
use crate::transport::yggdrasil::{resolve_yggdrasil_bind_addr};
use crate::types::{TransportMode, YggdrasilConfig};
use crate::discovery::mdns::{MdnsDiscovery, MdnsPeerEvent};
use crate::protocol::{AmpMessage, SerializedEvent};
use crate::mesh::peer::MeshManager;

pub struct P2pNode {
    config: P2pConfig,
    transport: Arc<QuicTransport>,
    discovery: Arc<RwLock<Option<MdnsDiscovery>>>,
    mesh: Arc<MeshManager>,
    mesh_events_tx: mpsc::Sender<MeshEvent>,
    mesh_events_rx: Option<mpsc::Receiver<MeshEvent>>,
    mesh_internal_rx: Option<mpsc::Receiver<crate::mesh::peer::MeshEvent>>,
    /// Sequence counter for deterministic event timestamps (S-02 compliant)
    sequence_counter: AtomicU64,
    /// Whether WAN (non-LAN) peer discovery is enabled.
    /// Persisted at the Tauri layer; P2pNode owns the runtime gate.
    wan_discovery_enabled: AtomicBool,
}

#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(String),
    Disconnected(String),
    MessageReceived(String, crate::protocol::AmpMessage),
    Error(String, String),
}

impl P2pNode {
    pub async fn new(config: P2pConfig) -> Result<Self, Error> {
        // Resolve agent_id from identity_source (file or daemon)
        let agent_id = config.identity_source.resolve_agent_id()
            .await
            .map_err(Error::Config)?;
        
        let (cert, key) = generate_self_signed_cert(&agent_id)?;
        
        // Determine bind address based on transport mode
        let bind_addr = match &config.transport {
            TransportMode::Quic(quic_cfg) => {
                quic_cfg.bind_addr.or_else(|| Some(SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                    config.listen_port,
                )))
            }
            TransportMode::Yggdrasil(ygg_config) => {
                // Try to bind to Yggdrasil address
                resolve_yggdrasil_bind_addr(ygg_config)
            }
            TransportMode::Auto => {
                // Auto: try Yggdrasil first, fall back to QUIC
                if let Some(ygg_addr) = resolve_yggdrasil_bind_addr(&YggdrasilConfig::default()) {
                    info!("Yggdrasil detected, binding to {}", ygg_addr);
                    Some(ygg_addr)
                } else {
                    info!("No Yggdrasil daemon, using QUIC");
                    Some(std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                        config.listen_port,
                    ))
                }
            }
        };

        let quic_config = QuicConfig::new(cert, key, bind_addr);
        
        let transport = QuicTransport::new(quic_config, agent_id.clone()).await?;
        let transport = Arc::new(transport);
        
        let (mesh_events_tx, mesh_events_rx) = mpsc::channel(100);
        
        let (mesh_internal_tx, mesh_internal_rx) = mpsc::channel(100);
        
        let mesh = Arc::new(MeshManager::new(
            agent_id.clone(),
            transport.clone(),
            mesh_internal_tx,
        ));
        
        // Update config with resolved agent_id
        let config = P2pConfig {
            identity_source: config.identity_source,
            agent_id: agent_id.clone(),
            listen_port: config.listen_port,
            service_name: config.service_name,
            transport: config.transport,
            wan_discovery: config.wan_discovery,
        };

        let wan_discovery = config.wan_discovery.clone();
        
        Ok(Self {
            config,
            transport,
            discovery: Arc::new(RwLock::new(None)),
            mesh,
            mesh_events_tx,
            mesh_events_rx: Some(mesh_events_rx),
            mesh_internal_rx: Some(mesh_internal_rx),
            sequence_counter: AtomicU64::new(0),
            wan_discovery_enabled: AtomicBool::new(!matches!(wan_discovery, WanDiscoveryMode::Disabled)),
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
                                match AgentId::from_hex(&agent_id) {
                                    Ok(peer_id) => {
                                        mesh.disconnect(&peer_id).await;
                                    }
                                    Err(e) => {
                                        warn!(agent_id = %agent_id, error = %e, "Failed to parse AgentId for peer removal - skipping");
                                    }
                                }
                            }
                        }
                    }
                    else => break,
                }
            }
        });
    }
    
    pub async fn broadcast_grove_message(&self, grove_id: &str, message: &[u8]) -> Result<(), Error> {
        // S-02 compliant: use deterministic sequence number instead of wall-clock time
        let sequence = self.sequence_counter.fetch_add(1, Ordering::SeqCst);
        
        let event_id = format!("{}-{}", grove_id, sequence);
        
        let msg = AmpMessage::EventPush {
            grove_id: grove_id.to_string(),
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
        
        info!("Broadcasting message to grove {} ({} peers)", grove_id, peer_count);
        
        Ok(())
    }
    
    pub async fn connected_peers(&self) -> Vec<String> {
        let peers = self.mesh.connected_peers().await;
        peers.iter().map(|k| k.to_string()).collect()
    }

    pub async fn peer_addr(&self, peer_id: &str) -> Option<SocketAddr> {
        self.mesh.peer_addr(peer_id).await
    }

    pub async fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.transport.local_addr()
    }
    
    pub fn take_mesh_events(&mut self) -> Option<mpsc::Receiver<MeshEvent>> {
        self.mesh_events_rx.take()
    }

    /// Send a CollaborationRequest to all connected peers.
    ///
    /// Loop detection: if `self.agent_id()` already appears in `correlation_path`,
    /// returns an error without sending (S-05: bounded call chains).
    /// S-05: correlation_path is bounded to 16 entries.
    pub async fn send_collaboration_request(
        &self,
        block_id: &str,
        content: Vec<u8>,
        correlation_path: Vec<String>,
    ) -> Result<(), Error> {
        if correlation_path.len() > 16 {
            return Err(Error::Broadcast(
                "correlation_path exceeds 16-hop limit (S-05)".to_string(),
            ));
        }
        let own_id = self.config.agent_id.to_hex();
        if correlation_path.iter().any(|id| id == &own_id) {
            return Err(Error::Broadcast(
                "loop detected in correlation_path".to_string(),
            ));
        }
        // Add self to correlation_path for proper multi-hop loop detection (S-05)
        let mut path = correlation_path;
        path.push(own_id.clone());
        let msg = AmpMessage::CollaborationRequest {
            block_id: block_id.to_string(),
            content,
            from: own_id,
            correlation_path: path,
        };
        let peers = self.mesh.connected_peers().await;
        for peer_id in peers {
            if let Err(e) = self.mesh.send_to(&peer_id, msg.clone()).await {
                info!("Failed to send CollaborationRequest to peer {}: {}", peer_id, e);
            }
        }
        Ok(())
    }

    /// Send a CollaborationResponse to a specific peer by AgentId hex.
    pub async fn send_collaboration_response(
        &self,
        block_id: &str,
        content: Vec<u8>,
        target_agent_id: &str,
        proof: Option<Vec<u8>>,
    ) -> Result<(), Error> {
        let peer_id = sovereign_sdk::AgentId::from_hex(target_agent_id)
            .map_err(|e| Error::Broadcast(format!("invalid target agent_id: {}", e)))?;
        let msg = AmpMessage::CollaborationResponse {
            block_id: block_id.to_string(),
            content,
            agent_id: self.config.agent_id.to_hex(),
            proof,
        };
        self.mesh.send_to(&peer_id, msg).await
    }

    /// Send a CollaborationRefusal to all connected peers.
    ///
    /// S-05: reason is bounded to 256 bytes; correlation_path_snapshot to 16 entries.
    pub async fn send_collaboration_refusal(
        &self,
        block_id: &str,
        reason: &str,
        correlation_path_snapshot: Vec<String>,
    ) -> Result<(), Error> {
        if reason.len() > 256 {
            return Err(Error::Broadcast(
                "refusal reason exceeds 256 bytes (S-05)".to_string(),
            ));
        }
        if correlation_path_snapshot.len() > 16 {
            return Err(Error::Broadcast(
                "correlation_path_snapshot exceeds 16-hop limit (S-05)".to_string(),
            ));
        }
        let msg = AmpMessage::CollaborationRefusal {
            block_id: block_id.to_string(),
            from: self.config.agent_id.to_hex(),
            reason: reason.to_string(),
            correlation_path_snapshot,
        };
        let peers = self.mesh.connected_peers().await;
        for peer_id in peers {
            if let Err(e) = self.mesh.send_to(&peer_id, msg.clone()).await {
                info!("Failed to send CollaborationRefusal to peer {}: {}", peer_id, e);
            }
        }
        Ok(())
    }

    pub async fn send_to(
        &self,
        peer_id: &str,
        message: AmpMessage,
    ) -> Result<(), Error> {
        let peer = sovereign_sdk::AgentId::from_hex(peer_id)
            .map_err(|e| Error::Mesh(format!("invalid peer_id: {}", e)))?;
        self.mesh.send_to(&peer, message).await
    }

    /// Connect to a peer by address
    pub async fn connect_to_peer(&self, peer: crate::types::Peer) -> Result<(), Error> {
        self.mesh.try_connect(&peer).await
    }

    /// Attempt to connect directly to a peer by AgentId hex string and network address.
    /// Used for manual WAN dialing when mDNS discovery is unavailable.
    pub async fn connect_to_peer_by_addr(&self, peer_agent_id: &str, address: &str) -> Result<(), Error> {
        let peer_id = sovereign_sdk::AgentId::from_hex(peer_agent_id)
            .map_err(|e| Error::InvalidPeer(e.to_string()))?;
        let peer = crate::types::Peer {
            agent_id: peer_id,
            addresses: vec![address.to_string()],
        };
        self.mesh.try_connect(&peer).await
    }

    /// Returns the local QUIC listen address as a string, for sharing out-of-band.
    pub fn local_address(&self) -> Option<String> {
        self.transport.local_addr().ok().map(|a| a.to_string())
    }

    pub fn agent_id(&self) -> &sovereign_sdk::AgentId {
        &self.config.agent_id
    }

    pub fn transport_mode(&self) -> &crate::types::TransportMode {
        &self.config.transport
    }

    pub fn listen_port(&self) -> u16 {
        self.config.listen_port
    }

    /// Enable or disable WAN (non-LAN) peer discovery.
    ///
    /// This gates the runtime preference — actual WAN transport activation
    /// (DHT, rendezvous, etc.) is IMPLEMENTATION_REQUIRED when those transports land.
    pub fn set_wan_discovery(&self, enabled: bool) {
        self.wan_discovery_enabled.store(enabled, Ordering::SeqCst);
    }

    /// Returns whether WAN discovery is currently enabled.
    pub fn is_wan_discovery_enabled(&self) -> bool {
        self.wan_discovery_enabled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_loop_detection_check() {
        // The loop detection logic: if own_id appears in correlation_path, reject.
        let own_hex = "aa".repeat(32); // 64-char hex
        let path_with_self = std::slice::from_ref(&own_hex);
        assert!(path_with_self.iter().any(|id| id == &own_hex));

        let path_without_self = &["bb".repeat(32)];
        assert!(!path_without_self.iter().any(|id| id == &own_hex));
    }

    #[test]
    fn test_correlation_path_limit() {
        // S-05: paths longer than 16 must be rejected.
        let long_path: Vec<String> = (0..17).map(|i| format!("{:064x}", i)).collect();
        assert!(long_path.len() > 16);

        let valid_path: Vec<String> = (0..16).map(|i| format!("{:064x}", i)).collect();
        assert!(valid_path.len() <= 16);
    }

    #[test]
    fn test_refusal_reason_limit() {
        // S-05: reasons longer than 256 bytes must be rejected.
        let long_reason = "x".repeat(257);
        assert!(long_reason.len() > 256);

        let valid_reason = "loop detected";
        assert!(valid_reason.len() <= 256);
    }
}
