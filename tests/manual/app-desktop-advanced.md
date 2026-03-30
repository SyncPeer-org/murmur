# Desktop App: Advanced Tests — Settings, Network Health, Edge Cases & Lifecycle

Settings configuration, bandwidth throttling, storage management, network
diagnostics, leave-network flow, daemon reconnection, join-network setup, and
real-time event updates. Run the **basic** and **intermediate** tests first.

This test requires **3 terminal windows**: Terminal 1 runs the daemon, Terminal 2
runs CLI commands, Terminal 3 runs the desktop app.

## Prerequisites

```bash
cargo build -p murmurd -p murmur-cli -p murmur-desktop
rm -rf /tmp/murmur-a /tmp/murmur-sync-a
mkdir -p /tmp/murmur-sync-a
```

**Terminal 1** — start daemon:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-a --name "node-a" --role full --verbose
```

**Save the mnemonic.**

**Terminal 2** — create a folder and add files:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder create "TestFolder"
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder subscribe <FOLDER_ID> /tmp/murmur-sync-a --name "TestFolder"
echo "file1" > /tmp/murmur-sync-a/file1.txt
echo "file2" > /tmp/murmur-sync-a/file2.txt
```

**Terminal 3** — launch the desktop app:

```bash
cargo run --bin murmur-desktop
```

---

## DA1 — Settings: Device Info

Navigate to **Settings** in the sidebar.

**Expected**:
- **Device section** (header size 18):
  - Name: "node-a"
  - ID: full 64-character hex string (monospace, size 11)

---

## DA2 — Settings: Auto-Approve Toggle

In the Network section:

**Expected**: "Auto-approve new devices:" label with **"OFF"** button (default).

Click **"OFF"** to toggle to ON.

**Expected**: Button text changes to **"ON"**.

Click **"ON"** to toggle back.

**Expected**: Button text changes to **"OFF"**.

**Terminal 2** — verify:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a config
```

**Expected**: `auto_approve` reflects the current toggle state.

---

## DA3 — Settings: mDNS Toggle

In the Network section:

**Expected**: "mDNS LAN discovery:" label with current state button.

Click to toggle mDNS ON/OFF.

**Expected**: Button text toggles between **"ON"** and **"OFF"**.

**Checkpoint DA3**: Network toggles work and persist.

---

## DA4 — Settings: Bandwidth Throttle

In the Bandwidth section:

**Expected**:
- Current display: "Upload: Unlimited | Download: Unlimited" (default)
- Four preset buttons: **"Unlimited"**, **"1 MB/s"**, **"5 MB/s"**, **"10 MB/s"**

Click **"1 MB/s"**.

**Expected**: Display updates to "Upload: 1048576/s | Download: 1048576/s"
(or human-readable "1 MB/s").

Click **"5 MB/s"**.

**Expected**: Display updates accordingly.

Click **"10 MB/s"**.

**Expected**: Display updates accordingly.

Click **"Unlimited"**.

**Expected**: Display returns to "Upload: Unlimited | Download: Unlimited".

**Checkpoint DA4**: Bandwidth throttle presets update correctly.

---

## DA5 — Settings: Storage Stats & Reclaim

In the Storage section:

**Expected**:
- Blobs: "N blobs (X MB)" (N >= 2 from test files)
- Orphaned: "M blobs (Y MB)" (may be 0)
- DAG entries: "Z entries"

Click **"Reclaim Orphaned Blobs"**.

**Expected**: Green success message appears showing bytes freed and blob count
(e.g., "Reclaimed 0 bytes from 0 blobs" if none are orphaned).

---

## DA6 — Settings: Global Sync Toggle

In the Sync section:

**Expected**: "Global sync:" label with **"Active - Pause"** button (default).

Click the button.

**Expected**:
- Button text changes to **"PAUSED - Resume"**
- Sidebar sync status shows "PAUSED"

Click again to resume.

**Expected**:
- Button text returns to **"Active - Pause"**
- Sidebar sync status shows "Syncing"

**Checkpoint DA6**: Settings sync toggle matches sidebar state.

---

## DA7 — Network Health: Peers

Navigate to **Network Health** in the sidebar.

**Expected**:
- **Peers section**:
  - Column headers: "Name", "Connection", "Last Seen", "ID"
  - If no peers connected: "No peers connected."
  - If peers connected: rows with device name, connection type ("direct" or
    "relay"), last seen time, truncated device ID

---

## DA8 — Network Health: Storage Stats

In the Storage section:

**Expected**:
- Total blobs: "N blobs (X bytes)"
- Orphaned blobs: "M blobs (Y bytes)"
- DAG entries: "Z entries"
- (Shows "Loading..." briefly on first load)

---

## DA9 — Network Health: Connectivity Check

In the Connectivity section:

**Expected**: **"Run Connectivity Check"** button visible.

Click the button.

**Expected** (after a brief delay):
- Result shows: "Relay: Reachable (X ms)" in green text, OR
- "Relay: Unreachable" in red text (if no internet/relay)
- Latency shown in milliseconds if reachable

Click the button again.

**Expected**: Previous result clears, new check runs.

---

## DA10 — Network Health: Export Diagnostics

In the Export section:

**Expected**: **"Export Diagnostics"** button visible.

Click the button.

**Expected**: Confirmation message appears. Diagnostics exported to
`~/murmur-diagnostics/diag-<timestamp>.json`.

**Terminal 2** — verify:

```bash
ls ~/murmur-diagnostics/
```

**Expected**: At least one `diag-*.json` file present.

**Checkpoint DA10**: Network health diagnostics fully functional.

---

## DA11 — Real-Time Event Updates

Navigate to **Status** in the sidebar. Watch the event log.

**Terminal 2** — trigger an event:

```bash
echo "new event file" > /tmp/murmur-sync-a/event-test.txt
```

**Expected**: Within a few seconds, a `file_synced` event appears in the Status
event log without manual refresh.

Navigate to **Folders**, open the folder.

**Expected**: `event-test.txt` appears in the file list (auto-refreshed by event).

---

## DA12 — Real-Time Conflict Count Update

Navigate to **Folders** (note the sidebar conflict count).

Create a conflict from CLI (requires a second daemon — or skip if single-node):

If a second daemon is running:

```bash
echo "conflict-a" > /tmp/murmur-sync-a/rt-conflict.txt
echo "conflict-b" > /tmp/murmur-sync-b/rt-conflict.txt
```

**Expected**: Sidebar **"Conflicts (N)"** count increments automatically without
navigating to the Conflicts screen.

---

## DA13 — Daemon Disconnection & Reconnection

Stop the daemon (Ctrl+C in Terminal 1).

**Expected** (in desktop app):
- After a few seconds, the app detects disconnection
- May show an error in the Status screen's "Last error" field
- IPC commands fail gracefully (no crash)

Restart the daemon:

**Terminal 1**:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-a --name "node-a" --role full --verbose
```

**Expected**: The desktop app reconnects. Status screen updates. Folder data
refreshes. No manual intervention needed.

**Checkpoint DA13**: App handles daemon restart gracefully.

---

## DA14 — Status Screen: Uptime Formats

Check the Status screen immediately after daemon restart.

**Expected**: Uptime shows seconds-level format (e.g., "0m 5s").

Wait a few minutes and check again.

**Expected**: Uptime shows minutes-level format (e.g., "2m 30s").

---

## DA15 — Status Screen: Error Display

If any IPC error occurred (e.g., during daemon downtime):

**Expected**: "Last error:" line shown in red text (size 12) below the stats line.

After successful reconnection and a status refresh:

**Expected**: Error may clear on next successful status fetch, or persist until
a new error replaces it.

---

## DA16 — Settings: Leave Network (Danger Zone)

Navigate to **Settings** and scroll to the **Danger Zone** section.

**Expected**:
- Header: "Danger Zone" (red text)
- **"Leave Network & Wipe Data"** button

Click the button.

**Expected** (confirmation state):
- Warning text (orange): "This will delete all Murmur data (config, keys, DAG,
  blobs). Files in synced folders on disk will NOT be deleted. The daemon will
  shut down."
- **"Yes, leave network and wipe data"** button (full width)
- **"Cancel"** button

Click **"Cancel"**.

**Expected**: Confirmation dismisses, returns to normal Danger Zone button.

---

## DA17 — Settings: Leave Network (Confirm)

Click **"Leave Network & Wipe Data"** again, then click
**"Yes, leave network and wipe data"**.

**Expected**:
- Daemon shuts down
- App resets to the **Setup** screen (or Daemon Check screen)
- All Murmur data wiped from data directory

**Terminal 2** — verify:

```bash
ls /tmp/murmur-a/
```

**Expected**: Data directory cleaned or daemon no longer running.

**Checkpoint DA17**: Leave network flow works with confirmation step.

---

## DA18 — Setup Wizard: Join Network

After leaving the network (DA17), the app shows the Setup screen.

Click **"Join Network"**.

**Expected**:
- Title: "Join Network"
- Text input for "Device name"
- Text input for "Enter mnemonic phrase..."
- Submit button: **"Start daemon & join"** (disabled until both fields filled)
- **"Back"** button

Enter a device name and the mnemonic from another network.

**Expected**: Submit button enables when both fields are non-empty.

Click **"Start daemon & join"**.

**Expected** (if another daemon with that network is running):
- Daemon starts and joins the network
- App transitions to Folders screen
- Sidebar shows connected state

**Expected** (if no other daemon available):
- Daemon starts but has 0 peers
- App still transitions to Folders screen

**Checkpoint DA18**: Join network setup flow works.

---

## DA18a — Join Network: Mnemonic Preserved in Settings

After completing DA18 (Leave → Join), navigate to **Settings** in the sidebar.

**Expected**:
- The **Network Mnemonic** section displays the exact mnemonic you entered in the
  Join form (DA18) — **not** a different randomly generated mnemonic
- The mnemonic words and order match exactly what you typed

This verifies that the Leave → Join flow preserves the user-provided mnemonic
across the data directory wipe triggered by LeaveNetwork.

**Checkpoint DA18a**: Settings mnemonic matches the one entered during Join.

---

## DA19 — Setup Wizard: Back Navigation

From the setup form (step 2), click **"Back"**.

**Expected**: Returns to step 1 (Choose Mode) with Create/Join buttons.

From step 1, click **"Back to daemon check!"**.

**Expected**: Returns to Daemon Check screen.

---

## DA20 — Auto-Launch Daemon on Startup

After a successful setup, close the desktop app. Then relaunch:

**Terminal 3**:

```bash
cargo run --bin murmur-desktop
```

**Expected**:
- App starts on Daemon Check screen
- Detects existing config, attempts to auto-launch murmurd
- Status: "Starting murmurd..." then transitions to Folders screen
- No manual setup needed if config exists from a previous session

**Checkpoint DA20**: App auto-launches daemon when config exists.

---

## Cleanup

Close the desktop app and stop any running daemons.

```bash
rm -rf /tmp/murmur-a /tmp/murmur-b /tmp/murmur-sync-a /tmp/murmur-sync-b
rm -rf ~/murmur-diagnostics
```

## Quick Reference

| #    | What                   | Pass criteria                                          |
| ---- | ---------------------- | ------------------------------------------------------ |
| DA1  | Settings: device       | Name and full device ID displayed                      |
| DA2  | Settings: auto-approve | Toggle switches ON/OFF, persists                       |
| DA3  | Settings: mDNS         | Toggle switches ON/OFF                                 |
| DA4  | Settings: bandwidth    | Preset buttons update throttle display                 |
| DA5  | Settings: storage      | Blob/DAG stats shown, reclaim button works             |
| DA6  | Settings: sync toggle  | Matches sidebar state when toggled                     |
| DA7  | Health: peers          | Peer table with name, connection, last seen, ID        |
| DA8  | Health: storage        | Blob and DAG stats displayed                           |
| DA9  | Health: connectivity   | Check runs, shows reachable/unreachable with latency   |
| DA10 | Health: diagnostics    | Export creates JSON file in ~/murmur-diagnostics/      |
| DA11 | Real-time events       | Events appear in log, folders auto-refresh             |
| DA12 | Real-time conflicts    | Sidebar conflict count updates automatically           |
| DA13 | Daemon reconnect       | App survives daemon stop/restart, reconnects           |
| DA14 | Uptime formats         | Seconds, minutes, hours/days formatted correctly       |
| DA15 | Error display          | IPC errors shown in red on Status screen               |
| DA16 | Leave: cancel          | Confirmation shows, cancel dismisses it                |
| DA17 | Leave: confirm         | Data wiped, daemon stops, app resets to Setup          |
| DA18 | Join network           | Mnemonic + name form, daemon joins, Folders shown      |
| DA18a| Join: mnemonic kept    | Settings shows the mnemonic entered during Join        |
| DA19 | Setup back nav         | Back buttons work through all setup steps              |
| DA20 | Auto-launch daemon     | App starts daemon automatically when config exists     |
