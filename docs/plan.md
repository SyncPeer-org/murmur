# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-03-20)

| Milestone               | Status                                                                                                |
| ----------------------- | ----------------------------------------------------------------------------------------------------- |
| 0–10                    | ✅ Complete (Types, Seed, DAG, Network, Engine, Server, Integration Tests, Desktop App, FFI, Android) |
| 11 — Daemon + CLI Split | ✅ Complete (murmur-ipc, murmur-cli, murmurd refactor, gossip networking)                             |
| 12 — Hardening          | 🔲 Planned                                                                                            |

---

## Milestone 12 — Hardening

**Crates**: across all crates, primarily `murmur-engine`, `murmurd`, `murmur-ffi`

**Goal**: Production-quality security, performance, and reliability. No new public API —
all changes are internal improvements and new configuration options.

### Security

- [ ] **Encrypted blob storage at rest**: AES-256-GCM with a key derived via
      `hkdf(seed, info="murmur/blob-encryption-key")`. `murmurd` and mobile platforms
      encrypt before write, decrypt after read. Core is unchanged (bytes in = bytes out).
- [ ] **Encrypted seed/keypair on disk**: In `murmurd`, wrap the stored seed and device
      keypair with a password-derived key (Argon2id). CLI prompts for password on `start`.
      Optional: keyring integration (Secret Service on Linux, Keychain on macOS/iOS, Keystore on Android).
- [ ] **Gossip message authentication**: already handled by DAG signatures, but add
      explicit sender verification in `GossipService` to reject messages from unknown devices
      before they reach the DAG layer.

### Performance

- [ ] **Chunked blob transfer**: split blobs > 4 MB into 1 MB chunks. Stream chunks over
      the QUIC connection instead of buffering the entire file in memory. Sender and receiver
      both process chunk-by-chunk. Add `BlobPushChunk` / `BlobPullChunk` message variants.
- [ ] **zstd compression**: compress DAG entry bytes before gossip broadcast and before
      writing to Fjall. Add `Compressed { algorithm, data }` wrapper in `MurmurMessage`.
      For blobs, compress text/JSON/document MIME types; skip already-compressed types
      (JPEG, PNG, MP4).
- [ ] **Bandwidth throttling**: configurable upload/download rate limit in `murmurd`
      config (tokens-per-second bucket). Primarily for NAS deployments on metered links.
- [ ] **Push queue persistence**: in `murmurd`, store the pending push queue in Fjall
      so it survives daemon restarts. Currently in-memory only.

### Observability

- [ ] **Progress events**: add `EngineEvent::BlobTransferProgress { blob_hash, bytes_sent, total_bytes }`
      so UIs can show a real progress bar for large transfers.
- [ ] **Metrics**: optional `prometheus` feature flag in `murmurd` exposing:
  - `murmur_dag_entries_total` (counter)
  - `murmur_blobs_stored_bytes` (gauge)
  - `murmur_sync_duration_seconds` (histogram)
  - `murmur_connected_peers` (gauge)
- [ ] **Health endpoint**: optional HTTP endpoint (`--http-port`) in `murmurd` serving
      `/health` (JSON: status, peer count, last sync time) for monitoring systems.

### Reliability

- [ ] **DAG compaction**: after N entries, emit a `Snapshot` entry capturing the full
      `MaterializedState`. Peers that are far behind can fast-forward to the snapshot instead
      of replaying the full history. Old entries before the snapshot can be archived.
- [ ] **Retry with backoff**: push queue retries should use exponential backoff with jitter
      (currently linear). Max retry interval: 30 minutes. Persist retry count in Fjall.
- [ ] **Peer discovery via mDNS**: use `mdns-sd` crate for LAN peer discovery, supplementing
      iroh's relay-based discovery. Reduces latency for local network syncs significantly.

### Tests (≥15)

- [ ] Encrypted blob: write encrypted, read back, verify plaintext matches
- [ ] Encrypted blob: tampered ciphertext → decryption error, blob rejected
- [ ] Encrypted seed: persist encrypted, reload with correct password → success
- [ ] Encrypted seed: reload with wrong password → error
- [ ] Chunked transfer: 8 MB file sent in 8 chunks, reassembled correctly on receiver
- [ ] Chunked transfer: corruption in chunk 3 → rejected, blake3 mismatch reported
- [ ] zstd roundtrip: compress DAG entry, decompress, verify identical
- [ ] Bandwidth throttle: upload speed stays within configured limit (±10%)
- [ ] Push queue persists across daemon restart: pending blobs re-queued on startup
- [ ] Progress events: `BlobTransferProgress` events fired for large transfers
- [ ] Snapshot entry: create snapshot, new peer joins, syncs via snapshot, not full history
- [ ] mDNS discovery: two daemons on LAN discover each other without relay

# DO NOT DO NOW, FOR LATER ONLY

## iOS App

**Directory**: `platforms/ios/`

**Goal**: Swift iOS application wrapping `murmur-ffi` via the generated Swift bindings.
iroh runs natively in the Rust core; Swift only handles iOS OS integration.

**Architecture**:

```
platforms/ios/
  MurmurApp/
    MurmurApp.swift           # @main, App lifecycle
    MurmurEngine.swift        # Swift wrapper around MurmurHandle
    MurmurService.swift       # Background task management
    PlatformCallbacksImpl.swift # implements PlatformCallbacks protocol
    Persistence/
      CoreDataStack.swift     # DAG entry persistence
      DagEntry+CoreData.swift
      BlobStore.swift         # Content-addressed blob storage in app container
    UI/
      DeviceListView.swift
      FileGridView.swift
      SyncStatusView.swift
    FileProvider/
      FileProviderExtension.swift
      FileProviderItem.swift
  MurmurCore.xcframework/     # built from murmur-ffi
  Package.swift               # SwiftPM (if using SPM for the framework)
```

**Tasks**:

- [ ] Xcode project + SwiftPM integration for `MurmurCore.xcframework`
- [ ] `MurmurEngine.swift` — wraps `MurmurHandle`, manages object lifecycle
- [ ] `PlatformCallbacksImpl` — implements the generated `PlatformCallbacks` protocol:
  - `onDagEntry(entryBytes:)` → Core Data insert
  - `onBlobReceived(blobHash:data:)` → write to `FileManager.default.containerURL()/blobs/`
  - `onBlobNeeded(blobHash:)` → read from same location
  - `onEvent(event:)` → post `NotificationCenter` notification → update SwiftUI state
- [ ] Core Data model:
  - `DagEntryMO`: `hash: String` (PK), `data: Data`
  - On startup: fetch all → call `loadDagEntry()` for each
- [ ] File Provider Extension (`MurmurFileProvider`):
  - `NSFileProviderManager` + `NSFileProviderItem` per synced file
  - `startProvidingItem(at:completionHandler:)` → `fetchBlob()` on demand
  - `importDocument(at:toParentItemIdentifier:completionHandler:)` → `addFile()`
- [ ] Background sync:
  - `BGProcessingTask` for periodic full DAG sync when app is in background
  - `BGAppRefreshTask` for lightweight heartbeat (check for pending approvals)
  - `NSURLSession` background transfer for large blobs (delegates to Rust core)
- [ ] PhotoKit integration:
  - `PHPhotoLibraryChangeObserver` → detect new photos
  - Hash + `addFile()` on new asset
- [ ] SwiftUI views: device list with approve sheet, file grid, sync status bar

**Build instructions** (xcframework from murmur-ffi):

```bash
cargo build --release --target aarch64-apple-ios -p murmur-ffi
cargo build --release --target aarch64-apple-ios-sim -p murmur-ffi
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libmurmur_ffi.a \
  -library target/aarch64-apple-ios-sim/release/libmurmur_ffi.a \
  -output MurmurCore.xcframework
```

**Tests** (≥10):

- [ ] `MurmurEngine` initializes without crash
- [ ] `onDagEntry` persists to Core Data
- [ ] Startup loads Core Data entries correctly
- [ ] `onBlobReceived` writes to expected path in container
- [ ] `onBlobNeeded` reads blob back
- [ ] File Provider item count matches `listFiles()` count
- [ ] `startProvidingItem` triggers `fetchBlob()`
- [ ] PhotoKit observer fires on new photo → `addFile()` called
- [ ] `BGProcessingTask` handler runs without crashing
- [ ] Approve device flow: join request notification → approve → device in list

---
