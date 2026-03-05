//! Platform callback trait.
//!
//! The engine calls these methods to communicate with the platform.
//! The platform implements this trait to persist data, serve blobs,
//! and display events to the user.

use murmur_types::BlobHash;

use crate::EngineEvent;

/// Callbacks from the engine to the platform.
///
/// The platform implements this trait. All methods have default no-op
/// implementations so platforms can override only what they need.
pub trait PlatformCallbacks: Send + Sync {
    /// Persist a new DAG entry (serialized bytes).
    fn on_dag_entry(&self, entry_bytes: Vec<u8>) {
        let _ = entry_bytes;
    }

    /// Store a received blob.
    fn on_blob_received(&self, blob_hash: BlobHash, data: Vec<u8>) {
        let _ = (blob_hash, data);
    }

    /// Load a blob for transfer to a peer. Returns `None` if not available.
    fn on_blob_needed(&self, blob_hash: BlobHash) -> Option<Vec<u8>> {
        let _ = blob_hash;
        None
    }

    /// Notify the platform of an engine event.
    fn on_event(&self, event: EngineEvent) {
        let _ = event;
    }
}
