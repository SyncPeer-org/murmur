//! Error types for the engine crate.

/// Errors that can occur in engine operations.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// DAG error.
    #[error("dag: {0}")]
    Dag(#[from] murmur_dag::DagError),
    /// Network error.
    #[error("net: {0}")]
    Net(#[from] murmur_net::NetError),
    /// Device not found.
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    /// Device not approved.
    #[error("device not approved: {0}")]
    DeviceNotApproved(String),
    /// File already exists (dedup).
    #[error("file already exists: {0}")]
    FileAlreadyExists(String),
    /// Access denied.
    #[error("access denied: {0}")]
    AccessDenied(String),
    /// Access expired.
    #[error("access expired")]
    AccessExpired,
    /// Blob integrity check failed.
    #[error("blob integrity: expected {expected}, got {actual}")]
    BlobIntegrity {
        /// Expected hash.
        expected: String,
        /// Actual hash.
        actual: String,
    },
}
