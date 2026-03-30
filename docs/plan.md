# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-03-29)

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
| 30 — Onboarding                                                  | 🔲 Planned |
| 31 — Desktop App UX                                              | 🔲 Planned |
| 32 — Miscellaneous Quality-of-Life                               | 🔲 Planned |

---

## Milestone 29 — Conflict Resolution Improvements

Better tooling and automation for conflict resolution.

### Features

- **Conflict diff viewer** — `murmur-cli conflicts diff <folder> <path>` renders a unified diff for text file conflicts directly in the terminal using the `similar` crate
- **Conflict expiry** — conflicts older than N days (configurable) that haven't been manually resolved get auto-resolved with "keep both" strategy; prevents accumulation of stale conflicts

### Implementation

1. Add `similar` crate as a dependency of `murmur-cli`
2. Add `ConflictDiff` IPC request that fetches both blob versions and returns their raw bytes
3. Implement `conflicts diff` subcommand in `murmur-cli` — load both blobs, detect if text (UTF-8), render unified diff with `similar::TextDiff`; fall back to "binary files differ" for non-text
4. Add `conflict_expiry_days: Option<u64>` to folder config in `config.toml` (default: `None` = disabled)
5. On each tick / DAG rebuild, check conflict timestamps against expiry; auto-resolve expired conflicts with "keep both" (write both versions to disk with conflict suffixes)
6. Add `SetConflictExpiry` IPC request; wire into CLI (`murmur-cli folder set-conflict-expiry <id> <days>`) and desktop settings

### Tests

- Unit test: `similar`-based diff output for known text inputs
- Unit test: conflict expiry logic triggers auto-resolve after configured days
- Integration test: create a conflict, set expiry to 0, verify auto-resolution

---

## Milestone 30 — Onboarding

Streamline the device pairing and folder setup experience.

### Features

- **QR code device pairing** — `murmur-cli mnemonic qr` renders the mnemonic as a QR code in the terminal using `qrcodegen` + Unicode block characters; scan on mobile to onboard without typing 12 words
- **Folder templates** — preset `.murmurignore` rule sets for common use cases: `--template code` (excludes `node_modules/`, `target/`, `.git/`), `--template photos` (includes only image/video extensions), `--template documents`

### Implementation

1. Add `qrcodegen` crate as a dependency of `murmur-cli`
2. Implement `murmur-cli mnemonic qr` — fetch mnemonic via `ShowMnemonic` IPC, encode as QR, render using Unicode half-block characters (`▀▄█ `) for terminal display
3. Define built-in template map in `murmur-cli`: `code`, `photos`, `documents` with their respective ignore patterns
4. Add `--template <name>` flag to `murmur-cli folder create`; on folder creation, write the template's patterns as the initial `.murmurignore` via `SetIgnorePatterns` IPC
5. Expose templates in the desktop app's create-folder flow as a dropdown

### Tests

- Unit test: QR encoding roundtrip — encode mnemonic, verify QR data matches
- Unit test: each template produces valid ignore patterns
- CLI test: `folder create --template code` creates folder with correct `.murmurignore`

---

## Milestone 31 — Desktop App UX

Polish the desktop experience with transfer visibility and cross-platform packaging.

### Features

- **Sync progress with ETA** — during large transfers, show speed (MB/s), percentage, and estimated time remaining in the desktop UI
- **macOS / Windows desktop builds** — port the existing Linux iced desktop app to macOS and Windows with platform-appropriate packaging (DMG, MSI)

### Implementation

1. Extend `TransferInfoIpc` with `started_at_unix: u64` field (or add a new `TransferProgress` IPC response with speed/ETA)
2. In the desktop app's status/folder-detail views, compute transfer speed from `bytes_transferred` delta over time; estimate ETA from remaining bytes / current speed
3. Add a progress bar widget (iced `ProgressBar`) to the transfers section showing percentage and "X MB/s — ~Y min remaining"
4. macOS: set up `cargo-bundle` or manual `app bundle` build producing a `.app` with `Info.plist`; package as DMG using `create-dmg`
5. Windows: cross-compile with `x86_64-pc-windows-msvc` target; package with WiX or Inno Setup; sign with code signing certificate if available
6. CI: add GitHub Actions matrix builds for Linux, macOS, Windows

### Tests

- Unit test: ETA calculation — given bytes transferred, total bytes, and elapsed time, verify correct speed and ETA
- Build test: `cargo build -p murmur-desktop` succeeds on each target platform (CI matrix)

---

## Milestone 32 — Miscellaneous Quality-of-Life

Small but impactful improvements across daemon, CLI, and filesystem handling.

### Features

- **Duplicate detection** — warn (but don't block) when `add` or `modify` sees a file whose blake3 hash already exists under a different path in the same folder
- **Case-conflict detection** — on case-insensitive filesystems (macOS, Windows), warn when two files differ only in case; prevent silent overwrites
- **Configurable filesystem watch debounce** — allow users to set the debounce delay (default 500ms) in `config.toml`; useful for IDEs that write in rapid bursts
- **`murmur-cli doctor`** — comprehensive self-diagnostic: checks daemon running, socket accessible, config valid, disk space, network connectivity, DAG integrity, blob store completeness

### Implementation

1. **Duplicate detection**: in `handle_forward_sync_event`, after computing blake3, query the engine for existing files with same hash in the folder; emit `tracing::warn!` and optionally an `EngineEvent::DuplicateDetected`
2. **Case-conflict detection**: on file add/modify, normalize path to lowercase and check for collisions in the folder's file map; emit a warning event; block the add on case-insensitive platforms if a case-variant exists
3. **Configurable debounce**: add `watch_debounce_ms: u64` to `config.toml` (default 500); pass to `FolderWatcher::new()`; update the debounce interval in the watcher
4. **`murmur-cli doctor`**: add `Doctor` IPC request that returns a structured diagnostic report; checks: daemon reachable (socket connect), config parseable (`GetConfig`), storage accessible (`StorageStats`), DAG integrity (verify all hashes/signatures), blob completeness (all referenced hashes exist on disk), network connectivity (`RunConnectivityCheck`); print results as a checklist with pass/fail

### Tests

- Unit test: duplicate detection emits warning for same-hash different-path
- Unit test: case-conflict detection catches "README.md" vs "readme.md"
- Unit test: debounce config is respected (events within debounce window are coalesced)
- Integration test: `murmur-cli doctor` returns all-pass on a healthy daemon

---

## Phase 2: Desktop app remaining Features

System Tray & Notifications
