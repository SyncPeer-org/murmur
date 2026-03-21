# Murmur — Features

## Network & Identity

- **BIP39 mnemonic-based networks** — Generate a 12 or 24-word mnemonic to create a private sync network. The mnemonic is the single root of trust; all cryptographic material derives from it via HKDF.
- **Network isolation** — Each network has a unique ALPN (`murmur/0/<hex>`) preventing cross-network connections at the QUIC layer.
- **Deterministic first device** — The first device's Ed25519 keypair derives from the seed, making network creation reproducible from the same mnemonic.
- **Random device keys** — All subsequent devices generate fresh Ed25519 keypairs locally.

## Device Management

- **Device join with approval** — New devices enter the mnemonic and broadcast a join request. An existing approved device must explicitly approve before the new device participates.
- **Device roles** — Three roles: `Source` (produces files, e.g., phone), `Backup` (stores files, e.g., NAS), `Full` (both).
- **Device revocation** — Any approved device can revoke another. Revoked devices are excluded from sync and access.
- **Device name changes** — Human-readable device names can be updated at any time via DAG entry.
- **Auto-approve mode** — Server daemon (`murmurd`) supports automatic approval of join requests via config flag.

## File Synchronization

- **Push sync (automatic)** — Source devices automatically detect new files, compute blake3 hash, create a `FileAdded` DAG entry, and push blob data to backup nodes via iroh QUIC.
- **Content-addressed deduplication** — Files are identified by their blake3 hash. The same file from two devices produces the same hash and is stored once.
- **Blake3 integrity verification** — All blob transfers are verified against their blake3 hash on receipt. Corrupted transfers are rejected.
- **File deletion** — Files can be marked as deleted in the DAG via `FileDeleted` entries.
- **File metadata** — Each file tracks: blob hash, filename, size, MIME type, creation timestamp, and origin device.

## Access Control

- **On-demand pull access** — A device can request temporary access to another device's files via a point-to-point QUIC message.
- **Scoped access grants** — Access can be granted for: all files, files matching a prefix (e.g., `photos/2025/`), or a single file by hash.
- **Time-limited grants** — Access grants have an expiration timestamp. Access expires automatically.
- **Signed grants** — Each grant is signed by the grantor's Ed25519 key for authenticity.
- **Early revocation** — The grantor can revoke access before expiration via an `AccessRevoked` DAG entry.

## DAG (State Management)

- **Signed append-only DAG** — All state mutations are recorded as DAG entries: hash-chained (blake3), signed (Ed25519), and timestamped (HLC).
- **Hybrid Logical Clock** — Monotonic timestamps combining wall clock and logical counter. Thread-safe. Ensures causal ordering.
- **Automatic merge** — When multiple concurrent branches exist (multiple tips), the DAG auto-merges them.
- **Conflict resolution** — Last-Writer-Wins by HLC, with DeviceId as tiebreaker.
- **Delta sync** — Given a peer's current tips, compute exactly which entries they're missing (Kahn's topological sort).
- **Batch sync** — Apply multiple received entries at once with automatic topological ordering.
- **Materialized state** — Derived cache (device list, file index, access grants) rebuilt by replaying DAG entries. Platform can cache it but can always reconstruct from DAG.
- **In-memory DAG** — Core keeps the DAG in memory. Platform is responsible for persistence and feeding entries back on startup.

## Networking

- **iroh QUIC transport** — NAT hole punching, relay fallback, built-in peer discovery. Devices behind NAT can connect.
- **Gossip broadcast** — DAG entries propagate via iroh-gossip epidemic broadcast to all peers on the network topic. Payloads are deflate-compressed for efficiency.
- **Delta sync** — On peer connect (`NeighborUp`), peers exchange `DagSyncRequest` with their tips. The remote computes a delta (entries the requester is missing) and responds with `DagSyncResponse`. Both sides sync to convergence.
- **Blob sync** — When a `FileAdded` DAG entry arrives via gossip, the receiver requests missing blobs via `BlobRequest`. Small blobs (≤4 MB) are sent as a single `BlobResponse`; large blobs use chunked transfer via `BlobChunk` messages (1 MB chunks).
- **Wire compatibility** — `murmurd` and `murmur-ffi` use the same wire format (shared `murmur-net::wire` module): `GossipMessage` serialized with postcard, then compressed with deflate (1-byte flag prefix). Desktop and mobile peers interoperate seamlessly.
- **Point-to-point blob transfer** — Blob data transfers use direct QUIC streams (not gossip) for efficiency.
- **Connection pooling** — Reuse QUIC connections across multiple operations.
- **Length-prefixed messaging** — All messages use postcard serialization with length-prefix framing over QUIC.
- **Ping/Pong keepalive** — Heartbeat messages for connection liveness.

## Platform: Server Daemon (murmurd)

- **Headless operation** — Runs on NAS, Raspberry Pi, VPS without a display.
- **CLI management** — `init`, `start`, `approve`, `status` subcommands.
- **Fjall persistence** — DAG entries stored in Fjall v3 embedded key-value store.
- **Content-addressed blob storage** — Blobs stored on filesystem at `~/.murmur/blobs/<aa>/<rest>`.
- **TOML config** — `~/.murmur/config.toml` for device name, role, storage paths.
- **Graceful shutdown** — Signal handling for SIGTERM/SIGINT.
- **Boot persistence** — Reloads full DAG state from Fjall on startup.

## Platform: Desktop App (murmur-desktop)

- **iced UI** — Pure Rust GUI built with iced 0.14, visually compatible with COSMIC/Pop!_OS.
- **Setup wizard** — Device name input, create/join network toggle, mnemonic generation or entry.
- **Device management UI** — View approved devices, approve pending requests, revoke devices.
- **File browser** — List synced files with metadata, add files by filesystem path.
- **Status dashboard** — Device ID, DAG entry count, live event log.
- **Same storage as murmurd** — Fjall + filesystem, reuses the same `PlatformCallbacks` implementation.
- **Persistent config** — Auto-loads from `~/.murmur-desktop/config.toml` on startup.

## Platform: Android App

- **Jetpack Compose UI** — Material Design 3, with setup, devices, files, and status screens.
- **Foreground Service** — `MurmurService` runs the engine continuously with a persistent notification.
- **Room database** — DAG entries persisted as `DagEntryEntity(hash, data)` in Room/SQLite.
- **Content-addressed blob storage** — Blobs in `filesDir/blobs/<aa>/<rest>`.
- **MediaStore auto-upload** — `ContentObserver` on `MediaStore.Images` detects new photos and auto-uploads via `add_file()`.
- **DocumentsProvider** — Exposes synced files in Android's Files app. `queryRoots()`, `queryChildDocuments()`, `openDocument()`.
- **Boot receiver** — Automatically restarts `MurmurService` after device reboot.
- **ViewModel architecture** — `DeviceViewModel` and `FileViewModel` with StateFlow for reactive UI updates.
- **cargo-ndk integration** — Gradle tasks build the Rust FFI library for arm64-v8a, armeabi-v7a, and x86_64.
- **UniFFI Kotlin bindings** — Generated Kotlin wrapper around `MurmurHandle`.

## FFI Layer (murmur-ffi)

- **UniFFI 0.31 proc-macro** — No UDL file needed. Generates Kotlin and Swift bindings from Rust annotations.
- **Synchronous API** — All FFI calls are synchronous. Async engine calls are driven internally by a tokio runtime owned by `MurmurHandle`.
- **Thread-safe** — `MurmurHandle` wraps `Arc<Mutex<MurmurEngine>>`, safe to call from any thread. The `Arc` enables sharing the engine with async networking tasks.
- **Gossip networking** — `start()` creates an iroh endpoint, subscribes to gossip, and spawns background tasks for DAG delta sync and blob transfer. Same wire format as `murmurd` (shared via `murmur-net` wire utilities). `stop()` tears down networking. `connected_peers()` reports active gossip peer count.
- **Clean type boundary** — FFI wrapper types (`DeviceInfoFfi`, `FileMetadataFfi`, etc.) use `Vec<u8>` for bytes and `String` for IDs. No iroh or ed25519-dalek types cross the boundary.
- **Callback interface** — `FfiPlatformCallbacks` for platform-to-core communication (persist entries, store/load blobs, receive events).
- **Error handling** — `FfiError` enum with `InvalidMnemonic`, `InvalidDeviceId`, `OperationFailed`. No panics across FFI.

## Testing

- **175+ unit tests** across all core crates.
- **20+ integration tests** simulating multi-device scenarios with in-memory transport:
  - Two-device sync (create, join, approve, sync files)
  - Three-device topology (phone → NAS backup, tablet access request)
  - Concurrent edits with DAG merge
  - Offline reconnect and resync
  - Device revocation enforcement
  - Access grant lifecycle (request → grant → use → expiry)
  - Large file transfer with integrity verification
  - Deduplication across devices
  - DAG convergence after network partition
- **Android instrumented tests** — Room database, blob storage, DocumentsProvider, service lifecycle, engine integration.

## Security Properties

- **Ed25519 signatures** — Every DAG entry is signed by its author. Entries with invalid signatures are rejected.
- **Blake3 hash verification** — Every DAG entry hash and blob hash is verified. Tampered entries are rejected.
- **Network isolation** — ALPN-level isolation prevents devices from different networks from connecting.
- **Seed-based trust** — Only devices with the correct mnemonic can derive the network ID and join the gossip topic.
- **Approval gate** — Knowing the mnemonic alone is not enough; an existing device must approve the join.
