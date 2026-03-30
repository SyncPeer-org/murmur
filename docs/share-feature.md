# Ephemeral File Sharing — Implementation Plan

## Problem

Murmur's shared mnemonic model is designed for syncing your own devices. But sometimes you
need to send a big file to someone else — a friend, a colleague — without giving them your
mnemonic, without them installing murmur, and without uploading to a cloud service.

## Goals

- **One file, one recipient, time-limited** — share expires automatically
- **No network compromise** — recipient never learns the mnemonic or joins the DAG
- **No cloud** — data flows directly from sender's device to recipient's browser/CLI
- **Zero install for recipient** — a web browser is enough
- **Sender controls lifecycle** — can revoke before expiry

## Non-Goals

- Folder sharing (use the existing folder sync for that)
- Multi-recipient broadcast (create separate tickets)
- Offline recipient download (sender must be online when recipient fetches)

---

## Design

### Share Ticket

A self-contained, signed capability token. It lives entirely outside the DAG — no gossip,
no mnemonic involvement. The daemon keeps an in-memory allow-list of active shares.

```rust
/// The signed payload — this is what gets verified.
struct SharePayload {
    version: u8,              // protocol version (1)
    share_id: [u8; 16],       // random nonce identifying this share
    blob_hash: BlobHash,      // blake3 hash of the file
    file_name: String,        // original filename (for Content-Disposition)
    file_size: u64,           // total bytes (for progress bars)
    expires_at: u64,          // unix timestamp, checked on every request
}

/// The full ticket — payload + connection info + signature.
struct ShareTicket {
    payload: SharePayload,
    node_id: [u8; 32],            // sender's iroh node ID (ed25519 pubkey)
    relay_url: Option<String>,    // iroh relay URL for NAT traversal
    addrs: Vec<SocketAddr>,       // sender's direct socket addresses (best-effort)
    http_port: Option<u16>,       // port of the daemon's HTTP server
    signature: [u8; 64],          // ed25519_sign(postcard::to_vec(payload))
}
```

**Encoding:** `postcard::to_vec(ticket)` → base32 (no padding) → prefixed with `murmur1:`

**Compact URL:** `https://share.murmur.app/#murmur1:ABCDEF...`

The `share_id` is critical — it prevents blob enumeration. Knowing a blob hash is not enough;
you need a valid `share_id` that the daemon recognizes in its allow-list.

### Daemon Allow-List

```rust
struct ActiveShare {
    payload: SharePayload,
    created_at: u64,
    download_count: u64,
    max_downloads: Option<u64>,   // optional limit
    revoked: bool,
}

/// In-memory HashMap<[u8; 16], ActiveShare>, keyed by share_id.
/// Persisted to disk as a small postcard file so shares survive daemon restart.
```

On every download request, the daemon checks: `!revoked && now < expires_at && downloads < max`.

### Signature Verification

The ticket is self-authenticating. Anyone can verify it was created by the node that holds
the signing key for `node_id`. The daemon double-checks: "is this `share_id` in my allow-list
AND does the signature match my own key?" This prevents forged tickets.

---

## Architecture

```
Sender's device                         Recipient
┌─────────────────────┐
│  murmurd            │
│  ┌───────────────┐  │
│  │ Share Manager  │  │   CLI-to-CLI (Phase 3):
│  │ (allow-list)  │──│──── iroh QUIC ──────────────── murmur-cli fetch
│  └───────┬───────┘  │
│  ┌───────┴───────┐  │   Browser download (Phase 1-2):
│  │ HTTP Server   │──│──── HTTPS ──────────────────── Browser (static site)
│  │ (axum)        │  │         ▲
│  │ /share/:id    │  │         │
│  └───────────────┘  │    share.murmur.app
│                     │    (static HTML+JS,
└─────────────────────┘     parses ticket,
                            connects to sender)
```

**Key point:** the static website at `share.murmur.app` never touches the file data. It is
purely a UI that tells the browser where to download from. The bytes flow directly from the
sender's daemon to the recipient's browser.

---

## Phases

### Phase 1 — Core Types & Daemon HTTP Share

The MVP. Sender creates a share, gets a URL. Recipient opens it in a browser and downloads
directly from the sender's daemon. Works on LAN out of the box.

#### 1.1 Share types in `murmur-types`

Add `SharePayload`, `ShareTicket`, encoding/decoding, signature helpers.

```rust
// crates/murmur-types/src/share.rs

impl ShareTicket {
    /// Create and sign a new share ticket.
    pub fn new(
        blob_hash: BlobHash,
        file_name: String,
        file_size: u64,
        expires_at: u64,
        node_id: [u8; 32],
        relay_url: Option<String>,
        addrs: Vec<SocketAddr>,
        http_port: Option<u16>,
        signing_key: &SigningKey,
    ) -> Self { ... }

    /// Verify the ticket's signature against the embedded node_id.
    pub fn verify(&self) -> Result<(), ShareError> { ... }

    /// Check if the ticket has expired.
    pub fn is_expired(&self) -> bool { ... }

    /// Encode to a murmur1:... string.
    pub fn encode(&self) -> String { ... }

    /// Decode from a murmur1:... string.
    pub fn decode(s: &str) -> Result<Self, ShareError> { ... }

    /// Generate a full share URL for the daemon's HTTP server.
    /// Example: http://192.168.1.5:8384/share/AbC12xYz
    pub fn http_url(&self) -> Option<String> { ... }
}
```

#### 1.2 IPC commands in `murmur-ipc`

```rust
// New CliRequest variants
CreateShare {
    folder_id_hex: String,
    path: String,              // relative path within folder
    expires_secs: u64,         // duration from now
    max_downloads: Option<u64>,
}
ListShares
RevokeShare { share_id_hex: String }

// New CliResponse variants
ShareCreated {
    share_id_hex: String,
    ticket: String,            // encoded murmur1:... ticket
    url: Option<String>,       // http://... URL if HTTP server is active
    expires_at: u64,
}
ShareList { shares: Vec<ShareInfoIpc> }
ShareRevoked
```

#### 1.3 Share manager in `murmurd`

New module `crates/murmurd/src/share.rs`:

- `ShareManager` struct with `HashMap<[u8; 16], ActiveShare>`
- Persistence: save to `shares.bin` alongside `dag.bin` (postcard serialized Vec)
- Periodic cleanup task: remove expired shares every 60s
- Methods: `create()`, `verify_and_record_download()`, `revoke()`, `list()`, `load()`, `save()`

#### 1.4 HTTP share endpoints in `murmurd`

Extend the existing axum server (currently metrics-only behind a feature flag).
Make the HTTP server always-on (not gated behind `metrics` feature) when shares exist,
or add a `--share-port` / config option.

Routes:

```
GET /share/{share_id_base32}
    → Serves a minimal, self-contained HTML page with:
      - File name, size, expiry countdown
      - "Download" button
      - Download progress bar (JS)
      - Inline CSS, no external dependencies
    → Returns 404 if share_id unknown, 410 if expired/revoked

GET /share/{share_id_base32}/download
    → Verifies share_id, checks expiry, increments download count
    → Streams the blob with:
      - Content-Disposition: attachment; filename="photo.jpg"
      - Content-Length: {file_size}
      - Content-Type: application/octet-stream (or sniffed MIME)
      - Accept-Ranges: bytes (for resume support)
    → Supports Range requests for interrupted downloads
    → Returns 404/410/429 as appropriate

GET /share/{share_id_base32}/info
    → JSON metadata: { file_name, file_size, expires_at, mime_type }
    → Used by the static website (Phase 2) via fetch()
    → CORS headers: Access-Control-Allow-Origin: *
```

The HTML page served at `/share/{id}` is fully self-contained — it works even without
the static website. This is the simplest path: share a URL, recipient opens it, clicks
download.

#### 1.5 CLI commands in `murmur-cli`

```
murmur-cli share create <folder> <path> [--expires 24h] [--max-downloads 5]
    → Prints: ticket string, HTTP URL (if available), expiry time

murmur-cli share list
    → Table: share_id | file | expires | downloads | status

murmur-cli share revoke <share_id>
    → Confirms revocation
```

#### 1.6 Tests

- **Unit:** ShareTicket encode/decode roundtrip
- **Unit:** ShareTicket signature verification (valid key, wrong key, tampered payload)
- **Unit:** ShareTicket expiry check
- **Unit:** ShareManager create/verify/revoke/expire lifecycle
- **Integration:** create share via IPC, fetch via HTTP, verify file contents match
- **Integration:** expired share returns 410
- **Integration:** revoked share returns 410
- **Integration:** max_downloads enforced (N+1th request rejected)
- **Integration:** Range request resumes download correctly

---

### Phase 2 — Static Share Website (`share.murmur.app`)

A lightweight static website that provides a polished download experience. The recipient
pastes a ticket or opens a share URL, sees file info, and downloads directly from the
sender's daemon.

#### 2.1 Why a separate website?

The daemon-hosted HTML page (Phase 1) works, but:
- Its URL contains the sender's IP address (privacy concern for internet sharing)
- It can't provide a consistent, branded experience
- It can't work with the ticket string alone (needs HTTP URL)

The static website solves this: the URL is always `share.murmur.app/#<ticket>`, and the
site itself extracts the sender's address from the ticket to connect.

#### 2.2 Technology

**Vanilla HTML + JS.** Single `index.html` file, no build step, no framework. Reasons:
- Deployable anywhere (GitHub Pages, Cloudflare Pages, Netlify, S3)
- No supply chain risk
- Cacheable forever (content-addressed)
- Tiny (~15 KB gzipped)

Optional enhancement: compile `ShareTicket::decode()` to WASM for ticket parsing, ensuring
the browser uses the exact same deserialization logic as the Rust code. But vanilla JS
parsing is fine for the MVP.

#### 2.3 Website UX flow

```
1. User lands on share.murmur.app
   - URL fragment contains ticket: /#murmur1:ABCDEF...
   - OR: empty page with a text input: "Paste a share ticket"

2. Site parses the ticket (JS or WASM)
   - Extracts: file_name, file_size, expires_at, sender HTTP address
   - Shows: "vacation-photos.zip (2.4 GB) — expires in 23h"

3. User clicks "Download"
   - JS calls fetch() to sender's /share/{id}/download
   - Streams response via ReadableStream
   - Pipes to a download via <a download> + Blob URL (small files)
     or StreamSaver.js pattern using Service Worker (large files)
   - Shows progress bar with speed + ETA

4. Download complete
   - "Download complete" confirmation
   - File saved to browser's downloads folder
```

#### 2.4 Large file handling

Browsers can't hold multi-GB files in memory. For files > 100 MB, use the
Service Worker streaming pattern:

1. Register a Service Worker that intercepts a synthetic URL
2. Create a `ReadableStream` from the `fetch()` response
3. Navigate to the synthetic URL — Service Worker serves the stream
4. Browser streams directly to disk via its download manager

This keeps memory usage bounded regardless of file size.

Libraries that implement this: `StreamSaver.js`, `native-file-system-adapter`.
Or implement the ~50 lines of Service Worker code directly.

#### 2.5 Directory structure

```
web/share/
  index.html          # single-page app
  sw.js               # service worker for large file streaming
  style.css           # inline in index.html for single-file deploy
  ticket.js           # ticket parsing logic
  download.js         # fetch + stream + progress
```

Or a single `index.html` with everything inlined for maximum portability.

#### 2.6 CORS

The static site at `share.murmur.app` makes cross-origin requests to the sender's daemon.
The daemon's share endpoints must return:

```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, OPTIONS
Access-Control-Expose-Headers: Content-Length, Content-Disposition
```

This is safe because the share endpoints are authenticated by the `share_id` token —
there's nothing to protect via same-origin policy.

#### 2.7 Tests

- **Unit (JS):** ticket parsing for known test vectors
- **E2E:** Playwright/Puppeteer test — start daemon, create share, open website, download,
  verify file hash
- **Edge cases:** expired ticket shows error, invalid ticket shows error, network timeout
  shows retry prompt

---

### Phase 3 — CLI-to-CLI Fetch

For power users who have murmur-cli installed. Downloads over iroh QUIC (faster, no HTTP
overhead, works even without the HTTP server).

#### 3.1 New wire message

Add to the existing `MurmurMessage` enum in `murmur-net`:

```rust
/// Ticket-authenticated blob request (no DAG membership required).
ShareBlobRequest {
    share_id: [u8; 16],
    signature: [u8; 64],    // ticket signature (proves possession of valid ticket)
}

/// Response: stream blob chunks or reject.
ShareBlobResponse {
    share_id: [u8; 16],
    status: ShareBlobStatus,  // Ok | Expired | Revoked | NotFound
    // followed by blob chunks if Ok
}
```

#### 3.2 Standalone fetch mode

`murmur-cli share fetch <ticket> [--output <path>]`

This does NOT require a running daemon. The CLI:
1. Decodes the ticket
2. Creates a temporary iroh endpoint (random key, ephemeral)
3. Connects to the sender's node via `node_id` + `relay_url` + `addrs`
4. Sends `ShareBlobRequest`
5. Streams blob to disk, verifying blake3 hash
6. Prints result, exits

This requires adding `iroh` as a dependency of `murmur-cli` (currently it only depends
on `murmur-ipc` for Unix socket communication). Consider whether to put the fetch logic
in a small helper crate or accept the dependency.

#### 3.3 Daemon-side handler

In `murmurd`'s connection handler, match on `ShareBlobRequest`:
1. Look up `share_id` in `ShareManager`
2. Verify the ticket signature
3. Check expiry + download count
4. Stream the blob using existing blob transfer infrastructure
5. Increment download count

#### 3.4 Tests

- **Integration:** create share on daemon A, fetch from CLI B (no daemon on B), verify hash
- **Integration:** fetch with expired ticket fails
- **Integration:** fetch resumes after interruption (if we implement chunked transfer)

---

### Phase 4 — Internet Sharing & NAT Traversal

Phases 1-3 work on LAN or when the sender has a public IP / port forwarding. This phase
makes sharing work reliably over the internet.

#### 4.1 The NAT problem

For CLI-to-CLI: iroh already handles NAT traversal via relay servers + QUIC hole-punching.
This works out of the box in Phase 3.

For browser downloads: the browser can't speak iroh's QUIC protocol. It needs HTTP(S),
and the sender's HTTP port may not be reachable from the internet.

#### 4.2 Option A — Reverse tunnel (recommended for MVP)

Integrate a lightweight reverse tunnel that exposes the daemon's HTTP share endpoint:

- **Built-in:** Use `bore` (pure Rust TCP tunnel) or similar
- **External:** Document how to use `cloudflared tunnel`, `ngrok`, `tailscale funnel`
- **Config:** `murmur-cli share create --expose` automatically starts a tunnel and
  includes the public URL in the ticket

```toml
# config.toml
[share]
tunnel = "bore"                    # or "cloudflared", "none"
tunnel_server = "bore.pub"         # public bore relay
```

The tunnel is only active while shares exist. Torn down when last share expires/is revoked.

#### 4.3 Option B — WebTransport (future)

When iroh adds WebTransport support (HTTP/3 over QUIC), the browser can connect directly
to the iroh endpoint — no HTTP server needed. The static website would use the
`WebTransport` browser API:

```js
const transport = new WebTransport(`https://${nodeAddr}:${port}/share`);
const reader = transport.incomingUnidirectionalStreams.getReader();
// ... stream blob to disk
```

This is the ideal end-state: true browser-to-device P2P with iroh's existing NAT traversal.
But it depends on iroh implementing WebTransport, which is not available as of iroh 0.96.

#### 4.4 Option C — Relay-assisted HTTP proxy

Run a lightweight relay service that:
1. Sender's daemon connects to relay via WebSocket (outbound, works behind NAT)
2. Relay assigns a public URL: `https://relay.murmur.app/s/{share_id}`
3. Browser hits the relay URL
4. Relay proxies the request through the WebSocket to the sender's daemon
5. Blob streams: sender → relay → browser

This is the most reliable option but requires running relay infrastructure. Could be
self-hosted or offered as a service.

#### 4.5 Recommendation

Start with Option A (reverse tunnel) — it's the fastest to implement and requires no
infrastructure. Document external tunnel options. Add built-in `bore` support as a
convenience. Move to Option B (WebTransport) when iroh supports it.

#### 4.6 Tests

- **Integration:** share with `--expose`, verify public URL is reachable
- **Integration:** tunnel teardown after share revocation

---

## Security Considerations

| Threat | Mitigation |
|---|---|
| Ticket interception (MITM) | Ticket is a capability token — treat like a password. Encourage HTTPS tunnel for internet shares. Blob integrity verified by blake3 hash in ticket |
| Brute-force share_id | 128-bit random nonce — computationally infeasible |
| Blob enumeration | share_id required for every request, not just blob_hash |
| Replay after expiry | Server-side expiry check on every request |
| Sender impersonation | Ticket signature ties it to sender's ed25519 key; client verifies before connecting (CLI path) |
| Resource exhaustion | max_downloads limit; optional rate limiting on HTTP endpoint |
| Recipient privacy | Static website URL doesn't contain sender's IP; sender only sees recipient's IP during download |
| Mnemonic exposure | Zero. Share feature never touches the mnemonic, seed, or DAG |

---

## UX Summary

### Sender (has murmur)

```bash
$ murmur-cli share create photos/ vacation.zip --expires 24h

  Share created!

  File:     vacation.zip (2.4 GB)
  Expires:  2026-03-31 14:30 UTC (in 24h)
  Share ID: a1b2c3d4

  Local URL (LAN):
    http://192.168.1.5:8384/share/a1b2c3d4

  Ticket (for CLI fetch):
    murmur1:CIQK3FWNRJ7Y...

  Web link:
    https://share.murmur.app/#murmur1:CIQK3FWNRJ7Y...

  Send the web link to the recipient. They can download
  directly from your device — no account needed.
```

### Recipient (has nothing)

1. Receives a link: `https://share.murmur.app/#murmur1:CIQK3FWNRJ7Y...`
2. Opens in browser
3. Sees: "vacation.zip — 2.4 GB — expires in 23h — [Download]"
4. Clicks download
5. File downloads with progress bar
6. Done

### Recipient (has murmur-cli)

```bash
$ murmur-cli share fetch murmur1:CIQK3FWNRJ7Y... --output ~/Downloads/

  Connecting to sender... connected
  Downloading: vacation.zip (2.4 GB)
  [████████████████░░░░] 82% — 145 MB/s — ~3s remaining

  Download complete: ~/Downloads/vacation.zip
  Hash verified: OK
```

---

## Implementation Order

| Step | Phase | What | Crate(s) | Depends on |
|------|-------|------|----------|------------|
| 1 | 1.1 | `SharePayload`, `ShareTicket`, encode/decode, sign/verify | murmur-types | — |
| 2 | 1.2 | IPC request/response variants | murmur-ipc | Step 1 |
| 3 | 1.3 | `ShareManager` in daemon | murmurd | Step 1 |
| 4 | 1.4 | HTTP share endpoints (axum) | murmurd | Step 3 |
| 5 | 1.5 | CLI `share` subcommand | murmur-cli | Step 2 |
| 6 | 1.6 | Tests for Phase 1 | tests/ | Steps 1-5 |
| 7 | 2 | Static share website | web/share/ | Step 4 |
| 8 | 3 | CLI `share fetch` + wire messages | murmur-cli, murmur-net | Step 1 |
| 9 | 4 | NAT traversal (tunnel integration) | murmurd | Step 4 |

Phases 2 and 3 are independent and can be built in parallel.

---

## Open Questions

1. **Encryption at rest for shared blobs?** Currently blobs are stored as plaintext on the
   sender's disk. The share feature doesn't change this, but if someone gains access to
   the daemon's HTTP port they could try to guess share IDs (mitigated by 128-bit nonce).
   Should we add an optional encryption layer?

2. **Multiple files / folders?** The current design is single-file. For sharing multiple
   files, the sender could create a zip/tar first. Should we add a `--zip` flag that
   bundles multiple files into a temporary archive?

3. **Share from mobile?** The share feature is designed for the daemon, but the core types
   (Phase 1.1) are platform-agnostic. Mobile apps could implement their own share UI
   using the same ticket format.

4. **Analytics / receipts?** Should the sender be notified when a download completes?
   Currently they can check `share list` to see download counts. Real-time notification
   would require an IPC event.

5. **Password-protected shares?** Add an optional passphrase that the recipient must enter
   before downloading. Derived into an encryption key via argon2, blob encrypted before
   serving. Adds complexity but useful for sensitive files.
