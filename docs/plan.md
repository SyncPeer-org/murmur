# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-03-21)

| Milestone               | Status                                                                                                |
| ----------------------- | ----------------------------------------------------------------------------------------------------- |
| 0–10                    | ✅ Complete (Types, Seed, DAG, Network, Engine, Server, Integration Tests, Desktop App, FFI, Android) |
| 11 — Daemon + CLI Split | ✅ Complete (murmur-ipc, murmur-cli, murmurd refactor, gossip networking, FFI networking)             |
| 12 — Hardening          | ✅ Complete (networking foundations, security, performance, observability, reliability)               |

---

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
