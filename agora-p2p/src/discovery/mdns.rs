use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use std::collections::HashMap;
use tracing::{info, warn, debug};

use crate::error::Error;
use crate::types::Peer;
use sovereign_sdk::AgentId;

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_to_agent: Arc<RwLock<HashMap<String, String>>>,
    peers: Arc<RwLock<HashMap<String, Peer>>>,
    peer_events: mpsc::Sender<MdnsPeerEvent>,
    service_type: String,
}

#[derive(Debug, Clone)]
pub enum MdnsPeerEvent {
    PeerDiscovered(Peer),
    PeerRemoved(String),
}

pub fn create_instance_name(agent_id: &str) -> String {
    let prefix: String = agent_id.chars().take(8).collect();
    format!("agora-{}", prefix)
}

impl MdnsDiscovery {
    pub fn new(
        agent_id: &str,
        port: u16,
        service_type: &str,
    ) -> Result<(Self, mpsc::Receiver<MdnsPeerEvent>), Error> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| Error::Discovery(e.to_string()))?;
        
        let (tx, rx) = mpsc::channel(100);
        
        let instance_name = create_instance_name(agent_id);
        
        let service_info = Self::build_service_info(
            &instance_name,
            service_type,
            port,
            agent_id,
        )?;
        
        daemon.register(service_info)
            .map_err(|e| Error::Discovery(e.to_string()))?;
        
        info!("mDNS service registered: {}", instance_name);
        
        Ok((
            Self {
                daemon,
                service_to_agent: Arc::new(RwLock::new(HashMap::new())),
                peers: Arc::new(RwLock::new(HashMap::new())),
                peer_events: tx,
                service_type: service_type.to_string(),
            },
            rx,
        ))
    }
    
    fn build_service_info(
        instance_name: &str,
        service_type: &str,
        port: u16,
        agent_id: &str,
    ) -> Result<ServiceInfo, Error> {
        let ips = get_local_ip()
            .map_err(|e| Error::Discovery(e.to_string()))?;
        
        let full_service_type = if service_type.ends_with(".local.") {
            service_type.to_string()
        } else if service_type.ends_with(".local") {
            format!("{}.", service_type)
        } else {
            format!("{}.local.", service_type)
        };
        
        let mut properties = HashMap::new();
        properties.insert("agent_id".to_string(), agent_id.to_string());
        
        ServiceInfo::new(
            &full_service_type,
            instance_name,
            &format!("{}.local.", instance_name),
            ips,
            port,
            properties,
        ).map_err(|e| Error::Discovery(e.to_string()))
    }
    
    pub async fn start_browse(&self) -> Result<(), Error> {
        info!("Starting mDNS browse for service type: {}", self.service_type);
        
        let receiver = self.daemon.browse(&self.service_type)
            .map_err(|e| Error::Discovery(e.to_string()))?;
        
        let service_to_agent = self.service_to_agent.clone();
        let peers = self.peers.clone();
        let peer_events = self.peer_events.clone();
        
        tokio::spawn(async move {
            loop {
                match receiver.recv_async().await {
                    Ok(event) => {
                        if let Err(e) = Self::handle_event(
                            event,
                            &service_to_agent,
                            &peers,
                            &peer_events,
                        ).await {
                            warn!("mDNS event error: {}", e);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        
        Ok(())
    }
    
    async fn handle_event(
        event: ServiceEvent,
        service_to_agent: &Arc<RwLock<HashMap<String, String>>>,
        peers: &Arc<RwLock<HashMap<String, Peer>>>,
        peer_events: &mpsc::Sender<MdnsPeerEvent>,
    ) -> Result<(), Error> {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                let fullname = info.get_fullname().to_string();
                let addr = info.get_addresses().iter().next().cloned();
                let port = info.get_port();
                
                let agent_id_str = info
                    .get_property_val_str("agent_id")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                
                let agent_id = AgentId::from_hex(&agent_id_str)
                    .unwrap_or_else(|_| {
                        AgentId::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
                            .unwrap()
                    });
                
                service_to_agent.write().await.insert(fullname.clone(), agent_id_str.clone());
                
                if let Some(ip) = addr {
                    let address = format!("{}:{}", ip, port);
                    let peer = Peer {
                        agent_id,
                        addresses: vec![address],
                    };
                    
                    peers.write().await.insert(agent_id_str.clone(), peer.clone());
                    
                    let _ = peer_events.send(MdnsPeerEvent::PeerDiscovered(peer)).await;
                    debug!("Discovered peer via mDNS: {}", agent_id_str);
                }
            }
            
            ServiceEvent::ServiceRemoved(_, fullname) => {
                let agent_id_opt = service_to_agent.read().await
                    .get(&fullname)
                    .cloned();
                
                if let Some(agent_id_str) = agent_id_opt {
                    service_to_agent.write().await.remove(&fullname);
                    peers.write().await.remove(&agent_id_str);
                    
                    let _ = peer_events.send(MdnsPeerEvent::PeerRemoved(agent_id_str.clone())).await;
                    debug!("Peer removed via mDNS: {}", agent_id_str);
                }
            }
            
            _ => {}
        }
        
        Ok(())
    }
}

fn get_local_ip() -> Result<IpAddr, Error> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| Error::Discovery(e.to_string()))?;
    socket.connect("8.8.8.8:80")
        .map_err(|e| Error::Discovery(e.to_string()))?;
    let addr = socket.local_addr()
        .map_err(|e| Error::Discovery(e.to_string()))?;
    Ok(addr.ip())
}
