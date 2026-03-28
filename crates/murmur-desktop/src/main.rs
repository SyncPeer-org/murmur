//! Murmur desktop application — thin IPC client to murmurd.
//!
//! Built with [`iced`](https://iced.rs), a pure-Rust cross-platform UI toolkit.
//! All state is fetched from `murmurd` via Unix socket IPC. The desktop app
//! does not embed any engine, storage, or networking.

mod ipc;

use std::path::PathBuf;

use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};

use murmur_ipc::{
    CliRequest, CliResponse, ConflictInfoIpc, DeviceInfoIpc, FileInfoIpc, FileVersionIpc,
    FolderInfoIpc,
};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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

/// Which screen is currently active.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    /// Checking whether murmurd is running.
    DaemonCheck,
    /// Setup wizard (create/join network).
    Setup,
    /// Folder list.
    Folders,
    /// Detail view for a specific folder.
    FolderDetail,
    /// Active conflicts.
    Conflicts,
    /// File version history.
    FileHistory,
    /// Device list.
    Devices,
    /// Network overview / status.
    Status,
}

/// Setup wizard step.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SetupStep {
    ChooseMode,
    Form,
}

/// Application state.
struct App {
    screen: Screen,
    socket_path: PathBuf,

    // Daemon check
    daemon_running: Option<bool>,
    daemon_error: Option<String>,

    // Setup
    setup_step: SetupStep,
    device_name: String,
    mnemonic_input: String,
    join_mode: bool,
    setup_error: Option<String>,

    // Status
    status_device_id: String,
    status_device_name: String,
    status_network_id: String,
    status_peer_count: u64,
    status_dag_entries: u64,
    status_uptime_secs: u64,

    // Folders
    folders: Vec<FolderInfoIpc>,

    // Folder detail
    selected_folder: Option<FolderInfoIpc>,
    folder_files: Vec<FileInfoIpc>,

    // Conflicts
    conflicts: Vec<ConflictInfoIpc>,

    // File history
    history_folder_id: String,
    history_path: String,
    history_versions: Vec<FileVersionIpc>,

    // Devices
    devices: Vec<DeviceInfoIpc>,
    pending: Vec<DeviceInfoIpc>,

    // Event log (real-time from SubscribeEvents)
    event_log: Vec<String>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let socket_path = murmur_ipc::default_socket_path();
        let app = Self {
            screen: Screen::DaemonCheck,
            socket_path,
            daemon_running: None,
            daemon_error: None,
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
            selected_folder: None,
            folder_files: Vec::new(),
            conflicts: Vec::new(),
            history_folder_id: String::new(),
            history_path: String::new(),
            history_versions: Vec::new(),
            devices: Vec::new(),
            pending: Vec::new(),
            event_log: Vec::new(),
        };
        let path = app.socket_path.clone();
        (
            app,
            Task::perform(ipc::daemon_is_running(path), Message::DaemonCheckResult),
        )
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// All possible UI messages.
#[derive(Debug, Clone)]
enum Message {
    // Daemon check
    DaemonCheckResult(bool),
    RetryDaemonCheck,

    // Setup
    SetupChooseCreate,
    SetupChooseJoin,
    SetupBack,
    DeviceNameChanged(String),
    MnemonicInputChanged(String),
    StartDaemon,
    StartDaemonResult(Result<(), String>),

    // Navigation
    Navigate(Screen),

    // IPC responses
    GotStatus(Result<CliResponse, String>),
    GotFolders(Result<CliResponse, String>),
    GotFolderFiles(Result<CliResponse, String>),
    GotConflicts(Result<CliResponse, String>),
    GotDevices(Result<CliResponse, String>),
    GotPending(Result<CliResponse, String>),
    GotFileHistory(Result<CliResponse, String>),
    // Actions
    CreateFolder,
    GotCreateFolder(Result<CliResponse, String>),
    SubscribeFolder(String),
    GotSubscribeFolder(Result<CliResponse, String>),
    UnsubscribeFolder(String),
    GotUnsubscribeFolder(Result<CliResponse, String>),
    SelectFolder(FolderInfoIpc),
    ResolveConflict {
        folder_id: String,
        path: String,
        chosen_hash: String,
    },
    GotResolveConflict(Result<CliResponse, String>),
    ViewFileHistory {
        folder_id: String,
        path: String,
    },
    RestoreVersion {
        folder_id: String,
        path: String,
        blob_hash: String,
    },
    GotRestoreVersion(Result<CliResponse, String>),
    ApproveDevice(String),
    GotApproveDevice(Result<CliResponse, String>),

    // Real-time events
    DaemonEvent(CliResponse),

    // Periodic refresh
    Tick,
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // -- Daemon check --
            Message::DaemonCheckResult(running) => {
                self.daemon_running = Some(running);
                if running {
                    self.screen = Screen::Folders;
                    return self.fetch_all();
                }
                self.screen = Screen::Setup;
            }
            Message::RetryDaemonCheck => {
                self.daemon_running = None;
                self.daemon_error = None;
                self.screen = Screen::DaemonCheck;
                let path = self.socket_path.clone();
                return Task::perform(ipc::daemon_is_running(path), Message::DaemonCheckResult);
            }

            // -- Setup --
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
            Message::DeviceNameChanged(name) => {
                self.device_name = name;
            }
            Message::MnemonicInputChanged(input) => {
                self.mnemonic_input = input;
            }
            Message::StartDaemon => {
                self.setup_error = None;
                let mnemonic = if self.join_mode {
                    Some(self.mnemonic_input.clone())
                } else {
                    None
                };
                let name = self.device_name.clone();
                return Task::perform(start_murmurd(name, mnemonic), Message::StartDaemonResult);
            }
            Message::StartDaemonResult(result) => match result {
                Ok(()) => {
                    // Give daemon a moment to start, then check.
                    let path = self.socket_path.clone();
                    return Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            ipc::daemon_is_running(path).await
                        },
                        Message::DaemonCheckResult,
                    );
                }
                Err(e) => {
                    self.setup_error = Some(e);
                }
            },

            // -- Navigation --
            Message::Navigate(screen) => {
                self.screen = screen.clone();
                return match screen {
                    Screen::Folders => self.fetch_folders(),
                    Screen::Conflicts => self.fetch_conflicts(),
                    Screen::Devices => self.fetch_devices(),
                    Screen::Status => self.fetch_status(),
                    _ => Task::none(),
                };
            }

            // -- IPC responses --
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
            Message::GotFolders(Ok(CliResponse::Folders { folders })) => {
                self.folders = folders;
            }
            Message::GotFolderFiles(Ok(CliResponse::Files { files })) => {
                self.folder_files = files;
            }
            Message::GotConflicts(Ok(CliResponse::Conflicts { conflicts })) => {
                self.conflicts = conflicts;
            }
            Message::GotDevices(Ok(CliResponse::Devices { devices })) => {
                self.devices = devices;
            }
            Message::GotPending(Ok(CliResponse::Pending { devices })) => {
                self.pending = devices;
            }
            Message::GotFileHistory(Ok(CliResponse::FileVersions { versions })) => {
                self.history_versions = versions;
            }
            // Folder actions
            Message::CreateFolder => {
                // For now use a hardcoded name dialog — will be improved in M19.
                let path = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        path,
                        CliRequest::CreateFolder {
                            name: "New Folder".to_string(),
                        },
                    ),
                    Message::GotCreateFolder,
                );
            }
            Message::GotCreateFolder(Ok(CliResponse::Ok { .. })) => {
                return self.fetch_folders();
            }
            Message::SubscribeFolder(folder_id) => {
                let path = self.socket_path.clone();
                let home = dirs_home().join("Murmur").join(&folder_id[..8]);
                return Task::perform(
                    ipc::send(
                        path,
                        CliRequest::SubscribeFolder {
                            folder_id_hex: folder_id,
                            local_path: home.to_string_lossy().to_string(),
                            mode: "read-write".to_string(),
                        },
                    ),
                    Message::GotSubscribeFolder,
                );
            }
            Message::GotSubscribeFolder(Ok(CliResponse::Ok { .. })) => {
                return self.fetch_folders();
            }
            Message::UnsubscribeFolder(folder_id) => {
                let path = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        path,
                        CliRequest::UnsubscribeFolder {
                            folder_id_hex: folder_id,
                            keep_local: true,
                        },
                    ),
                    Message::GotUnsubscribeFolder,
                );
            }
            Message::GotUnsubscribeFolder(Ok(CliResponse::Ok { .. })) => {
                return self.fetch_folders();
            }
            Message::SelectFolder(folder) => {
                let folder_id = folder.folder_id.clone();
                self.selected_folder = Some(folder);
                self.screen = Screen::FolderDetail;
                let path = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        path,
                        CliRequest::FolderFiles {
                            folder_id_hex: folder_id,
                        },
                    ),
                    Message::GotFolderFiles,
                );
            }

            // Conflicts
            Message::ResolveConflict {
                folder_id,
                path,
                chosen_hash,
            } => {
                let sock = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        sock,
                        CliRequest::ResolveConflict {
                            folder_id_hex: folder_id,
                            path,
                            chosen_hash_hex: chosen_hash,
                        },
                    ),
                    Message::GotResolveConflict,
                );
            }
            Message::GotResolveConflict(Ok(CliResponse::Ok { .. })) => {
                return self.fetch_conflicts();
            }

            // File history
            Message::ViewFileHistory { folder_id, path } => {
                self.history_folder_id = folder_id.clone();
                self.history_path = path.clone();
                self.screen = Screen::FileHistory;
                let sock = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        sock,
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
                let sock = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        sock,
                        CliRequest::RestoreFileVersion {
                            folder_id_hex: folder_id,
                            path,
                            blob_hash_hex: blob_hash,
                        },
                    ),
                    Message::GotRestoreVersion,
                );
            }
            Message::GotRestoreVersion(Ok(CliResponse::Ok { message })) => {
                self.event_log.push(message);
            }

            // Devices
            Message::ApproveDevice(device_id) => {
                let sock = self.socket_path.clone();
                return Task::perform(
                    ipc::send(
                        sock,
                        CliRequest::ApproveDevice {
                            device_id_hex: device_id,
                            role: "full".to_string(),
                        },
                    ),
                    Message::GotApproveDevice,
                );
            }
            Message::GotApproveDevice(Ok(CliResponse::Ok { .. })) => {
                return self.fetch_devices();
            }

            // Real-time events
            Message::DaemonEvent(CliResponse::Event { event }) => {
                self.event_log
                    .push(format!("{}: {}", event.event_type, event.data));
                // Refresh relevant state based on event type.
                match event.event_type.as_str() {
                    "file_synced" | "dag_synced" => {
                        return self.fetch_folders();
                    }
                    "conflict_detected" => {
                        if self.screen == Screen::Conflicts {
                            return self.fetch_conflicts();
                        }
                    }
                    "device_approved" | "device_join_requested" => {
                        if self.screen == Screen::Devices {
                            return self.fetch_devices();
                        }
                    }
                    _ => {}
                }
            }

            // Periodic refresh
            Message::Tick => {
                return self.fetch_status();
            }

            // Catch-all for error/unexpected responses.
            Message::GotStatus(Err(e))
            | Message::GotFolders(Err(e))
            | Message::GotFolderFiles(Err(e))
            | Message::GotConflicts(Err(e))
            | Message::GotDevices(Err(e))
            | Message::GotPending(Err(e))
            | Message::GotFileHistory(Err(e))
            | Message::GotCreateFolder(Err(e))
            | Message::GotSubscribeFolder(Err(e))
            | Message::GotUnsubscribeFolder(Err(e))
            | Message::GotResolveConflict(Err(e))
            | Message::GotRestoreVersion(Err(e))
            | Message::GotApproveDevice(Err(e)) => {
                self.daemon_error = Some(e);
            }

            // Catch unexpected response shapes.
            _ => {}
        }
        Task::none()
    }
}

// ---------------------------------------------------------------------------
// IPC fetch helpers
// ---------------------------------------------------------------------------

impl App {
    /// Fetch all data needed on initial connect.
    fn fetch_all(&self) -> Task<Message> {
        Task::batch([
            self.fetch_status(),
            self.fetch_folders(),
            self.fetch_conflicts(),
            self.fetch_devices(),
        ])
    }

    fn fetch_status(&self) -> Task<Message> {
        let path = self.socket_path.clone();
        Task::perform(ipc::send(path, CliRequest::Status), Message::GotStatus)
    }

    fn fetch_folders(&self) -> Task<Message> {
        let path = self.socket_path.clone();
        Task::perform(
            ipc::send(path, CliRequest::ListFolders),
            Message::GotFolders,
        )
    }

    fn fetch_conflicts(&self) -> Task<Message> {
        let path = self.socket_path.clone();
        Task::perform(
            ipc::send(
                path,
                CliRequest::ListConflicts {
                    folder_id_hex: None,
                },
            ),
            Message::GotConflicts,
        )
    }

    fn fetch_devices(&self) -> Task<Message> {
        let path = self.socket_path.clone();
        let path2 = self.socket_path.clone();
        Task::batch([
            Task::perform(
                ipc::send(path, CliRequest::ListDevices),
                Message::GotDevices,
            ),
            Task::perform(
                ipc::send(path2, CliRequest::ListPending),
                Message::GotPending,
            ),
        ])
    }
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

impl App {
    fn subscription(&self) -> iced::Subscription<Message> {
        let daemon_ok = self.daemon_running == Some(true);
        if daemon_ok {
            iced::Subscription::batch([
                // Real-time events from murmurd.
                ipc::event_subscription(self.socket_path.clone()).map(Message::DaemonEvent),
                // Periodic status refresh every 5 s.
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

    // -- Daemon check --

    fn view_daemon_check(&self) -> Element<'_, Message> {
        let content = match self.daemon_running {
            None => column![text("Connecting to murmurd…").size(20)],
            Some(false) => column![
                text("murmurd is not running").size(20),
                text("Start the daemon first, then retry.").size(14),
                button(text("Retry")).on_press(Message::RetryDaemonCheck),
                button(text("Setup new network")).on_press(Message::Navigate(Screen::Setup)),
            ]
            .spacing(12),
            Some(true) => column![text("Connected.").size(20)],
        }
        .padding(30)
        .max_width(400);

        container(content)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    // -- Setup --

    fn view_setup(&self) -> Element<'_, Message> {
        match self.setup_step {
            SetupStep::ChooseMode => self.view_setup_choose(),
            SetupStep::Form => self.view_setup_form(),
        }
    }

    fn view_setup_choose(&self) -> Element<'_, Message> {
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
            button(text("Back to daemon check")).on_press(Message::RetryDaemonCheck),
        ]
        .spacing(16)
        .padding(30)
        .max_width(400);

        container(col)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn view_setup_form(&self) -> Element<'_, Message> {
        let title = if self.join_mode {
            "Join Network"
        } else {
            "Create Network"
        };

        let name_input = text_input("Device name (e.g., My Desktop)", &self.device_name)
            .on_input(Message::DeviceNameChanged)
            .padding(10);

        let mut col = column![
            button(text("Back")).on_press(Message::SetupBack),
            text(title).size(24),
            rule::horizontal(1),
            name_input,
        ]
        .spacing(12)
        .padding(30)
        .max_width(600);

        if self.join_mode {
            col = col.push(
                text_input("Enter mnemonic phrase…", &self.mnemonic_input)
                    .on_input(Message::MnemonicInputChanged)
                    .padding(10),
            );
        }

        col = col.push(
            text(
                "This will start murmurd with the given settings. \
                  Make sure murmurd is installed and in your PATH.",
            )
            .size(12),
        );

        let can_proceed = !self.device_name.is_empty()
            && if self.join_mode {
                !self.mnemonic_input.is_empty()
            } else {
                true
            };

        let label = if self.join_mode {
            "Start daemon & join"
        } else {
            "Start daemon & create"
        };
        let mut btn = button(text(label));
        if can_proceed {
            btn = btn.on_press(Message::StartDaemon);
        }
        col = col.push(btn);

        if let Some(ref err) = self.setup_error {
            col = col.push(text(format!("Error: {err}")).color([1.0, 0.3, 0.3]));
        }

        container(col)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    // -- Main layout with sidebar --

    fn view_main(&self) -> Element<'_, Message> {
        let conflict_count = self.conflicts.len();
        let conflict_label = if conflict_count > 0 {
            format!("Conflicts ({conflict_count})")
        } else {
            "Conflicts".to_string()
        };

        let sidebar = container(
            column![
                text("Murmur").size(20),
                rule::horizontal(1),
                self.nav_button("Folders", Screen::Folders),
                self.nav_button(&conflict_label, Screen::Conflicts),
                self.nav_button("Devices", Screen::Devices),
                self.nav_button("Status", Screen::Status),
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
            Screen::DaemonCheck | Screen::Setup => unreachable!(),
        };

        row![sidebar, container(content).width(Length::Fill).padding(16)]
            .height(Length::Fill)
            .into()
    }

    // -- Folders --

    fn view_folders(&self) -> Element<'_, Message> {
        let mut col = column![
            row![
                text("Folders").size(24).width(Length::Fill),
                button(text("New Folder")).on_press(Message::CreateFolder),
            ]
            .spacing(8),
            rule::horizontal(1),
        ]
        .spacing(8);

        if self.folders.is_empty() {
            col = col.push(text("No folders yet. Create one to get started.").size(14));
        } else {
            for folder in &self.folders {
                let status_badge = if folder.subscribed {
                    "Subscribed"
                } else {
                    "—"
                };
                let mode_str = folder.mode.as_deref().unwrap_or("—");

                let mut r = row![
                    text(&folder.name).size(16).width(Length::Fill),
                    text(format!("{} files", folder.file_count)).width(Length::Fixed(100.0)),
                    text(status_badge).width(Length::Fixed(100.0)),
                    text(mode_str).width(Length::Fixed(100.0)),
                ]
                .spacing(8);

                if folder.subscribed {
                    r = r
                        .push(button(text("Open")).on_press(Message::SelectFolder(folder.clone())));
                    r = r.push(
                        button(text("Unsub"))
                            .on_press(Message::UnsubscribeFolder(folder.folder_id.clone())),
                    );
                } else {
                    r = r.push(
                        button(text("Subscribe"))
                            .on_press(Message::SubscribeFolder(folder.folder_id.clone())),
                    );
                }

                col = col.push(r);
            }
        }

        scrollable(col).into()
    }

    // -- Folder detail --

    fn view_folder_detail(&self) -> Element<'_, Message> {
        let folder = match &self.selected_folder {
            Some(f) => f,
            None => {
                return text("No folder selected.").into();
            }
        };

        let mut col = column![
            row![
                button(text("Back")).on_press(Message::Navigate(Screen::Folders)),
                text(&folder.name).size(24),
            ]
            .spacing(8),
            text(format!(
                "ID: {}  |  {} files  |  Mode: {}",
                &folder.folder_id[..16],
                folder.file_count,
                folder.mode.as_deref().unwrap_or("—"),
            ))
            .size(12),
            rule::horizontal(1),
        ]
        .spacing(8);

        if self.folder_files.is_empty() {
            col = col.push(text("No files in this folder.").size(14));
        } else {
            // Header
            col = col.push(
                row![
                    text("Path").width(Length::Fill),
                    text("Size").width(Length::Fixed(100.0)),
                    text("Type").width(Length::Fixed(120.0)),
                    text("Actions").width(Length::Fixed(120.0)),
                ]
                .spacing(8),
            );

            for file in &self.folder_files {
                let size_str = format_size(file.size);
                let mime = file.mime_type.as_deref().unwrap_or("—");
                let r = row![
                    text(&file.path).width(Length::Fill),
                    text(size_str).width(Length::Fixed(100.0)),
                    text(mime).width(Length::Fixed(120.0)),
                    button(text("History")).on_press(Message::ViewFileHistory {
                        folder_id: file.folder_id.clone(),
                        path: file.path.clone(),
                    }),
                ]
                .spacing(8);
                col = col.push(r);
            }
        }

        scrollable(col).into()
    }

    // -- Conflicts --

    fn view_conflicts(&self) -> Element<'_, Message> {
        let mut col = column![text("Conflicts").size(24), rule::horizontal(1)].spacing(8);

        if self.conflicts.is_empty() {
            col = col.push(text("No active conflicts.").size(14));
        } else {
            for conflict in &self.conflicts {
                col = col
                    .push(text(format!("{} — {}", conflict.folder_name, conflict.path)).size(16));

                for version in &conflict.versions {
                    let hash_short = if version.blob_hash.len() > 16 {
                        &version.blob_hash[..16]
                    } else {
                        &version.blob_hash
                    };
                    let r = row![
                        text(format!(
                            "  {} ({}…)  HLC: {}",
                            version.device_name, hash_short, version.hlc
                        ))
                        .width(Length::Fill),
                        button(text("Keep")).on_press(Message::ResolveConflict {
                            folder_id: conflict.folder_id.clone(),
                            path: conflict.path.clone(),
                            chosen_hash: version.blob_hash.clone(),
                        }),
                    ]
                    .spacing(8);
                    col = col.push(r);
                }

                col = col.push(rule::horizontal(1));
            }
        }

        scrollable(col).into()
    }

    // -- File history --

    fn view_file_history(&self) -> Element<'_, Message> {
        let mut col = column![
            row![
                button(text("Back")).on_press(Message::Navigate(Screen::Folders)),
                text(format!("History: {}", self.history_path)).size(24),
            ]
            .spacing(8),
            rule::horizontal(1),
        ]
        .spacing(8);

        if self.history_versions.is_empty() {
            col = col.push(text("No versions found.").size(14));
        } else {
            for v in &self.history_versions {
                let hash_short = if v.blob_hash.len() > 16 {
                    &v.blob_hash[..16]
                } else {
                    &v.blob_hash
                };
                let r = row![
                    text(format!(
                        "{}…  by {}  HLC: {}  ({})",
                        hash_short,
                        v.device_name,
                        v.modified_at,
                        format_size(v.size),
                    ))
                    .width(Length::Fill),
                    button(text("Restore")).on_press(Message::RestoreVersion {
                        folder_id: self.history_folder_id.clone(),
                        path: self.history_path.clone(),
                        blob_hash: v.blob_hash.clone(),
                    }),
                ]
                .spacing(8);
                col = col.push(r);
            }
        }

        scrollable(col).into()
    }

    // -- Devices --

    fn view_devices(&self) -> Element<'_, Message> {
        let mut col = column![text("Devices").size(24)].spacing(8);

        if !self.pending.is_empty() {
            col = col.push(text("Pending Approval").size(18));
            for dev in &self.pending {
                let id_short = if dev.device_id.len() > 16 {
                    &dev.device_id[..16]
                } else {
                    &dev.device_id
                };
                let r = row![
                    text(format!("{} ({}…)", dev.name, id_short)).width(Length::Fill),
                    button(text("Approve")).on_press(Message::ApproveDevice(dev.device_id.clone())),
                ]
                .spacing(8);
                col = col.push(r);
            }
            col = col.push(rule::horizontal(1));
        }

        col = col.push(text("Approved Devices").size(18));
        if self.devices.is_empty() {
            col = col.push(text("No approved devices.").size(14));
        } else {
            for dev in &self.devices {
                let id_short = if dev.device_id.len() > 16 {
                    &dev.device_id[..16]
                } else {
                    &dev.device_id
                };
                let r = row![
                    text(&dev.name).width(Length::Fill),
                    text(&dev.role).width(Length::Fixed(80.0)),
                    text(format!("{}…", id_short)).size(11),
                ]
                .spacing(8);
                col = col.push(r);
            }
        }

        scrollable(col).into()
    }

    // -- Status --

    fn view_status(&self) -> Element<'_, Message> {
        let mut col = column![text("Status").size(24), rule::horizontal(1)].spacing(8);

        // Network info.
        col = col.push(text("Network").size(18));
        let nid_short = if self.status_network_id.len() > 16 {
            format!("{}…", &self.status_network_id[..16])
        } else {
            self.status_network_id.clone()
        };
        col = col.push(text(format!("Network ID: {nid_short}")));
        col = col.push(text(format!("Device: {} ({})", self.status_device_name, {
            if self.status_device_id.len() > 16 {
                format!("{}…", &self.status_device_id[..16])
            } else {
                self.status_device_id.clone()
            }
        })));
        col = col.push(text(format!(
            "Peers: {}  |  DAG entries: {}  |  Folders: {}  |  Conflicts: {}",
            self.status_peer_count,
            self.status_dag_entries,
            self.folders.len(),
            self.conflicts.len(),
        )));
        col = col.push(text(format!("Uptime: {}s", self.status_uptime_secs)).size(12));

        if let Some(ref err) = self.daemon_error {
            col = col.push(
                text(format!("Last error: {err}"))
                    .color([1.0, 0.3, 0.3])
                    .size(12),
            );
        }

        // Event log.
        col = col.push(rule::horizontal(1));
        col = col.push(text("Event Log").size(18));
        if self.event_log.is_empty() {
            col = col.push(text("No events yet.").size(14));
        } else {
            for event in self.event_log.iter().rev().take(50) {
                col = col.push(text(event).size(12));
            }
        }

        scrollable(col).into()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl App {
    fn nav_button(&self, label: &str, target: Screen) -> iced::widget::Button<'_, Message> {
        let mut btn = button(text(label.to_string()).width(Length::Fill)).width(Length::Fill);
        if self.screen != target {
            btn = btn.on_press(Message::Navigate(target));
        }
        btn
    }
}

/// Format a byte count for display.
fn format_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Get the user's home directory.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Start murmurd as a child process.
///
/// If `mnemonic` is `Some`, writes it to `~/.murmur/mnemonic` before starting
/// (join mode). Otherwise murmurd auto-creates a new network on first run.
async fn start_murmurd(name: String, mnemonic: Option<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let base = murmur_ipc::default_base_dir();
        std::fs::create_dir_all(&base).map_err(|e| format!("create dir: {e}"))?;

        // If joining, write the mnemonic so murmurd picks it up.
        if let Some(m) = &mnemonic {
            std::fs::write(base.join("mnemonic"), m).map_err(|e| format!("write mnemonic: {e}"))?;
        }

        let mut cmd = std::process::Command::new("murmurd");
        cmd.arg("--name").arg(&name);
        cmd.arg("--data-dir").arg(&base);

        // Detach so it outlives the desktop app if needed.
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        cmd.spawn().map_err(|e| format!("spawn murmurd: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}
