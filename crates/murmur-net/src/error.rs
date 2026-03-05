//! Error types for the network crate.

/// Errors that can occur in network operations.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// Message deserialization failed.
    #[error("deserialization: {0}")]
    Deserialization(String),
    /// QUIC connection error.
    #[error("connection: {0}")]
    Connection(String),
    /// QUIC stream write error.
    #[error("write: {0}")]
    Write(String),
    /// QUIC stream read error.
    #[error("read: {0}")]
    Read(String),
    /// Message too large.
    #[error("message too large: {size} bytes (max {max})")]
    MessageTooLarge {
        /// Actual size.
        size: usize,
        /// Maximum allowed.
        max: usize,
    },
    /// Blob integrity check failed.
    #[error("blob integrity: expected {expected}, got {actual}")]
    BlobIntegrity {
        /// Expected hash.
        expected: String,
        /// Actual hash.
        actual: String,
    },
    /// Gossip error.
    #[error("gossip: {0}")]
    Gossip(String),
}
