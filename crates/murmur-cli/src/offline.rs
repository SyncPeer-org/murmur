//! Offline commands that work without a running daemon.
//!
//! `join` sets up configuration files for joining an existing network.
//! Network creation is handled automatically by `murmurd` on first run.
//!
//! Also hosts the built-in folder-template map used by `folder create
//! --template <name>`.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Config types (shared with murmurd)
// ---------------------------------------------------------------------------

/// Top-level configuration (matches murmurd's config format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Device configuration.
    pub device: DeviceConfig,
    /// Storage paths.
    pub storage: StorageConfig,
    /// Network behaviour.
    #[serde(default)]
    pub network: NetworkConfig,
}

/// Device identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Human-readable device name.
    pub name: String,
}

/// Paths for persistent storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for content-addressed blobs.
    pub blob_dir: std::path::PathBuf,
    /// Directory for Fjall database (DAG persistence).
    pub data_dir: std::path::PathBuf,
}

/// Network behaviour options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Automatically approve new devices.
    #[serde(default)]
    pub auto_approve: bool,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn config_path(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join("config.toml")
}

fn mnemonic_path(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join("mnemonic")
}

fn device_key_path(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join("device.key")
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Join an existing Murmur network.
pub fn cmd_join(base_dir: &Path, mnemonic_str: &str, name: &str) -> Result<()> {
    let cfg_path = config_path(base_dir);
    if cfg_path.exists() {
        anyhow::bail!(
            "already initialized at {}. Remove the directory to reinitialize.",
            base_dir.display()
        );
    }

    // Validate mnemonic.
    let mnemonic = murmur_seed::parse_mnemonic(mnemonic_str).context("invalid mnemonic")?;

    std::fs::create_dir_all(base_dir).context("create base directory")?;

    // Save mnemonic.
    std::fs::write(mnemonic_path(base_dir), mnemonic.to_string().as_bytes())
        .context("save mnemonic")?;

    // Generate random device key for joining device.
    let kp = murmur_seed::DeviceKeyPair::generate();
    let device_id = kp.device_id();
    std::fs::write(device_key_path(base_dir), kp.to_bytes()).context("save device key")?;

    // Write config.
    let config = Config {
        device: DeviceConfig {
            name: name.to_string(),
        },
        storage: StorageConfig {
            blob_dir: base_dir.join("blobs"),
            data_dir: base_dir.join("db"),
        },
        network: NetworkConfig::default(),
    };

    let toml_str = toml::to_string_pretty(&config).context("serialize config")?;
    std::fs::write(&cfg_path, toml_str).context("write config")?;

    println!("Joined Murmur network (pending approval).");
    println!("Device ID: {device_id}");
    println!();
    println!(
        "Start the daemon with: murmurd --data-dir {}",
        base_dir.display()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Folder templates
// ---------------------------------------------------------------------------
//
// Thin re-export of `murmur_ipc::templates` so both the CLI and the desktop
// app ship identical patterns. Kept here as `pub` so the existing CLI
// call-sites (`offline::template_patterns`, `offline::TEMPLATES`) keep
// working without a larger refactor.
pub use murmur_ipc::templates::{TEMPLATES, template_patterns};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_valid_mnemonic() {
        let mnemonic = murmur_seed::generate_mnemonic(murmur_seed::WordCount::Twelve);
        let dir = tempfile::tempdir().unwrap();
        cmd_join(dir.path(), &mnemonic.to_string(), "Phone").unwrap();

        assert!(config_path(dir.path()).exists());
        assert!(mnemonic_path(dir.path()).exists());
        // Joining device has its own key.
        assert!(device_key_path(dir.path()).exists());
    }

    #[test]
    fn test_join_invalid_mnemonic() {
        let dir = tempfile::tempdir().unwrap();
        let result = cmd_join(dir.path(), "not a valid mnemonic", "Phone");
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_reexports_templates() {
        // Sanity check: the CLI re-export surfaces the same map as murmur-ipc.
        assert_eq!(TEMPLATES, murmur_ipc::templates::TEMPLATES);
        assert_eq!(
            template_patterns("rust"),
            murmur_ipc::templates::template_patterns("rust")
        );
    }
}
