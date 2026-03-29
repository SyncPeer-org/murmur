# Desktop App: Basic Tests — Startup, Setup, Navigation & Folders

Core functionality: daemon connection, setup wizard, sidebar navigation, folder
creation, basic status, and single-device verification. This is the smoke test
that should pass before moving to intermediate or advanced tests.

**Architecture**: murmurd is the daemon. The desktop app is a thin IPC client
that connects via Unix socket at `~/.murmur/murmurd.sock`. The app never embeds
the engine — all state comes from murmurd.

This test requires **3 terminal windows**: Terminal 1 runs the daemon, Terminal 2
runs CLI commands for verification, Terminal 3 runs the desktop app.

## Prerequisites

```bash
cargo build -p murmurd -p murmur-cli -p murmur-desktop
rm -rf /tmp/murmur-a
```

---

## DB1 — Daemon Check Screen (No Daemon)

Ensure no daemon is running, then launch the desktop app:

**Terminal 3**:

```bash
cargo run --bin murmur-desktop
```

**Expected**:
- App opens to the **Daemon Check** screen
- Status text: "Connecting to murmurd..."
- After a brief delay: "murmurd is not running" or connection error message
- Two buttons visible: **"Retry"** and **"Setup new network"**

---

## DB2 — Setup Wizard: Choose Mode

Click **"Setup new network"**.

**Expected**:
- Screen changes to Setup step 1 (Choose Mode)
- Title: "Murmur" with subtitle "Private Device Sync Network"
- Three buttons visible:
  - **"Create Network"**
  - **"Join Network"**
  - **"Back to daemon check!"**

---

## DB3 — Setup Wizard: Create Network Form

Click **"Create Network"**.

**Expected**:
- Screen changes to Setup step 2 (Form)
- Title: "Create Network"
- **"Back" button** visible (returns to step 1)
- Text input for "Device name"
- Submit button "Start daemon & create" — initially disabled (name empty)
- No mnemonic input field (create mode only needs device name)

---

## DB4 — Setup Wizard: Start Daemon & Create

Enter "test-desktop" in the device name field, then click **"Start daemon & create"**.

**Expected**:
- Status text changes to "Starting murmurd..."
- After a few seconds, the daemon starts and the app transitions to the **Folders screen**
- Sidebar appears on the left with navigation items

**Terminal 2** — verify daemon is running:

```bash
cargo run --bin murmur-cli -- status
```

**Expected**: `Device: test-desktop` with network ID and 0 peers.

**Checkpoint DB4**: Setup wizard successfully creates network and connects.

---

## DB5 — Sidebar Navigation

Verify the sidebar layout (left side, 180px):

**Expected sidebar items (top to bottom)**:
- "Murmur" header
- Sync status indicator (shows "Syncing" or "PAUSED")
- Horizontal rule
- **"Folders"** (currently selected)
- **"Conflicts (0)"** — shows pending conflict count
- **"Devices (0)"** — shows pending device count
- **"Recent Files"**
- **"Status"**
- Horizontal rule
- **"Network Health"**
- **"Settings"**
- Horizontal rule
- **"Pause Sync"** button (global sync toggle)

Click each sidebar item and verify it navigates to the correct screen:
1. Click **"Status"** — Status screen loads
2. Click **"Devices"** — Devices screen loads
3. Click **"Conflicts"** — Conflicts screen loads
4. Click **"Recent Files"** — Recent Files screen loads
5. Click **"Network Health"** — Network Health screen loads
6. Click **"Settings"** — Settings screen loads
7. Click **"Folders"** — Folders screen loads

**Checkpoint DB5**: All sidebar navigation items work.

---

## DB6 — Folders Screen: Empty State

Navigate to the Folders screen.

**Expected**:
- Title: "Folders"
- **"New Folder"** button visible at top
- Empty state message: "No folders yet. Create one to get started."

---

## DB7 — Create Folder

Create a test directory with some files:

```bash
mkdir -p /tmp/murmur-test-photos
echo "photo1" > /tmp/murmur-test-photos/photo1.jpg
echo "photo2" > /tmp/murmur-test-photos/photo2.jpg
```

Click **"New Folder"**.

**Expected**:
- The OS native **directory picker** (file explorer) opens
- No inline text input — the picker is the entire UI

Navigate to `/tmp/murmur-test-photos` and select it.

**Expected**:
- Folder "murmur-test-photos" appears in the "My Folders" section
  (name derived from the directory name)
- Row shows: folder name with **"Rename"** button, local path
  (`/tmp/murmur-test-photos`) below the name, file count ("2 files"),
  mode ("read-write"), **"Open"** and **"Unsub"** buttons
- Existing files on disk are scanned and counted immediately

**Terminal 2** — verify from CLI:

```bash
cargo run --bin murmur-cli -- folder list
```

**Expected**: "murmur-test-photos" folder listed with 2 files.

Click **"Open"** on "murmur-test-photos" to view the Folder Detail.

**Expected file list**:
- `photo1.jpg` — size ~7 B, type "image/jpeg"
- `photo2.jpg` — size ~7 B, type "image/jpeg"
- Total: exactly **2 files** matching what is on disk

Click **"Back"** to return to the Folders screen.

**Checkpoint DB7**: Folder creation picks directory, scans files, correct count.

---

## DB8 — Create Second Folder

Create another test directory:

```bash
mkdir -p /tmp/murmur-test-docs
echo "readme" > /tmp/murmur-test-docs/readme.txt
echo '{"key": "value"}' > /tmp/murmur-test-docs/data.json
echo "notes" > /tmp/murmur-test-docs/notes.txt
```

Click **"New Folder"** again, pick `/tmp/murmur-test-docs`.

**Expected**:
- Both "murmur-test-photos" and "murmur-test-docs" appear in "My Folders"
- "murmur-test-photos" shows **"2 files"**
- "murmur-test-docs" shows **"3 files"**

Click **"Open"** on "murmur-test-docs".

**Expected file list** (exactly 3 files):
- `data.json` — type "application/json"
- `notes.txt` — type "text/plain"
- `readme.txt` — type "text/plain"

**Terminal 2** — verify total file counts from CLI:

```bash
cargo run --bin murmur-cli -- folder list
```

**Expected**: two folders, "murmur-test-photos" with 2 files, "murmur-test-docs" with 3 files.

**Checkpoint DB8**: Multiple folders, file counts match disk contents exactly.

---

## DB8a — Rename Folder from Desktop

On the Folders screen, click **"Rename"** next to "murmur-test-photos".

**Expected**:
- The folder name is replaced by a text input pre-filled with "murmur-test-photos"
- **"Save"** and **"Cancel"** buttons appear next to the input

Change the name to "Photos" and click **"Save"** (or press Enter).

**Expected**:
- The folder name updates to "Photos"
- Local path still shows `/tmp/murmur-test-photos`
- File count unchanged ("2 files")

Click **"Rename"** again, then click **"Cancel"**.

**Expected**: name remains "Photos", no change.

**Checkpoint DB8a**: Folder rename works inline with save/cancel.

---

## DB9 — Cancel Folder Creation

Click **"New Folder"**, then close the OS directory picker without selecting.

**Expected**: Nothing happens, no new folder created.

---

## DB10 — Status Screen

Navigate to **Status** in the sidebar.

**Expected**:
- **Network section**:
  - Network ID (truncated hex)
  - Device: "test-desktop (ID:...)"
  - Stats: "Peers: 0 | DAG: N | Folders: 2 | Conflicts: 0"
  - Uptime in human-readable format (e.g., "0m 30s")
- **Event Log section**:
  - Header: "Event Log"
  - Shows recent engine events (folder_created, etc.)
  - If no events: "No events yet."
- No error displayed

**Checkpoint DB10**: Status screen shows correct network overview.

---

## DB11 — Devices Screen (Single Device)

Navigate to **Devices** in the sidebar.

**Expected**:
- **"This Device" section**:
  - Green bullet, device name "test-desktop"
  - Role: "full"
  - Status: "Online"
  - Device ID (truncated)
- **No "Pending Approval" section** (no pending devices)
- **"Other Devices" section**:
  - "No other devices on this network."

**Checkpoint DB11**: Device screen shows local device correctly.

---

## DB12 — Conflicts Screen (Empty)

Navigate to **Conflicts** in the sidebar.

**Expected**:
- Title: "Conflicts"
- "No active conflicts."

---

## DB13 — Recent Files Screen (Empty)

Navigate to **Recent Files** in the sidebar.

**Expected**:
- Title: "Recent Files"
- Search input: "Search across all folders..."
- Message: "Type a search term to find files across all folders."

---

## DB14 — Global Sync Pause/Resume

In the sidebar, click **"Pause Sync"**.

**Expected**:
- Sidebar sync status changes to "PAUSED"
- Button text changes to **"Resume Sync"**

Click **"Resume Sync"**.

**Expected**:
- Sidebar sync status changes to "Syncing"
- Button text changes to **"Pause Sync"**

**Checkpoint DB14**: Global sync toggle works.

---

## DB15 — Restart Desktop App: Folders Persist

Close the desktop app (Ctrl+C or close window). The daemon stays running.

Relaunch:

```bash
cargo run --bin murmur-desktop
```

Navigate to the **Folders** screen.

**Expected**:
- Both folders ("Photos" / "murmur-test-docs") appear in "My Folders"
- Each folder shows its name, local path, file count, mode, **"Rename"** / **"Open"** / **"Unsub"** buttons
- File counts unchanged ("2 files" and "3 files")

Click **"Open"** on "Photos".

**Expected**: same 2 files as before (photo1.jpg, photo2.jpg).

**Checkpoint DB15**: Folders survive desktop app restart.

---

## DB16 — Restart Daemon: Folders Persist

Close the desktop app. Stop the daemon (Ctrl+C in Terminal 1).

Restart the daemon:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-a --name test-desktop --role full
```

Relaunch the desktop app:

```bash
cargo run --bin murmur-desktop
```

Navigate to the **Folders** screen.

**Expected**:
- Both folders still present with correct names, paths, file counts
- Subscriptions intact (both show "read-write")

Click **"Open"** on either folder.

**Expected**: file list matches what was there before the restart.

**Checkpoint DB16**: Folders survive full daemon restart (DAG + config persistence).

---

## Cleanup

Stop the daemon (Ctrl+C in Terminal 1) and close the desktop app.

```bash
rm -rf /tmp/murmur-a /tmp/murmur-test-photos /tmp/murmur-test-docs
```

## Quick Reference

| #    | What               | Pass criteria                                       |
| ---- | ------------------ | --------------------------------------------------- |
| DB1  | Daemon check       | Shows connection error, Retry & Setup buttons       |
| DB2  | Setup: choose mode | Create/Join/Back buttons visible                    |
| DB3  | Setup: create form | Device name input, disabled submit                  |
| DB4  | Setup: start       | Daemon starts, app transitions to Folders screen    |
| DB5  | Sidebar nav        | All 7 screens reachable from sidebar                |
| DB6  | Folders empty      | Empty state message shown                           |
| DB7  | Create folder      | OS picker opens, folder created, files scanned      |
| DB8  | Second folder      | Both folders listed with correct file counts        |
| DB9  | Cancel create      | Picker closed, no folder created                    |
| DB10 | Status screen      | Network info, uptime, event log displayed           |
| DB11 | Devices screen     | Local device shown with green online indicator      |
| DB12 | Conflicts empty    | "No active conflicts" message                       |
| DB13 | Recent Files       | Search input and placeholder text                   |
| DB14 | Sync pause/resume  | Sidebar status and button text toggle correctly     |
