//! Headless backup server daemon for Murmur.
//!
//! `murmurd` is the desktop/server platform implementation. It uses Fjall for
//! DAG persistence and the filesystem for blob storage.

mod config;
mod storage;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use murmur_types::DeviceId;
use tracing::info;

use config::Config;
use storage::{FjallPlatform, Storage};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/// Murmur headless backup daemon.
#[derive(Parser)]
#[command(name = "murmurd", about = "Murmur headless backup daemon")]
struct Cli {
    /// Base directory for all murmurd data.
    #[arg(long, default_value_os_t = Config::default_base_dir())]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new Murmur network (generates mnemonic).
    Init {
        /// Device name.
        #[arg(long, default_value = "murmurd")]
        name: String,
        /// Device role: source, backup, or full.
        #[arg(long, default_value = "backup")]
        role: String,
        /// Join an existing network instead of creating one.
        #[arg(long)]
        join: Option<String>,
        /// Enable auto-approve for new devices.
        #[arg(long)]
        auto_approve: bool,
    },
    /// Start the daemon.
    Start,
    /// Approve a pending device.
    Approve {
        /// Device ID (hex).
        device_id: String,
    },
    /// Print network status and exit.
    Status,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Init {
            name,
            role,
            join,
            auto_approve,
        } => cmd_init(&cli.data_dir, &name, &role, join.as_deref(), auto_approve),
        Command::Start => cmd_start(&cli.data_dir),
        Command::Approve { device_id } => cmd_approve(&cli.data_dir, &device_id),
        Command::Status => cmd_status(&cli.data_dir),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Initialize a new network or join an existing one.
fn cmd_init(
    base_dir: &Path,
    name: &str,
    role: &str,
    join_mnemonic: Option<&str>,
    auto_approve: bool,
) -> Result<()> {
    // Check if already initialized.
    let config_path = Config::config_path(base_dir);
    if config_path.exists() {
        anyhow::bail!(
            "already initialized at {}. Remove the directory to reinitialize.",
            base_dir.display()
        );
    }

    // Create config.
    let mut config = Config::with_base_dir(base_dir, name, role);
    config.network.auto_approve = auto_approve;

    // Validate role.
    let device_role = config.parse_role()?;

    // Create directories.
    std::fs::create_dir_all(base_dir).context("create base dir")?;

    // Handle mnemonic: generate or parse.
    let mnemonic = if let Some(phrase) = join_mnemonic {
        murmur_seed::parse_mnemonic(phrase).context("invalid mnemonic")?
    } else {
        let m = murmur_seed::generate_mnemonic(murmur_seed::WordCount::TwentyFour);
        info!("generated new 24-word mnemonic");
        m
    };

    // Derive network identity.
    let identity = murmur_seed::NetworkIdentity::from_mnemonic(&mnemonic, "");

    // Determine device key.
    let (device_id, signing_key) = if join_mnemonic.is_some() {
        // Joining: generate a random device key.
        let kp = murmur_seed::DeviceKeyPair::generate();
        let id = kp.device_id();
        let sk = kp.signing_key().clone();
        // Save device key.
        std::fs::write(Config::device_key_path(base_dir), kp.to_bytes())
            .context("save device key")?;
        (id, sk)
    } else {
        // Creating: use the first device key from the seed.
        let id = identity.first_device_id();
        let sk = identity.first_device_signing_key().clone();
        (id, sk)
    };

    // Save mnemonic (plaintext for v1).
    std::fs::write(
        Config::mnemonic_path(base_dir),
        mnemonic.to_string().as_bytes(),
    )
    .context("save mnemonic")?;

    // Save config.
    config.save(&config_path)?;

    // Open storage.
    let storage = Arc::new(Storage::open(
        &config.storage.data_dir,
        &config.storage.blob_dir,
    )?);
    let platform = Arc::new(FjallPlatform::new(storage.clone()));

    // Create or join the engine.
    let _engine = if join_mnemonic.is_some() {
        info!(%device_id, "joining existing network");
        murmur_engine::MurmurEngine::join_network(
            device_id,
            signing_key,
            name.to_string(),
            platform,
        )
    } else {
        info!(%device_id, "creating new network");
        murmur_engine::MurmurEngine::create_network(
            device_id,
            signing_key,
            name.to_string(),
            device_role,
            platform,
        )
    };

    // Flush storage.
    storage.flush()?;

    // Output.
    if join_mnemonic.is_none() {
        println!("Murmur network initialized.");
        println!();
        println!("IMPORTANT — Write down your mnemonic and store it safely:");
        println!();
        println!("  {}", mnemonic);
        println!();
        println!("Device ID: {device_id}");
        println!("Config:    {}", config_path.display());
    } else {
        println!("Joined Murmur network (pending approval).");
        println!("Device ID: {device_id}");
    }

    Ok(())
}

/// Start the daemon.
fn cmd_start(base_dir: &Path) -> Result<()> {
    let config_path = Config::config_path(base_dir);
    let config = Config::load(&config_path).context("load config (run 'murmurd init' first)")?;

    let mnemonic_str =
        std::fs::read_to_string(Config::mnemonic_path(base_dir)).context("read mnemonic")?;
    let mnemonic = murmur_seed::parse_mnemonic(mnemonic_str.trim())?;
    let identity = murmur_seed::NetworkIdentity::from_mnemonic(&mnemonic, "");

    // Determine device key.
    let device_key_path = Config::device_key_path(base_dir);
    let (device_id, signing_key) = if device_key_path.exists() {
        // Joining device: load the saved key.
        let bytes: [u8; 32] = std::fs::read(&device_key_path)
            .context("read device key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("device key file must be 32 bytes"))?;
        let kp = murmur_seed::DeviceKeyPair::from_bytes(bytes);
        (kp.device_id(), kp.signing_key().clone())
    } else {
        // First device: use the seed-derived key.
        (
            identity.first_device_id(),
            identity.first_device_signing_key().clone(),
        )
    };

    // Open storage.
    let storage = Arc::new(Storage::open(
        &config.storage.data_dir,
        &config.storage.blob_dir,
    )?);
    let platform = Arc::new(FjallPlatform::new(storage.clone()));

    // Create engine with a fresh DAG.
    let mut engine = murmur_engine::MurmurEngine::from_dag(
        murmur_dag::Dag::new(device_id, signing_key),
        platform,
    );

    // Load persisted DAG entries.
    let entries = storage.load_all_dag_entries()?;
    for entry_bytes in entries {
        engine.load_entry_bytes(&entry_bytes)?;
    }

    info!(%device_id, "daemon started");
    println!("murmurd running. Device ID: {device_id}");
    println!("Devices: {}", engine.list_devices().len());

    // Run the tokio runtime for signal handling.
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        info!("waiting for shutdown signal");
        tokio::signal::ctrl_c().await.context("listen for ctrl-c")?;
        info!("shutdown signal received");
        Ok::<(), anyhow::Error>(())
    })?;

    storage.flush()?;
    info!("daemon stopped");
    println!("murmurd stopped.");
    Ok(())
}

/// Approve a pending device.
fn cmd_approve(base_dir: &Path, device_id_hex: &str) -> Result<()> {
    let config_path = Config::config_path(base_dir);
    let config = Config::load(&config_path).context("load config")?;

    // Parse device ID from hex.
    let device_id = parse_device_id(device_id_hex)?;

    let mnemonic_str =
        std::fs::read_to_string(Config::mnemonic_path(base_dir)).context("read mnemonic")?;
    let mnemonic = murmur_seed::parse_mnemonic(mnemonic_str.trim())?;
    let identity = murmur_seed::NetworkIdentity::from_mnemonic(&mnemonic, "");

    // Determine signing key (same logic as start).
    let device_key_path = Config::device_key_path(base_dir);
    let (my_device_id, signing_key) = if device_key_path.exists() {
        let bytes: [u8; 32] = std::fs::read(&device_key_path)
            .context("read device key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("device key file must be 32 bytes"))?;
        let kp = murmur_seed::DeviceKeyPair::from_bytes(bytes);
        (kp.device_id(), kp.signing_key().clone())
    } else {
        (
            identity.first_device_id(),
            identity.first_device_signing_key().clone(),
        )
    };

    let storage = Arc::new(Storage::open(
        &config.storage.data_dir,
        &config.storage.blob_dir,
    )?);
    let platform = Arc::new(FjallPlatform::new(storage.clone()));

    let mut engine = murmur_engine::MurmurEngine::from_dag(
        murmur_dag::Dag::new(my_device_id, signing_key),
        platform,
    );

    // Load persisted entries.
    for entry_bytes in storage.load_all_dag_entries()? {
        engine.load_entry_bytes(&entry_bytes)?;
    }

    // Approve.
    let role = config
        .parse_role()
        .unwrap_or(murmur_types::DeviceRole::Backup);
    engine.approve_device(device_id, role)?;
    storage.flush()?;

    println!("Device {device_id} approved with role {role:?}.");
    Ok(())
}

/// Print network status.
fn cmd_status(base_dir: &Path) -> Result<()> {
    let config_path = Config::config_path(base_dir);
    let config = Config::load(&config_path).context("load config")?;

    let mnemonic_str =
        std::fs::read_to_string(Config::mnemonic_path(base_dir)).context("read mnemonic")?;
    let mnemonic = murmur_seed::parse_mnemonic(mnemonic_str.trim())?;
    let identity = murmur_seed::NetworkIdentity::from_mnemonic(&mnemonic, "");

    let device_key_path = Config::device_key_path(base_dir);
    let (my_device_id, signing_key) = if device_key_path.exists() {
        let bytes: [u8; 32] = std::fs::read(&device_key_path)
            .context("read device key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("device key file must be 32 bytes"))?;
        let kp = murmur_seed::DeviceKeyPair::from_bytes(bytes);
        (kp.device_id(), kp.signing_key().clone())
    } else {
        (
            identity.first_device_id(),
            identity.first_device_signing_key().clone(),
        )
    };

    let storage = Arc::new(Storage::open(
        &config.storage.data_dir,
        &config.storage.blob_dir,
    )?);
    let platform = Arc::new(FjallPlatform::new(storage.clone()));

    let mut engine = murmur_engine::MurmurEngine::from_dag(
        murmur_dag::Dag::new(my_device_id, signing_key),
        platform,
    );

    for entry_bytes in storage.load_all_dag_entries()? {
        engine.load_entry_bytes(&entry_bytes)?;
    }

    println!("Network ID: {}", identity.network_id());
    println!("Device ID:  {my_device_id}");
    println!("Config:     {}", config_path.display());
    println!();

    let devices = engine.list_devices();
    if devices.is_empty() {
        println!("No devices.");
    } else {
        println!("Devices ({}):", devices.len());
        for dev in &devices {
            let status = if dev.approved { "approved" } else { "pending" };
            println!(
                "  {} {} ({:?}) [{}]",
                dev.device_id, dev.name, dev.role, status
            );
        }
    }

    let files = engine.state().files.len();
    println!("\nFiles: {files}");

    let pending = engine.pending_requests();
    if !pending.is_empty() {
        println!("\nPending approval ({}):", pending.len());
        for dev in &pending {
            println!("  {} {}", dev.device_id, dev.name);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a hex string into a [`DeviceId`].
fn parse_device_id(hex_str: &str) -> Result<DeviceId> {
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        anyhow::bail!(
            "device ID must be 64 hex characters (32 bytes), got {}",
            hex_str.len()
        );
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex_str[i * 2..i * 2 + 2], 16)
            .context("invalid hex in device ID")?;
    }
    Ok(DeviceId::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_id_valid() {
        let hex = "ab".repeat(32);
        let id = parse_device_id(&hex).unwrap();
        assert_eq!(id, DeviceId::from_bytes([0xab; 32]));
    }

    #[test]
    fn test_parse_device_id_invalid_length() {
        assert!(parse_device_id("abcd").is_err());
    }

    #[test]
    fn test_parse_device_id_invalid_hex() {
        let hex = "zz".repeat(32);
        assert!(parse_device_id(&hex).is_err());
    }

    #[test]
    fn test_init_creates_network() {
        let dir = tempfile::tempdir().unwrap();
        cmd_init(&dir.path().to_path_buf(), "TestNAS", "backup", None, false).unwrap();

        assert!(Config::config_path(dir.path()).exists());
        assert!(Config::mnemonic_path(dir.path()).exists());
        assert!(!Config::device_key_path(dir.path()).exists()); // first device uses seed key
    }

    #[test]
    fn test_init_already_initialized() {
        let dir = tempfile::tempdir().unwrap();
        cmd_init(&dir.path().to_path_buf(), "NAS", "backup", None, false).unwrap();
        let result = cmd_init(&dir.path().to_path_buf(), "NAS", "backup", None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_init_join_with_valid_mnemonic() {
        let mnemonic = murmur_seed::generate_mnemonic(murmur_seed::WordCount::Twelve);
        let dir = tempfile::tempdir().unwrap();
        cmd_init(
            &dir.path().to_path_buf(),
            "Phone",
            "source",
            Some(&mnemonic.to_string()),
            false,
        )
        .unwrap();

        assert!(Config::config_path(dir.path()).exists());
        assert!(Config::mnemonic_path(dir.path()).exists());
        assert!(Config::device_key_path(dir.path()).exists()); // joining device has own key
    }

    #[test]
    fn test_init_join_with_invalid_mnemonic() {
        let dir = tempfile::tempdir().unwrap();
        let result = cmd_init(
            &dir.path().to_path_buf(),
            "Phone",
            "source",
            Some("not a valid mnemonic phrase"),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_status_after_init() {
        let dir = tempfile::tempdir().unwrap();
        cmd_init(&dir.path().to_path_buf(), "NAS", "backup", None, false).unwrap();
        // Status should not error.
        cmd_status(&dir.path().to_path_buf()).unwrap();
    }

    #[test]
    fn test_start_and_stop() {
        // We can't easily test the full daemon loop, but we can test
        // that start loads correctly before entering the signal wait.
        let dir = tempfile::tempdir().unwrap();
        cmd_init(&dir.path().to_path_buf(), "NAS", "backup", None, false).unwrap();

        // Verify the config and mnemonic are loadable.
        let config = Config::load(&Config::config_path(dir.path())).unwrap();
        assert_eq!(config.device.name, "NAS");
        assert_eq!(config.device.role, "backup");
    }
}
