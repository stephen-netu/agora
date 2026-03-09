//! Rust-native mesh networking module
//!
//! This module provides pure Rust implementations for mesh networking
//! including address derivation, routing, and cryptographic primitives.
//!
//! # S-02 Compliance
//! - Uses BTreeMap/BTreeSet for deterministic iteration
//! - No SystemTime::now() or other non-deterministic time sources
//! - All operations are sequence-based and deterministic
//!
//! IMPLEMENTATION_REQUIRED: Phase 1-4 implementation complete
//! - address.rs: Yggdrasil IPv6 address derivation from Ed25519 keys
//! - crypto.rs: X25519 key exchange + ChaCha20-Poly1305 E2EE
//! - routing.rs: BTreeMap-based peer routing
//! - Phase 5: Integration with P2pNode (wt-200 sub-tasks pending)

pub mod address;
pub mod crypto;
pub mod routing;

// IMPLEMENTATION_REQUIRED: Used by rust_mesh_transport for network integration
pub use address::YggdrasilAddress;
pub use crypto::CryptoProvider;
pub use routing::RoutingTable;
