//! Rust-native mesh networking module
//!
//! This module provides pure Rust implementations for mesh networking
//! including address derivation, routing, and cryptographic primitives.
//!
//! # S-02 Compliance
//! - Uses BTreeMap/BTreeSet for deterministic iteration
//! - No SystemTime::now() or other non-deterministic time sources
//! - All operations are sequence-based and deterministic

#![forbid(dead_code)]

pub mod address;
pub mod crypto;
pub mod routing;

// IMPLEMENTATION_REQUIRED: Wire into rust_mesh_transport for address derivation
pub use address::{yggdrasil_addr_from_pubkey, YggdrasilAddress};

// IMPLEMENTATION_REQUIRED: Integrate with RustMeshTransport for E2EE
pub use crypto::{ecies_decrypt, ecies_encrypt, CryptoProvider};

// IMPLEMENTATION_REQUIRED: Use for peer routing in mesh network
pub use routing::RoutingTable;
