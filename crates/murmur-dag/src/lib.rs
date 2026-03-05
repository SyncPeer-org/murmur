//! Signed append-only DAG for Murmur, adapted from Shoal's LogTree.
//!
//! The DAG records all mutations (device joins, file additions, access grants)
//! as signed, hash-chained entries. It is **in-memory only** — the platform is
//! responsible for persisting entries and feeding them back on startup via
//! [`Dag::load_entry`].
//!
//! # Key concepts
//!
//! - Each [`DagEntry`] references its parent entries by hash, forming a DAG.
//! - Entries are signed with Ed25519 and verified on receive.
//! - Tips are entries with no children — they represent the current frontier.
//! - When multiple tips exist (concurrent branches), [`Dag::maybe_merge`]
//!   produces a merge entry that collapses them.
//! - [`MaterializedState`] is a derived cache rebuilt by replaying the DAG.

mod entry;
mod error;
mod state;

pub use entry::DagEntry;
pub use error::DagError;
pub use state::MaterializedState;

use std::collections::{HashMap, HashSet, VecDeque};

use ed25519_dalek::SigningKey;
use murmur_types::{Action, DeviceId, HybridClock};
use tracing::debug;

/// An in-memory signed append-only DAG.
///
/// Owns the entry store, tip set, HLC, and device identity. The platform
/// persists entries externally and feeds them back via [`Dag::load_entry`].
pub struct Dag {
    /// All entries indexed by hash.
    entries: HashMap<[u8; 32], DagEntry>,
    /// Current tip hashes (entries with no children).
    tips: HashSet<[u8; 32]>,
    /// Hybrid logical clock for this device.
    clock: HybridClock,
    /// This device's ID.
    device_id: DeviceId,
    /// This device's signing key.
    signing_key: SigningKey,
    /// Materialized state derived from DAG replay.
    state: MaterializedState,
}

impl Dag {
    /// Create a new empty DAG for the given device.
    pub fn new(device_id: DeviceId, signing_key: SigningKey) -> Self {
        Self {
            entries: HashMap::new(),
            tips: HashSet::new(),
            clock: HybridClock::new(),
            device_id,
            signing_key,
            state: MaterializedState::new(),
        }
    }

    /// Load a previously persisted entry on startup.
    ///
    /// Verifies the entry's hash and signature, adds it to the store, and
    /// applies it to the materialized state. Parents need not be loaded in
    /// order — tip tracking is rebuilt from the full set of loaded entries.
    pub fn load_entry(&mut self, entry: DagEntry) -> Result<(), DagError> {
        entry.verify_hash()?;
        entry.verify_signature()?;

        let hash = entry.hash;

        // Witness the HLC to keep our clock up to date.
        self.clock.witness(entry.hlc);

        // Apply to materialized state.
        self.state.apply(&entry);

        self.entries.insert(hash, entry);

        // Rebuild tips: we do a simple rebuild after all loads.
        // For incremental loading this is fine — the caller should call
        // `rebuild_tips()` after loading all entries if performance matters.
        self.rebuild_tips();

        Ok(())
    }

    /// Rebuild the tip set from scratch.
    ///
    /// An entry is a tip if no other entry references it as a parent.
    pub fn rebuild_tips(&mut self) {
        let all_hashes: HashSet<[u8; 32]> = self.entries.keys().copied().collect();
        let mut referenced: HashSet<[u8; 32]> = HashSet::new();
        for entry in self.entries.values() {
            for parent in &entry.parents {
                referenced.insert(*parent);
            }
        }
        self.tips = all_hashes.difference(&referenced).copied().collect();
    }

    /// Append a new action to the DAG.
    ///
    /// Ticks the HLC, uses current tips as parents, signs the entry, stores it,
    /// updates tips, and applies to materialized state. Returns the new entry
    /// for the platform to persist.
    pub fn append(&mut self, action: Action) -> DagEntry {
        let hlc = self.clock.tick();
        let parents: Vec<[u8; 32]> = self.tips.iter().copied().collect();

        let entry = DagEntry::new_signed(
            hlc,
            self.device_id,
            action,
            parents.clone(),
            &self.signing_key,
        );

        debug!(
            hash = %hex_short(&entry.hash),
            device = %self.device_id,
            "dag: appended entry"
        );

        // Update tips: remove parents, add new entry.
        for p in &parents {
            self.tips.remove(p);
        }
        self.tips.insert(entry.hash);

        // Apply to state.
        self.state.apply(&entry);

        self.entries.insert(entry.hash, entry.clone());
        entry
    }

    /// Receive an entry from a remote peer.
    ///
    /// Verifies hash and signature, checks that all parents exist, stores it,
    /// updates tips, and applies to materialized state. Returns the entry for
    /// platform persistence.
    pub fn receive_entry(&mut self, entry: DagEntry) -> Result<DagEntry, DagError> {
        entry.verify_hash()?;
        entry.verify_signature()?;

        // Check that all parents exist locally.
        let missing: Vec<[u8; 32]> = entry
            .parents
            .iter()
            .filter(|p| !self.entries.contains_key(*p))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(DagError::MissingParents(missing));
        }

        // Skip if already known.
        if self.entries.contains_key(&entry.hash) {
            return Ok(entry);
        }

        // Witness remote HLC.
        self.clock.witness(entry.hlc);

        // Update tips: remove parents that are now non-tips, add new entry.
        for p in &entry.parents {
            self.tips.remove(p);
        }
        self.tips.insert(entry.hash);

        // Apply to state.
        self.state.apply(&entry);

        let hash = entry.hash;
        self.entries.insert(hash, entry.clone());

        debug!(
            hash = %hex_short(&hash),
            "dag: received remote entry"
        );

        Ok(entry)
    }

    /// Apply a batch of sync entries in topological order.
    ///
    /// Sorts entries so that parents come before children, then applies each.
    /// Returns the entries that were actually new (not already known).
    pub fn apply_sync_entries(
        &mut self,
        entries: Vec<DagEntry>,
    ) -> Result<Vec<DagEntry>, DagError> {
        let sorted = topological_sort(entries)?;
        let mut new_entries = Vec::new();
        for entry in sorted {
            if !self.entries.contains_key(&entry.hash) {
                self.receive_entry(entry.clone())?;
                new_entries.push(entry);
            }
        }
        Ok(new_entries)
    }

    /// Compute the delta of entries the remote is missing.
    ///
    /// Given the remote's tip set, walks backward from our tips and returns
    /// all entries not reachable from the remote tips, in topological order.
    pub fn compute_delta(&self, remote_tips: &HashSet<[u8; 32]>) -> Vec<DagEntry> {
        // BFS backward from our tips, stopping at entries the remote already has.
        // An entry is "known to remote" if it's in remote_tips or all paths from
        // it lead to remote tips.

        // First, find all entries reachable from remote tips (what the remote has).
        let remote_known = self.reachable_from(remote_tips);

        // Then collect everything we have that they don't, in topological order.
        let mut delta_hashes: HashSet<[u8; 32]> = HashSet::new();
        for hash in self.entries.keys() {
            if !remote_known.contains(hash) {
                delta_hashes.insert(*hash);
            }
        }

        // Topological sort of the delta entries.
        let delta_entries: Vec<DagEntry> = delta_hashes
            .iter()
            .filter_map(|h| self.entries.get(h).cloned())
            .collect();

        topological_sort(delta_entries).unwrap_or_default()
    }

    /// Auto-merge when multiple tips exist.
    ///
    /// If there are 2+ tips, creates a `Merge` entry that references all tips
    /// as parents, collapsing them into a single tip. Returns the merge entry
    /// if one was created.
    pub fn maybe_merge(&mut self) -> Option<DagEntry> {
        if self.tips.len() < 2 {
            return None;
        }
        debug!(tips = self.tips.len(), "dag: auto-merging tips");
        Some(self.append(Action::Merge))
    }

    /// The current tip hashes.
    pub fn tips(&self) -> &HashSet<[u8; 32]> {
        &self.tips
    }

    /// The current materialized state.
    pub fn state(&self) -> &MaterializedState {
        &self.state
    }

    /// Number of entries in the DAG.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an entry by hash.
    pub fn get_entry(&self, hash: &[u8; 32]) -> Option<&DagEntry> {
        self.entries.get(hash)
    }

    /// Get all entries (for serialization / transfer).
    pub fn all_entries(&self) -> Vec<DagEntry> {
        self.entries.values().cloned().collect()
    }

    /// This device's ID.
    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    /// BFS reachable set from a given set of starting hashes.
    fn reachable_from(&self, start: &HashSet<[u8; 32]>) -> HashSet<[u8; 32]> {
        let mut visited = HashSet::new();
        let mut queue: VecDeque<[u8; 32]> = start.iter().copied().collect();
        while let Some(hash) = queue.pop_front() {
            if !visited.insert(hash) {
                continue;
            }
            if let Some(entry) = self.entries.get(&hash) {
                for parent in &entry.parents {
                    if !visited.contains(parent) {
                        queue.push_back(*parent);
                    }
                }
            }
        }
        visited
    }
}

/// Topological sort using Kahn's algorithm.
///
/// Returns entries ordered so that parents come before children.
fn topological_sort(entries: Vec<DagEntry>) -> Result<Vec<DagEntry>, DagError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let entry_map: HashMap<[u8; 32], DagEntry> =
        entries.iter().map(|e| (e.hash, e.clone())).collect();
    let hashes: HashSet<[u8; 32]> = entry_map.keys().copied().collect();

    // Compute in-degree (only counting edges within this batch).
    let mut in_degree: HashMap<[u8; 32], usize> = HashMap::new();
    for hash in &hashes {
        in_degree.entry(*hash).or_insert(0);
    }
    for entry in entry_map.values() {
        for parent in &entry.parents {
            // Only count parents that are in this batch.
            if hashes.contains(parent) {
                *in_degree.entry(entry.hash).or_insert(0) += 1;
            }
        }
    }

    // Seed the queue with entries that have no in-batch parents.
    let mut queue: VecDeque<[u8; 32]> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(h, _)| *h)
        .collect();

    // Sort queue for deterministic output (by HLC then DeviceId).
    let mut queue_vec: Vec<[u8; 32]> = queue.drain(..).collect();
    queue_vec.sort_by(|a, b| {
        let ea = &entry_map[a];
        let eb = &entry_map[b];
        ea.hlc.cmp(&eb.hlc).then(ea.device_id.cmp(&eb.device_id))
    });
    queue = queue_vec.into_iter().collect();

    let mut result = Vec::with_capacity(entry_map.len());

    while let Some(hash) = queue.pop_front() {
        let entry = &entry_map[&hash];
        result.push(entry.clone());

        // Find children in this batch (entries that have `hash` as a parent).
        let mut children: Vec<[u8; 32]> = Vec::new();
        for (h, e) in &entry_map {
            if e.parents.contains(&hash) && hashes.contains(h) {
                let deg = in_degree.get_mut(h).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    children.push(*h);
                }
            }
        }
        // Sort children for determinism.
        children.sort_by(|a, b| {
            let ea = &entry_map[a];
            let eb = &entry_map[b];
            ea.hlc.cmp(&eb.hlc).then(ea.device_id.cmp(&eb.device_id))
        });
        for c in children {
            queue.push_back(c);
        }
    }

    Ok(result)
}

/// Format a hash as a short hex string for logging.
fn hex_short(hash: &[u8; 32]) -> String {
    hash.iter()
        .take(4)
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use murmur_types::*;
    use rand::rngs::OsRng;

    fn make_keypair() -> (DeviceId, SigningKey) {
        let sk = SigningKey::generate(&mut OsRng);
        let id = DeviceId::from_verifying_key(&sk.verifying_key());
        (id, sk)
    }

    fn make_dag() -> Dag {
        let (id, sk) = make_keypair();
        Dag::new(id, sk)
    }

    fn sample_file_metadata(device_id: DeviceId) -> FileMetadata {
        FileMetadata {
            blob_hash: BlobHash::from_data(b"test file content"),
            filename: "photo.jpg".to_string(),
            size: 1024,
            mime_type: Some("image/jpeg".to_string()),
            created_at: 1700000000,
            device_origin: device_id,
        }
    }

    // --- Entry basics ---

    #[test]
    fn test_append_single_entry_verify() {
        let mut dag = make_dag();
        let entry = dag.append(Action::Merge);
        assert!(entry.verify_hash().is_ok());
        assert!(entry.verify_signature().is_ok());
    }

    #[test]
    fn test_append_multiple_entries_parents_correct() {
        let mut dag = make_dag();
        let e1 = dag.append(Action::Merge);
        let e2 = dag.append(Action::Merge);
        // e2 should have e1 as parent.
        assert!(e2.parents.contains(&e1.hash));
        assert_eq!(dag.tips().len(), 1);
        assert!(dag.tips().contains(&e2.hash));
    }

    #[test]
    fn test_receive_remote_entry() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let entry = dag_a.append(Action::Merge);
        let received = dag_b.receive_entry(entry.clone()).unwrap();
        assert_eq!(received.hash, entry.hash);
        assert_eq!(dag_b.len(), 1);
    }

    #[test]
    fn test_reject_bad_hash() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let mut entry = dag_a.append(Action::Merge);
        entry.hash[0] ^= 0xff; // corrupt hash
        let result = dag_b.receive_entry(entry);
        assert!(matches!(result, Err(DagError::InvalidHash)));
    }

    #[test]
    fn test_reject_bad_signature() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let mut entry = dag_a.append(Action::Merge);
        entry.signature_s[0] ^= 0xff; // corrupt signature
        let result = dag_b.receive_entry(entry);
        assert!(matches!(result, Err(DagError::InvalidSignature)));
    }

    #[test]
    fn test_reject_missing_parents() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let _e1 = dag_a.append(Action::Merge);
        let e2 = dag_a.append(Action::Merge);
        // dag_b doesn't have e1, so receiving e2 should fail.
        let result = dag_b.receive_entry(e2);
        assert!(matches!(result, Err(DagError::MissingParents(_))));
    }

    // --- Concurrent append & merge ---

    #[test]
    fn test_two_devices_concurrent_merge() {
        let (id_a, sk_a) = make_keypair();
        let (id_b, sk_b) = make_keypair();

        let mut dag_a = Dag::new(id_a, sk_a);
        let mut dag_b = Dag::new(id_b, sk_b);

        // Both append independently.
        let ea = dag_a.append(Action::Merge);
        let eb = dag_b.append(Action::Merge);

        // dag_a receives eb → now has 2 tips.
        dag_a.receive_entry(eb).unwrap();
        assert_eq!(dag_a.tips().len(), 2);

        // dag_a receives ea (already there).
        dag_b.receive_entry(ea).unwrap();
        assert_eq!(dag_b.tips().len(), 2);

        // Merge on dag_a.
        let merge = dag_a.maybe_merge().unwrap();
        assert_eq!(dag_a.tips().len(), 1);
        assert!(dag_a.tips().contains(&merge.hash));

        // dag_b receives the merge.
        dag_b.receive_entry(merge).unwrap();
        assert_eq!(dag_b.tips().len(), 1);
    }

    #[test]
    fn test_no_merge_single_tip() {
        let mut dag = make_dag();
        dag.append(Action::Merge);
        assert!(dag.maybe_merge().is_none());
    }

    // --- Delta computation ---

    #[test]
    fn test_delta_computation() {
        let (id_a, sk_a) = make_keypair();
        let (id_b, sk_b) = make_keypair();

        let mut dag_a = Dag::new(id_a, sk_a);
        let mut dag_b = Dag::new(id_b, sk_b);

        // dag_a has 3 entries.
        let e1 = dag_a.append(Action::Merge);
        let _e2 = dag_a.append(Action::Merge);
        let _e3 = dag_a.append(Action::Merge);

        // dag_b has only the first entry.
        dag_b.receive_entry(e1).unwrap();

        // Delta from dag_a's perspective, given dag_b's tips.
        let delta = dag_a.compute_delta(dag_b.tips());
        assert_eq!(delta.len(), 2); // e2 and e3
    }

    // --- Topological sort ---

    #[test]
    fn test_topological_sort_ordering() {
        let mut dag = make_dag();
        let e1 = dag.append(Action::Merge);
        let e2 = dag.append(Action::Merge);
        let e3 = dag.append(Action::Merge);

        let entries = vec![e3.clone(), e1.clone(), e2.clone()]; // scrambled
        let sorted = topological_sort(entries).unwrap();
        assert_eq!(sorted[0].hash, e1.hash);
        assert_eq!(sorted[1].hash, e2.hash);
        assert_eq!(sorted[2].hash, e3.hash);
    }

    // --- Materialized state: devices ---

    #[test]
    fn test_state_approve_device() {
        let mut dag = make_dag();
        let (new_id, _) = make_keypair();

        dag.append(Action::DeviceApproved {
            device_id: new_id,
            role: DeviceRole::Source,
        });

        let devices = &dag.state().devices;
        assert!(devices.contains_key(&new_id));
        let info = &devices[&new_id];
        assert!(info.approved);
        assert_eq!(info.role, DeviceRole::Source);
    }

    #[test]
    fn test_state_revoke_device() {
        let mut dag = make_dag();
        let (new_id, _) = make_keypair();

        dag.append(Action::DeviceApproved {
            device_id: new_id,
            role: DeviceRole::Backup,
        });
        dag.append(Action::DeviceRevoked { device_id: new_id });

        let info = &dag.state().devices[&new_id];
        assert!(!info.approved);
    }

    #[test]
    fn test_state_device_name_changed() {
        let mut dag = make_dag();
        let (new_id, _) = make_keypair();

        dag.append(Action::DeviceJoinRequest {
            device_id: new_id,
            name: "Old Name".to_string(),
        });
        dag.append(Action::DeviceApproved {
            device_id: new_id,
            role: DeviceRole::Full,
        });
        dag.append(Action::DeviceNameChanged {
            device_id: new_id,
            name: "New Name".to_string(),
        });

        assert_eq!(dag.state().devices[&new_id].name, "New Name");
    }

    // --- Materialized state: files ---

    #[test]
    fn test_state_add_file() {
        let mut dag = make_dag();
        let meta = sample_file_metadata(dag.device_id());

        dag.append(Action::FileAdded {
            metadata: meta.clone(),
        });

        assert!(dag.state().files.contains_key(&meta.blob_hash));
    }

    #[test]
    fn test_state_delete_file() {
        let mut dag = make_dag();
        let meta = sample_file_metadata(dag.device_id());
        let hash = meta.blob_hash;

        dag.append(Action::FileAdded { metadata: meta });
        dag.append(Action::FileDeleted { blob_hash: hash });

        assert!(!dag.state().files.contains_key(&hash));
    }

    // --- Materialized state: access ---

    #[test]
    fn test_state_grant_access() {
        let mut dag = make_dag();
        let (to_id, _) = make_keypair();

        let grant = AccessGrant {
            to: to_id,
            from: dag.device_id(),
            scope: AccessScope::AllFiles,
            expires_at: 9999999999,
            signature_r: [0xab; 32],
            signature_s: [0xcd; 32],
        };

        dag.append(Action::AccessGranted {
            grant: grant.clone(),
        });

        assert_eq!(dag.state().grants.len(), 1);
        assert_eq!(dag.state().grants[0].to, to_id);
    }

    #[test]
    fn test_state_revoke_access() {
        let mut dag = make_dag();
        let (to_id, _) = make_keypair();

        let grant = AccessGrant {
            to: to_id,
            from: dag.device_id(),
            scope: AccessScope::AllFiles,
            expires_at: 9999999999,
            signature_r: [0xab; 32],
            signature_s: [0xcd; 32],
        };

        dag.append(Action::AccessGranted { grant });
        dag.append(Action::AccessRevoked { to: to_id });

        assert!(dag.state().grants.is_empty());
    }

    // --- Serialization roundtrip ---

    #[test]
    fn test_entry_serialization_roundtrip_merge() {
        let mut dag = make_dag();
        let entry = dag.append(Action::Merge);
        let bytes = entry.to_bytes();
        let decoded = DagEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry.hash, decoded.hash);
        assert_eq!(entry.hlc, decoded.hlc);
        assert!(decoded.verify_hash().is_ok());
        assert!(decoded.verify_signature().is_ok());
    }

    #[test]
    fn test_entry_serialization_roundtrip_file_added() {
        let mut dag = make_dag();
        let meta = sample_file_metadata(dag.device_id());
        let entry = dag.append(Action::FileAdded { metadata: meta });
        let bytes = entry.to_bytes();
        let decoded = DagEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry, decoded);
    }

    #[test]
    fn test_entry_serialization_roundtrip_all_variants() {
        let mut dag = make_dag();
        let (other_id, _) = make_keypair();

        let actions = vec![
            Action::DeviceJoinRequest {
                device_id: other_id,
                name: "Phone".to_string(),
            },
            Action::DeviceApproved {
                device_id: other_id,
                role: DeviceRole::Source,
            },
            Action::DeviceRevoked {
                device_id: other_id,
            },
            Action::DeviceNameChanged {
                device_id: other_id,
                name: "New".to_string(),
            },
            Action::FileAdded {
                metadata: sample_file_metadata(dag.device_id()),
            },
            Action::FileDeleted {
                blob_hash: BlobHash::from_data(b"x"),
            },
            Action::AccessGranted {
                grant: AccessGrant {
                    to: other_id,
                    from: dag.device_id(),
                    scope: AccessScope::SingleFile(BlobHash::from_data(b"f")),
                    expires_at: 42,
                    signature_r: [1; 32],
                    signature_s: [2; 32],
                },
            },
            Action::AccessRevoked { to: other_id },
            Action::Merge,
            Action::Snapshot {
                state_hash: [0xfe; 32],
            },
        ];

        for action in actions {
            let entry = dag.append(action);
            let bytes = entry.to_bytes();
            let decoded = DagEntry::from_bytes(&bytes).unwrap();
            assert_eq!(entry, decoded);
        }
    }

    // --- Load entries on startup ---

    #[test]
    fn test_load_entries_reconstructs_state() {
        let mut dag = make_dag();
        let device_id = dag.device_id();
        let (new_id, _) = make_keypair();

        // Build some history.
        let e1 = dag.append(Action::DeviceApproved {
            device_id: new_id,
            role: DeviceRole::Backup,
        });
        let meta = sample_file_metadata(device_id);
        let e2 = dag.append(Action::FileAdded {
            metadata: meta.clone(),
        });

        // Create a fresh DAG and load entries.
        let (id2, sk2) = make_keypair();
        let mut fresh = Dag::new(id2, sk2);
        fresh.load_entry(e1).unwrap();
        fresh.load_entry(e2).unwrap();

        assert!(fresh.state().devices.contains_key(&new_id));
        assert!(fresh.state().files.contains_key(&meta.blob_hash));
        assert_eq!(fresh.len(), 2);
    }

    // --- Sync roundtrip ---

    #[test]
    fn test_sync_roundtrip_two_dags_converge() {
        let (id_a, sk_a) = make_keypair();
        let (id_b, sk_b) = make_keypair();

        let mut dag_a = Dag::new(id_a, sk_a);
        let mut dag_b = Dag::new(id_b, sk_b);

        // dag_a adds some entries.
        dag_a.append(Action::DeviceApproved {
            device_id: id_b,
            role: DeviceRole::Source,
        });
        dag_a.append(Action::FileAdded {
            metadata: FileMetadata {
                blob_hash: BlobHash::from_data(b"file1"),
                filename: "a.txt".to_string(),
                size: 10,
                mime_type: None,
                created_at: 0,
                device_origin: id_a,
            },
        });

        // dag_b adds some entries.
        dag_b.append(Action::FileAdded {
            metadata: FileMetadata {
                blob_hash: BlobHash::from_data(b"file2"),
                filename: "b.txt".to_string(),
                size: 20,
                mime_type: None,
                created_at: 0,
                device_origin: id_b,
            },
        });

        // Sync: send dag_a's entries to dag_b.
        let delta_a = dag_a.compute_delta(dag_b.tips());
        dag_b.apply_sync_entries(delta_a).unwrap();

        // Sync: send dag_b's entries to dag_a.
        let delta_b = dag_b.compute_delta(dag_a.tips());
        dag_a.apply_sync_entries(delta_b).unwrap();

        // Both should have all files.
        assert!(
            dag_a
                .state()
                .files
                .contains_key(&BlobHash::from_data(b"file1"))
        );
        assert!(
            dag_a
                .state()
                .files
                .contains_key(&BlobHash::from_data(b"file2"))
        );
        assert!(
            dag_b
                .state()
                .files
                .contains_key(&BlobHash::from_data(b"file1"))
        );
        assert!(
            dag_b
                .state()
                .files
                .contains_key(&BlobHash::from_data(b"file2"))
        );

        // Merge on both to converge tips.
        dag_a.maybe_merge();
        dag_b.maybe_merge();
    }

    // --- LWW conflict resolution ---

    #[test]
    fn test_lww_higher_hlc_wins() {
        // When two devices name the same device differently, the later HLC wins.
        let mut dag = make_dag();
        let (target, _) = make_keypair();

        dag.append(Action::DeviceApproved {
            device_id: target,
            role: DeviceRole::Full,
        });
        dag.append(Action::DeviceNameChanged {
            device_id: target,
            name: "First".to_string(),
        });
        dag.append(Action::DeviceNameChanged {
            device_id: target,
            name: "Second".to_string(),
        });

        // Last write wins — "Second" should be the name.
        assert_eq!(dag.state().devices[&target].name, "Second");
    }

    #[test]
    fn test_lww_tiebreak_by_device_id() {
        // When HLC is equal (simulated), higher DeviceId wins.
        // We simulate this by constructing entries manually with the same HLC.
        let (id_a, sk_a) = make_keypair();
        let (id_b, sk_b) = make_keypair();
        let (target, _) = make_keypair();

        // Determine which device ID is "higher".
        let (high_id, high_sk, low_id, low_sk) = if id_a > id_b {
            (id_a, sk_a, id_b, sk_b)
        } else {
            (id_b, sk_b, id_a, sk_a)
        };

        let hlc = 1000u64;

        // Both create a DeviceNameChanged with the same HLC.
        let entry_low = DagEntry::new_signed(
            hlc,
            low_id,
            Action::DeviceNameChanged {
                device_id: target,
                name: "ByLow".to_string(),
            },
            vec![],
            &low_sk,
        );
        let entry_high = DagEntry::new_signed(
            hlc,
            high_id,
            Action::DeviceNameChanged {
                device_id: target,
                name: "ByHigh".to_string(),
            },
            vec![],
            &high_sk,
        );

        // Load into a fresh dag — the state applies entries in order loaded,
        // but LWW with tiebreak means higher DeviceId should win.
        let (viewer_id, viewer_sk) = make_keypair();
        let mut dag = Dag::new(viewer_id, viewer_sk);

        // First approve the target so it exists.
        dag.append(Action::DeviceApproved {
            device_id: target,
            role: DeviceRole::Full,
        });

        // Load both entries (low first, then high).
        dag.load_entry(entry_low).unwrap();
        dag.load_entry(entry_high).unwrap();

        assert_eq!(dag.state().devices[&target].name, "ByHigh");
    }

    // --- Snapshot ---

    #[test]
    fn test_snapshot_entry() {
        let mut dag = make_dag();
        let entry = dag.append(Action::Snapshot {
            state_hash: [0xab; 32],
        });
        assert!(entry.verify_hash().is_ok());
        let bytes = entry.to_bytes();
        let decoded = DagEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry, decoded);
    }

    // --- Edge cases ---

    #[test]
    fn test_empty_dag() {
        let dag = make_dag();
        assert!(dag.is_empty());
        assert_eq!(dag.tips().len(), 0);
    }

    #[test]
    fn test_receive_duplicate_entry() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let entry = dag_a.append(Action::Merge);
        dag_b.receive_entry(entry.clone()).unwrap();
        // Receiving again should succeed (idempotent).
        dag_b.receive_entry(entry).unwrap();
        assert_eq!(dag_b.len(), 1);
    }

    #[test]
    fn test_apply_sync_entries_batch() {
        let mut dag_a = make_dag();
        let mut dag_b = make_dag();

        let e1 = dag_a.append(Action::Merge);
        let e2 = dag_a.append(Action::Merge);
        let e3 = dag_a.append(Action::Merge);

        // Apply all at once (in scrambled order).
        let new = dag_b
            .apply_sync_entries(vec![e3.clone(), e1.clone(), e2.clone()])
            .unwrap();
        assert_eq!(new.len(), 3);
        assert_eq!(dag_b.len(), 3);
    }

    #[test]
    fn test_delta_empty_when_synced() {
        let (id_a, sk_a) = make_keypair();
        let (id_b, sk_b) = make_keypair();

        let mut dag_a = Dag::new(id_a, sk_a);
        let mut dag_b = Dag::new(id_b, sk_b);

        let e1 = dag_a.append(Action::Merge);
        dag_b.receive_entry(e1).unwrap();

        // Both have the same entries — delta should be empty.
        let delta = dag_a.compute_delta(dag_b.tips());
        assert!(delta.is_empty());
    }

    #[test]
    fn test_device_join_request_in_state() {
        let mut dag = make_dag();
        let (new_id, _) = make_keypair();

        dag.append(Action::DeviceJoinRequest {
            device_id: new_id,
            name: "Phone".to_string(),
        });

        // Join request creates a pending (not approved) device.
        let info = &dag.state().devices[&new_id];
        assert!(!info.approved);
        assert_eq!(info.name, "Phone");
    }
}
