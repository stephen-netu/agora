use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{Endpoint, Connection, RecvStream, SendStream, Incoming, VarInt};
use quinn::crypto::rustls::{QuicServerConfig, QuicClientConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig as TlsServerConfig, ClientConfig as TlsClientConfig};
use rcgen::{generate_simple_self_signed, CertifiedKey};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tracing::{info, error, debug};

use crate::error::Error;
use crate::transport::tls::{FingerprintStore, FingerprintServerVerifier};
use agora_crypto::AgentId;

pub struct QuicConfig {
    pub tls_cert: CertificateDer<'static>,
    pub tls_key: PrivateKeyDer<'static>,
    pub max_idle_timeout: u64,
    pub keepalive_interval: u64,
}

impl QuicConfig {
    pub fn new(cert: CertificateDer<'static>, key: PrivateKeyDer<'static>) -> Self {
        Self {
            tls_cert: cert,
            tls_key: key,
            max_idle_timeout: 30_000,
            keepalive_interval: 15_000,
        }
    }
}

#[derive(Clone)]
pub struct QuicConnection {
    pub connection: Connection,
    pub peer_id: AgentId,
    pub remote_addr: SocketAddr,
}

pub struct QuicTransport {
    endpoint: Endpoint,
    config: QuicConfig,
    agent_id: AgentId,
    fingerprint_store: FingerprintStore,
    connections: Arc<RwLock<HashMap<AgentId, QuicConnection>>>,
    incoming_sender: mpsc::Sender<mpsc::Sender<Incoming>>,
}

pub fn generate_self_signed_cert(agent_id: &AgentId) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Error> {
    let subject = agent_id.to_string();
    let cert: CertifiedKey = generate_simple_self_signed(vec![subject])
        .map_err(|e| Error::Tls(e.to_string()))?;
    
    let cert_ref = cert.cert.der();
    let cert_der = CertificateDer::from(cert_ref.to_vec());
    let key_der = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));
    
    Ok((cert_der, key_der))
}

fn make_quinn_server_config(cert: CertificateDer<'static>, key: PrivateKeyDer<'static>, max_idle_timeout: u64, keepalive_interval: u64) -> Result<quinn::ServerConfig, Error> {
    let mut server_config = TlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| Error::Tls(e.to_string()))?;
    
    let alpn: Vec<u8> = b"agora-p2p".to_vec();
    server_config.alpn_protocols = vec![alpn];
    
    let quic_server_config = QuicServerConfig::try_from(server_config)
        .map_err(|e| Error::Tls(format!("failed to convert to quic config: {}", e)))?;
    
    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_server_config));
    server_config.transport_config(Arc::new({
        let mut config = quinn::TransportConfig::default();
        config.max_idle_timeout(Some(VarInt::from_u32(max_idle_timeout as u32).into()));
        config.keep_alive_interval(Some(Duration::from_millis(keepalive_interval)));
        config
    }));
    
    Ok(server_config)
}

fn make_quinn_client_config(
    store: &FingerprintStore,
    expected_agent_id: Option<&AgentId>,
    expected_fingerprint: Option<[u8; 32]>,
    max_idle_timeout: u64,
    keepalive_interval: u64,
) -> Result<quinn::ClientConfig, Error> {
    let verifier = if let (Some(agent_id), Some(fingerprint)) = (expected_agent_id, expected_fingerprint) {
        Arc::new(FingerprintServerVerifier::with_expected_agent(store.clone(), agent_id.clone(), fingerprint))
    } else {
        Arc::new(FingerprintServerVerifier::new(store.clone()))
    };
    
    let mut client_config = TlsClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    
    let alpn: Vec<u8> = b"agora-p2p".to_vec();
    client_config.alpn_protocols = vec![alpn];
    
    let quic_client_config = QuicClientConfig::try_from(client_config)
        .map_err(|e| Error::Tls(format!("failed to convert to quic client config: {}", e)))?;
    
    let mut client_config = quinn::ClientConfig::new(Arc::new(quic_client_config));
    client_config.transport_config(Arc::new({
        let mut config = quinn::TransportConfig::default();
        config.max_idle_timeout(Some(VarInt::from_u32(max_idle_timeout as u32).into()));
        config.keep_alive_interval(Some(Duration::from_millis(keepalive_interval)));
        config
    }));
    
    Ok(client_config)
}

impl QuicTransport {
    pub async fn new(config: QuicConfig, agent_id: AgentId) -> Result<Self, Error> {
        let server_config = make_quinn_server_config(config.tls_cert.clone(), config.tls_key.clone_key(), config.max_idle_timeout, config.keepalive_interval)?;
        
        let endpoint = Endpoint::server(server_config, "0.0.0.0:0".parse().map_err(|e: std::net::AddrParseError| Error::Transport(e.to_string()))?)?;
        
        let (tx, mut rx) = mpsc::channel::<mpsc::Sender<Incoming>>(100);
        
        let incoming = endpoint.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(request_tx) = rx.recv() => {
                        match incoming.accept().await {
                            Some(connecting) => {
                                if request_tx.send(connecting).await.is_err() {
                                    debug!("failed to send incoming connection to requestor");
                                }
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    else => break,
                }
            }
        });
        
        info!("QUIC transport initialized for agent: {}", agent_id);
        
        Ok(Self {
            endpoint,
            config,
            agent_id,
            fingerprint_store: FingerprintStore::new(),
            connections: Arc::new(RwLock::new(HashMap::new())),
            incoming_sender: tx,
        })
    }
    
    pub async fn listen(&self, addr: SocketAddr) -> Result<(), Error> {
        let server_config = make_quinn_server_config(self.config.tls_cert.clone(), self.config.tls_key.clone_key(), self.config.max_idle_timeout, self.config.keepalive_interval)?;
        
        let _ = self.endpoint.set_server_config(Some(server_config));
        
        info!("QUIC transport listening on: {}", addr);
        Ok(())
    }
    
    pub async fn connect(&self, addr: SocketAddr, peer_id: &AgentId, peer_fingerprint: Option<[u8; 32]>) -> Result<QuicConnection, Error> {
        let expected_fingerprint = if let Some(fp) = peer_fingerprint {
            Some(fp)
        } else {
            self.fingerprint_store.get_by_agent_id(peer_id).await
        };
        
        let client_config = make_quinn_client_config(
            &self.fingerprint_store,
            Some(peer_id),
            expected_fingerprint,
            self.config.max_idle_timeout,
            self.config.keepalive_interval,
        )?;
        
        let connecting = self.endpoint.connect_with(client_config, addr, &peer_id.to_string())
            .map_err(|e| Error::Transport(format!("connect error: {}", e)))?;
        let connection = connecting.await
            .map_err(|e| Error::Transport(format!("connection failed: {}", e)))?;
        
        let remote_addr = connection.remote_address();
        
        let quic_connection = QuicConnection {
            connection: connection.clone(),
            peer_id: peer_id.clone(),
            remote_addr,
        };
        
        self.connections.write().await.insert(peer_id.clone(), quic_connection.clone());
        
        info!("Connected to peer at: {}", addr);
        
        Ok(quic_connection)
    }
    
    pub async fn connected_peers(&self) -> Vec<AgentId> {
        self.connections.read().await.keys().cloned().collect()
    }
    
    pub async fn accept(&self) -> Result<(QuicConnection, AgentId), Error> {
        let (tx, mut rx) = mpsc::channel(1);
        self.incoming_sender.send(tx).await.map_err(|_| Error::Transport("incoming channel closed".to_string()))?;
        
        let incoming = rx.recv().await
            .ok_or_else(|| Error::Transport("connection channel closed".to_string()))?;
        
        let connection: Connection = match incoming.await {
            Ok(conn) => conn,
            Err(e) => return Err(Error::Transport(format!("connection failed: {}", e))),
        };
        
        let addr = connection.remote_address();
        
        let peer_id = match connection.peer_identity() {
            Some(cert_any) => {
                let cert_chain = cert_any
                    .downcast_ref::<Vec<CertificateDer<'_>>>()
                    .ok_or_else(|| Error::Transport("peer identity is not a certificate chain".to_string()))?;
                
                let cert = cert_chain.first()
                    .ok_or_else(|| Error::Transport("no certificate in peer identity".to_string()))?;
                
                let (_, x509) = X509Certificate::from_der(cert.as_ref())
                    .map_err(|e| Error::Transport(format!("failed to parse certificate: {}", e)))?;
                
                let subject_str = x509.subject().to_string();
                let agent_id_str = subject_str.split(':').last()
                    .ok_or_else(|| Error::Transport("certificate subject has no agent id".to_string()))?;
                
                AgentId::from_hex(agent_id_str.trim())
                    .map_err(|_| Error::Transport(format!("invalid agent_id in certificate subject: {}", agent_id_str)))?
            }
            None => {
                return Err(Error::Transport("no peer identity (TLS certificate) provided".to_string()));
            }
        };
        
        let quic_connection = QuicConnection {
            connection: connection.clone(),
            peer_id: peer_id.clone(),
            remote_addr: addr,
        };
        
        self.connections.write().await.insert(peer_id.clone(), quic_connection.clone());
        
        info!("Accepted connection from: {} (peer_id: {})", addr, peer_id);
        
        Ok((quic_connection, peer_id))
    }
    
    pub async fn open_stream(&self, peer: &AgentId) -> Result<SendStream, Error> {
        let connections = self.connections.read().await;
        let connection = connections.get(peer)
            .ok_or_else(|| Error::Transport("peer not connected".to_string()))?;
        
        let (send, _recv) = connection.connection.open_bi().await
            .map_err(|e| Error::Transport(e.to_string()))?;
        
        debug!("Opened stream to peer: {}", peer);
        
        Ok(send)
    }
    
    pub async fn get_connection(&self, peer: &AgentId) -> Option<QuicConnection> {
        self.connections.read().await.get(peer).cloned()
    }
    
    pub async fn close(&self) {
        info!("Closing QUIC transport");
        
        let mut connections = self.connections.write().await;
        for (peer_id, conn) in connections.drain() {
            debug!("Closing connection to peer: {}", peer_id);
            conn.connection.close(0u8.into(), b"closing".as_ref());
        }
        
        self.endpoint.close(0u8.into(), b"transport closed".as_ref());
        info!("QUIC transport closed");
    }
    
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.endpoint.local_addr()
            .map_err(|e| Error::Transport(e.to_string()))
    }
    
    pub async fn remove_connection(&self, peer: &AgentId) {
        if let Some(conn) = self.connections.write().await.remove(peer) {
            debug!("Removing connection to peer: {}", peer);
            conn.connection.close(0u8.into(), b"connection removed".as_ref());
        }
    }
    
    pub async fn add_peer(&self, agent_id: &AgentId, fingerprint: [u8; 32]) {
        self.fingerprint_store.add_peer(agent_id, fingerprint).await;
        info!("Added peer {} to fingerprint store", agent_id);
    }
    
    pub fn fingerprint_store(&self) -> &FingerprintStore {
        &self.fingerprint_store
    }
    
    pub async fn get_connected_peers(&self) -> Vec<AgentId> {
        self.connections.read().await.keys().cloned().collect()
    }
}

pub async fn read_message(stream: &mut RecvStream) -> Result<Vec<u8>, Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await
        .map_err(|e| Error::Transport(e.to_string()))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await
        .map_err(|e| Error::Transport(e.to_string()))?;
    
    debug!("Read message of {} bytes", len);
    Ok(buf)
}

pub async fn write_message(stream: &mut SendStream, data: &[u8]) -> Result<(), Error> {
    let len = data.len() as u32;
    stream.write_all(&len.to_le_bytes()).await
        .map_err(|e| Error::Transport(e.to_string()))?;
    stream.write_all(data).await
        .map_err(|e| Error::Transport(e.to_string()))?;
    
    debug!("Wrote message of {} bytes", data.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_agent_id() -> AgentId {
        AgentId::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap()
    }
    
    #[tokio::test]
    async fn test_quic_config_default() {
        let (cert, key) = generate_self_signed_cert(&test_agent_id()).unwrap();
        let config = QuicConfig::new(cert, key);
        
        assert_eq!(config.max_idle_timeout, 30_000);
        assert_eq!(config.keepalive_interval, 15_000);
    }
    
    #[tokio::test]
    async fn test_quic_transport_creation() {
        rustls::crypto::aws_lc_rs::default_provider().install_default().ok();
        let (cert, key) = generate_self_signed_cert(&test_agent_id()).unwrap();
        let config = QuicConfig::new(cert, key);
        let agent_id = test_agent_id();
        
        let transport = QuicTransport::new(config, agent_id.clone()).await.unwrap();
        
        let addr = transport.local_addr().unwrap();
        assert!(addr.port() > 0);
    }
}
