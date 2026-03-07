#![warn(
    missing_docs,
    rust_2018_idioms,
    unused_import_braces,
    unused_qualifications,
    clippy::all,
    clippy::pedantic
)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

//! Core types and utilities for the Agora platform.

/// Matrix Client-Server API request/response types
pub mod api;
/// Event types for rooms and spaces
pub mod events;
/// Identifier types (UserId, RoomId, EventId, etc.)
pub mod identifiers;

/// Presence types and utilities
pub mod presence {
    pub use crate::events::presence::*;
}

// Re-export common types for convenience
pub use api::errcode;
pub use api::ErrorResponse;
