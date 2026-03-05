//! DAG entry: the fundamental unit of the append-only log.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use murmur_types::{Action, DeviceId};
use serde::{Deserialize, Serialize};

use crate::DagError;

/// A single entry in the DAG.
///
/// Contains the action, HLC timestamp, author device, parent hashes,
/// content hash, and Ed25519 signature. Adapted from Shoal's `LogEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagEntry {
    /// blake3 hash of `(hlc, device_id, action, parents)`.
    pub hash: [u8; 32],
    /// Hybrid logical clock timestamp.
    pub hlc: u64,
    /// Author device.
    pub device_id: DeviceId,
    /// The mutation this entry records.
    pub action: Action,
    /// Parent entry hashes (DAG edges).
    pub parents: Vec<[u8; 32]>,
    /// Ed25519 signature R component.
    pub signature_r: [u8; 32],
    /// Ed25519 signature S component.
    pub signature_s: [u8; 32],
}

impl DagEntry {
    /// Create a new entry, compute its hash, and sign it.
    pub fn new_signed(
        hlc: u64,
        device_id: DeviceId,
        action: Action,
        parents: Vec<[u8; 32]>,
        signing_key: &SigningKey,
    ) -> Self {
        let hash = Self::compute_hash(hlc, device_id, &action, &parents);
        let signature = signing_key.sign(&hash);
        let sig_bytes = signature.to_bytes();

        Self {
            hash,
            hlc,
            device_id,
            action,
            parents,
            signature_r: sig_bytes[..32].try_into().unwrap(),
            signature_s: sig_bytes[32..].try_into().unwrap(),
        }
    }

    /// Compute the blake3 hash of the entry's content fields.
    pub fn compute_hash(
        hlc: u64,
        device_id: DeviceId,
        action: &Action,
        parents: &[[u8; 32]],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&hlc.to_le_bytes());
        hasher.update(device_id.as_bytes());
        let action_bytes = postcard::to_allocvec(action).expect("action serialization");
        hasher.update(&action_bytes);
        for parent in parents {
            hasher.update(parent);
        }
        *hasher.finalize().as_bytes()
    }

    /// Verify that the stored hash matches the content.
    pub fn verify_hash(&self) -> Result<(), DagError> {
        let expected = Self::compute_hash(self.hlc, self.device_id, &self.action, &self.parents);
        if self.hash != expected {
            return Err(DagError::InvalidHash);
        }
        Ok(())
    }

    /// Verify the Ed25519 signature against the hash.
    pub fn verify_signature(&self) -> Result<(), DagError> {
        let mut sig_bytes = [0u8; 64];
        sig_bytes[..32].copy_from_slice(&self.signature_r);
        sig_bytes[32..].copy_from_slice(&self.signature_s);

        let signature = Signature::from_bytes(&sig_bytes);

        let vk_bytes: [u8; 32] = *self.device_id.as_bytes();
        let verifying_key =
            VerifyingKey::from_bytes(&vk_bytes).map_err(|_| DagError::InvalidSignature)?;

        verifying_key
            .verify(&self.hash, &signature)
            .map_err(|_| DagError::InvalidSignature)
    }

    /// Serialize to bytes (postcard) for platform persistence.
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("DagEntry serialization")
    }

    /// Deserialize from bytes (postcard).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DagError> {
        postcard::from_bytes(bytes).map_err(|e| DagError::Deserialization(e.to_string()))
    }
}
