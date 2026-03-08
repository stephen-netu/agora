//! TLS certificate verification for agora-p2p.
//!
//! Security Design:
//! This module implements certificate fingerprint verification for the QUIC transport.
//! Server certificates are verified against known fingerprints via FingerprintServerVerifier.
//! 
//! Note: Client certificate verification (mTLS) was removed from the TLS layer.
//! Authentication of connecting peers is handled at the application layer via the
//! sequence-based handshake in mesh/peer.rs. This design allows for:
//! - Simpler TLS configuration without client auth
//! - Flexible peer identity verification based on AgentId rather than certificates
//! - Future support for alternative transport layers (e.g., Yggdrasil) without TLS changes
//!
use std::sync::{Arc, RwLock};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::client::danger::{ServerCertVerifier, ServerCertVerified, HandshakeSignatureValid};
use rustls::DigitallySignedStruct;
use rustls::{SignatureScheme, Error as TlsError};
use std::collections::HashMap;
use sovereign_sdk::AgentId;

#[derive(Clone, Debug)]
pub struct FingerprintStore {
    inner: Arc<RwLock<HashMap<String, [u8; 32]>>>,
}

/// IMPLEMENTATION_REQUIRED: wired in future wt-XXX for certificate fingerprint verification
impl FingerprintStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn add(&self, agent_id: &str, fingerprint: [u8; 32]) {
        if let Ok(mut guard) = self.inner.write() {
            guard.insert(agent_id.to_string(), fingerprint);
        }
    }
    
    pub async fn add_peer(&self, agent_id: &AgentId, fingerprint: [u8; 32]) {
        self.add(&agent_id.to_string(), fingerprint);
    }
    
    pub fn is_trusted(&self, agent_id: &str, fingerprint: &[u8; 32]) -> bool {
        self.inner.read()
            .ok()
            .and_then(|guard| guard.get(agent_id).map(|fp| *fp == *fingerprint))
            .unwrap_or(false)
    }
    
    pub fn get(&self, agent_id: &str) -> Option<[u8; 32]> {
        self.inner.read().ok().and_then(|guard| guard.get(agent_id).copied())
    }
    
    pub fn inner(&self) -> &Arc<RwLock<HashMap<String, [u8; 32]>>> {
        &self.inner
    }
    
    pub async fn get_by_agent_id(&self, agent_id: &AgentId) -> Option<[u8; 32]> {
        self.get(&agent_id.to_string())
    }
    
    pub fn cert_fingerprint(cert: &CertificateDer) -> [u8; 32] {
        blake3::hash(cert.as_ref()).into()
    }
}

impl Default for FingerprintStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct FingerprintServerVerifier {
    store: FingerprintStore,
    expected_agent_id: Option<String>,
    expected_fingerprint: Option<[u8; 32]>,
}

impl FingerprintServerVerifier {
    pub fn new(store: FingerprintStore) -> Self {
        Self { 
            store, 
            expected_agent_id: None,
            expected_fingerprint: None,
        }
    }
    
    pub fn with_expected_agent(store: FingerprintStore, agent_id: AgentId, fingerprint: [u8; 32]) -> Self {
        Self {
            store,
            expected_agent_id: Some(agent_id.to_string()),
            expected_fingerprint: Some(fingerprint),
        }
    }
}

impl ServerCertVerifier for FingerprintServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let fingerprint = FingerprintStore::cert_fingerprint(end_entity);
        
        if let (Some(expected_agent_id), Some(expected_fingerprint)) = 
            (&self.expected_agent_id, &self.expected_fingerprint) 
        {
            if fingerprint != *expected_fingerprint {
                return Err(TlsError::General(
                    format!("certificate fingerprint mismatch for agent {}", expected_agent_id)
                ));
            }
            
            let name_str = match server_name {
                rustls::pki_types::ServerName::DnsName(name) => name.as_ref(),
                _ => return Ok(ServerCertVerified::assertion()),
            };
            if name_str != *expected_agent_id {
                return Err(TlsError::General(
                    format!("server name '{}' does not match expected agent_id '{}'", name_str, expected_agent_id)
                ));
            }
        } else {
            let trusted = self.store.inner().read()
                .ok()
                .and_then(|guard| {
                    let name_str = match server_name {
                        rustls::pki_types::ServerName::DnsName(name) => name.as_ref(),
                        _ => return None,
                    };
                    guard.get(name_str).map(|fp| *fp == fingerprint)
                })
                .unwrap_or(false);
            
            if !trusted {
                return Err(TlsError::General(
                    "unknown certificate fingerprint".to_string()
                ));
    }
}
        Ok(ServerCertVerified::assertion())
    }
    
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}


