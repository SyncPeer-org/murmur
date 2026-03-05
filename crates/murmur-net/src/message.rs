//! Wire protocol message types for Murmur.

use murmur_types::{AccessGrant, AccessScope, BlobHash, DeviceId};
use serde::{Deserialize, Serialize};

use crate::NetError;

/// Wire protocol messages exchanged between Murmur peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MurmurMessage {
    /// Gossip broadcast of a new DAG entry.
    DagEntryBroadcast {
        /// Postcard-serialized DAG entry bytes.
        entry_bytes: Vec<u8>,
    },
    /// Request missing DAG entries from a peer.
    DagSyncRequest {
        /// The requester's current tip hashes.
        tips: Vec<[u8; 32]>,
    },
    /// Response with missing DAG entries.
    DagSyncResponse {
        /// Serialized DAG entries (each as postcard bytes).
        entries: Vec<Vec<u8>>,
    },
    /// Push file data to a backup node.
    BlobPush {
        /// Content hash of the blob.
        blob_hash: BlobHash,
        /// Raw blob data.
        data: Vec<u8>,
    },
    /// Acknowledge a blob push.
    BlobPushAck {
        /// Content hash of the blob.
        blob_hash: BlobHash,
        /// Whether the blob was accepted.
        ok: bool,
    },
    /// Request a blob from a peer.
    BlobRequest {
        /// Content hash of the requested blob.
        blob_hash: BlobHash,
    },
    /// Response with blob data.
    BlobResponse {
        /// Content hash of the blob.
        blob_hash: BlobHash,
        /// Blob data, or `None` if the peer doesn't have it.
        data: Option<Vec<u8>>,
    },
    /// Request temporary access to files.
    AccessRequest {
        /// The requesting device.
        from: DeviceId,
        /// Scope of the access request.
        scope: AccessScope,
    },
    /// Response to an access request.
    AccessResponse {
        /// The grant, or `None` if rejected.
        grant: Option<AccessGrant>,
    },
    /// Ping (liveness check).
    Ping {
        /// Timestamp (HLC or unix nanos).
        timestamp: u64,
    },
    /// Pong (liveness response).
    Pong {
        /// Echoed timestamp.
        timestamp: u64,
    },
}

impl MurmurMessage {
    /// Serialize to postcard bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("MurmurMessage serialization")
    }

    /// Deserialize from postcard bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NetError> {
        postcard::from_bytes(bytes).map_err(|e| NetError::Deserialization(e.to_string()))
    }
}
