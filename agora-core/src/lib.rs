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

pub mod api;
pub mod events;
pub mod identifiers;

pub mod presence {
    pub use crate::events::presence::*;
}

// Re-export common types for convenience
pub use api::errcode;
pub use api::ErrorResponse;
