# Desktop-to-Desktop: Full Sync Test Suite

Two desktop app instances running on the same machine (different data dirs),
each connected to its own murmurd daemon. Tests the complete peer-to-peer
experience: network creation, device joining and approval, folder sharing,
file sync, conflict detection and resolution, and device management — all
from the desktop UI.

**Architecture**: Each "device" is a murmurd + murmur-desktop pair. The two
daemons discover each other via iroh relay and sync over QUIC. No CLI commands
are used during the test — everything goes through the desktop app.

This test requires **4 terminal windows**:
- Terminal 1: murmurd for Device A
- Terminal 2: murmurd for Device B
- Terminal 3: murmur-desktop for Device A
- Terminal 4: murmur-desktop for Device B

## Prerequisites

```bash
cargo build -p murmurd -p murmur-desktop
rm -rf /tmp/dd-a /tmp/dd-b /tmp/dd-sync-a /tmp/dd-sync-b
mkdir -p /tmp/dd-sync-a/Photos /tmp/dd-sync-b/Photos
```

Create test content on Device A's side:

```bash
echo "Sunset at the beach" > /tmp/dd-sync-a/Photos/sunset.txt
echo "Mountain landscape" > /tmp/dd-sync-a/Photos/mountain.txt
echo "City skyline" > /tmp/dd-sync-a/Photos/city.txt
```

---

## Phase 1 — Network Creation (Device A)

### DD1 — Start Device A's Daemon

**Terminal 1**:

```bash
cargo run --bin murmurd -- --data-dir /tmp/dd-a --name "Desktop-A" --role full --verbose
```

**Save the mnemonic** printed to stdout. You'll need it for Device B.

**Expected**: Daemon starts, prints mnemonic and device ID.

### DD2 — Launch Device A's Desktop App

**Terminal 3**:

```bash
MURMUR_SOCKET=/tmp/dd-a/murmurd.sock cargo run --bin murmur-desktop
```

> Note: The desktop app connects to `~/.murmur/murmurd.sock` by default.
> For this test, either symlink or set `--data-dir` on the daemon to use
> `~/.murmur`, or connect via the default socket path. The simplest approach:

```bash
# If ~/.murmur doesn't exist, the app shows Setup. Instead, point the
# default socket to daemon A:
rm -rf ~/.murmur
ln -s /tmp/dd-a ~/.murmur
cargo run --bin murmur-desktop
```

**Expected**:
- App connects to daemon A
- Green "Connected to murmurd!" message for ~600ms
- Transitions to the **Folders** screen
- Sidebar shows: Folders, Conflicts, Devices, Recent Files, Status, Network Health, Settings

**Checkpoint DD2**: Device A's desktop app connected and showing Folders.

---

## Phase 2 — Create Shared Folder (Device A)

### DD3 — Create Folder from Desktop A

In Device A's desktop app, click **"New Folder"**.

**Expected**: OS directory picker opens.

Navigate to `/tmp/dd-sync-a/Photos` and select it.

**Expected**:
- Folder "Photos" appears in "My Folders"
- Local path `/tmp/dd-sync-a/Photos` shown below the name
- **"Rename"** button next to the folder name
- File count shows **"3 files"** (sunset.txt, mountain.txt, city.txt)
- Mode: "read-write"

### DD4 — Verify Files in Folder Detail

Click **"Open"** on "Photos".

**Expected file list** (3 files):
- `city.txt` — text/plain
- `mountain.txt` — text/plain
- `sunset.txt` — text/plain

Click **"Back"** to return to Folders.

**Checkpoint DD4**: Device A has a folder with 3 scanned files.

---

## Phase 3 — Device B Joins the Network

### DD5 — Start Device B's Daemon (Join Mode)

**Terminal 2**:

```bash
# Write mnemonic so daemon B joins the same network
mkdir -p /tmp/dd-b
echo "<MNEMONIC_FROM_DD1>" > /tmp/dd-b/mnemonic

# Create a device key (different from A)
dd if=/dev/urandom of=/tmp/dd-b/device.key bs=32 count=1 2>/dev/null

# Create config
cat > /tmp/dd-b/config.toml << 'EOF'
[device]
name = "Desktop-B"
role = "full"

[storage]
blob_dir = "/tmp/dd-b/blobs"
data_dir = "/tmp/dd-b/db"
EOF

cargo run --bin murmurd -- --data-dir /tmp/dd-b --name "Desktop-B" --role full --verbose
```

**Expected**: Daemon B starts, logs "joining existing network — awaiting approval",
and connects to daemon A via iroh relay.

### DD6 — Launch Device B's Desktop App

**Terminal 4**:

```bash
rm -rf ~/.murmur
ln -s /tmp/dd-b ~/.murmur
cargo run --bin murmur-desktop
```

**Expected**: Device B's app connects, shows Folders screen (empty — no folders yet
because B isn't approved).

**Checkpoint DD6**: Both desktop apps running, connected to their daemons.

---

## Phase 4 — Device Approval (from Desktop A)

### DD7 — Approve Device B from Desktop A

In **Device A's desktop app**, navigate to **Devices**.

**Expected**:
- **"This Device"** section: Desktop-A, Online, green indicator
- **"Pending Approval"** section: Desktop-B listed with **"Approve"** button
- **"Other Devices"** section: empty

Click **"Approve"** next to Desktop-B.

**Expected**:
- Desktop-B moves from "Pending Approval" to **"Other Devices"**
- Shows role "full"
- Sidebar "Devices" count returns to 0

### DD8 — Verify Approval on Device B

In **Device B's desktop app**, navigate to **Devices**.

**Expected** (after a few seconds for gossip sync):
- **"This Device"** section: Desktop-B, Online
- **"Other Devices"** section: Desktop-A listed
- No pending devices

**Checkpoint DD8**: Device approval works from desktop, both sides see each other.

---

## Phase 5 — Folder Discovery & Subscribe (Device B)

### DD9 — Device B Sees Available Folders

In **Device B's desktop app**, navigate to **Folders**.

**Expected** (after sync):
- **"Available on Network"** section visible
- "Photos" folder listed with:
  - File count: 3
  - Subscriber count: 1 (Device A)
  - **"Subscribe"** button

### DD10 — Subscribe to Folder from Device B

Click **"Subscribe"** next to "Photos".

**Expected**: OS directory picker opens.

Navigate to `/tmp/dd-sync-b/Photos` and select it.

**Expected**:
- "Photos" moves from "Available on Network" to "My Folders"
- The folder name ("Photos") is carried into the subscription
- Local path `/tmp/dd-sync-b/Photos` shown below the name
- **"Rename"** button next to the folder name
- Shows mode "read-write"
- File count: initially 0 (files haven't synced yet) or 3 (if blob sync is fast)

### DD11 — Verify Files Sync to Device B

Wait 10-15 seconds for blob sync via iroh.

In **Device B's desktop app**, click **"Open"** on "Photos".

**Expected** (after sync completes):
- 3 files listed: city.txt, mountain.txt, sunset.txt
- Same sizes as Device A

**Terminal 2** — verify files on disk:

```bash
ls /tmp/dd-sync-b/Photos/
```

**Expected**: sunset.txt, mountain.txt, city.txt present (if reverse sync is wired).

**Checkpoint DD11**: Files sync from Device A to Device B via desktop apps.

---

## Phase 6 — Bidirectional File Sync

### DD12 — Add File on Device B

On Device B's machine, create a new file:

```bash
echo "New file from Device B" > /tmp/dd-sync-b/Photos/from-b.txt
```

Wait for the filesystem watcher to detect it (5-10 seconds).

In **Device B's desktop app**, navigate to Photos folder detail.

**Expected**: `from-b.txt` appears in the file list (4 files total).

### DD13 — Verify File Appears on Device A

Wait 10-15 seconds for gossip + blob sync.

In **Device A's desktop app**, navigate to Photos folder detail.

**Expected**: `from-b.txt` appears in the file list (4 files total).

**Checkpoint DD13**: Bidirectional file sync works.

---

## Phase 7 — Conflict Detection & Resolution

### DD14 — Create a Conflict

Simultaneously edit the same file on both devices:

```bash
# Terminal window (Device A side):
echo "Version from Desktop-A — edited at $(date)" > /tmp/dd-sync-a/Photos/sunset.txt

# Terminal window (Device B side — run within 2 seconds):
echo "Version from Desktop-B — edited at $(date)" > /tmp/dd-sync-b/Photos/sunset.txt
```

Wait 10-15 seconds for both edits to sync and conflict to be detected.

### DD15 — Verify Conflict on Device A

In **Device A's desktop app**, check the sidebar.

**Expected**: **"Conflicts (1)"** shown in sidebar.

Navigate to **Conflicts**.

**Expected**:
- Conflict for "Photos -- sunset.txt"
- Two competing versions listed:
  - One from "Desktop-A" with truncated hash and HLC
  - One from "Desktop-B" with truncated hash and HLC
- **"Keep"** button next to each version
- **"Keep Both (dismiss)"** button at the bottom

### DD16 — Verify Conflict on Device B

In **Device B's desktop app**, check the sidebar.

**Expected**: Same conflict visible — **"Conflicts (1)"** in sidebar.

Navigate to **Conflicts** on Device B.

**Expected**: Same conflict entry with both versions.

**Checkpoint DD16**: Conflict detected and visible on both devices.

### DD17 — Resolve Conflict from Device A

In **Device A's desktop app** Conflicts screen, click **"Keep"** next to the
version from "Desktop-A".

**Expected**:
- Conflict disappears from Device A's conflict list
- Sidebar shows "Conflicts" (no count)

### DD18 — Verify Resolution on Device B

Wait 5-10 seconds for the resolution to sync.

In **Device B's desktop app**, check Conflicts.

**Expected**: Conflict for sunset.txt is resolved (removed from list). The
file content on Device B now matches Device A's version.

**Checkpoint DD18**: Conflict resolution syncs across devices.

---

## Phase 8 — Bulk Conflicts

### DD19 — Create Multiple Conflicts

```bash
# Device A side:
echo "bulk-a-1" > /tmp/dd-sync-a/Photos/bulk1.txt
echo "bulk-a-2" > /tmp/dd-sync-a/Photos/bulk2.txt
echo "bulk-a-3" > /tmp/dd-sync-a/Photos/bulk3.txt

# Device B side (within 2 seconds):
echo "bulk-b-1" > /tmp/dd-sync-b/Photos/bulk1.txt
echo "bulk-b-2" > /tmp/dd-sync-b/Photos/bulk2.txt
echo "bulk-b-3" > /tmp/dd-sync-b/Photos/bulk3.txt
```

Wait for sync and conflict detection (10-15 seconds).

### DD20 — Bulk Resolve from Device A

In **Device A's desktop app**, navigate to Conflicts.

**Expected**:
- 3 conflicts listed for Photos folder
- **"Keep All Newest"** button visible in bulk toolbar

Click **"Keep All Newest"**.

**Expected**: All 3 conflicts resolved at once. List clears.

### DD21 — Verify Bulk Resolution on Device B

Wait for sync. In **Device B's desktop app**, check Conflicts.

**Expected**: All conflicts resolved. List empty.

**Checkpoint DD21**: Bulk conflict resolution syncs across devices.

---

## Phase 9 — File Deletion Across Devices

### DD22 — Delete File from Device A

In **Device A's desktop app**, open Photos folder detail.

Click **"Del"** next to `city.txt`.

**Expected**: File removed from Device A's list.

### DD23 — Verify Deletion on Device B

Wait for sync. In **Device B's desktop app**, open Photos folder detail.

**Expected**: `city.txt` no longer listed.

**Checkpoint DD23**: File deletion syncs across devices.

---

## Phase 10 — File History Across Devices

### DD24 — View History from Device B

Create multiple versions of a file:

```bash
echo "v1 from A" > /tmp/dd-sync-a/Photos/versioned.txt
```

Wait for sync. Then:

```bash
echo "v2 from B" > /tmp/dd-sync-b/Photos/versioned.txt
```

Wait for sync. Then:

```bash
echo "v3 from A" > /tmp/dd-sync-a/Photos/versioned.txt
```

Wait for sync.

In **Device B's desktop app**, open Photos folder detail, click **"History"**
next to `versioned.txt`.

**Expected**: 3 versions listed with:
- Different blob hashes
- Device names alternating (Desktop-A, Desktop-B, Desktop-A)
- Increasing HLC timestamps
- **"Restore"** button on each

### DD25 — Restore Old Version from Device B

Click **"Restore"** on the v1 entry (oldest, from Desktop-A).

**Expected**: File restored. The filesystem watcher detects the change and
creates a new DAG entry.

**Checkpoint DD25**: File history and restore work across devices.

---

## Phase 11 — Device Presence

### DD26 — Online/Offline Detection

In **Device A's desktop app**, navigate to **Devices**.

**Expected**: Desktop-B shows "Online" with green indicator.

Stop Device B's daemon (Ctrl+C in Terminal 2).

Wait 10-30 seconds. Check **Device A's** Devices screen.

**Expected**: Desktop-B status changes from "Online" to relative time
(e.g., "1 min ago") with gray indicator.

Restart Device B's daemon:

```bash
cargo run --bin murmurd -- --data-dir /tmp/dd-b --name "Desktop-B" --role full --verbose
```

Wait for reconnection. Check Device A's Devices screen.

**Expected**: Desktop-B returns to "Online" with green indicator.

**Checkpoint DD26**: Device presence updates across the network.

---

## Phase 12 — Settings & Network Health (Cross-Device)

### DD27 — Storage Stats Show Both Devices' Data

In **Device A's desktop app**, navigate to **Network Health**.

**Expected**:
- **Peers** section: Desktop-B listed with connection type and last seen
- **Storage** section: blob count reflects all synced files

### DD28 — Export Diagnostics

Click **"Export Diagnostics"**.

**Expected**: Diagnostics file created at `~/murmur-diagnostics/diag-<timestamp>.json`.

Verify:

```bash
cat ~/murmur-diagnostics/diag-*.json | python3 -m json.tool | head -20
```

**Expected**: Valid JSON with `"peers"` array containing Desktop-B info.

**Checkpoint DD28**: Network health and diagnostics reflect multi-device state.

---

## Phase 12a — Restart Persistence

### DD28a — Restart Desktop A: Folders Persist

Close Device A's desktop app. Daemon A stays running.

Relaunch:

```bash
cargo run --bin murmur-desktop
```

Navigate to the **Folders** screen.

**Expected**:
- "Photos" folder still in "My Folders" with correct name, path, file count
- Mode: "read-write"
- **"Rename"** / **"Open"** / **"Unsub"** buttons present

**Checkpoint DD28a**: Folders survive desktop app restart in multi-device setup.

### DD28b — Restart Both Daemons: Folders Persist

Close both desktop apps. Stop both daemons (Ctrl+C).

Restart both daemons:

```bash
# Terminal 1
cargo run --bin murmurd -- --data-dir /tmp/dd-a --name Desktop-A --role full
# Terminal 2
cargo run --bin murmurd -- --data-dir /tmp/dd-b --name Desktop-B --role full
```

Relaunch both desktop apps.

**Expected** on both devices:
- "Photos" folder present with correct file counts
- Subscriptions intact
- Files accessible from folder detail

**Checkpoint DD28b**: Full daemon restart preserves folders on both devices.

---

## Phase 13 — Leave Network (Device B)

### DD29 — Leave from Device B

In **Device B's desktop app**, navigate to **Settings**, scroll to **Danger Zone**.

Click **"Leave Network & Wipe Data"**.

**Expected**: Confirmation warning appears.

Click **"Yes, leave network and wipe data"**.

**Expected**:
- Device B's app resets to the **Setup** screen (Create/Join)
- Daemon B shuts down

### DD30 — Verify Device B Gone on Device A

In **Device A's desktop app**, navigate to **Devices**.

**Expected** (after 10-30 seconds):
- Desktop-B status changes to offline / "X min ago"
- Eventually Desktop-B may still appear in the device list (DAG entries persist)

### DD31 — Verify User Files Preserved

```bash
ls /tmp/dd-sync-b/Photos/
```

**Expected**: All synced files still present on disk (not deleted by LeaveNetwork).

**Checkpoint DD31**: Leave network works, user files preserved, Device A sees departure.

---

## Cleanup

Close both desktop apps. Stop both daemons.

```bash
rm -rf /tmp/dd-a /tmp/dd-b /tmp/dd-sync-a /tmp/dd-sync-b
rm -f ~/.murmur  # remove symlink
rm -rf ~/murmur-diagnostics
```

---

## Quick Reference

| #    | What                        | Pass criteria                                             |
| ---- | --------------------------- | --------------------------------------------------------- |
| DD2  | Device A connects           | Desktop app shows Folders screen                          |
| DD4  | Create folder + scan        | 3 files scanned from disk                                 |
| DD6  | Device B connects           | Both desktop apps running                                 |
| DD8  | Device approval             | A approves B from desktop, both see each other            |
| DD11 | Folder subscribe + sync     | B subscribes, files sync from A to B                      |
| DD13 | Bidirectional sync          | File added on B appears on A                              |
| DD16 | Conflict detection          | Same file edited on both, conflict visible on both        |
| DD18 | Conflict resolution         | A resolves, resolution syncs to B                         |
| DD21 | Bulk conflict resolution    | 3 conflicts resolved at once, syncs to B                  |
| DD23 | File deletion sync          | A deletes file, deletion syncs to B                       |
| DD25 | File history + restore      | Cross-device version history, restore works               |
| DD26 | Device presence             | Online/offline status updates across devices              |
| DD28 | Network health              | Peers list and diagnostics show multi-device state        |
| DD31 | Leave network               | B leaves, files preserved, A sees B go offline            |
