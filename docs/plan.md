# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-04-17)

| Milestone                                                        | Status     |
| ---------------------------------------------------------------- | ---------- |
| 0–17 — MVP : DAG, Network, Engine, Daemon, FFI, Android, Desktop | ✅ Done    |
| 18 — Desktop App: IPC Refactor & Core Screens                    | ✅ Done    |
| 19 — Zero-Config Onboarding & Default Folder                     | ✅ Done    |
| 20 — System Tray & Notifications (IPC: PauseSync/ResumeSync)     | ✅ Done    |
| 21 — Folder Discovery & Selective Sync                           | ✅ Done    |
| 22 — Rich Conflict Resolution                                    | ✅ Done    |
| 23 — Device Management Improvements                              | ✅ Done    |
| 24 — Sync Progress, Pause/Resume & Bandwidth                     | ✅ Done    |
| 25 — File Browser & Search                                       | ✅ Done    |
| 26 — Settings & Configuration UI                                 | ✅ Done    |
| 27 — Diagnostics & Network Health                                | ✅ Done    |
| 29 — Conflict Resolution Improvements                            | 🔲 Planned |
| 30 — Onboarding                                                  | ✅ Done    |
| 31 — Sync Progress & Desktop UX Polish                           | 🔲 Planned |
| 32 — Miscellaneous Quality-of-Life                               | 🔲 Planned |
| 33 — Cross-Platform Desktop Builds & Distribution                | 🔲 Planned |
| 34 — iOS App                                                     | 🔲 Planned |

---

## Milestone 29 — Conflict Resolution Improvements

Better tooling and automation for conflict resolution. The diff viewer and expiry must
work consistently across CLI and desktop — conflicts are mostly resolved in the GUI.

### Features

- **Conflict diff viewer** — unified diff for text file conflicts, available in both CLI (`murmur-cli conflicts diff <folder> <path>`) and the desktop Conflicts view (inline panel)
- **Conflict expiry** — conflicts older than N days (configurable) that haven't been manually resolved get auto-resolved using the folder's existing auto-resolve strategy (`none/newest/mine` from M22), or fall back to "keep both" if strategy is `none`. Prevents accumulation of stale conflicts without overriding user intent.

### Implementation

1. Add `similar` crate as a dependency of `murmur-cli` and `murmur-desktop`
2. Add `ConflictDiff` IPC request returning a `ConflictDiff` response with both blobs' raw bytes plus a `is_text: bool` flag (UTF-8 detection done daemon-side, single source of truth)
3. Implement `conflicts diff` subcommand in `murmur-cli` — render unified diff with `similar::TextDiff`; fall back to "binary files differ — N vs M bytes" for non-text
4. Add an inline diff preview panel to the desktop `views/conflicts.rs` reusing the same IPC; collapsed by default, expand-on-click
5. Add `conflict_expiry_days: Option<u64>` to folder config in `config.toml` (default: `None` = disabled)
6. On each tick / DAG rebuild, check conflict timestamps against expiry. For expired conflicts: invoke the folder's configured auto-resolve strategy if set; otherwise apply "keep both" (write both versions to disk with conflict suffixes). Emit `EngineEvent::ConflictAutoResolved { folder, path, strategy }` so the desktop activity feed surfaces the action.
7. Add `SetConflictExpiry` IPC request; wire into CLI (`murmur-cli folder set-conflict-expiry <id> <days>`) and desktop settings

### Tests

- Unit test: `similar`-based diff output for known text inputs
- Unit test: binary detection — non-UTF-8 bytes flagged as binary
- Unit test: expiry triggers auto-resolve using folder's strategy when set
- Unit test: expiry falls back to "keep both" when strategy is `none`
- Integration test: create a conflict, set expiry to 0, verify auto-resolution and event emission

---

## Milestone 30 — Onboarding ✅ Done

Streamline device pairing and folder setup. Reduce reliance on typing 12 words and
template the common `.murmurignore` cases. The mnemonic-verification step from the
original plan was dropped at the user's request — see "Scope changes" below.

### Features (shipped)

- **QR pairing token (preferred)** — short-lived [`PairingToken`](../crates/murmur-seed/src/pairing.rs)
  (32-byte random nonce + 5-minute expiry + issuer `DeviceId` + ed25519 signature +
  AES-256-GCM-encrypted mnemonic). Encoded as `murmur://join?token=<base64url>`. Mint via
  `murmur-cli pair invite` (renders an ASCII QR) or the desktop **Devices → Invite device**
  button. Joiners redeem offline with `murmur-cli pair redeem <url>` — no running daemon
  needed, since the URL is self-contained.
- **Raw mnemonic QR (fallback)** — `murmur-cli mnemonic-cmd qr --i-understand-this-is-secret`.
  Gated behind an explicit consent flag; prints a stderr warning before the QR.
- **Invite-link URL scheme** — `murmur://join?token=…` deep links. Android registered via
  `AndroidManifest.xml` intent-filter (`scheme="murmur"`, `host="join"`). Linux registered
  via `crates/murmur-desktop/packaging/linux/murmur-desktop.desktop` (`MimeType=x-scheme-handler/murmur`).
  macOS/Windows registration deferred to M33 (signed installers).
- **Folder templates** — `murmur_ipc::templates::{TEMPLATES, template_patterns}` — stable
  slugs `rust`, `node`, `python`, `photos`, `documents`, `office`. Exposed via
  `murmur-cli folder create <name> --local-path <path> --template rust` and as a
  click-to-prefill button row in the desktop folder-detail "Ignore Patterns" card.

### Scope changes from the original plan

- **Mnemonic verification step was dropped.** Not implemented at user request.
- **One-shot Noise handshake over iroh was simplified** to a self-contained URL: the
  mnemonic is encrypted with AES-256-GCM under a key derived (HKDF-SHA256) from the URL's
  random nonce. Security properties:
  - Signed origin (joiner verifies the invite came from a real network device).
  - 5-minute expiry; single-use enforced per-daemon on the issuer side.
  - **No forward secrecy against URL capture** — same threat model as reading a paper
    mnemonic. Acceptable because the QR is a brief visual handshake; we don't claim
    stronger. Documented in `crates/murmur-seed/src/pairing.rs`.

### Implementation

1. `qrcodegen` added to `murmur-cli` and `murmur-desktop` workspace deps. `image` and
   `data-encoding` added to the workspace; `data-encoding` + `aes-gcm` + `postcard`
   promoted to hard deps of `murmur-seed` for the pairing module.
2. `murmur-seed::pairing` module hosts `PairingToken`, `PairingError`,
   `MURMUR_URL_SCHEME`, `JOIN_URL_HOST`, `DEFAULT_EXPIRY_SECS`, plus
   `issue` / `issue_default` / `redeem` / `to_url` / `from_url`.
3. `murmur-ipc` gained `CliRequest::{IssuePairingInvite, RedeemPairingInvite}` and
   `CliResponse::{PairingInvite, RedeemedMnemonic}`. `CliRequest::CreateFolder` gained
   an optional `ignore_patterns` field written to `.murmurignore` after folder creation.
4. Daemon signs invites with its device key; tracks redeemed nonces in-process to reject
   same-daemon replays.
5. `murmur-cli pair invite` renders a terminal QR (half-block ASCII). `pair redeem`
   works fully offline — no daemon needed on the joining side.
6. `murmur-ipc::templates` is the single source of truth for template slugs, descriptions,
   and patterns. Both the CLI (`--template`) and the desktop template-row buttons consume
   it directly.
7. Linux URL-scheme registration shipped as a `.desktop` file in
   `crates/murmur-desktop/packaging/linux/`. Android manifest updated. macOS/Windows
   packaging intentionally deferred to M33.

### Tests

- `murmur-seed::pairing`: 9 unit tests covering sign/verify, URL roundtrip, expiry,
  wrong-issuer rejection, tampered-ciphertext rejection, trailing-fragment tolerance,
  nonce uniqueness, default expiry window.
- `murmur-ipc::templates`: every slug has patterns + description; non-empty; unknown
  slug returns `None`; key templates exclude/include expected paths.
- `murmurd` bin tests: `IssuePairingInvite` returns a parseable URL, the same daemon's
  `RedeemPairingInvite` recovers the network mnemonic, replays are rejected, invalid
  URLs are rejected, `CreateFolder { ignore_patterns: Some(...) }` writes `.murmurignore`,
  and `None` does not.
- `murmur-cli::offline`: re-export parity with `murmur-ipc::templates`.
- Integration (`tests/integration/pairing.rs`): end-to-end invite → redeem → identical
  `NetworkIdentity`; cross-signed invites rejected; expired invites rejected; every
  template slug exposes non-empty multi-line patterns.

---

## Milestone 31 — Sync Progress & Desktop UX Polish

Polish the desktop experience with transfer visibility and ergonomic improvements.
Cross-platform builds are split into M33 — they're a separate concern with very
different risks (signing, notarization, updaters) and shouldn't gate UX work.

### Features

- **Sync progress with ETA (smoothed)** — during large transfers, show speed (MB/s), percentage, and estimated time remaining. Use **EWMA over a 30s window** for the speed sample (`speed_t = α·instantaneous + (1−α)·speed_{t−1}`, α≈0.2). Naive `bytes/elapsed` is too jittery on real networks to produce useful ETAs.
- **Drag-and-drop into folder** — drag files/folders from the OS file manager onto the desktop folder-detail view to add them to the sync set. iced supports drop targets natively.
- **Per-folder color/icon** — small visual marker users can set per folder, surfaced in the sidebar and tray menu. Reduces "which folder am I looking at?" friction with many folders.
- **Activity feed** — a chronological view of recent engine events (synced, conflict detected/resolved, device joined, transfer started/finished). Replaces "did anything just happen?" guesswork.
- **Notification preferences** — per-event toggles in settings: conflict, transfer-completed, device-joined, error. Currently all-or-nothing.

### Implementation

1. Extend `TransferInfoIpc` with `started_at_unix: u64` and `last_progress_unix: u64`. Compute speed/ETA daemon-side (using EWMA); expose `bytes_per_sec_smoothed: u64` and `eta_seconds: Option<u64>` on the IPC response so all clients render identical numbers.
2. Add a progress bar widget (iced `ProgressBar`) to the transfers section showing percentage and "X MB/s — ~Y min remaining".
3. Wire iced drop-target events on `views/folders.rs` folder-detail to call `AddFile` IPC for each dropped path; show progress per file.
4. Add `color_hex: Option<String>` and `icon: Option<String>` fields to folder local config (`murmurd config.toml`, not the DAG — these are per-device cosmetic). Add `SetFolderColor` / `SetFolderIcon` IPC.
5. Replace today's tray notification logic with a `notification_settings: NotificationSettings` config struct; respect per-event toggles in the tray code.
6. Add `views/activity.rs` consuming the existing `SubscribeEvents` stream; ring-buffer last 200 events in the desktop process; render newest-first.

### Tests

- Unit test: EWMA ETA — given a sequence of progress samples, verify smoothed speed converges and ETA stabilizes within ±10% after 30s
- Unit test: notification settings — disabled events do not enqueue tray notifications
- UI smoke test: drag-and-drop calls `AddFile` for each path (mock IPC)
- Visual: confirm activity feed updates in near-real-time when files sync

---

## Milestone 32 — Miscellaneous Quality-of-Life

Small but impactful improvements across daemon, CLI, and filesystem handling.
Two anti-goals worth calling out:

- **Don't block sync silently.** Several proposed "warnings" can become user-facing
  stalls if implemented as hard rejects. Default to *quarantine + warn*, never *drop*.
- **Don't bury signals in `tracing`.** `tracing::warn!` is invisible to GUI users —
  every detection here must also emit an `EngineEvent` so the activity feed surfaces it.

### Features

- **Duplicate detection** — warn (never block) when `add` or `modify` sees a file whose blake3 hash already exists under a different path in the same folder. Surface in desktop activity feed and CLI status.
- **Case-conflict detection** — on case-insensitive filesystems (macOS, Windows), detect when two files differ only in case. **Quarantine** the second variant under `<folder>/.murmur-quarantine/<original-path>.case-N` rather than blocking the add. Emit a warning event so the user can rename one. Blocking would cause confusing "files not appearing" support reports.
- **Configurable filesystem watch debounce** — allow users to set the debounce delay (default 500ms) in `config.toml` and via desktop settings; useful for IDEs that write in rapid bursts (saving on every keystroke).
- **`murmur-cli doctor`** — comprehensive self-diagnostic with a `--deep` mode for expensive checks. Default mode is fast (sub-second); `--deep` verifies cryptographic integrity.
- **Selective scrub** — `murmur-cli scrub <folder>` re-verifies all blob hashes for a folder against the DAG. Used after suspected disk corruption or filesystem repair.
- **Dry-run flags** — `--dry-run` on destructive operations (`folder remove`, `leave-network`, `reclaim-orphans`) shows what would happen without doing it.
- **Daemon backup/restore** — `murmur-cli backup <out.tar.zst>` exports config + DAG + key material (encrypted with the mnemonic); `restore <in.tar.zst>` rehydrates a daemon. For migrations and disaster recovery. (Blobs are *not* in the backup — they re-sync from peers.)

### Implementation

1. **Duplicate detection**: in `handle_forward_sync_event`, after computing blake3, query the engine for existing files with same hash in the folder. Emit `tracing::warn!` AND `EngineEvent::DuplicateDetected { folder, new_path, existing_paths, hash }`. Desktop activity feed renders these.
2. **Case-conflict detection**: on file add, normalize path to lowercase and check the folder's file map. On collision, write to `<folder>/.murmur-quarantine/<path>.case-<N>` and emit `EngineEvent::CaseConflictQuarantined { folder, original_path, quarantine_path }`. The quarantine directory is in default `.murmurignore`.
3. **Configurable debounce**: add `watch_debounce_ms: u64` to `config.toml` (default 500, min 50, max 10000); pass to `FolderWatcher::new()`. Add `SetWatchDebounce` IPC and surface in desktop settings.
4. **`murmur-cli doctor`**: add `Doctor { deep: bool }` IPC request. Fast checks: daemon reachable (socket connect), config parseable (`GetConfig`), storage accessible (`StorageStats`), all subscribed folder paths exist + writable, disk space ≥ 1 GB free, relay connectivity (`RunConnectivityCheck`), peer reachability (`ListPeers` shows ≥ 1 alive peer if any are configured), HLC clock reasonable (within ±5 min of system time). Deep checks: DAG signature verification for every entry, blob hash verification for every blob, blob completeness (all referenced hashes exist on disk). Print as checklist with pass/fail/warn.
5. **Scrub**: add `ScrubFolder { folder_id_hex: String }` IPC. Streams progress events; reports any blob whose disk content doesn't hash to its expected value. Quarantines corrupt blobs (move to `<blob_store>/.corrupt/`) so re-sync from peers can restore them.
6. **Dry-run**: add `dry_run: bool` to `RemoveFolder`, `LeaveNetwork`, `ReclaimOrphanedBlobs` IPC requests; CLI exposes `--dry-run`. Daemon returns the would-be effect (file count, byte count, etc.) without applying.
7. **Backup/restore**: serialize config + DAG entries + signing key material into a tar.zst archive, AES-256-GCM encrypted with a key derived from the mnemonic (HKDF, distinct salt from network ID). `murmur-cli backup` and `restore` subcommands.

### Tests

- Unit test: duplicate detection emits both `tracing::warn!` and `EngineEvent::DuplicateDetected`
- Unit test: case-conflict moves to quarantine directory and emits event (does not block add)
- Unit test: debounce config is respected; out-of-range values clamped
- Integration test: `murmur-cli doctor` returns all-pass on a healthy daemon
- Integration test: `doctor --deep` detects a tampered DAG entry (manually corrupt one byte)
- Integration test: `scrub` detects a corrupt blob (truncate one) and quarantines it
- Integration test: `folder remove --dry-run` reports byte count without removing
- Integration test: backup → wipe daemon → restore → DAG state identical, peers re-sync blobs

---

## Milestone 33 — Cross-Platform Desktop Builds & Distribution

Ship the desktop app on macOS, Windows, and Linux with proper packaging, code signing,
and auto-update. This is split from M31 because the work is largely orthogonal to UX
(it's mostly toolchain / CI / signing key management) and has very different failure
modes (signing-cert expiry, notarization rejection, store review).

### Features

- **macOS build** — universal binary (x86_64 + aarch64), signed with a Developer ID Application certificate, notarized via `notarytool`, packaged as DMG with `create-dmg`
- **Windows build** — x86_64 binary, signed with an EV or OV code-signing certificate, packaged as MSI (WiX v4) and a portable zip
- **Linux packaging** — AppImage (portable, no install), `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE); Flatpak as stretch goal
- **Auto-update** — `murmur-desktop` polls a release manifest URL on startup + every 24h; downloads new versions, verifies a minisign signature, prompts the user to restart. Manifest hosted on GitHub Releases.
- **CI matrix** — GitHub Actions: build + test on Linux/macOS/Windows; release pipeline tags → builds → signs → notarizes → uploads artifacts → updates manifest

### Implementation

1. macOS: `cargo-bundle` for `.app` (with `Info.plist`, including `murmur://` URL scheme registration); `lipo` to merge x86_64+aarch64 binaries; `codesign` + `notarytool`; `create-dmg` for the DMG installer
2. Windows: `cargo build --target x86_64-pc-windows-msvc`; WiX v4 toolset for MSI; `signtool` for signing
3. Linux: `cargo-appimage` or manual AppImage assembly; `cargo-deb` for `.deb`; `cargo-generate-rpm` for `.rpm`
4. Auto-update: add `update.rs` to `murmur-desktop`; manifest schema `{ version, url, minisign_sig, sha256, channel }`; verify with the project's pinned minisign public key (compiled in)
5. GitHub Actions: matrix `os: [ubuntu-latest, macos-latest, windows-latest]`; tag-triggered release workflow uses encrypted secrets for signing keys (Apple App Store Connect API key, Windows code-signing PFX, minisign secret key)
6. Document the signing-key rotation procedure in `docs/release.md` (new file)

### Tests

- CI: `cargo build -p murmur-desktop` succeeds on each matrix entry
- CI: produced macOS `.app` passes `spctl --assess` (Gatekeeper)
- CI: produced Windows MSI is signed (`signtool verify /pa`)
- Manual smoke: install on each OS, verify daemon starts, verify `murmur://` URL handler works
- Unit test: update manifest signature verification — valid sig accepted, tampered manifest rejected
- Unit test: version comparison — only update when newer (semver)

---

## Milestone 34 — iOS App

Port the existing Android architecture (Rust core via FFI + native UI) to iOS.
Listed as planned in `docs/features.md` but absent from the plan — adding it here.

### Features

- **iOS app** — Swift + SwiftUI app embedding the `murmur-ffi` Rust core
- **Background sync** — using `BGProcessingTask` for periodic sync when the app is suspended; full background mode is restricted on iOS so sync windows are best-effort
- **Photos auto-backup** — opt-in PhotoKit observer that adds new photos/videos to a designated folder, similar to the Android flow
- **Files app integration** — File Provider extension exposing synced folders inside the iOS Files app

### Implementation

1. Verify `murmur-ffi` cross-compiles to `aarch64-apple-ios` and `aarch64-apple-ios-sim` (it should — pure Rust, no C deps)
2. Generate Swift bindings: continue with UniFFI (already used for Kotlin) or hand-write a thin C-ABI wrapper if UniFFI's Swift output proves friction-prone
3. Build the SwiftUI app: `Onboarding`, `Folders`, `FolderDetail`, `Conflicts`, `Devices`, `Settings` screens — same information architecture as Android for consistency
4. Background sync: register `BGProcessingTask` identifiers; schedule from foreground on app-background transition; the task runs the engine for up to ~30s
5. Photos backup: PhotoKit `PHPhotoLibrary` observer; add new assets to the configured folder via the FFI engine
6. File Provider extension: implement `NSFileProviderReplicatedExtension`; back it with the same engine instance via shared App Group container
7. Distribution: TestFlight first; App Store review will likely require justifying the background networking entitlement

### Tests

- Build: iOS app builds and links against `murmur-ffi` for both device and simulator targets
- Integration: pair an iOS device into a network with an existing daemon; verify file sync both directions
- Manual: foreground sync, background sync window, conflict detection, photo auto-backup
- App Store: TestFlight build accepted by Apple review

---

## Phase 2: Desktop app remaining Features

System Tray & Notifications
