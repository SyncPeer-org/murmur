## Phase 2: Feature Completion for Web Dashboard

### Milestone 28 — Web Dashboard (htmx)

**Crate**: `murmurd` (HTTP module expansion)

**Dependencies**: `askama` (Jinja-like templates, pure Rust, compile-time checked), htmx (JS library, ~14 KB, embedded as static asset)

**Goal**: Full-featured web management dashboard served by murmurd. Server-rendered HTML with htmx for
dynamic updates. Accessible at `http://localhost:<port>` when murmurd is started with `--http-port`.
Provides the same management capabilities as the desktop app, for use on headless servers or when
a browser is more convenient.

**Architecture**:

```
Browser ──HTTP──▶ murmurd (axum) ──▶ Engine (same in-process engine used by IPC)
                      │
                      ├── GET /                       → dashboard
                      ├── GET /folders                → folder list (all network folders)
                      ├── POST /folders               → create folder
                      ├── POST /folders/:id/subscribe → subscribe with local path
                      ├── GET /folders/:id            → folder detail + file browser
                      ├── GET /folders/:id/history/*  → file version history
                      ├── GET /devices                → device list with online/offline status
                      ├── POST /devices/:id/approve   → approve pending device
                      ├── POST /devices/:id/revoke    → revoke device
                      ├── GET /conflicts              → conflict list
                      ├── POST /conflicts/resolve     → resolve conflict
                      ├── POST /conflicts/dismiss     → dismiss (keep both)
                      ├── POST /conflicts/bulk        → bulk resolve
                      ├── GET /settings               → settings page
                      ├── POST /settings/throttle     → set bandwidth limits
                      ├── POST /sync/pause            → pause global sync
                      ├── POST /sync/resume           → resume global sync
                      ├── POST /sync/:id/pause        → pause folder sync
                      ├── POST /sync/:id/resume       → resume folder sync
                      ├── GET /storage                → storage stats
                      ├── GET /events (SSE)           → Server-Sent Events stream
                      └── GET /static/*               → htmx, CSS
```

**Pages**:

1. **Dashboard** (`GET /`): Overview cards — folder count, device count, conflict count, connected peers, DAG entries. Live-updating via htmx polling or SSE. Pause/resume sync toggle.
2. **Folders** (`GET /folders`): Both subscribed and available-on-network folders. Subscribe action with local path input. Pause/resume per folder. Sync status badges.
3. **Folder Detail** (`GET /folders/:id`): File list with directory grouping, sizes, dates, device origin. File history link per file. Subscribe/unsubscribe/change-mode controls.
4. **File History** (`GET /folders/:id/history/*path`): Version timeline for a file. Restore button.
5. **Devices** (`GET /devices`): Approved + pending devices with online/offline indicators. Approve/revoke buttons. Per-device folder subscriptions.
6. **Conflicts** (`GET /conflicts`): Conflict list grouped by folder. Each shows competing versions with device names and timestamps. Resolve / Keep Both / Bulk-resolve buttons.
7. **Settings** (`GET /settings`): Bandwidth throttle sliders, auto-approve toggle, mDNS toggle, storage stats with orphaned blob reclaim.

**htmx patterns**:

- `hx-post` for actions (approve device, resolve conflict, create folder)
- `hx-get` with `hx-trigger="every 2s"` for live sync status on dashboard
- Server-Sent Events (`GET /events`) with `hx-ext="sse"` for real-time conflict/sync notifications
- `hx-swap="outerHTML"` for partial page updates after actions
- `hx-confirm` for destructive actions (remove folder, revoke device)

**Styling**: Minimal, clean CSS embedded in templates. No CSS framework dependency. Dark/light mode via `prefers-color-scheme`.

**Tasks**:

- [ ] Add `askama` dependency to `murmurd`; remove `metrics` feature gate (web UI always enabled when `--http-port` is set)
- [ ] Embed htmx JS as static asset via `include_str!`
- [ ] Create base template with navigation sidebar: Dashboard / Folders / Devices / Conflicts / Settings
- [ ] Dashboard page: overview cards, pause/resume sync toggle (`POST /sync/pause`, `POST /sync/resume`); live-update via `hx-trigger="every 3s"`
- [ ] Folders list page: both subscribed and all-network folders; subscribe form with local path input; per-folder pause/resume; sync status badges
- [ ] Folder detail page: file list with directory grouping, sizes, dates, device origin; file history links
- [ ] File history page: version timeline per file with restore button (`POST /folders/:id/restore`)
- [ ] Devices page: online/offline indicator per device (polled from `GetDevicePresence` equivalent in-process); approve/revoke; per-device folder subscriptions
- [ ] Conflicts page: resolve buttons; "Keep Both" (`POST /conflicts/dismiss`); bulk resolve form (`POST /conflicts/bulk`)
- [ ] Settings page: bandwidth throttle form (`POST /settings/throttle`); auto-approve toggle; mDNS toggle; storage stats with orphaned blob reclaim button
- [ ] SSE endpoint (`GET /events`) streaming `EngineEvent`s as `text/event-stream`
- [ ] CSS: clean, minimal, responsive, dark/light via `prefers-color-scheme`; no external CSS framework
- [ ] Wire all routes to the shared in-process engine (same instance as IPC handler)

**Tests** (≥12):

- [ ] Dashboard page returns 200 with correct folder/device/conflict counts
- [ ] Folder list page renders both subscribed and unsubscribed network folders
- [ ] `POST /folders` creates folder and redirects to folder list
- [ ] `POST /folders/:id/subscribe` subscribes device and redirects
- [ ] Device list shows online indicator for recently-active devices
- [ ] `POST /devices/:id/approve` approves pending device
- [ ] Conflicts page renders all active conflicts with device names and timestamps
- [ ] `POST /conflicts/resolve` resolves conflict and removes it from the list
- [ ] `POST /conflicts/dismiss` dismisses conflict without choosing a version
- [ ] `POST /sync/pause` pauses global sync
- [ ] SSE endpoint streams at least one event after a file is synced
- [ ] `POST /settings/throttle` updates bandwidth config and persists to `config.toml`
- [ ] Static assets (htmx JS, CSS) served with correct content-type
- [ ] File history page shows version list for a known file

---

## Later milestones

### Milestone 29 — Protocol Specification v0.1

**Location**: `docs/protocol.md`

**Goal**: Write a complete, implementer-facing specification of the Murmur protocol. Any developer
should be able to read this document and build a compatible client in any language. The spec covers
wire formats, DAG semantics, folder model, conflict resolution, and security — everything needed for
interoperability.

**Sections**:

1. **Overview** — What Murmur is, design goals (privacy, no servers, BIP39 trust root), target use cases
2. **Terminology** — Definitions: network, device, folder, DAG, entry, tip, HLC, blob
3. **Cryptographic Primitives** — Ed25519 (signing), blake3 (hashing), HKDF-SHA256 (key derivation), BIP39 (mnemonic), AES-256-GCM (blob encryption at rest)
4. **Identity & Key Derivation** — NetworkId derivation from mnemonic, DeviceId from Ed25519 public key, first-device deterministic key, joiner random key, ALPN string format
5. **DAG Structure** — Entry format (hlc, device_id, action, parents, hash, signature), hash computation algorithm (exact byte layout), signature scheme, HLC rules
6. **Actions** — Complete list of all action variants with:
   - Serialization format (postcard field order)
   - Authorization rules (who can create this action)
   - State transition (how it modifies materialized state)
7. **Folder Model** — SharedFolder, FolderSubscription, SyncMode, folder lifecycle (create → subscribe → use → unsubscribe → remove)
8. **File Versioning** — FileMetadata fields, version chains, path semantics (relative, forward-slash separated, UTF-8)
9. **Conflict Detection & Resolution** — Algorithm (ancestry check on concurrent file mutations), ConflictInfo structure, resolution protocol, conflict file naming convention
10. **Wire Protocol** — Message framing (4-byte BE length prefix + postcard payload), all `MurmurMessage` variants with field layouts, compression (deflate with 1-byte flag), chunking (1 MiB chunks, 4 MiB threshold)
11. **Gossip Protocol** — Topic derivation from NetworkId, `GossipPayload` variants, PlumTree semantics (eager push + lazy pull), DAG sync algorithm (tip exchange → delta computation → entry transfer)
12. **Blob Transfer** — Push vs pull, chunked streaming, backpressure (ack-based), integrity verification (blake3), resume semantics
13. **Discovery & Connection** — iroh relay servers, direct connections, ALPN negotiation (`murmur/0/<hex_prefix>`), hole punching
14. **Security Model** — Threat model (what Murmur protects against, what it doesn't), approval gate (mnemonic alone is insufficient), blob encryption at rest, transport encryption (QUIC TLS), replay protection (HLC + signature)
15. **IPC Protocol** — Unix socket, framing, request/response types, event streaming — for local tool integration
16. **Compatibility & Versioning** — Protocol version negotiation, backwards compatibility policy, breaking change rules

**Deliverables**:

- [ ] Write `docs/protocol.md` — complete v0.1 specification
- [ ] Include worked examples: hash computation for a sample DAG entry, key derivation from a sample mnemonic
- [ ] Include message diagrams: device join flow, file sync flow, conflict resolution flow
- [ ] Update `docs/architecture.md` with folder model and streaming changes
- [ ] Update `docs/features.md` with all new features (folders, conflicts, versioning, watching, web UI)
- [ ] Update `CLAUDE.md` with protocol spec reference and final dependency list

**Verification** (not automated tests, but manual checks):

- [ ] Every `Action` variant in code has a corresponding section in the spec
- [ ] Every `MurmurMessage` variant in code has a corresponding wire format description
- [ ] Hash computation example in spec matches `DagEntry::compute_hash()` output
- [ ] Key derivation example in spec matches `NetworkIdentity` output for a known mnemonic
- [ ] Conflict detection algorithm in spec matches implementation in `murmur-dag`

---
