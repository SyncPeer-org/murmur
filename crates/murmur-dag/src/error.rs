//! Error types for the DAG crate.

/// Errors that can occur in DAG operations.
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    /// The entry's hash does not match its content.
    #[error("invalid hash")]
    InvalidHash,
    /// The entry's Ed25519 signature is invalid.
    #[error("invalid signature")]
    InvalidSignature,
    /// The entry references parent hashes that are not in the local store.
    #[error("missing parents: {0:?}")]
    MissingParents(Vec<[u8; 32]>),
    /// Deserialization failed.
    #[error("deserialization: {0}")]
    Deserialization(String),
}
