//! Storage backend for murmurd: Fjall for DAG entries, filesystem for blobs.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use murmur_types::BlobHash;
use tracing::{debug, info};

/// The keyspace name for DAG entries within the Fjall database.
const DAG_KEYSPACE: &str = "dag_entries";

/// Persistent storage backed by Fjall (DAG) and filesystem (blobs).
pub struct Storage {
    /// Fjall database.
    db: Database,
    /// Keyspace for DAG entries (key = entry hash, value = serialized bytes).
    dag_ks: Keyspace,
    /// Directory for content-addressed blob files.
    blob_dir: std::path::PathBuf,
}

impl Storage {
    /// Open or create storage at the given paths.
    pub fn open(data_dir: &Path, blob_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).context("create data_dir")?;
        std::fs::create_dir_all(blob_dir).context("create blob_dir")?;

        let db = Database::builder(data_dir)
            .open()
            .context("open fjall database")?;

        let dag_ks = db
            .keyspace(DAG_KEYSPACE, KeyspaceCreateOptions::default)
            .context("open dag_entries keyspace")?;

        info!(?data_dir, ?blob_dir, "storage opened");

        Ok(Self {
            db,
            dag_ks,
            blob_dir: blob_dir.to_path_buf(),
        })
    }

    /// Persist a DAG entry (keyed by its hash, first 32 bytes of entry_bytes).
    pub fn persist_dag_entry(&self, entry_bytes: &[u8]) -> Result<()> {
        if entry_bytes.len() < 32 {
            anyhow::bail!("entry_bytes too short to contain hash");
        }
        // The hash is the first field in the postcard-serialized DagEntry.
        // We use the first 32 bytes as the key for content-addressable lookup.
        let hash = &entry_bytes[..32];
        self.dag_ks
            .insert(hash, entry_bytes)
            .context("persist dag entry")?;
        debug!(hash = hex_short(hash), "dag entry persisted");
        Ok(())
    }

    /// Load all persisted DAG entries (for feeding into the engine on startup).
    pub fn load_all_dag_entries(&self) -> Result<Vec<Vec<u8>>> {
        let mut entries = Vec::new();
        for guard in self.dag_ks.iter() {
            let (_key, value) = guard.into_inner().context("iterate dag entries")?;
            entries.push(value.to_vec());
        }
        info!(count = entries.len(), "loaded dag entries from storage");
        Ok(entries)
    }

    /// Store a blob to the filesystem (content-addressed).
    pub fn store_blob(&self, blob_hash: BlobHash, data: &[u8]) -> Result<()> {
        let path = self.blob_path(blob_hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("create blob parent dir")?;
        }
        std::fs::write(&path, data).context("write blob")?;
        debug!(%blob_hash, "blob stored");
        Ok(())
    }

    /// Load a blob from the filesystem.
    pub fn load_blob(&self, blob_hash: BlobHash) -> Result<Option<Vec<u8>>> {
        let path = self.blob_path(blob_hash);
        if path.exists() {
            let data = std::fs::read(&path).context("read blob")?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Verify a blob's integrity after reading.
    pub fn load_blob_verified(&self, blob_hash: BlobHash) -> Result<Option<Vec<u8>>> {
        match self.load_blob(blob_hash)? {
            Some(data) => {
                let actual = BlobHash::from_data(&data);
                if actual != blob_hash {
                    anyhow::bail!(
                        "blob integrity check failed: expected {blob_hash}, got {actual}"
                    );
                }
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /// Flush the Fjall database to ensure durability.
    pub fn flush(&self) -> Result<()> {
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    /// Content-addressed blob path: `blob_dir/ab/cd/<full_hex>`.
    fn blob_path(&self, blob_hash: BlobHash) -> std::path::PathBuf {
        let hex = blob_hash.to_string();
        self.blob_dir.join(&hex[..2]).join(&hex[2..4]).join(&hex)
    }
}

/// Short hex string for logging.
fn hex_short(bytes: &[u8]) -> String {
    let full: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if full.len() > 16 {
        format!("{}…", &full[..16])
    } else {
        full
    }
}

/// Platform callbacks implementation backed by [`Storage`].
pub struct FjallPlatform {
    storage: Arc<Storage>,
}

impl FjallPlatform {
    /// Create a new platform callbacks instance.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl murmur_engine::PlatformCallbacks for FjallPlatform {
    fn on_dag_entry(&self, entry_bytes: Vec<u8>) {
        if let Err(e) = self.storage.persist_dag_entry(&entry_bytes) {
            tracing::error!(error = %e, "failed to persist DAG entry");
        }
    }

    fn on_blob_received(&self, blob_hash: BlobHash, data: Vec<u8>) {
        if let Err(e) = self.storage.store_blob(blob_hash, &data) {
            tracing::error!(error = %e, %blob_hash, "failed to store blob");
        }
    }

    fn on_blob_needed(&self, blob_hash: BlobHash) -> Option<Vec<u8>> {
        match self.storage.load_blob_verified(blob_hash) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!(error = %e, %blob_hash, "failed to load blob");
                None
            }
        }
    }

    fn on_event(&self, event: murmur_engine::EngineEvent) {
        info!(?event, "engine event");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_open_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("db");
        let blob_dir = dir.path().join("blobs");

        let _storage = Storage::open(&data_dir, &blob_dir).unwrap();

        assert!(data_dir.exists());
        assert!(blob_dir.exists());
    }

    #[test]
    fn test_blob_store_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();

        let data = b"hello world";
        let hash = BlobHash::from_data(data);

        storage.store_blob(hash, data).unwrap();
        let loaded = storage.load_blob(hash).unwrap().unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_blob_load_verified() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();

        let data = b"verified content";
        let hash = BlobHash::from_data(data);

        storage.store_blob(hash, data).unwrap();
        let loaded = storage.load_blob_verified(hash).unwrap().unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_blob_load_missing() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();

        let hash = BlobHash::from_data(b"nonexistent");
        assert!(storage.load_blob(hash).unwrap().is_none());
    }

    #[test]
    fn test_dag_entry_persist_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();

        use ed25519_dalek::SigningKey;
        use murmur_dag::Dag;
        use murmur_types::{Action, DeviceId, DeviceRole};
        use rand::rngs::OsRng;

        let sk = SigningKey::generate(&mut OsRng);
        let device_id = DeviceId::from_verifying_key(&sk.verifying_key());
        let mut dag = Dag::new(device_id, sk);

        let entry = dag.append(Action::DeviceApproved {
            device_id,
            role: DeviceRole::Backup,
        });
        let entry_bytes = entry.to_bytes();

        storage.persist_dag_entry(&entry_bytes).unwrap();
        storage.flush().unwrap();

        let all = storage.load_all_dag_entries().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], entry_bytes);
    }

    #[test]
    fn test_dag_persist_multiple_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();

        use ed25519_dalek::SigningKey;
        use murmur_dag::Dag;
        use murmur_types::{Action, DeviceId, DeviceRole};
        use rand::rngs::OsRng;

        let sk = SigningKey::generate(&mut OsRng);
        let device_id = DeviceId::from_verifying_key(&sk.verifying_key());
        let mut dag = Dag::new(device_id, sk);

        let e1 = dag.append(Action::DeviceApproved {
            device_id,
            role: DeviceRole::Backup,
        });
        let e2 = dag.append(Action::DeviceNameChanged {
            device_id,
            name: "NAS".to_string(),
        });

        storage.persist_dag_entry(&e1.to_bytes()).unwrap();
        storage.persist_dag_entry(&e2.to_bytes()).unwrap();
        storage.flush().unwrap();

        // Drop the first storage to release the lock before reopening.
        drop(storage);

        // Reopen storage and reload.
        let storage2 = Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap();
        let all = storage2.load_all_dag_entries().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_platform_callbacks_dag_entry() {
        let dir = tempfile::tempdir().unwrap();
        let storage =
            Arc::new(Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap());
        let platform = FjallPlatform::new(storage.clone());

        use ed25519_dalek::SigningKey;
        use murmur_dag::Dag;
        use murmur_types::{Action, DeviceId, DeviceRole};
        use rand::rngs::OsRng;

        let sk = SigningKey::generate(&mut OsRng);
        let device_id = DeviceId::from_verifying_key(&sk.verifying_key());
        let mut dag = Dag::new(device_id, sk);

        let entry = dag.append(Action::DeviceApproved {
            device_id,
            role: DeviceRole::Backup,
        });

        use murmur_engine::PlatformCallbacks;
        platform.on_dag_entry(entry.to_bytes());
        storage.flush().unwrap();

        let all = storage.load_all_dag_entries().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_platform_callbacks_blob() {
        let dir = tempfile::tempdir().unwrap();
        let storage =
            Arc::new(Storage::open(&dir.path().join("db"), &dir.path().join("blobs")).unwrap());
        let platform = FjallPlatform::new(storage.clone());

        let data = b"blob data for callback test";
        let hash = BlobHash::from_data(data);

        use murmur_engine::PlatformCallbacks;
        platform.on_blob_received(hash, data.to_vec());

        let loaded = platform.on_blob_needed(hash);
        assert_eq!(loaded, Some(data.to_vec()));
    }
}
