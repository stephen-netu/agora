//! Sigchain identity and behavioral ledger for agora-cli.
//!
//! Maintains a persistent Ed25519 agent identity and an append-only sigchain
//! recording every action taken by this CLI invocation. The chain is stored
//! as JSON alongside the session token:
//!
//! - `~/.config/agora/identity_seed`  — 32-byte raw binary seed (never logged)
//! - `~/.config/agora/sigchain.json`  — full sigchain as JSON
//!
//! On first run both files are created. The seed is generated from the OS
//! random source; subsequent runs load the same identity.
//!
//! S-02: timestamps are `chain.len()` (monotonically increasing, no SystemTime).
//! S-05: no unbounded loops; all operations are O(1) or O(chain_len).

use std::io::Write;
use std::path::{Path, PathBuf};

use agora_crypto::{AgentId, AgentIdentity, Sigchain, SigchainBody, SigchainLink};
use rand_core::{OsRng, RngCore};

/// Manages a CLI agent's persistent identity and sigchain.
pub struct SigchainManager {
    pub identity: AgentIdentity,
    pub chain: Sigchain,
    data_dir: PathBuf,
}

#[derive(Debug)]
pub enum SigchainError {
    Io(String),
    Crypto(String),
}

impl std::fmt::Display for SigchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SigchainError::Io(s) | SigchainError::Crypto(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for SigchainError {}

impl SigchainManager {
    /// Open or create the sigchain manager for the given config directory.
    ///
    /// On first call: generates a 32-byte seed, creates a genesis sigchain link,
    /// and persists both to disk.
    /// On subsequent calls: loads the existing seed and chain.
    pub fn open(data_dir: &Path) -> Result<Self, SigchainError> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| SigchainError::Io(format!("create config dir: {e}")))?;

        let seed = load_or_generate_seed(data_dir)?;
        let identity = AgentIdentity::from_seed(&seed);

        let chain = load_or_create_chain(data_dir, &identity)?;

        Ok(Self {
            identity,
            chain,
            data_dir: data_dir.to_owned(),
        })
    }

    /// Return the agent's full 64-char hex `AgentId` for use in API calls.
    pub fn agent_id_hex(&self) -> String {
        self.chain.agent_id.to_hex()
    }

    /// Append a new `Action` link and return it for publishing.
    ///
    /// - `event_type`: Matrix event type string (e.g. `"m.room.message"`).
    /// - `room_id`:    Matrix room ID string for hashing.
    /// - `content`:    The JSON event content to commit to.
    /// - `correlation_path`: caller-supplied path (empty for top-level calls).
    ///
    /// The `timestamp` is `chain.len()` before appending — monotonically
    /// increasing and S-02 compliant (no `SystemTime`).
    pub fn append_action(
        &mut self,
        event_type: &str,
        room_id: &str,
        content: &serde_json::Value,
        correlation_path: Vec<AgentId>,
    ) -> Result<SigchainLink, SigchainError> {
        let room_id_hash = *blake3::hash(room_id.as_bytes()).as_bytes();

        // Hash the canonical JSON serialization of the event content.
        let content_bytes = serde_json::to_vec(content)
            .map_err(|e| SigchainError::Crypto(format!("serialize content: {e}")))?;
        let content_hash = *blake3::hash(&content_bytes).as_bytes();

        // S-02: timestamp = chain length before append (monotonically increasing).
        let timestamp = self.chain.len() as u64;

        // S-05: enforce loop detection here so callers cannot bypass it.
        // has_loop() returns false on an empty path, so no guard needed.
        if Sigchain::has_loop(&self.chain.agent_id, &correlation_path) {
            return Err(SigchainError::Crypto(
                "loop detected: agent_id appears in correlation_path — call append_refusal() instead".into(),
            ));
        }

        // Validate path length (S-05: max 16).
        if correlation_path.len() > 16 {
            return Err(SigchainError::Crypto(
                "correlation_path exceeds 16-hop limit (S-05)".into(),
            ));
        }

        let body = SigchainBody::Action {
            event_type: event_type.to_owned(),
            event_id_hash: [0u8; 32], // event_id unknown until after publish
            room_id_hash,
            content_hash,
            effect_hash: None,
            timestamp,
            correlation_path,
        };

        self.chain
            .append(body, &self.identity)
            .map_err(|e| SigchainError::Crypto(format!("append action: {e}")))?;

        Ok(self.chain.links.last().expect("just appended").clone())
    }

    /// Check whether `correlation_path` contains this agent's `AgentId`.
    ///
    /// Callers MUST invoke this before `append_action` when the path is
    /// non-empty. If `true`, call `append_refusal` instead and return an error.
    pub fn has_loop(&self, correlation_path: &[AgentId]) -> bool {
        Sigchain::has_loop(&self.chain.agent_id, correlation_path)
    }

    /// Append a `Refusal` link (loop detected) and return it for publishing.
    ///
    /// Should be called when `has_loop()` returns `true`. Records the refusal
    /// on-chain so it is auditable and non-repudiable.
    pub fn append_refusal(
        &mut self,
        refused_event_type: &str,
        correlation_path_snapshot: Vec<AgentId>,
    ) -> Result<SigchainLink, SigchainError> {
        if correlation_path_snapshot.len() > 16 {
            return Err(SigchainError::Crypto(
                "correlation_path_snapshot exceeds 16-hop limit (S-05)".into(),
            ));
        }

        let timestamp = self.chain.len() as u64;

        let body = SigchainBody::Refusal {
            refused_event_type: refused_event_type.to_owned(),
            reason: "loop detected: agent_id appears in correlation_path".to_owned(),
            correlation_path_snapshot,
            timestamp,
        };

        self.chain
            .append(body, &self.identity)
            .map_err(|e| SigchainError::Crypto(format!("append refusal: {e}")))?;

        Ok(self.chain.links.last().expect("just appended").clone())
    }

    /// Persist the current chain state to disk.
    pub fn save(&self) -> Result<(), SigchainError> {
        let chain_path = self.data_dir.join("sigchain.json");
        let json = serde_json::to_string_pretty(&self.chain)
            .map_err(|e| SigchainError::Io(format!("serialize chain: {e}")))?;
        std::fs::write(&chain_path, json)
            .map_err(|e| SigchainError::Io(format!("write sigchain: {e}")))?;
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load_or_generate_seed(data_dir: &Path) -> Result<[u8; 32], SigchainError> {
    let seed_path = data_dir.join("identity_seed");

    match std::fs::read(&seed_path) {
        Ok(data) => {
            if data.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data);
                return Ok(arr);
            }
            // Wrong length — truncation or corruption. Fail loudly; silently
            // regenerating would create a new identity and lose chain history.
            return Err(SigchainError::Io(
                "identity_seed has wrong length — file may be corrupted; remove it manually to regenerate".into(),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(SigchainError::Io(format!("read identity_seed: {e}"))),
    }

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    write_seed_secret(&seed_path, &seed)
        .map_err(|e| SigchainError::Io(format!("write identity_seed: {e}")))?;
    Ok(seed)
}

/// Write the 32-byte identity seed with owner-only permissions (0o600 on Unix).
/// Prevents other users on the system from reading the private key material.
#[cfg(unix)]
fn write_seed_secret(path: &Path, seed: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(seed)
}

#[cfg(not(unix))]
fn write_seed_secret(path: &Path, seed: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, seed)
}

fn load_or_create_chain(data_dir: &Path, identity: &AgentIdentity) -> Result<Sigchain, SigchainError> {
    let chain_path = data_dir.join("sigchain.json");

    match std::fs::read_to_string(&chain_path) {
        Ok(data) => {
            let chain: Sigchain = serde_json::from_str(&data)
                .map_err(|e| SigchainError::Io(format!("parse sigchain.json: {e}")))?;

            // Full chain integrity verification (seqno, hash-links, signatures).
            chain
                .verify_chain()
                .map_err(|e| SigchainError::Crypto(format!("sigchain integrity check failed: {e}")))?;

            // Identity↔chain consistency.
            if chain.agent_id != identity.agent_id {
                return Err(SigchainError::Crypto(
                    "identity agent_id does not match sigchain agent_id — store is corrupted".into(),
                ));
            }

            Ok(chain)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First run: create genesis chain and persist it.
            let chain = Sigchain::genesis(identity, vec![], None)
                .map_err(|e| SigchainError::Crypto(format!("genesis: {e}")))?;

            let json = serde_json::to_string_pretty(&chain)
                .map_err(|e| SigchainError::Io(format!("serialize genesis: {e}")))?;
            std::fs::write(&chain_path, json)
                .map_err(|e| SigchainError::Io(format!("write genesis: {e}")))?;

            Ok(chain)
        }
        // Any other I/O error (permissions, device error, etc.) is fatal.
        Err(e) => Err(SigchainError::Io(format!("read sigchain.json: {e}"))),
    }
}
