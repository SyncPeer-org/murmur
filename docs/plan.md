# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-03-27)

| Milestone                                                        | Status     |
| ---------------------------------------------------------------- | ---------- |
| 0–17 — MVP : DAG, Network, Engine, Darmon, FFI, Android, Desktop | ✅ Done    |
| 18 — Desktop App: IPC Refactor & Core Screens                    | 🔲 Planned |
| 19 — Zero-Config Onboarding & Default Folder                     | 🔲 Planned |
| 20 — System Tray & Notifications                                 | 🔲 Planned |
| 21 — Folder Discovery & Selective Sync                           | 🔲 Planned |
| 22 — Rich Conflict Resolution                                    | 🔲 Planned |
| 23 — Device Management Improvements                              | 🔲 Planned |
| 24 — Sync Progress, Pause/Resume & Bandwidth                     | 🔲 Planned |
| 25 — File Browser & Search                                       | 🔲 Planned |
| 26 — Settings & Configuration UI                                 | 🔲 Planned |
| 27 — Diagnostics & Network Health                                | 🔲 Planned |
| 28 — Web Dashboard (htmx)                                        | 🔲 Planned |

---

## Design Decisions (Milestones 13–20)

These decisions were agreed upon before implementation and guide all milestones below:

| Decision            | Choice                                                                              |
| ------------------- | ----------------------------------------------------------------------------------- |
| Folder model        | Syncthing-style shared folders mapped to real directories on each device            |
| Selective sync      | Per-device subscribe model; choose folders on join, add/remove later                |
| Folder permissions  | Self-selected: each device chooses read-write or read-only per folder               |
| File modifications  | Explicit version chain (`FileModified` action in DAG, full history)                 |
| Conflict strategy   | Fork-based: keep both versions on disk, surface to user for resolution              |
| Filesystem watching | `notify` crate in murmurd, auto-detect changes in shared folder directories         |
| Ignore patterns     | `.murmurignore` per folder (gitignore syntax), plus sensible defaults               |
| Interfaces          | Desktop app (iced) + Web UI (htmx) + CLI — all thin clients calling murmurd via IPC |
| Folder deletion     | Configurable: unsubscribing device chooses to keep or delete local files            |
| Large files         | No hard size limit; streaming blake3, chunked storage/transfer, bounded memory      |

---

## Phase 0: MVP

### Milestone 18 — Desktop App (iced) — Folders, Conflicts & Sync

**Crate**: `murmur-desktop`

**Goal**: Refactor the desktop app from a standalone engine-embedding app into a **thin IPC client**
to murmurd. Then add full UI for folder management, conflict resolution, file history, and sync status.
The desktop app, web UI, and CLI all use the same murmurd backend via IPC.

**Architecture change**: Currently `murmur-desktop` runs its own `MurmurEngine` with embedded storage
and networking. After this milestone, it connects to murmurd via Unix socket IPC (same protocol as
`murmur-cli`). Uses the `SubscribeEvents` IPC stream for real-time UI updates.

- On launch: check if murmurd is running (try connecting to socket). If not, offer to start it.
- All state comes from IPC responses. No local engine, no local storage, no local networking.

**New IPC commands**:

- `BlobPreview { blob_hash_hex, max_bytes }` — returns up to `max_bytes` raw bytes of blob content; murmurd reads from the local blob store; used by the Conflicts preview panel and File History screen
- `RestoreFileVersion { folder_id_hex, path, blob_hash_hex }` — writes the specified historical blob to the file's local path in the subscribed folder, displacing the current version on disk; murmurd's filesystem watcher picks up the change and emits a new `FileModified` DAG entry automatically

**Screens**:

1. **Setup** (existing, updated): Create network or join existing. Now also shows available folders to subscribe to after joining.
2. **Folders** (new): List all network folders. Create new folder. Subscribe/unsubscribe. Per-folder sync status indicator (synced, syncing, conflicts).
3. **Folder Detail** (new): File browser with directory tree for a specific folder. Shows file sizes, modification dates, device origin. Sync progress bar.
4. **Conflicts** (new): List of active conflicts across all folders. Each conflict shows the file path, competing versions with device names and timestamps. Buttons: "Keep this version", "Keep other", "Keep both". Preview panel if file is text/image.
5. **File History** (new): Version list for a selected file. Shows each version's hash, device, timestamp, size. "Restore" button to revert to a previous version.
6. **Devices** (existing, updated): Device list with per-folder subscription info. Approve/revoke. (Online/offline status and identicons are added in Milestone 23; this screen shows basic device info only.)
7. **Status** (existing, updated): Network overview — folder count, total files, total conflicts, connected peers, DAG entries, event log.

**Tasks**:

- [ ] Refactor `murmur-desktop` to remove embedded engine, storage, and networking
- [ ] Implement IPC client: connect to murmurd socket, send requests, receive responses
- [ ] Implement event stream listener: subscribe to murmurd events for real-time updates
- [ ] Auto-detect murmurd: check socket on launch, offer to start daemon
- [ ] **Folders screen**: list folders, create folder dialog, subscribe/unsubscribe buttons, sync status badges
- [ ] **Folder Detail screen**: directory tree view, file list with metadata, sync progress
- [ ] **Conflicts screen**: conflict list, version comparison, resolution buttons
- [ ] **File History screen**: version timeline, restore button
- [ ] **Updated Devices screen**: per-folder subscription info per device
- [ ] **Updated Status screen**: folder count, conflict count, event log with real-time updates
- [ ] **Updated Setup screen**: folder selection after joining network
- [ ] Navigation: sidebar with folder list + Devices + Status sections

**Tests** (≥8):

- [ ] App connects to murmurd via IPC socket
- [ ] Folder list view populates from IPC `ListFolders` response
- [ ] Create folder → IPC `CreateFolder` sent → folder appears in list
- [ ] Subscribe folder → IPC `SubscribeFolder` sent with correct mode
- [ ] Conflict list populates from IPC `ListConflicts` response
- [ ] Resolve conflict → IPC `ResolveConflict` sent → conflict removed from list
- [ ] File history view shows version chain from IPC `FileHistory` response
- [ ] Real-time event: file synced on another device → UI updates without manual refresh
- [ ] Graceful handling when murmurd is not running (error message, not crash)
- [ ] `BlobPreview` returns correct byte count capped at `max_bytes`
- [ ] `RestoreFileVersion` IPC sent with correct folder ID, path, and blob hash when "Restore" is clicked

---

## Phase 1: Feature Completion for Desktop App (iced)

All milestones in this phase target `murmur-desktop` as the primary crate. Many also extend
`murmur-ipc` and `murmurd` with new IPC commands required to support the new UI features.
Each milestone depends on Milestone 18 being complete (the thin IPC client refactor).

---

### Milestone 19 — Zero-Config Onboarding & Default Folder

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: Make Murmur work out of the box with zero folder configuration. When a network is
created, a "Murmur" folder is automatically created and subscribed at `~/Murmur`. When a device
joins and gets approved, it auto-subscribes to that same folder. Users share files by dropping
them in `~/Murmur` — no folder IDs, no paths to configure. Also adds QR code display for
effortless device pairing (scan instead of typing a 24-word mnemonic).

**New IPC commands**:

- `GetConfig` — returns the current daemon configuration (device name, role, network settings, folder list with local paths)
- `InitDefaultFolder { local_path }` — creates the "Murmur" default folder in the network and subscribes the local device to it; idempotent (safe to call if it already exists)

**Tasks**:

- [ ] Add `GetConfig` and `InitDefaultFolder` to `murmur-ipc`; implement both in `murmurd`
- [ ] On first network creation: send `InitDefaultFolder` with `~/Murmur`; create the directory on disk if it does not exist
- [ ] On first join + approval event: detect the default "Murmur" folder in the network (match by name), auto-subscribe at `~/Murmur`
- [ ] Welcome card on Setup completion: display `~/Murmur` path prominently with a "Your Murmur folder is ready" message
- [ ] QR code panel: display the network mnemonic as a QR code (using the `qrcode` crate, pure Rust) in the Setup screen after creation/join; also accessible via "Add Device" button on Devices screen
- [ ] Store `default_folder_initialized` flag in local app config (persisted) so the welcome flow does not repeat on restart
- [ ] Handle `~/Murmur` already existing on disk: use it without error
- [ ] Handle joining a network where the default folder was created by another device: still auto-subscribe after approval

**Tests** (≥9):

- [ ] First network creation sends `InitDefaultFolder` IPC command
- [ ] `InitDefaultFolder` causes `~/Murmur` directory to be created on disk
- [ ] Joining a network → receiving approval → `SubscribeFolder` sent for the "Murmur" folder
- [ ] Welcome card displays the `~/Murmur` path string
- [ ] QR code widget renders without panic for any valid mnemonic string
- [ ] QR code encodes the mnemonic verbatim (decoded QR == mnemonic)
- [ ] Calling `InitDefaultFolder` twice does not create duplicate folders (idempotent)
- [ ] `GetConfig` response populates the device name field in the UI
- [ ] `default_folder_initialized` flag prevents re-running init on second launch

---

### Milestone 20 — System Tray & Notifications

**Crate**: `murmur-desktop`

**Goal**: Murmur runs silently in the background and surfaces important events without requiring
the main window. A system tray icon shows sync status at a glance. Right-click menu provides
quick actions. System notifications alert the user to events needing attention (pending device
approvals, conflicts). Window close hides to tray rather than quitting.

**Dependencies**: `tray-icon` (tray icon + native menu), `notify-rust` (desktop notifications on Linux/macOS)

**New IPC commands**:

- `PauseSync` — pause all blob transfers globally; murmurd stops sending and receiving blob chunks
- `ResumeSync` — resume all blob transfers

**Tasks**:

- [ ] Add `PauseSync` and `ResumeSync` to `murmur-ipc`; implement in `murmurd` (gate blob chunk send/receive on an in-memory flag)
- [ ] Integrate `tray-icon` with the iced event loop; create the tray icon from embedded PNG bytes
- [ ] Tray icon visual states driven by event stream:
  - Syncing: animated indicator (cycle icon frames)
  - Up to date: solid/static icon
  - Conflict present: warning badge with conflict count
  - Daemon offline: grey icon
- [ ] Tray right-click menu:
  - "Open Murmur" — shows and focuses the main window
  - "Pause Sync" / "Resume Sync" — toggle; sends `PauseSync` / `ResumeSync` IPC
  - "N conflicts" — disabled label showing count; click opens Conflicts screen
  - "N pending approvals" — click opens Devices screen
  - Separator, then "Quit"
- [ ] Window close button (X): hide window, keep process alive in tray
- [ ] "Quit" in tray menu: send disconnect, exit process
- [ ] System notifications via `notify-rust`:
  - `DeviceJoinRequested` event → "New device wants to join: {name}"
  - `ConflictDetected` event → "Conflict in {folder}: {path}"
  - `DagSynced` with >0 new entries → "Sync complete" (debounced: one notification per sync burst, not one per entry)
- [ ] Notification preferences: per-type on/off toggles stored in local app config; read on startup

**Tests** (≥8):

- [ ] Tray icon is created on app startup without panic
- [ ] Tray menu "N conflicts" label shows the count from `ListConflicts` IPC response
- [ ] Clicking "Pause Sync" sends `PauseSync` IPC command
- [ ] Clicking "Resume Sync" sends `ResumeSync` IPC command
- [ ] Closing the main window keeps the process running (process is still alive after window close)
- [ ] `DeviceJoinRequested` engine event triggers a system notification
- [ ] `ConflictDetected` engine event triggers a system notification
- [ ] Notification type disabled in preferences → corresponding event does not trigger notification

---

### Milestone 21 — Folder Discovery & Selective Sync

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: When multiple folders exist on a network, any device can browse all available folders
and subscribe to only the ones it wants. A new device joining can see what is on offer before
committing storage. Folder invitations make it easy to share a specific folder via a short code
or QR scan.

**Dependencies**: `rfd` (native file dialog for local path selection, pure Rust)

**New IPC commands**:

- `ListNetworkFolders` — returns all folders on the network including unsubscribed ones; each entry includes subscriber count, file count, and creator device name
- `FolderSubscribers { folder_id_hex }` — returns the list of devices subscribed to a folder, with their sync mode

**Tasks**:

- [ ] Add `ListNetworkFolders` and `FolderSubscribers` to `murmur-ipc`; implement in `murmurd` (query materialized DAG state)
- [ ] Folders screen: two sections — "My Folders" (subscribed) and "Available on Network" (unsubscribed)
- [ ] Unsubscribed folder card: shows name, file count, subscriber count, creator device name, and a "Subscribe" button
- [ ] Subscribe flow: clicking Subscribe opens a native directory picker (`rfd::FileDialog`) for local sync path; then sends `SubscribeFolder`
- [ ] Folder Detail screen for unsubscribed folders: shows file list (read-only browse via `FolderFiles`), subscriber list (from `FolderSubscribers`), folder metadata, and a "Subscribe" button at top
- [ ] Folder invite: "Invite" button in subscribed Folder Detail generates a text code `murmur-invite:<network_id_hex>:<folder_id_hex>` and displays it as QR + copyable text; the code does NOT embed the mnemonic (NetworkId is a one-way derivation — you cannot reverse it); the recipient still needs the mnemonic to connect; the invite only tells the Setup screen which folder to auto-subscribe to after approval
- [ ] Join via invite: Setup screen accepts an invite code in addition to a bare mnemonic; the invite code validates that the network_id matches the entered mnemonic, then stores the folder_id for auto-subscription after approval
- [ ] Live update: when `FolderCreated` event arrives via event stream, refresh the network folder list

**Tests** (≥8):

- [ ] `ListNetworkFolders` returns both subscribed and unsubscribed folders
- [ ] Unsubscribed folders appear under "Available on Network" section
- [ ] Subscribe button on unsubscribed folder opens path picker then sends `SubscribeFolder` IPC
- [ ] Folder Detail for unsubscribed folder shows file list without requiring subscription
- [ ] `FolderSubscribers` IPC returns the correct list of device names and modes
- [ ] Invite code encodes the correct network ID and folder ID hex strings
- [ ] Joining with an invite code auto-subscribes to the specified folder after approval
- [ ] `FolderCreated` engine event causes the available folders list to refresh

---

### Milestone 22 — Rich Conflict Resolution

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: Conflict resolution must be understandable and actionable for non-technical users.
Side-by-side diffs for text conflicts, image previews for photo conflicts, bulk resolution for
power users, and per-folder auto-resolve rules for users who never want to be interrupted.

**Dependencies**: `similar` (pure Rust diff engine for computing text diffs)

**New IPC commands**:

- `BulkResolveConflicts { folder_id_hex, strategy }` — strategy: `KeepNewest` | `KeepLocal` | `KeepRemote`; resolves all conflicts in the folder atomically by calling `resolve_conflict` for each with the chosen hash
- `SetFolderAutoResolve { folder_id_hex, strategy }` — persisted as an `auto_resolve` field in the matching `[[folders]]` entry in `config.toml`; murmurd applies the strategy automatically when a new conflict is detected
- `DismissConflict { folder_id_hex, path }` — removes the conflict from the active list without choosing a version; both conflict files remain on disk with their existing conflict-named suffixes; no `ConflictResolved` DAG entry is created; this is the "keep both" action

Note: `BlobPreview` was already added in Milestone 18 and is reused here for conflict previews.

**Tasks**:

- [ ] Add `BulkResolveConflicts`, `SetFolderAutoResolve`, and `DismissConflict` to `murmur-ipc`; implement all in `murmurd`
- [ ] Conflict detail panel: clicking a conflict expands it to show both competing versions side by side — device name, timestamp (human-readable), size, blob hash (truncated)
- [ ] Text conflict diff: fetch both versions via `BlobPreview` (max 32 KB each, from M18), compute line-level unified diff with `similar`, render in a scrollable widget with added lines green and removed lines red
- [ ] Image conflict preview: fetch both blobs via `BlobPreview` (max 2 MB each), render as `iced::widget::image::Image` thumbnails side by side
- [ ] Resolution buttons per conflict: "Keep this version" and "Keep other version" send `ResolveConflict` with the chosen blob hash; "Keep both" sends `DismissConflict` — removes the active conflict marker while leaving both files on disk with their existing conflict suffixes for manual handling
- [ ] Bulk resolve toolbar: "Keep all newest", "Keep all mine", "Keep all theirs" buttons; confirmation dialog before sending `BulkResolveConflicts`
- [ ] Per-folder auto-resolve setting in Folder Detail settings panel: None / Newest / Mine dropdown; sends `SetFolderAutoResolve`; requires adding `auto_resolve` field to `FolderConfig` in `config.toml`; setting read back from `GetConfig` on panel open
- [ ] Conflict history: append resolved/dismissed conflicts (path, action, timestamp) to an in-memory list shown below the active conflicts list; cleared on app restart (session-scoped)
- [ ] Conflict count badge on sidebar navigation item; updated in real-time via event stream

**Tests** (≥9):

- [ ] Conflict detail panel shows device name and timestamp for both competing versions
- [ ] Text preview panel fetches both blob versions and renders them before the diff is computed
- [ ] Text diff correctly shows added/removed lines for two known text strings
- [ ] Image preview renders without panic for a valid JPEG blob
- [ ] "Keep this version" sends `ResolveConflict` with the correct `chosen_hash`
- [ ] "Keep both" sends `DismissConflict` and removes the conflict from the active list without deleting either file
- [ ] Bulk "Keep all newest" sends `BulkResolveConflicts` with `KeepNewest` strategy
- [ ] Confirmation dialog appears before bulk resolution executes
- [ ] `SetFolderAutoResolve` with `Newest` persists and is read back on panel reopen

---

### Milestone 23 — Device Management Improvements

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: Devices should feel like recognizable participants. Online/offline presence, visual
identicons, detailed per-device info, and QR-code-based joining make the device experience
trustworthy and approachable for non-technical users.

**New IPC commands**:

- `GetDevicePresence` — returns per-device online status and last-seen UNIX timestamp; murmurd tracks these from `MembershipEvent` gossip messages
- `SetDeviceName { name }` — renames the local device (creates a `DeviceNameChanged` DAG entry)

**Tasks**:

- [ ] Add `GetDevicePresence` and `SetDeviceName` to `murmur-ipc`; implement in `murmurd`
  - Track `device_id → (online: bool, last_seen_unix: u64)` in `murmurd` in-memory state, updated on each incoming `MembershipEvent` gossip message
- [ ] Device list: show colored dot (green = online, grey = offline) and "Online now" or "Last seen 3h ago" / "Never connected" text per device
- [ ] Poll `GetDevicePresence` every 30 s; also refresh immediately on `FileSynced` and `DagSynced` events from event stream
- [ ] Device identicon: derive a deterministic 5×5 pixel pattern + accent color from the 32-byte device ID; render as a small grid widget inline with the device name
- [ ] Device detail screen (open by clicking any device): shows name, truncated ID, role, identicon, subscribed folders (from `FolderSubscribers`), last-seen, files contributed (count from `FolderFiles`)
- [ ] Local device alias: text field in device detail for a user-defined nickname, stored in local app config (not in DAG); displayed in parentheses next to the device name
- [ ] QR code join: "Add Device" button on Devices screen opens a dialog displaying the mnemonic as a large QR code with the instruction "Scan on another device to join"
- [ ] Pending devices section: "Pending Approval" group at top of device list; select-all checkbox + "Approve selected" button that sends `ApproveDevice` for each
- [ ] Device filter tabs: "All" / "Online" / "Offline" / "Pending"

**Tests** (≥8):

- [ ] `GetDevicePresence` returns at least the local device entry
- [ ] Device with `online: true` shows a green indicator
- [ ] `last_seen_unix` renders as a correct relative-time string (e.g. "5 minutes ago")
- [ ] Identicon renders the same pixel pattern for the same device ID bytes on repeated calls
- [ ] Device detail shows folder subscriptions fetched from `FolderSubscribers` IPC
- [ ] Local alias saved in app config persists across restart
- [ ] "Add Device" QR code dialog encodes the mnemonic string
- [ ] "Approve selected" with two pending devices sends two `ApproveDevice` IPC calls

---

### Milestone 24 — Sync Progress, Pause/Resume & Bandwidth

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: Full visibility into what is syncing and control over when and how fast it happens.
Per-file progress bars, folder-level aggregate progress, pause/resume per folder, and bandwidth
throttle sliders make Murmur well-behaved on metered or slow connections.

**New IPC commands**:

- `PauseFolderSync { folder_id_hex }` — pauses blob send/receive for one folder; murmurd skips it when selecting blobs to push and rejects incoming chunks for it
- `ResumeFolderSync { folder_id_hex }` — resumes blob transfer for a paused folder

Note: `PauseSync` / `ResumeSync` (global) were added in Milestone 20. Bandwidth throttle (`SetThrottle`) is added in Milestone 26 alongside the Settings UI that exposes it.

**Tasks**:

- [ ] Add `PauseFolderSync` and `ResumeFolderSync` to `murmur-ipc`; implement in `murmurd`
  - Paused-folder set: `HashSet<FolderId>` in murmurd state; blob selection skips paused folders; incoming chunk handler checks the set and returns a transient error for paused folders
- [ ] Folder Detail: active transfers panel at the top — each in-flight file shows its name, `bytes_transferred / total_bytes`, a progress bar, and an ETA string; driven by `BlobTransferProgress` events from the event stream; ETA calculated from a 10-second sliding-window bytes/second estimate
- [ ] Folder list card: aggregate progress indicator "12 / 47 files synced" when a folder is actively syncing; refreshed by polling `FolderStatus`
- [ ] Pause/Resume button per folder in both the folder list card and the Folder Detail header; sends `PauseFolderSync` / `ResumeFolderSync`
- [ ] Paused folders display a "Paused" badge and a muted sync indicator; the tray conflict badge from M20 also shows pause state
- [ ] Sync activity log at the bottom of the Status screen: scrollable list of engine events with timestamps — shows `FileSynced`, `BlobReceived`, `ConflictDetected`, `DagSynced`, `DeviceApproved`; live-updated via event stream; capped at 500 entries (oldest entries dropped when limit is reached)

**Tests** (≥8):

- [ ] `BlobTransferProgress` event updates the correct file's progress bar
- [ ] ETA field is non-empty when bytes/sec > 0
- [ ] Folder list card shows "N / M files" ratio during active sync
- [ ] Pause button sends `PauseFolderSync` with the correct folder ID
- [ ] Resume button sends `ResumeFolderSync` with the correct folder ID
- [ ] Paused folder shows "Paused" badge in both the folder list card and the Folder Detail header
- [ ] Sync activity log appends one entry per received engine event
- [ ] Sync activity log drops the oldest entry when the 501st event arrives (cap enforcement)

---

### Milestone 25 — File Browser & Search

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: Power users need to find and interact with their files without leaving Murmur.
Cross-folder search, type/date/device filters, a file preview pane, and OS integration
(open, reveal, copy path) make Murmur a productive interface for synced content.

**Dependencies**: `open` (pure Rust wrapper for `xdg-open` / `open` / `start`)

**New IPC commands**:

- `DeleteFile { folder_id_hex, path }` — creates a `FileDeleted` DAG entry; murmurd removes the blob from the local folder path on disk

Note: `BlobPreview` (added in M18) is reused for the preview pane.

**Tasks**:

- [ ] Search bar in the Folders screen header: client-side filter across all `FolderFiles` results by file name substring (case-insensitive)
- [ ] Filter panel in Folder Detail (collapsible sidebar):
  - File type: All / Images / Videos / Documents / Archives / Other (grouped by MIME prefix)
  - Date modified: Any / Today / This week / This month / Custom range
  - Device origin: dropdown of contributing device names
  - Size: Any / < 1 MB / 1–100 MB / > 100 MB
- [ ] Sort controls (clickable column headers in file list): Name / Size / Modified / Device; click again to reverse order
- [ ] Preview pane (right-side panel, togglable with a button):
  - Text files (`text/*` MIME): raw content via `BlobPreview` IPC (max 16 KB), rendered in a scrollable monospace widget
  - Image files (`image/*` MIME): decoded via `BlobPreview` (max 2 MB), rendered with `iced::widget::image::Image`
  - All other types: metadata card (hash, size, device origin, timestamps)
- [ ] File context menu (right-click):
  - "Open" — `open::that(local_absolute_path)` if the file exists locally; error toast if file not present locally
  - "Reveal in file manager" — `open::that(parent_directory)`
  - "Copy path" — places the absolute local path in the system clipboard
  - "View history" — navigates to File History screen for this file
  - "Delete" — confirmation dialog, then sends `DeleteFile` IPC
- [ ] Directory tree in Folder Detail: files grouped by path prefix; directory nodes expand/collapse; state preserved per-folder per session
- [ ] Recent Files view (new screen accessible from sidebar): aggregates `FolderFiles` across all subscribed folders, sorted by `modified_at` descending, showing the 100 most recent

**Tests** (≥8):

- [ ] Search "abc" shows only files whose names contain "abc" (case-insensitive)
- [ ] Filter by "Images" type excludes files without an `image/` MIME prefix
- [ ] Sorting by size orders file entries by the `size` field (both ascending and descending)
- [ ] Text preview pane fetches and displays content via `BlobPreview` IPC
- [ ] Image preview pane renders without panic for a valid PNG blob
- [ ] "Copy path" places the correct absolute path string in the clipboard
- [ ] "Delete" confirmation dialog appears before `DeleteFile` IPC is sent
- [ ] Directory tree renders correct nesting for a folder containing subdirectories

---

### Milestone 26 — Settings & Configuration UI

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: All murmurd configuration options are accessible from the UI without editing
`config.toml` by hand. A comprehensive Settings screen covers device identity, appearance,
notifications, sync behavior, network settings, and storage. Per-folder settings live in
Folder Detail. A mnemonic backup flow with a verification quiz ensures users never lose access.

**New IPC commands**:

- `SetAutoApprove { enabled }` — toggle the `network.auto_approve` flag; persist to `config.toml`
- `SetMdns { enabled }` — toggle the `network.mdns` flag; persist to `config.toml`
- `ReclaimOrphanedBlobs` — delete blobs from disk that have no corresponding DAG entry; return bytes freed
- `SetFolderLocalPath { folder_id_hex, new_local_path }` — change where a folder syncs locally; murmurd moves existing files to the new path
- `GetIgnorePatterns { folder_id_hex }` — reads and returns the contents of the folder's `.murmurignore` file; returns an empty string if the file does not exist
- `SetIgnorePatterns { folder_id_hex, patterns }` — writes the given newline-separated pattern string to the folder's `.murmurignore` file; murmurd reloads the ignore ruleset immediately
- `SetThrottle { upload_bytes_per_sec, download_bytes_per_sec }` — updates `ThrottleConfig` at runtime (0 = unlimited); murmurd applies a token-bucket rate limiter and persists to `config.toml`
- (`GetConfig`, `SetDeviceName`, `SetFolderAutoResolve`, `PauseSync`, `ResumeSync` already added in earlier milestones)

**Tasks**:

- [ ] Add `SetAutoApprove`, `SetMdns`, `ReclaimOrphanedBlobs`, `SetFolderLocalPath`, `SetIgnorePatterns` to `murmur-ipc`; implement all in `murmurd`
- [ ] Settings screen with sectioned layout (tabs or sidebar):
  - **Device**: editable device name (sends `SetDeviceName` on save); read-only role; copy-able device ID
  - **Appearance**: Dark / Light / System theme toggle; Small / Medium / Large font size — persisted in local app config
  - **Notifications**: per-type on/off toggles (device approval request, conflict detected, sync complete) — persisted locally
  - **Sync**: global pause toggle (sends `PauseSync` / `ResumeSync`, added in M20); auto-start murmurd on login toggle (Linux: `~/.config/autostart/*.desktop`, macOS: `~/Library/LaunchAgents/*.plist`); socket path display
  - **Bandwidth**: upload throttle slider; download throttle slider (sends `SetThrottle`); steps: unlimited / 512 KB/s / 1 MB/s / 2 MB/s / 5 MB/s / 10 MB/s; current values loaded from `GetConfig` on screen open
  - **Network**: auto-approve toggle (sends `SetAutoApprove`); mDNS toggle (sends `SetMdns`)
  - **Storage**: data directory path; total blob storage size; "Reclaim orphaned blobs" button (sends `ReclaimOrphanedBlobs`; shows bytes-freed in a toast)
- [ ] Per-folder settings panel (gear icon in Folder Detail header):
  - Local path display + "Change" button (path picker → sends `SetFolderLocalPath`)
  - Sync mode toggle: Read-Write / Read-Only (sends `SetFolderMode`)
  - Auto-resolve strategy: None / Newest / Mine (sends `SetFolderAutoResolve`)
  - Ignore patterns: text area pre-filled via `GetIgnorePatterns` IPC; "Save" button sends `SetIgnorePatterns`
  - Folder name is read-only (renaming requires a new DAG action — planned for a future milestone)
  - "Unsubscribe from folder" button (confirmation, sends `UnsubscribeFolder`)
- [ ] Mnemonic backup screen (link in Device settings):
  - Show all mnemonic words numbered in a grid
  - "I've written them down" checkbox to enable the Continue button
  - Verification quiz: three randomly chosen word positions; user types the word; green check on correct; red on wrong; must pass all three to complete
- [ ] "Reset / Leave Network" in Device settings: three-click confirmation (warning → type "RESET" → final confirm); sends disconnect IPC then deletes murmurd's data directory (`~/.murmur`: config, mnemonic, device key, DAG database, blob store); user files in subscribed folders on disk (e.g., `~/Murmur`) are NOT deleted

**Tests** (≥10):

- [ ] `SetDeviceName` IPC sent when device name field is saved with a new value
- [ ] Theme "Dark" persists across app restart (loaded from local config)
- [ ] Notification toggle disabled state persists across restart
- [ ] `SetAutoApprove { enabled: true }` sent when auto-approve toggle is switched on
- [ ] Bandwidth upload slider sends `SetThrottle` with the correct `upload_bytes_per_sec` value
- [ ] `GetConfig` populates bandwidth slider positions on Settings screen open
- [ ] `ReclaimOrphanedBlobs` IPC sent on button click; toast displays bytes freed
- [ ] `SetFolderLocalPath` IPC sent with the new path after path picker confirms
- [ ] `SetFolderMode` IPC sent when sync mode toggle changes in per-folder panel
- [ ] `GetIgnorePatterns` IPC pre-fills the ignore patterns text area on per-folder panel open
- [ ] `SetIgnorePatterns` IPC sent with the text area contents when "Save" is clicked
- [ ] Mnemonic backup screen shows the correct words from `ShowMnemonic` IPC response
- [ ] Mnemonic verification quiz rejects a wrong word and accepts the correct one

---

### Milestone 27 — Diagnostics & Network Health

**Crates**: `murmur-desktop`, `murmur-ipc`, `murmurd`

**Goal**: When things go wrong, users and developers need visibility into connection quality,
sync errors, and storage state. A diagnostics screen surfaces the information needed to
self-diagnose issues or produce a useful report.

**New IPC commands**:

- `ListPeers` — returns per-peer connection info: device ID, device name, connection type (relay / direct), last-seen UNIX timestamp; murmurd tracks this from iroh endpoint events
- `StreamLogs { level }` — long-lived streaming IPC (like `SubscribeEvents`); forwards `tracing` log records at or above the requested level as event responses
- `StorageStats` — returns per-folder file count and bytes on disk, total blob count and bytes, orphaned blob count and bytes, DAG entry count, and fjall DB size on disk
- `RunConnectivityCheck` — murmurd tests reachability to the iroh relay server and returns `{ relay_reachable: bool, latency_ms: Option<u64> }`
- `ExportDiagnostics { output_path }` — serializes stats, last 100 log lines, and peer list (no file content) as JSON to the given path; returns the path on success

**Tasks**:

- [ ] Add all five commands to `murmur-ipc`; implement all in `murmurd`:
  - `ListPeers`: track `NodeId → (name, connection_type, last_seen)` in murmurd state from iroh endpoint peer events
  - `StreamLogs`: add a `tracing` subscriber layer that forwards records to a broadcast channel; IPC handler subscribes and streams to the client
  - `StorageStats`: query fjall for DAG entry count and DB size; walk the blob directory for file counts and sizes; track orphaned blob count from delta between blob dir and DAG hashes
  - `RunConnectivityCheck`: attempt an iroh relay QUIC connection; measure round-trip time
  - `ExportDiagnostics`: collect stats + recent logs + peer list; serialize as JSON
- [ ] Network Health screen (new screen, accessible from sidebar):
  - Peer list table: device name, truncated ID, relay/direct badge, last-seen relative time
  - DAG stats card: entry count, tip count
  - "Run connectivity check" button: shows spinner then result card (relay reachable, latency ms)
- [ ] Log Viewer screen (accessible from Status screen or sidebar):
  - Level filter buttons: Error / Warn / Info / Debug
  - Search text input: client-side filter of displayed lines
  - Scrollable log output with auto-scroll; pauses auto-scroll when user scrolls up manually
  - "Export" button: sends `ExportDiagnostics` with a timestamped filename in `~/murmur-diagnostics/`; shows file path in toast
- [ ] Storage Inspector section in the Settings Storage tab (supplementing M26):
  - Table: per-folder name, file count, total bytes on disk
  - Summary row with totals
  - Orphaned blobs row: count and bytes

**Tests** (≥8):

- [ ] `ListPeers` returns a valid (possibly empty) list without error
- [ ] `StorageStats` returns a file count that matches the number of files in a test folder
- [ ] `RunConnectivityCheck` returns a result struct without panicking
- [ ] Log Viewer receives at least one log line after startup via `StreamLogs`
- [ ] Log level filter "Error" hides lines below Error severity
- [ ] Log search filters displayed lines by substring (case-insensitive)
- [ ] `ExportDiagnostics` creates a file at the specified output path
- [ ] Exported diagnostics file is valid JSON containing a `"peers"` key
