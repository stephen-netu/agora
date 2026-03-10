//! Error types for agora-p2p

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("discovery error: {0}")]
    Discovery(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("mesh error: {0}")]
    Mesh(String),

    #[error("broadcast error: {0}")]
    Broadcast(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("invalid peer: {0}")]
    InvalidPeer(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
