//! Orchestrator for Murmur: sync logic, approval flow, blob transfer.
//!
//! The [`MurmurEngine`] ties the DAG and network layers together. It is
//! **storage-agnostic** — it communicates with the platform via
//! [`PlatformCallbacks`] and emits [`EngineEvent`]s for UI updates.
//!
//! The platform is responsible for:
//! - Persisting DAG entries and blobs
//! - Loading persisted entries on startup via [`MurmurEngine::load_entry`]
//! - Providing blobs when the engine requests them

mod callbacks;
mod engine;
mod error;
mod event;

pub use callbacks::PlatformCallbacks;
pub use engine::MurmurEngine;
pub use error::EngineError;
pub use event::EngineEvent;
