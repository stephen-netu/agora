//! Agora P2P Mesh Networking
//! 
//! This crate provides peer-to-peer networking capabilities for Agora,
//! enabling direct communication between peers on local networks.

pub mod error;
pub mod types;
pub mod transport;
pub mod protocol;
pub mod discovery;
pub mod mesh;
pub mod node;

pub use error::Error;
pub use types::*;
pub use node::{P2pNode, MeshEvent};
