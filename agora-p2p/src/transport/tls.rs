use std::sync::{Arc, RwLock, Mutex};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerifier, ClientCertVerified};
use rustls::client::danger::{ServerCertVerifier, ServerCertVerified, HandshakeSignatureValid};
use rustls::DigitallySignedStruct;
use rustls::{SignatureScheme, Error as TlsError, DistinguishedName};
use std::collections::HashMap;
use crate::error::Error;
use agora_crypto::AgentId;

#[derive(Clone, Debug)]
pub struct FingerprintStore {
    inner: Arc<RwLock<HashMap<String, [u8; 32]>>>,
}

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

#[derive(Debug)]
pub struct FingerprintClientVerifier {
    store: FingerprintStore,
    expected_agent_id: Option<String>,
    verified_fingerprint: Mutex<Option<[u8; 32]>>,
}

impl FingerprintClientVerifier {
    pub fn new(store: FingerprintStore, expected_agent_id: Option<String>) -> Self {
        Self { 
            store, 
            expected_agent_id, 
            verified_fingerprint: Mutex::new(None),
        }
    }

    pub async fn get_verified_fingerprint(&self) -> Option<[u8; 32]> {
        *self.verified_fingerprint.lock().unwrap()
    }
}

impl ClientCertVerifier for FingerprintClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        let _fingerprint = FingerprintStore::cert_fingerprint(end_entity);
        
        Ok(ClientCertVerified::assertion())
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

#[derive(Debug)]
pub struct FingerprintClientConfig {
    verifier: Arc<FingerprintClientVerifier>,
    store: FingerprintStore,
    expected_agent_id: Option<String>,
}

impl FingerprintClientConfig {
    pub fn new(store: FingerprintStore, expected_agent_id: Option<String>) -> Self {
        let verifier = Arc::new(FingerprintClientVerifier::new(store.clone(), expected_agent_id.clone()));
        Self {
            verifier,
            store,
            expected_agent_id,
        }
    }

    pub fn store(&self) -> &FingerprintStore {
        &self.store
    }

    pub fn expected_agent_id(&self) -> Option<&String> {
        self.expected_agent_id.as_ref()
    }

    pub fn verifier(&self) -> Arc<FingerprintClientVerifier> {
        self.verifier.clone()
    }
}

pub fn make_client_config(
    store: FingerprintStore,
    expected_agent_id: Option<String>,
    expected_fingerprint: Option<[u8; 32]>,
) -> Result<rustls::ClientConfig, Error> {
    let verifier = if let (Some(agent_id), Some(fingerprint)) = (expected_agent_id, expected_fingerprint) {
        if let Ok(agent_id_bytes) = AgentId::from_hex(&agent_id) {
            Arc::new(FingerprintServerVerifier::with_expected_agent(store, agent_id_bytes, fingerprint))
        } else {
            return Err(Error::Tls(format!("invalid agent_id: {}", agent_id)));
        }
    } else {
        Arc::new(FingerprintServerVerifier::new(store))
    };
    
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    
    config.alpn_protocols = vec![b"agora-p2p".to_vec()];
    
    Ok(config)
}

pub fn make_server_config(
    store: FingerprintStore,
    _require_known: bool,
    cert: CertificateDer<'static>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<rustls::ServerConfig, Error> {
    let verifier = Arc::new(FingerprintClientVerifier::new(store, None));
    
    let mut config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![cert], key)
        .map_err(|e| Error::Tls(e.to_string()))?;
    
    config.alpn_protocols = vec![b"agora-p2p".to_vec()];
    
    Ok(config)
}
