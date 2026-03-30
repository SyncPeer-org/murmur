//! Murmur desktop application — thin IPC client to murmurd.
//!
//! Built with [`iced`](https://iced.rs), a pure-Rust cross-platform UI toolkit.
//! All state is fetched from `murmurd` via Unix socket IPC. The desktop app
//! does not embed any engine, storage, or networking.

mod ipc;

use std::path::PathBuf;
use std::sync::Mutex;

use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};

use murmur_ipc::{
    CliRequest, CliResponse, ConflictInfoIpc, DeviceInfoIpc, DevicePresenceIpc, FileInfoIpc,
    FileVersionIpc, FolderInfoIpc, FolderSubscriberIpc, NetworkFolderInfoIpc, PeerInfoIpc,
};

const MAX_EVENT_LOG: usize = 500;

/// Global handle to the daemon child so we can kill it on exit.
///
/// Used by both the `atexit` handler (covers `std::process::exit()`) and
/// signal handlers (covers SIGINT/SIGTERM).
static DAEMON_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);

/// Kill the daemon child and exit. Called from atexit and signal handlers.
fn kill_daemon_child() {
    if let Ok(mut guard) = DAEMON_CHILD.lock()
        && let Some(ref mut child) = *guard
    {
        let pid = child.id() as i32;
        unsafe { libc::kill(pid, libc::SIGTERM) };
        for _ in 0..20 {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => break,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

extern "C" fn on_exit() {
    kill_daemon_child();
}

extern "C" fn on_signal(sig: libc::c_int) {
    kill_daemon_child();
    // Re-raise with default handler so the process actually exits.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Kill the daemon on any exit path:
    // - atexit: covers std::process::exit() (iced window close)
    // - SIGINT: covers Ctrl+C in terminal
    // - SIGTERM: covers `kill <pid>` or system shutdown
    // - SIGHUP: covers terminal close
    unsafe {
        libc::atexit(on_exit);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, on_signal as *const () as libc::sighandler_t);
    }

    iced::application(App::new, App::update, App::view)
        .title("Murmur")
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size(iced::Size::new(960.0, 640.0))
        .run()
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    DaemonCheck,
    Setup,
    Folders,
    FolderDetail,
    Conflicts,
    FileHistory,
    Devices,
    Status,
    RecentFiles,
    Settings,
    NetworkHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SetupStep {
    ChooseMode,
    Form,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortField {
    Name,
    Size,
    Type,
}

struct App {
    screen: Screen,
    socket_path: PathBuf,
    daemon_running: Option<bool>,
    daemon_error: Option<String>,
    /// True while a launch-and-poll task is in flight; prevents double-spawn.
    daemon_launching: bool,
    setup_step: SetupStep,
    device_name: String,
    mnemonic_input: String,
    join_mode: bool,
    setup_error: Option<String>,
    status_device_id: String,
    status_device_name: String,
    status_network_id: String,
    status_peer_count: u64,
    status_dag_entries: u64,
    status_uptime_secs: u64,
    folders: Vec<FolderInfoIpc>,
    network_folders: Vec<NetworkFolderInfoIpc>,
    selected_folder: Option<FolderInfoIpc>,
    folder_files: Vec<FileInfoIpc>,
    folder_subscribers: Vec<FolderSubscriberIpc>,
    folder_paused: bool,
    conflicts: Vec<ConflictInfoIpc>,
    history_folder_id: String,
    history_path: String,
    history_versions: Vec<FileVersionIpc>,
    devices: Vec<DeviceInfoIpc>,
    pending: Vec<DeviceInfoIpc>,
    device_presence: Vec<DevicePresenceIpc>,
    sync_paused: bool,
    search_query: String,
    sort_field: SortField,
    sort_ascending: bool,
    event_log: Vec<String>,
    // Settings (M26a)
    leave_network_confirm: bool,
    cfg_auto_approve: bool,
    cfg_mdns: bool,
    cfg_upload_throttle: u64,
    cfg_download_throttle: u64,
    settings_toast: Option<String>,
    // Folder settings
    folder_ignore_patterns: String,
    // Folder rename state
    renaming_folder_id: Option<String>,
    rename_input: String,
    // Diagnostics (M27a)
    peers: Vec<PeerInfoIpc>,
    storage_stats: Option<StorageStatsCache>,
    connectivity_result: Option<(bool, Option<u64>)>,
}

/// Cached storage stats for display.
#[derive(Debug, Clone)]
struct StorageStatsCache {
    total_blob_count: u64,
    total_blob_bytes: u64,
    orphaned_blob_count: u64,
    orphaned_blob_bytes: u64,
    dag_entry_count: u64,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let socket_path = murmur_ipc::default_socket_path();
        let app = Self {
            screen: Screen::DaemonCheck,
            socket_path,
            daemon_running: None,
            daemon_error: None,
            daemon_launching: false,
            setup_step: SetupStep::ChooseMode,
            device_name: String::new(),
            mnemonic_input: String::new(),
            join_mode: false,
            setup_error: None,
            status_device_id: String::new(),
            status_device_name: String::new(),
            status_network_id: String::new(),
            status_peer_count: 0,
            status_dag_entries: 0,
            status_uptime_secs: 0,
            folders: Vec::new(),
            network_folders: Vec::new(),
            selected_folder: None,
            folder_files: Vec::new(),
            folder_subscribers: Vec::new(),
            folder_paused: false,
            conflicts: Vec::new(),
            history_folder_id: String::new(),
            history_path: String::new(),
            history_versions: Vec::new(),
            devices: Vec::new(),
            pending: Vec::new(),
            device_presence: Vec::new(),
            sync_paused: false,
            search_query: String::new(),
            sort_field: SortField::Name,
            sort_ascending: true,
            event_log: Vec::new(),
            leave_network_confirm: false,
            cfg_auto_approve: false,
            cfg_mdns: false,
            cfg_upload_throttle: 0,
            cfg_download_throttle: 0,
            settings_toast: None,
            folder_ignore_patterns: String::new(),
            renaming_folder_id: None,
            rename_input: String::new(),
            peers: Vec::new(),
            storage_stats: None,
            connectivity_result: None,
        };
        let path = app.socket_path.clone();
        (
            app,
            Task::perform(ipc::daemon_is_running(path), Message::DaemonCheckResult),
        )
    }
}

impl App {
    fn push_event(&mut self, entry: String) {
        if self.event_log.len() >= MAX_EVENT_LOG {
            self.event_log.remove(0);
        }
        self.event_log.push(entry);
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Message {
    DaemonCheckResult(bool),
    DaemonConnected,
    /// Launch-and-poll completed: Ok(()) means socket is ready, Err has details.
    DaemonLaunchResult(Result<(), String>),
    RetryDaemonCheck,
    SetupChooseCreate,
    SetupChooseJoin,
    SetupBack,
    DeviceNameChanged(String),
    MnemonicInputChanged(String),
    StartDaemon,
    Navigate(Screen),
    GotStatus(Result<CliResponse, String>),
    GotFolders(Result<CliResponse, String>),
    GotNetworkFolders(Result<CliResponse, String>),
    GotFolderFiles(Result<CliResponse, String>),
    GotFolderSubscribers(Result<CliResponse, String>),
    GotConflicts(Result<CliResponse, String>),
    GotDevices(Result<CliResponse, String>),
    GotPending(Result<CliResponse, String>),
    GotDevicePresence(Result<CliResponse, String>),
    GotFileHistory(Result<CliResponse, String>),
    GotGeneric(Result<CliResponse, String>),
    GotConfig(Result<CliResponse, String>),
    GotIgnorePatterns(Result<CliResponse, String>),
    GotPeers(Result<CliResponse, String>),
    GotStorageStats(Result<CliResponse, String>),
    GotConnectivity(Result<CliResponse, String>),
    GotReclaim(Result<CliResponse, String>),
    CreateFolderFromPicker,
    PickedNewFolder(Option<PathBuf>),
    /// User wants to subscribe — open directory picker first.
    SubscribeFolder(String, String),
    /// Directory picker returned a path for subscribing.
    PickedFolderPath(String, String, Option<PathBuf>),
    UnsubscribeFolder(String),
    SelectFolder(FolderInfoIpc),
    ResolveConflict {
        folder_id: String,
        path: String,
        chosen_hash: String,
    },
    DismissConflict {
        folder_id: String,
        path: String,
    },
    BulkResolve {
        folder_id: String,
        strategy: String,
    },
    ViewFileHistory {
        folder_id: String,
        path: String,
    },
    RestoreVersion {
        folder_id: String,
        path: String,
        blob_hash: String,
    },
    DeleteFile {
        folder_id: String,
        path: String,
    },
    StartRenameFolder(String, String),
    RenameInputChanged(String),
    SubmitRenameFolder,
    CancelRenameFolder,
    SearchQueryChanged(String),
    SortBy(SortField),
    ApproveDevice(String),
    ToggleGlobalSync,
    ToggleFolderSync(String),
    // Settings (M26a)
    ToggleAutoApprove,
    ToggleMdns,
    SetThrottle(u64, u64),
    ReclaimOrphanedBlobs,
    FolderIgnorePatternsChanged(String),
    SaveIgnorePatterns(String),
    // Leave network
    LeaveNetworkStart,
    LeaveNetworkConfirm,
    LeaveNetworkCancel,
    GotLeaveNetwork(#[allow(dead_code)] Result<CliResponse, String>),
    // Diagnostics (M27a)
    RunConnectivityCheck,
    ExportDiagnostics,
    DaemonEvent(CliResponse),
    Tick,
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DaemonCheckResult(running) => {
                tracing::info!(running, screen = ?self.screen, "DaemonCheckResult received");
                self.daemon_running = Some(running);
                if running {
                    tracing::info!("daemon is running — transitioning to connected");
                    return Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        },
                        |_| Message::DaemonConnected,
                    );
                }
                // Daemon not running. Auto-launch if a previous network exists,
                // otherwise show Setup.
                if self.daemon_launching {
                    // A launch task is already in flight — don't spawn again.
                    return Task::none();
                }
                let base = murmur_ipc::default_base_dir();
                if base.join("config.toml").exists() {
                    tracing::info!("daemon is NOT running — auto-launching murmurd");
                    self.daemon_error = None;
                    self.daemon_launching = true;
                    return self.do_launch_daemon(None, None);
                }
                tracing::info!("no existing network — showing Setup screen");
                self.screen = Screen::Setup;
            }
            Message::DaemonLaunchResult(Ok(())) => {
                tracing::info!("daemon is ready");
                self.daemon_launching = false;
                self.daemon_running = Some(true);
                self.screen = Screen::Folders;
                return self.fetch_all();
            }
            Message::DaemonLaunchResult(Err(e)) => {
                tracing::warn!(error = %e, "daemon launch failed");
                self.daemon_launching = false;
                self.daemon_running = Some(false);
                self.daemon_error = Some(e);
                // Stay on DaemonCheck screen so user sees the error + Retry.
                self.screen = Screen::DaemonCheck;
            }
            Message::DaemonConnected => {
                tracing::info!("DaemonConnected — navigating to Folders");
                self.screen = Screen::Folders;
                return self.fetch_all();
            }
            Message::RetryDaemonCheck => {
                tracing::info!(socket = %self.socket_path.display(), "RetryDaemonCheck");
                self.daemon_running = None;
                self.daemon_error = None;
                self.daemon_launching = false;
                self.screen = Screen::DaemonCheck;
                let p = self.socket_path.clone();
                return Task::perform(ipc::daemon_is_running(p), Message::DaemonCheckResult);
            }
            Message::SetupChooseCreate => {
                self.join_mode = false;
                self.setup_step = SetupStep::Form;
                self.setup_error = None;
            }
            Message::SetupChooseJoin => {
                self.join_mode = true;
                self.setup_step = SetupStep::Form;
                self.setup_error = None;
            }
            Message::SetupBack => {
                self.setup_step = SetupStep::ChooseMode;
                self.setup_error = None;
            }
            Message::DeviceNameChanged(n) => self.device_name = n,
            Message::MnemonicInputChanged(n) => self.mnemonic_input = n,
            Message::StartDaemon => {
                self.setup_error = None;
                self.daemon_launching = true;
                self.screen = Screen::DaemonCheck;
                let m = if self.join_mode {
                    Some(self.mnemonic_input.clone())
                } else {
                    None
                };
                let n = self.device_name.clone();
                return self.do_launch_daemon(Some(n), m);
            }
            Message::Navigate(screen) => {
                self.screen = screen.clone();
                return match screen {
                    Screen::Folders => {
                        Task::batch([self.fetch_folders(), self.fetch_network_folders()])
                    }
                    Screen::Conflicts => self.fetch_conflicts(),
                    Screen::Devices => Task::batch([self.fetch_devices(), self.fetch_presence()]),
                    Screen::Status => self.fetch_status(),
                    Screen::Settings => self.fetch_config(),
                    Screen::NetworkHealth => {
                        Task::batch([self.fetch_peers(), self.fetch_storage_stats()])
                    }
                    _ => Task::none(),
                };
            }
            // IPC responses
            Message::GotStatus(Ok(CliResponse::Status {
                device_id,
                device_name,
                network_id,
                peer_count,
                dag_entries,
                uptime_secs,
            })) => {
                self.status_device_id = device_id;
                self.status_device_name = device_name;
                self.status_network_id = network_id;
                self.status_peer_count = peer_count;
                self.status_dag_entries = dag_entries;
                self.status_uptime_secs = uptime_secs;
            }
            Message::GotFolders(Ok(CliResponse::Folders { folders })) => self.folders = folders,
            Message::GotNetworkFolders(Ok(CliResponse::NetworkFolders { folders })) => {
                self.network_folders = folders
            }
            Message::GotFolderFiles(Ok(CliResponse::Files { files })) => self.folder_files = files,
            Message::GotFolderSubscribers(Ok(CliResponse::FolderSubscriberList {
                subscribers,
            })) => self.folder_subscribers = subscribers,
            Message::GotConflicts(Ok(CliResponse::Conflicts { conflicts })) => {
                self.conflicts = conflicts
            }
            Message::GotDevices(Ok(CliResponse::Devices { devices })) => self.devices = devices,
            Message::GotPending(Ok(CliResponse::Pending { devices })) => self.pending = devices,
            Message::GotDevicePresence(Ok(CliResponse::DevicePresence { devices })) => {
                self.device_presence = devices
            }
            Message::GotFileHistory(Ok(CliResponse::FileVersions { versions })) => {
                self.history_versions = versions
            }
            Message::GotConfig(Ok(CliResponse::Config {
                auto_approve,
                mdns,
                upload_throttle,
                download_throttle,
                sync_paused,
                device_name,
                ..
            })) => {
                self.cfg_auto_approve = auto_approve;
                self.cfg_mdns = mdns;
                self.cfg_upload_throttle = upload_throttle;
                self.cfg_download_throttle = download_throttle;
                self.sync_paused = sync_paused;
                self.status_device_name = device_name;
            }
            Message::GotIgnorePatterns(Ok(CliResponse::IgnorePatterns { patterns })) => {
                self.folder_ignore_patterns = patterns;
            }
            Message::GotPeers(Ok(CliResponse::Peers { peers })) => self.peers = peers,
            Message::GotStorageStats(Ok(CliResponse::StorageStatsResponse {
                total_blob_count,
                total_blob_bytes,
                orphaned_blob_count,
                orphaned_blob_bytes,
                dag_entry_count,
                ..
            })) => {
                self.storage_stats = Some(StorageStatsCache {
                    total_blob_count,
                    total_blob_bytes,
                    orphaned_blob_count,
                    orphaned_blob_bytes,
                    dag_entry_count,
                });
            }
            Message::GotConnectivity(Ok(CliResponse::ConnectivityResult {
                relay_reachable,
                latency_ms,
            })) => {
                self.connectivity_result = Some((relay_reachable, latency_ms));
            }
            Message::GotReclaim(Ok(CliResponse::ReclaimedBytes {
                bytes_freed,
                blobs_removed,
            })) => {
                self.settings_toast = Some(format!(
                    "Reclaimed {blobs_removed} blobs, {} freed",
                    format_size(bytes_freed)
                ));
                return self.fetch_storage_stats();
            }
            Message::GotGeneric(Ok(CliResponse::Ok { ref message })) => {
                tracing::info!(%message, "GotGeneric Ok");
                return Task::batch([self.fetch_folders(), self.fetch_conflicts()]);
            }
            Message::GotGeneric(Ok(ref other)) => {
                tracing::warn!(?other, "GotGeneric received unexpected response variant");
            }
            // Folder actions
            Message::CreateFolderFromPicker => {
                // Open OS directory picker. The selected directory becomes
                // the synced folder, using its name as the folder name.
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose a folder to sync")
                            .pick_folder()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Message::PickedNewFolder,
                );
            }
            Message::PickedNewFolder(Some(path)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unnamed".to_string());
                let local_path = path.to_string_lossy().to_string();
                let p = self.socket_path.clone();
                tracing::info!(%name, %local_path, "creating folder from picked directory");
                return Task::perform(
                    ipc::send(
                        p,
                        CliRequest::CreateFolder {
                            name,
                            local_path: Some(local_path),
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::PickedNewFolder(None) => {
                tracing::info!("new folder picker cancelled");
            }
            Message::SubscribeFolder(fid, fname) => {
                // Open a native directory picker so the user chooses where to sync.
                tracing::info!(folder_id = %fid, folder_name = %fname, "SubscribeFolder: opening directory picker");
                return Task::perform(pick_directory(fid.clone()), move |path| {
                    Message::PickedFolderPath(fid, fname, path)
                });
            }
            Message::PickedFolderPath(fid, fname, Some(path)) => {
                let p = self.socket_path.clone();
                let local_path = path.to_string_lossy().to_string();
                tracing::info!(%fid, %fname, %local_path, "subscribing to folder");
                return Task::perform(
                    ipc::send(
                        p,
                        CliRequest::SubscribeFolder {
                            folder_id_hex: fid,
                            name: Some(fname),
                            local_path,
                            mode: "read-write".to_string(),
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::PickedFolderPath(_fid, _fname, None) => {
                tracing::info!("directory picker cancelled");
            }
            Message::UnsubscribeFolder(fid) => {
                let p = self.socket_path.clone();
                self.selected_folder = None;
                self.screen = Screen::Folders;
                return Task::perform(
                    ipc::send(
                        p,
                        CliRequest::UnsubscribeFolder {
                            folder_id_hex: fid,
                            keep_local: true,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::SelectFolder(folder) => {
                let fid = folder.folder_id.clone();
                self.selected_folder = Some(folder);
                self.screen = Screen::FolderDetail;
                let p = self.socket_path.clone();
                let p2 = self.socket_path.clone();
                let p3 = self.socket_path.clone();
                let fid2 = fid.clone();
                let fid3 = fid.clone();
                return Task::batch([
                    Task::perform(
                        ipc::send(p, CliRequest::FolderFiles { folder_id_hex: fid }),
                        Message::GotFolderFiles,
                    ),
                    Task::perform(
                        ipc::send(
                            p2,
                            CliRequest::FolderSubscribers {
                                folder_id_hex: fid2,
                            },
                        ),
                        Message::GotFolderSubscribers,
                    ),
                    Task::perform(
                        ipc::send(
                            p3,
                            CliRequest::GetIgnorePatterns {
                                folder_id_hex: fid3,
                            },
                        ),
                        Message::GotIgnorePatterns,
                    ),
                ]);
            }
            Message::ResolveConflict {
                folder_id,
                path,
                chosen_hash,
            } => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::ResolveConflict {
                            folder_id_hex: folder_id,
                            path,
                            chosen_hash_hex: chosen_hash,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::DismissConflict { folder_id, path } => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::DismissConflict {
                            folder_id_hex: folder_id,
                            path,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::BulkResolve {
                folder_id,
                strategy,
            } => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::BulkResolveConflicts {
                            folder_id_hex: folder_id,
                            strategy,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::ViewFileHistory { folder_id, path } => {
                self.history_folder_id = folder_id.clone();
                self.history_path = path.clone();
                self.screen = Screen::FileHistory;
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::FileHistory {
                            folder_id_hex: folder_id,
                            path,
                        },
                    ),
                    Message::GotFileHistory,
                );
            }
            Message::RestoreVersion {
                folder_id,
                path,
                blob_hash,
            } => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::RestoreFileVersion {
                            folder_id_hex: folder_id,
                            path,
                            blob_hash_hex: blob_hash,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::DeleteFile { folder_id, path } => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::DeleteFile {
                            folder_id_hex: folder_id,
                            path,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::StartRenameFolder(fid, current_name) => {
                self.renaming_folder_id = Some(fid);
                self.rename_input = current_name;
            }
            Message::RenameInputChanged(val) => self.rename_input = val,
            Message::CancelRenameFolder => {
                self.renaming_folder_id = None;
                self.rename_input.clear();
            }
            Message::SubmitRenameFolder => {
                if let Some(fid) = self.renaming_folder_id.take() {
                    let name = self.rename_input.clone();
                    self.rename_input.clear();
                    let p = self.socket_path.clone();
                    return Task::perform(
                        ipc::send(
                            p,
                            CliRequest::SetFolderName {
                                folder_id_hex: fid,
                                name,
                            },
                        ),
                        Message::GotGeneric,
                    );
                }
            }
            Message::SearchQueryChanged(q) => self.search_query = q,
            Message::SortBy(f) => {
                if self.sort_field == f {
                    self.sort_ascending = !self.sort_ascending;
                } else {
                    self.sort_field = f;
                    self.sort_ascending = true;
                }
            }
            Message::ApproveDevice(did) => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::ApproveDevice {
                            device_id_hex: did,
                            role: "full".to_string(),
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::ToggleGlobalSync => {
                let s = self.socket_path.clone();
                let req = if self.sync_paused {
                    CliRequest::ResumeSync
                } else {
                    CliRequest::PauseSync
                };
                self.sync_paused = !self.sync_paused;
                return Task::perform(ipc::send(s, req), Message::GotGeneric);
            }
            Message::ToggleFolderSync(fid) => {
                let s = self.socket_path.clone();
                let req = if self.folder_paused {
                    CliRequest::ResumeFolderSync { folder_id_hex: fid }
                } else {
                    CliRequest::PauseFolderSync { folder_id_hex: fid }
                };
                self.folder_paused = !self.folder_paused;
                return Task::perform(ipc::send(s, req), Message::GotGeneric);
            }
            // Settings (M26a)
            Message::ToggleAutoApprove => {
                self.cfg_auto_approve = !self.cfg_auto_approve;
                let s = self.socket_path.clone();
                let enabled = self.cfg_auto_approve;
                return Task::perform(
                    ipc::send(s, CliRequest::SetAutoApprove { enabled }),
                    Message::GotGeneric,
                );
            }
            Message::ToggleMdns => {
                self.cfg_mdns = !self.cfg_mdns;
                let s = self.socket_path.clone();
                let enabled = self.cfg_mdns;
                return Task::perform(
                    ipc::send(s, CliRequest::SetMdns { enabled }),
                    Message::GotGeneric,
                );
            }
            Message::SetThrottle(up, down) => {
                self.cfg_upload_throttle = up;
                self.cfg_download_throttle = down;
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::SetThrottle {
                            upload_bytes_per_sec: up,
                            download_bytes_per_sec: down,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            Message::ReclaimOrphanedBlobs => {
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(s, CliRequest::ReclaimOrphanedBlobs),
                    Message::GotReclaim,
                );
            }
            Message::FolderIgnorePatternsChanged(p) => self.folder_ignore_patterns = p,
            Message::SaveIgnorePatterns(fid) => {
                let s = self.socket_path.clone();
                let patterns = self.folder_ignore_patterns.clone();
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::SetIgnorePatterns {
                            folder_id_hex: fid,
                            patterns,
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            // Leave network
            Message::LeaveNetworkStart => {
                self.leave_network_confirm = true;
            }
            Message::LeaveNetworkCancel => {
                self.leave_network_confirm = false;
            }
            Message::LeaveNetworkConfirm => {
                self.leave_network_confirm = false;
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(s, CliRequest::LeaveNetwork),
                    Message::GotLeaveNetwork,
                );
            }
            Message::GotLeaveNetwork(_) => {
                // The daemon wipes its data dir (including the socket) and
                // shuts down, so we typically get a connection error here
                // rather than an Ok response. Either way, reset to Setup.
                self.daemon_running = Some(false);
                self.screen = Screen::Setup;
                self.setup_step = SetupStep::ChooseMode;
                self.folders.clear();
                self.devices.clear();
                self.conflicts.clear();
                self.event_log.clear();
                self.daemon_error = None;
                self.setup_error = None;
            }
            // Diagnostics (M27a)
            Message::RunConnectivityCheck => {
                self.connectivity_result = None;
                let s = self.socket_path.clone();
                return Task::perform(
                    ipc::send(s, CliRequest::RunConnectivityCheck),
                    Message::GotConnectivity,
                );
            }
            Message::ExportDiagnostics => {
                let s = self.socket_path.clone();
                let path = dirs_home().join("murmur-diagnostics").join(format!(
                    "diag-{}.json",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                ));
                return Task::perform(
                    ipc::send(
                        s,
                        CliRequest::ExportDiagnostics {
                            output_path: path.to_string_lossy().to_string(),
                        },
                    ),
                    Message::GotGeneric,
                );
            }
            // Events
            Message::DaemonEvent(CliResponse::Event { event }) => {
                self.push_event(format!("{}: {}", event.event_type, event.data));
                match event.event_type.as_str() {
                    "file_synced" | "dag_synced" => return self.fetch_folders(),
                    "conflict_detected" => return self.fetch_conflicts(),
                    "device_approved" | "device_join_requested"
                        if self.screen == Screen::Devices =>
                    {
                        return self.fetch_devices();
                    }
                    "folder_created" => {
                        return Task::batch([self.fetch_folders(), self.fetch_network_folders()]);
                    }
                    _ => {}
                }
            }
            Message::Tick => return self.fetch_status(),
            // Error handling
            Message::GotStatus(Err(e))
            | Message::GotFolders(Err(e))
            | Message::GotNetworkFolders(Err(e))
            | Message::GotFolderFiles(Err(e))
            | Message::GotFolderSubscribers(Err(e))
            | Message::GotConflicts(Err(e))
            | Message::GotDevices(Err(e))
            | Message::GotPending(Err(e))
            | Message::GotDevicePresence(Err(e))
            | Message::GotFileHistory(Err(e))
            | Message::GotGeneric(Err(e))
            | Message::GotConfig(Err(e))
            | Message::GotIgnorePatterns(Err(e))
            | Message::GotPeers(Err(e))
            | Message::GotStorageStats(Err(e))
            | Message::GotConnectivity(Err(e))
            | Message::GotReclaim(Err(e)) => {
                tracing::warn!(error = %e, "IPC error response");
                if self.daemon_running == Some(true) {
                    self.daemon_error = Some(e);
                }
            }
            _ => {}
        }
        Task::none()
    }
}

// ---------------------------------------------------------------------------
// IPC fetch helpers
// ---------------------------------------------------------------------------

impl App {
    /// Spawn murmurd, monitor the process, and poll the socket.
    ///
    /// If `name` is provided, the daemon is launched with `--name` (Setup flow).
    /// If `mnemonic` is provided, it is written to disk before launch (Join flow).
    /// Returns `DaemonLaunchResult` when the socket is ready or the process dies.
    fn do_launch_daemon(&self, name: Option<String>, mnemonic: Option<String>) -> Task<Message> {
        let socket_path = self.socket_path.clone();
        Task::perform(
            launch_and_wait(socket_path, name, mnemonic),
            Message::DaemonLaunchResult,
        )
    }

    fn fetch_all(&self) -> Task<Message> {
        Task::batch([
            self.fetch_status(),
            self.fetch_folders(),
            self.fetch_network_folders(),
            self.fetch_conflicts(),
            self.fetch_devices(),
            self.fetch_presence(),
        ])
    }
    fn fetch_status(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::Status),
            Message::GotStatus,
        )
    }
    fn fetch_folders(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::ListFolders),
            Message::GotFolders,
        )
    }
    fn fetch_network_folders(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::ListNetworkFolders),
            Message::GotNetworkFolders,
        )
    }
    fn fetch_conflicts(&self) -> Task<Message> {
        Task::perform(
            ipc::send(
                self.socket_path.clone(),
                CliRequest::ListConflicts {
                    folder_id_hex: None,
                },
            ),
            Message::GotConflicts,
        )
    }
    fn fetch_devices(&self) -> Task<Message> {
        let p = self.socket_path.clone();
        let p2 = self.socket_path.clone();
        Task::batch([
            Task::perform(ipc::send(p, CliRequest::ListDevices), Message::GotDevices),
            Task::perform(ipc::send(p2, CliRequest::ListPending), Message::GotPending),
        ])
    }
    fn fetch_presence(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::GetDevicePresence),
            Message::GotDevicePresence,
        )
    }
    fn fetch_config(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::GetConfig),
            Message::GotConfig,
        )
    }
    fn fetch_peers(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::ListPeers),
            Message::GotPeers,
        )
    }
    fn fetch_storage_stats(&self) -> Task<Message> {
        Task::perform(
            ipc::send(self.socket_path.clone(), CliRequest::StorageStats),
            Message::GotStorageStats,
        )
    }
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

impl App {
    fn subscription(&self) -> iced::Subscription<Message> {
        if self.daemon_running == Some(true) {
            iced::Subscription::batch([
                ipc::event_subscription(self.socket_path.clone()).map(Message::DaemonEvent),
                iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Tick),
            ])
        } else {
            iced::Subscription::none()
        }
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

impl App {
    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::DaemonCheck => self.view_daemon_check(),
            Screen::Setup => self.view_setup(),
            _ => self.view_main(),
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view_daemon_check(&self) -> Element<'_, Message> {
        let mut content = match self.daemon_running {
            None => column![text("Connecting to murmurd...").size(20)],
            Some(false) => column![text("Starting murmurd...").size(20),],
            Some(true) => column![
                text("Connected to murmurd!")
                    .size(20)
                    .color([0.3, 0.9, 0.3])
            ],
        };
        if let Some(ref e) = self.daemon_error {
            content = content.push(text(format!("Error: {e}")).color([1.0, 0.3, 0.3]).size(14));
            content = content.push(button(text("Retry")).on_press(Message::RetryDaemonCheck));
            content = content
                .push(button(text("Setup new network")).on_press(Message::Navigate(Screen::Setup)));
        }
        let content = content.spacing(12).padding(30).max_width(400);
        container(content)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn view_setup(&self) -> Element<'_, Message> {
        match self.setup_step {
            SetupStep::ChooseMode => {
                let col = column![
                    text("Murmur").size(32),
                    text("Private Device Sync Network").size(16),
                    rule::horizontal(1),
                    button(text("Create Network").width(Length::Fill))
                        .width(Length::Fill)
                        .on_press(Message::SetupChooseCreate),
                    button(text("Join Network").width(Length::Fill))
                        .width(Length::Fill)
                        .on_press(Message::SetupChooseJoin),
                    button(text("Back to daemon check!")).on_press(Message::RetryDaemonCheck),
                ]
                .spacing(16)
                .padding(30)
                .max_width(400);
                container(col)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            }
            SetupStep::Form => {
                let title = if self.join_mode {
                    "Join Network"
                } else {
                    "Create Network"
                };
                let mut col = column![
                    button(text("Back")).on_press(Message::SetupBack),
                    text(title).size(24),
                    rule::horizontal(1),
                    text_input("Device name", &self.device_name)
                        .on_input(Message::DeviceNameChanged)
                        .padding(10),
                ]
                .spacing(12)
                .padding(30)
                .max_width(600);
                if self.join_mode {
                    col = col.push(
                        text_input("Enter mnemonic phrase...", &self.mnemonic_input)
                            .on_input(Message::MnemonicInputChanged)
                            .padding(10),
                    );
                }
                let can = !self.device_name.is_empty()
                    && (!self.join_mode || !self.mnemonic_input.is_empty());
                let label = if self.join_mode {
                    "Start daemon & join"
                } else {
                    "Start daemon & create"
                };
                let mut btn = button(text(label));
                if can {
                    btn = btn.on_press(Message::StartDaemon);
                }
                col = col.push(btn);
                if let Some(ref e) = self.setup_error {
                    col = col.push(text(format!("Error: {e}")).color([1.0, 0.3, 0.3]));
                }
                container(col)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            }
        }
    }

    fn view_main(&self) -> Element<'_, Message> {
        let conflict_label = if self.conflicts.is_empty() {
            "Conflicts".to_string()
        } else {
            format!("Conflicts ({})", self.conflicts.len())
        };
        let devices_label = if self.pending.is_empty() {
            "Devices".to_string()
        } else {
            format!("Devices ({})", self.pending.len())
        };
        let sync_label = if self.sync_paused {
            "PAUSED"
        } else {
            "Syncing"
        };

        let sidebar = container(
            column![
                text("Murmur").size(20),
                text(sync_label).size(10),
                rule::horizontal(1),
                self.nav_button("Folders", Screen::Folders),
                self.nav_button(&conflict_label, Screen::Conflicts),
                self.nav_button(&devices_label, Screen::Devices),
                self.nav_button("Recent Files", Screen::RecentFiles),
                self.nav_button("Status", Screen::Status),
                rule::horizontal(1),
                self.nav_button("Network Health", Screen::NetworkHealth),
                self.nav_button("Settings", Screen::Settings),
                rule::horizontal(1),
                button(text(if self.sync_paused {
                    "Resume Sync"
                } else {
                    "Pause Sync"
                }))
                .on_press(Message::ToggleGlobalSync)
                .width(Length::Fill),
            ]
            .spacing(4)
            .padding(8)
            .width(180),
        );

        let content: Element<Message> = match self.screen {
            Screen::Folders => self.view_folders(),
            Screen::FolderDetail => self.view_folder_detail(),
            Screen::Conflicts => self.view_conflicts(),
            Screen::FileHistory => self.view_file_history(),
            Screen::Devices => self.view_devices(),
            Screen::Status => self.view_status(),
            Screen::RecentFiles => self.view_recent_files(),
            Screen::Settings => self.view_settings(),
            Screen::NetworkHealth => self.view_network_health(),
            Screen::DaemonCheck | Screen::Setup => unreachable!(),
        };

        row![sidebar, container(content).width(Length::Fill).padding(16)]
            .height(Length::Fill)
            .into()
    }

    fn view_folders(&self) -> Element<'_, Message> {
        let mut col = column![
            row![
                text("Folders").size(24).width(Length::Fill),
                button(text("New Folder")).on_press(Message::CreateFolderFromPicker)
            ]
            .spacing(8),
            rule::horizontal(1),
        ]
        .spacing(8);
        let subscribed: Vec<_> = self.folders.iter().filter(|f| f.subscribed).collect();
        for f in &subscribed {
            let path_text = f.local_path.as_deref().unwrap_or("(no local path)");
            let info = column![
                text(&f.name).size(16),
                text(path_text).size(11),
            ]
            .spacing(2)
            .width(Length::Fill);
            col = col.push(
                button(
                    row![
                        info,
                        text(&f.sync_status).size(12).width(Length::Shrink),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .on_press(Message::SelectFolder((*f).clone()))
                .width(Length::Fill)
                .style(iced::widget::button::secondary),
            );
        }
        let available: Vec<_> = self
            .network_folders
            .iter()
            .filter(|f| !f.subscribed)
            .collect();
        if !available.is_empty() {
            col = col
                .push(rule::horizontal(1))
                .push(text("Available on Network").size(18));
            for f in available {
                col = col.push(
                    row![
                        text(&f.name).size(16).width(Length::Fill),
                        text(format!("{} subs", f.subscriber_count)).width(Length::Fixed(80.0)),
                        button(text("Subscribe")).on_press(Message::SubscribeFolder(
                            f.folder_id.clone(),
                            f.name.clone()
                        )),
                    ]
                    .spacing(8),
                );
            }
        }
        if subscribed.is_empty() && self.network_folders.is_empty() {
            col = col.push(text("No folders yet. Create one to get started.").size(14));
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_folder_detail(&self) -> Element<'_, Message> {
        let folder = match &self.selected_folder {
            Some(f) => f,
            None => return text("No folder selected.").into(),
        };
        let pause_label = if self.folder_paused {
            "Resume"
        } else {
            "Pause"
        };
        // Header row: Back, name (or inline rename), action buttons.
        let is_renaming = self.renaming_folder_id.as_deref() == Some(&folder.folder_id);
        let name_el: Element<'_, Message> = if is_renaming {
            row![
                text_input("Folder name", &self.rename_input)
                    .on_input(Message::RenameInputChanged)
                    .on_submit(Message::SubmitRenameFolder)
                    .padding(4)
                    .width(Length::Fill),
                button(text("Save")).on_press(Message::SubmitRenameFolder),
                button(text("Cancel")).on_press(Message::CancelRenameFolder),
            ]
            .spacing(4)
            .into()
        } else {
            row![
                text(&folder.name).size(24).width(Length::Fill),
                button(text("Rename")).on_press(Message::StartRenameFolder(
                    folder.folder_id.clone(),
                    folder.name.clone()
                )),
            ]
            .spacing(4)
            .into()
        };
        let mut col = column![
            row![
                button(text("Back")).on_press(Message::Navigate(Screen::Folders)),
                name_el,
                button(text(pause_label))
                    .on_press(Message::ToggleFolderSync(folder.folder_id.clone())),
                button(text("Unsub"))
                    .on_press(Message::UnsubscribeFolder(folder.folder_id.clone())),
            ]
            .spacing(8),
            text(format!(
                "ID: {}  |  {} files  |  Mode: {}  |  {}",
                &folder.folder_id[..16],
                folder.file_count,
                folder.mode.as_deref().unwrap_or("--"),
                folder.local_path.as_deref().unwrap_or("(no local path)")
            ))
            .size(12),
        ]
        .spacing(8);
        if !self.folder_subscribers.is_empty() {
            let sub_text = self
                .folder_subscribers
                .iter()
                .map(|s| format!("{} [{}]", s.device_name, s.mode))
                .collect::<Vec<_>>()
                .join(", ");
            col = col.push(text(format!("Subscribers: {sub_text}")).size(11));
        }
        // Ignore patterns (M26a)
        col = col
            .push(rule::horizontal(1))
            .push(text("Ignore Patterns").size(14));
        let fid = folder.folder_id.clone();
        col = col.push(
            row![
                text_input(".murmurignore patterns", &self.folder_ignore_patterns)
                    .on_input(Message::FolderIgnorePatternsChanged)
                    .padding(6)
                    .width(Length::Fill),
                button(text("Save")).on_press(Message::SaveIgnorePatterns(fid)),
            ]
            .spacing(4),
        );
        col = col.push(rule::horizontal(1));
        // Search and sort
        col = col.push(
            row![
                text_input("Search files...", &self.search_query)
                    .on_input(Message::SearchQueryChanged)
                    .padding(6)
                    .width(Length::Fill),
                button(text("Name")).on_press(Message::SortBy(SortField::Name)),
                button(text("Size")).on_press(Message::SortBy(SortField::Size)),
                button(text("Type")).on_press(Message::SortBy(SortField::Type)),
            ]
            .spacing(4),
        );
        let mut files: Vec<_> = self
            .folder_files
            .iter()
            .filter(|f| {
                self.search_query.is_empty()
                    || f.path
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
            })
            .collect();
        match self.sort_field {
            SortField::Name => files.sort_by(|a, b| {
                if self.sort_ascending {
                    a.path.cmp(&b.path)
                } else {
                    b.path.cmp(&a.path)
                }
            }),
            SortField::Size => files.sort_by(|a, b| {
                if self.sort_ascending {
                    a.size.cmp(&b.size)
                } else {
                    b.size.cmp(&a.size)
                }
            }),
            SortField::Type => files.sort_by(|a, b| {
                let at = a.mime_type.as_deref().unwrap_or("");
                let bt = b.mime_type.as_deref().unwrap_or("");
                if self.sort_ascending {
                    at.cmp(bt)
                } else {
                    bt.cmp(at)
                }
            }),
        }
        if files.is_empty() {
            col = col.push(text("No files match.").size(14));
        } else {
            col = col.push(
                row![
                    text("Path").width(Length::Fill),
                    text("Size").width(Length::Fixed(80.0)),
                    text("Type").width(Length::Fixed(100.0)),
                    text("Actions").width(Length::Fixed(160.0))
                ]
                .spacing(8),
            );
            for file in files {
                col = col.push(
                    row![
                        text(&file.path).width(Length::Fill),
                        text(format_size(file.size)).width(Length::Fixed(80.0)),
                        text(file.mime_type.as_deref().unwrap_or("--")).width(Length::Fixed(100.0)),
                        button(text("History")).on_press(Message::ViewFileHistory {
                            folder_id: file.folder_id.clone(),
                            path: file.path.clone()
                        }),
                        button(text("Del")).on_press(Message::DeleteFile {
                            folder_id: file.folder_id.clone(),
                            path: file.path.clone()
                        }),
                    ]
                    .spacing(4),
                );
            }
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_conflicts(&self) -> Element<'_, Message> {
        let mut col = column![text("Conflicts").size(24), rule::horizontal(1)].spacing(8);
        if self.conflicts.is_empty() {
            col = col.push(text("No active conflicts.").size(14));
        } else {
            let mut folder_ids: Vec<String> =
                self.conflicts.iter().map(|c| c.folder_id.clone()).collect();
            folder_ids.dedup();
            for fid in &folder_ids {
                let fname = self
                    .conflicts
                    .iter()
                    .find(|c| c.folder_id == *fid)
                    .map(|c| c.folder_name.as_str())
                    .unwrap_or("unknown");
                col = col.push(
                    row![
                        text(format!("{fname}:")).width(Length::Fill),
                        button(text("Keep All Newest")).on_press(Message::BulkResolve {
                            folder_id: fid.clone(),
                            strategy: "keep_newest".to_string()
                        })
                    ]
                    .spacing(8),
                );
            }
            col = col.push(rule::horizontal(1));
            for conflict in &self.conflicts {
                col = col
                    .push(text(format!("{} -- {}", conflict.folder_name, conflict.path)).size(16));
                for v in &conflict.versions {
                    col = col.push(
                        row![
                            text(format!(
                                "  {} ({})  HLC: {}",
                                v.device_name,
                                truncate_hex(&v.blob_hash, 16),
                                v.hlc
                            ))
                            .width(Length::Fill),
                            button(text("Keep")).on_press(Message::ResolveConflict {
                                folder_id: conflict.folder_id.clone(),
                                path: conflict.path.clone(),
                                chosen_hash: v.blob_hash.clone()
                            }),
                        ]
                        .spacing(4),
                    );
                }
                col = col.push(button(text("Keep Both (dismiss)")).on_press(
                    Message::DismissConflict {
                        folder_id: conflict.folder_id.clone(),
                        path: conflict.path.clone(),
                    },
                ));
                col = col.push(rule::horizontal(1));
            }
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_file_history(&self) -> Element<'_, Message> {
        let mut col = column![
            row![
                button(text("Back")).on_press(Message::Navigate(Screen::Folders)),
                text(format!("History: {}", self.history_path)).size(24)
            ]
            .spacing(8),
            rule::horizontal(1),
        ]
        .spacing(8);
        if self.history_versions.is_empty() {
            col = col.push(text("No versions found.").size(14));
        } else {
            for v in &self.history_versions {
                col = col.push(
                    row![
                        text(format!(
                            "{}  by {}  HLC: {}  ({})",
                            truncate_hex(&v.blob_hash, 16),
                            v.device_name,
                            v.modified_at,
                            format_size(v.size)
                        ))
                        .width(Length::Fill),
                        button(text("Restore")).on_press(Message::RestoreVersion {
                            folder_id: self.history_folder_id.clone(),
                            path: self.history_path.clone(),
                            blob_hash: v.blob_hash.clone()
                        }),
                    ]
                    .spacing(8),
                );
            }
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_devices(&self) -> Element<'_, Message> {
        let mut col = column![text("Devices").size(24)].spacing(8);

        // Current device
        if let Some(local) = self
            .devices
            .iter()
            .find(|d| d.device_id == self.status_device_id)
        {
            col = col.push(text("This Device").size(18));
            col = col.push(
                row![
                    text("*").color([0.3, 0.9, 0.3]),
                    text(&local.name).width(Length::Fill),
                    text(&local.role).width(Length::Fixed(80.0)),
                    text("Online").width(Length::Fixed(120.0)),
                    text(truncate_hex(&local.device_id, 16)).size(11),
                ]
                .spacing(8),
            );
            col = col.push(rule::horizontal(1));
        }

        // Pending approval
        if !self.pending.is_empty() {
            col = col.push(text("Pending Approval").size(18));
            for d in &self.pending {
                col = col.push(
                    row![
                        text(format!("{} ({})", d.name, truncate_hex(&d.device_id, 16)))
                            .width(Length::Fill),
                        button(text("Approve"))
                            .on_press(Message::ApproveDevice(d.device_id.clone())),
                    ]
                    .spacing(8),
                );
            }
            col = col.push(rule::horizontal(1));
        }

        // Other approved devices
        let others: Vec<_> = self
            .devices
            .iter()
            .filter(|d| d.device_id != self.status_device_id)
            .collect();
        if !others.is_empty() {
            col = col.push(text("Other Devices").size(18));
            for d in others {
                let presence = self
                    .device_presence
                    .iter()
                    .find(|p| p.device_id == d.device_id);
                let status = match presence {
                    Some(p) if p.online => "Online".to_string(),
                    Some(p) if p.last_seen_unix > 0 => format_relative_time(p.last_seen_unix),
                    _ => "Never connected".to_string(),
                };
                let color = if matches!(presence, Some(p) if p.online) {
                    [0.3, 0.9, 0.3]
                } else {
                    [0.5, 0.5, 0.5]
                };
                col = col.push(
                    row![
                        text("*").color(color),
                        text(&d.name).width(Length::Fill),
                        text(&d.role).width(Length::Fixed(80.0)),
                        text(status).width(Length::Fixed(120.0)),
                        text(truncate_hex(&d.device_id, 16)).size(11),
                    ]
                    .spacing(8),
                );
            }
        } else if self.devices.len() <= 1 {
            col = col.push(text("Other Devices").size(18));
            col = col.push(text("No other devices on this network.").size(14));
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_status(&self) -> Element<'_, Message> {
        let mut col = column![text("Status").size(24), rule::horizontal(1)].spacing(8);
        col = col.push(text("Network").size(18));
        col = col.push(text(format!(
            "Network ID: {}",
            truncate_hex(&self.status_network_id, 16)
        )));
        col = col.push(text(format!(
            "Device: {} ({})",
            self.status_device_name,
            truncate_hex(&self.status_device_id, 16)
        )));
        col = col.push(text(format!(
            "Peers: {}  |  DAG: {}  |  Folders: {}  |  Conflicts: {}",
            self.status_peer_count,
            self.status_dag_entries,
            self.folders.len(),
            self.conflicts.len()
        )));
        col = col.push(
            text(format!(
                "Uptime: {}",
                format_uptime(self.status_uptime_secs)
            ))
            .size(12),
        );
        if let Some(ref e) = self.daemon_error {
            col = col.push(
                text(format!("Last error: {e}"))
                    .color([1.0, 0.3, 0.3])
                    .size(12),
            );
        }
        col = col
            .push(rule::horizontal(1))
            .push(text("Event Log").size(18));
        if self.event_log.is_empty() {
            col = col.push(text("No events yet.").size(14));
        } else {
            for ev in self.event_log.iter().rev().take(50) {
                col = col.push(text(ev).size(12));
            }
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    fn view_recent_files(&self) -> Element<'_, Message> {
        let mut col = column![
            text("Recent Files").size(24),
            text_input("Search across all folders...", &self.search_query)
                .on_input(Message::SearchQueryChanged)
                .padding(8),
            rule::horizontal(1),
        ]
        .spacing(8);
        if self.search_query.is_empty() {
            col = col.push(text("Type a search term to find files across all folders.").size(14));
        } else {
            col = col.push(text(format!("Searching for: {}", self.search_query)).size(14));
        }
        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    // -- Settings (M26a) --

    fn view_settings(&self) -> Element<'_, Message> {
        let mut col = column![text("Settings").size(24), rule::horizontal(1)].spacing(8);

        // Device section
        col = col.push(text("Device").size(18));
        col = col.push(text(format!("Name: {}", self.status_device_name)));
        col = col.push(text(format!("ID: {}", truncate_hex(&self.status_device_id, 32))).size(11));

        col = col.push(rule::horizontal(1));

        // Network section
        col = col.push(text("Network").size(18));
        col = col.push(
            row![
                text("Auto-approve new devices:").width(Length::Fill),
                button(text(if self.cfg_auto_approve { "ON" } else { "OFF" }))
                    .on_press(Message::ToggleAutoApprove),
            ]
            .spacing(8),
        );
        col = col.push(
            row![
                text("mDNS LAN discovery:").width(Length::Fill),
                button(text(if self.cfg_mdns { "ON" } else { "OFF" }))
                    .on_press(Message::ToggleMdns),
            ]
            .spacing(8),
        );

        col = col.push(rule::horizontal(1));

        // Bandwidth section
        col = col.push(text("Bandwidth").size(18));
        let up_label = if self.cfg_upload_throttle == 0 {
            "Unlimited".to_string()
        } else {
            format!("{}/s", format_size(self.cfg_upload_throttle))
        };
        let down_label = if self.cfg_download_throttle == 0 {
            "Unlimited".to_string()
        } else {
            format!("{}/s", format_size(self.cfg_download_throttle))
        };
        col = col.push(text(format!(
            "Upload: {up_label}  |  Download: {down_label}"
        )));
        col = col.push(
            row![
                button(text("Unlimited")).on_press(Message::SetThrottle(0, 0)),
                button(text("1 MB/s")).on_press(Message::SetThrottle(1_048_576, 1_048_576)),
                button(text("5 MB/s")).on_press(Message::SetThrottle(5_242_880, 5_242_880)),
                button(text("10 MB/s")).on_press(Message::SetThrottle(10_485_760, 10_485_760)),
            ]
            .spacing(4),
        );

        col = col.push(rule::horizontal(1));

        // Storage section
        col = col.push(text("Storage").size(18));
        if let Some(ref stats) = self.storage_stats {
            col = col.push(text(format!(
                "Blobs: {} ({})",
                stats.total_blob_count,
                format_size(stats.total_blob_bytes)
            )));
            col = col.push(text(format!(
                "Orphaned: {} ({})",
                stats.orphaned_blob_count,
                format_size(stats.orphaned_blob_bytes)
            )));
            col = col.push(text(format!("DAG entries: {}", stats.dag_entry_count)));
        }
        col = col
            .push(button(text("Reclaim Orphaned Blobs")).on_press(Message::ReclaimOrphanedBlobs));
        if let Some(ref toast) = self.settings_toast {
            col = col.push(text(toast).color([0.3, 0.9, 0.3]).size(12));
        }

        col = col.push(rule::horizontal(1));

        // Sync section
        col = col.push(text("Sync").size(18));
        col = col.push(
            row![
                text("Global sync:").width(Length::Fill),
                button(text(if self.sync_paused {
                    "PAUSED - Resume"
                } else {
                    "Active - Pause"
                }))
                .on_press(Message::ToggleGlobalSync),
            ]
            .spacing(8),
        );

        col = col.push(rule::horizontal(1));

        // Danger zone
        col = col.push(text("Danger Zone").size(18).color([1.0, 0.3, 0.3]));
        if self.leave_network_confirm {
            col = col.push(
                text("This will delete all Murmur data (config, keys, DAG, blobs). Files in synced folders on disk will NOT be deleted. The daemon will shut down.")
                    .size(13)
                    .color([1.0, 0.6, 0.3]),
            );
            col = col.push(
                row![
                    button(text("Yes, leave network and wipe data").width(Length::Fill))
                        .on_press(Message::LeaveNetworkConfirm),
                    button(text("Cancel")).on_press(Message::LeaveNetworkCancel),
                ]
                .spacing(8),
            );
        } else {
            col = col.push(
                button(text("Leave Network & Wipe Data")).on_press(Message::LeaveNetworkStart),
            );
        }

        scrollable(col.padding(iced::Padding { top: 0.0, right: 15.0, bottom: 0.0, left: 0.0 })).into()
    }

    // -- Network Health (M27a) --

    fn view_network_health(&self) -> Element<'_, Message> {
        let mut col = column![text("Network Health").size(24), rule::horizontal(1)].spacing(8);

        // Peer list
        col = col.push(text("Peers").size(18));
        if self.peers.is_empty() {
            col = col.push(text("No peers connected.").size(14));
        } else {
            col = col.push(
                row![
                    text("Name").width(Length::Fill),
                    text("Connection").width(Length::Fixed(80.0)),
                    text("Last Seen").width(Length::Fixed(120.0)),
                    text("ID").width(Length::Fixed(140.0)),
                ]
                .spacing(8),
            );
            for p in &self.peers {
                col = col.push(
                    row![
                        text(&p.device_name).width(Length::Fill),
                        text(&p.connection_type).width(Length::Fixed(80.0)),
                        text(if p.last_seen_unix > 0 {
                            format_relative_time(p.last_seen_unix)
                        } else {
                            "Never".to_string()
                        })
                        .width(Length::Fixed(120.0)),
                        text(truncate_hex(&p.device_id, 16))
                            .size(11)
                            .width(Length::Fixed(140.0)),
                    ]
                    .spacing(8),
                );
            }
        }

        col = col.push(rule::horizontal(1));

        // Storage stats
        col = col.push(text("Storage").size(18));
        if let Some(ref stats) = self.storage_stats {
            col = col.push(text(format!(
                "Total blobs: {} ({})",
                stats.total_blob_count,
                format_size(stats.total_blob_bytes)
            )));
            col = col.push(text(format!(
                "Orphaned blobs: {} ({})",
                stats.orphaned_blob_count,
                format_size(stats.orphaned_blob_bytes)
            )));
            col = col.push(text(format!("DAG entries: {}", stats.dag_entry_count)));
        } else {
            col = col.push(text("Loading...").size(14));
        }

        col = col.push(rule::horizontal(1));

        // Connectivity check
        col = col.push(text("Connectivity").size(18));
        col = col
            .push(button(text("Run Connectivity Check")).on_press(Message::RunConnectivityCheck));
        if let Some((reachable, latency)) = &self.connectivity_result {
            let status = if *reachable {
                "Reachable"
            } else {
                "Unreachable"
            };
            let latency_str = latency.map(|ms| format!(" ({ms} ms)")).unwrap_or_default();
            let color = if *reachable {
                [0.3, 0.9, 0.3]
            } else {
                [1.0, 0.3, 0.3]
            };
            col = col.push(text(format!("Relay: {status}{latency_str}")).color(color));
        }

        col = col.push(rule::horizontal(1));

        // Export
        col = col.push(button(text("Export Diagnostics")).on_press(Message::ExportDiagnostics));

        scrollable(col).into()
    }

    fn nav_button(&self, label: &str, target: Screen) -> iced::widget::Button<'_, Message> {
        let mut btn = button(text(label.to_string()).width(Length::Fill)).width(Length::Fill);
        if self.screen != target {
            btn = btn.on_press(Message::Navigate(target));
        }
        btn
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_uptime(secs: u64) -> String {
    if secs >= 86400 {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    } else if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn format_relative_time(unix_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(unix_secs);
    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{} min ago", diff / 60)
    } else if diff < 86400 {
        format!("{} hours ago", diff / 3600)
    } else {
        format!("{} days ago", diff / 86400)
    }
}

/// Open a native directory picker dialog.
/// Returns `None` if the user cancelled.
async fn pick_directory(_folder_id: String) -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Choose folder location")
        .pick_folder()
        .await;
    handle.map(|h| h.path().to_path_buf())
}

fn truncate_hex(hex: &str, max_len: usize) -> String {
    if hex.len() > max_len {
        format!("{}...", &hex[..max_len])
    } else {
        hex.to_string()
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve the murmurd binary path.
///
/// Looks for `murmurd` next to the current executable first (same build dir),
/// then falls back to PATH lookup.
/// Check if a process is alive and not a zombie.
fn is_pid_alive(pid: i32) -> bool {
    if unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
        for line in status.lines() {
            if let Some(state) = line.strip_prefix("State:") {
                return !state.trim().starts_with('Z');
            }
        }
    }
    true
}

fn resolve_murmurd() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("murmurd");
        if sibling.exists() {
            tracing::info!(path = %sibling.display(), "using sibling murmurd binary");
            return sibling;
        }
    }
    PathBuf::from("murmurd")
}

/// Spawn murmurd, monitor the child process, and poll the socket until ready.
///
/// This is the single entry point for all daemon launch paths (auto-launch on
/// restart and first-time Setup). It:
/// 1. Optionally writes a mnemonic to disk (Join flow)
/// 2. Spawns murmurd and stores the `Child` handle
/// 3. Polls the socket for up to 10 seconds
/// 4. On each poll iteration, checks if the child process crashed
///
/// Stale socket cleanup is left to murmurd itself — the desktop app never
/// removes the socket file, avoiding races with a daemon that is still
/// shutting down.
///
/// Returns `Ok(())` when the socket is connectable, or `Err` with a
/// descriptive message if the process died or timed out.
async fn launch_and_wait(
    socket_path: PathBuf,
    name: Option<String>,
    mnemonic: Option<String>,
) -> Result<(), String> {
    use std::io::Read as _;
    use std::process::Stdio;

    // Check if a daemon is already running (another process, or previous launch).
    if ipc::daemon_is_running(socket_path.clone()).await {
        tracing::info!("daemon already running — skipping launch");
        return Ok(());
    }

    // Kill any previous child we spawned that might still be shutting down.
    {
        let mut guard = DAEMON_CHILD.lock().unwrap();
        if let Some(ref mut old) = *guard {
            tracing::info!(pid = old.id(), "killing previous daemon before re-launch");
            let _ = old.kill();
            let _ = old.wait();
            *guard = None;
        }
    }

    tokio::task::spawn_blocking(move || {
        let base = murmur_ipc::default_base_dir();
        std::fs::create_dir_all(&base).map_err(|e| format!("create base dir: {e}"))?;

        // Kill any orphan daemon from a previous desktop session that didn't
        // clean up (e.g. the desktop was SIGKILL'd and Drop didn't fire).
        let pid_path = base.join("murmurd.pid");
        if let Ok(contents) = std::fs::read_to_string(&pid_path)
            && let Ok(pid) = contents.trim().parse::<i32>()
            && is_pid_alive(pid)
        {
            tracing::info!(pid, "killing orphan murmurd from previous session");
            unsafe { libc::kill(pid, libc::SIGTERM) };
            // Wait up to 3 seconds for graceful exit.
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if unsafe { libc::kill(pid, 0) } != 0 {
                    break;
                }
            }
            // Force-kill if still alive.
            if unsafe { libc::kill(pid, 0) } == 0 {
                tracing::warn!(pid, "orphan murmurd did not exit, sending SIGKILL");
                unsafe { libc::kill(pid, libc::SIGKILL) };
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        let _ = std::fs::remove_file(&pid_path);

        // Write mnemonic for Join flow.
        if let Some(ref m) = mnemonic {
            std::fs::write(base.join("mnemonic"), m).map_err(|e| format!("write mnemonic: {e}"))?;
        }

        // Build command.
        let bin = resolve_murmurd();
        tracing::info!(base = %base.display(), bin = %bin.display(), "launching murmurd");
        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("--data-dir").arg(&base);
        if let Some(ref n) = name {
            cmd.arg("--name").arg(n);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| format!("spawn murmurd ({bin:?}): {e}"))?;
        tracing::info!(pid = child.id(), "murmurd process spawned");

        // Store the child handle so atexit can kill it on exit.
        *DAEMON_CHILD.lock().unwrap() = Some(child);

        let sock = murmur_ipc::socket_path(&base);

        // Poll: check socket readiness + process health.
        for i in 0..20u32 {
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Check if the process is still alive.
            let mut guard = DAEMON_CHILD.lock().unwrap();
            if let Some(ref mut c) = *guard {
                match c.try_wait() {
                    Ok(Some(status)) => {
                        // Process exited — read stderr for diagnostics.
                        let stderr = c
                            .stderr
                            .take()
                            .map(|mut s| {
                                let mut buf = String::new();
                                let _ = s.read_to_string(&mut buf);
                                buf
                            })
                            .unwrap_or_default();
                        let msg = if stderr.trim().is_empty() {
                            format!("murmurd exited with {status}")
                        } else {
                            // Show last meaningful line of stderr.
                            let last = stderr
                                .lines()
                                .rev()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("(no output)");
                            format!("murmurd exited with {status}: {last}")
                        };
                        *guard = None;
                        return Err(msg);
                    }
                    Ok(None) => {} // Still running — good.
                    Err(e) => {
                        return Err(format!("check murmurd process: {e}"));
                    }
                }
            }
            drop(guard);

            // Check socket.
            if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
                tracing::info!(attempts = i + 1, "murmurd socket is ready");
                return Ok(());
            }
        }

        Err("murmurd did not become ready in 10 seconds — check logs".to_string())
    })
    .await
    .map_err(|e| format!("launch task panicked: {e}"))?
}
