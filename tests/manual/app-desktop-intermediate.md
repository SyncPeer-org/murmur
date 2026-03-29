# Desktop App: Intermediate Tests — Files, Conflicts, Devices & Folder Management

File sync, folder detail, search/sort, file history, conflict resolution, device
approval, folder subscribe/unsubscribe, network folder discovery, and ignore
patterns. Run the **basic tests** first.

This test requires **4 terminal windows**: Terminals 1 and 2 run daemons,
Terminals 3 and 4 run CLI and desktop app respectively.

## Prerequisites

Complete the basic test suite (app-desktop-basic.md) or set up equivalent state:

```bash
cargo build -p murmurd -p murmur-cli -p murmur-desktop
rm -rf /tmp/murmur-a /tmp/murmur-b /tmp/murmur-sync-a /tmp/murmur-sync-b
mkdir -p /tmp/murmur-sync-a /tmp/murmur-sync-b
```

**Terminal 1** — start daemon A:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-a --name "node-a" --role full --verbose
```

**Save the mnemonic** printed to stdout.

**Terminal 3** — create a folder and add a file:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder create "Photos"
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder subscribe <PHOTOS_FOLDER_ID> /tmp/murmur-sync-a --name "Photos"
echo "Hello from CLI" > /tmp/murmur-sync-a/test-file.txt
```

Wait a few seconds for the filesystem watcher to pick up the file.

**Terminal 4** — launch the desktop app:

```bash
cargo run --bin murmur-desktop
```

The app should connect to murmurd and show the Folders screen.

---

## DI1 — Folder Detail: Open Folder

On the Folders screen, click **"Open"** next to "Photos".

**Expected**:
- Navigates to the **Folder Detail** screen
- Header shows: **"Back"** button, folder name "Photos", **"Pause"** button
- Info line: "ID: [hex:16] | 1 files | Mode: read-write"
- Subscribers section shows: "Subscribers: node-a [read-write]"
- Ignore Patterns section with text area and **"Save"** button
- Search input: "Search files..."
- Sort buttons: **"Name"**, **"Size"**, **"Type"**
- File list with columns: Path, Size, Type, Actions

---

## DI2 — Folder Detail: File List

In the folder detail, verify the file list.

**Expected**:
- `test-file.txt` listed with:
  - Path: `test-file.txt`
  - Size: human-readable (e.g., "15 B")
  - Type: MIME type or "--"
  - **"History"** button
  - **"Del"** button

**Checkpoint DI2**: CLI-added file visible in desktop folder detail.

---

## DI3 — File Search

Type "test" in the "Search files..." input.

**Expected**: `test-file.txt` still visible (matches search).

Type "nonexistent" in the search input.

**Expected**: "No files match." message.

Clear the search input.

**Expected**: All files shown again.

---

## DI4 — File Sort

Add more files from CLI for sort testing:

**Terminal 3**:

```bash
echo "AAAA" > /tmp/murmur-sync-a/alpha.txt
echo "A bigger file with more content for size testing" > /tmp/murmur-sync-a/zeta.dat
```

Wait a few seconds, then refresh (navigate away and back to "Photos").

Click **"Name"** sort button.

**Expected**: Files sorted alphabetically by path (ascending). Click again to reverse.

Click **"Size"** sort button.

**Expected**: Files sorted by size. Click again to reverse direction.

Click **"Type"** sort button.

**Expected**: Files sorted by MIME type.

**Checkpoint DI4**: Search and sort work in folder detail.

---

## DI5 — File History

Click **"History"** next to `test-file.txt`.

**Expected**:
- Navigates to the **File History** screen
- Header: **"Back"** button, "History: test-file.txt"
- At least one version listed with:
  - Truncated blob hash
  - Device name ("node-a")
  - HLC timestamp
  - Size
  - **"Restore"** button

---

## DI6 — File Version Restore

First, create a second version from CLI:

**Terminal 3**:

```bash
echo "Updated content v2" > /tmp/murmur-sync-a/test-file.txt
```

Wait a few seconds, then navigate to the file history for `test-file.txt` again.

**Expected**: Two versions listed (most recent first).

Click **"Restore"** on the older version.

**Expected**: File restored to older version. Navigate back to folder detail to
confirm the file is still listed.

**Checkpoint DI6**: File history and restore work.

---

## DI7 — File Deletion

Click **"Del"** next to `alpha.txt` in the folder detail.

**Expected**: File removed from the list. Folder file count decreases by 1.

**Terminal 3** — verify:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <PHOTOS_FOLDER_ID>
```

**Expected**: `alpha.txt` no longer listed.

**Checkpoint DI7**: File deletion from desktop works.

---

## DI8 — Per-Folder Sync Pause/Resume

In the folder detail header, click **"Pause"**.

**Expected**: Button text changes to **"Resume"**.

Click **"Resume"**.

**Expected**: Button text changes to **"Pause"**.

---

## DI9 — Ignore Patterns

In the folder detail, scroll to the "Ignore Patterns" section.

Type the following in the ignore patterns text area:

```
*.log
temp/
```

Click **"Save"**.

**Expected**: Patterns saved. Files matching `*.log` and files under `temp/`
will be excluded from sync.

**Terminal 3** — verify:

```bash
echo "should be ignored" > /tmp/murmur-sync-a/debug.log
```

Wait a few seconds. Navigate back to the folder detail.

**Expected**: `debug.log` does NOT appear in the file list.

**Checkpoint DI9**: Ignore patterns save and apply correctly.

---

## DI10 — Conflict Detection

Create a conflict scenario. This requires a second daemon:

**Terminal 2** — start daemon B:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-b --name "node-b" --role full --verbose --join "<MNEMONIC>"
```

**Terminal 3** — approve node-b:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a pending
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a approve <NODE_B_DEVICE_ID> full
```

Wait for sync. Subscribe node-b to Photos:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder subscribe <PHOTOS_FOLDER_ID> /tmp/murmur-sync-b --name "Photos"
```

Wait for initial sync. Then create concurrent edits:

```bash
echo "Version from node-a" > /tmp/murmur-sync-a/conflict-file.txt
echo "Version from node-b" > /tmp/murmur-sync-b/conflict-file.txt
```

Wait for sync and conflict detection.

In the desktop app, navigate to **Conflicts** in the sidebar.

**Expected**:
- Sidebar shows **"Conflicts (1)"** (or more)
- Conflicts screen shows the conflict for `conflict-file.txt`
- Each conflict entry shows:
  - File path: "[Photos] -- conflict-file.txt"
  - Two (or more) competing versions, each with:
    - Device name
    - Truncated device ID
    - HLC timestamp
    - **"Keep"** button
  - **"Keep Both (dismiss)"** button at the bottom of the conflict

**Checkpoint DI10**: Conflicts detected and displayed.

---

## DI11 — Resolve Conflict (Keep One Version)

Click **"Keep"** next to the version from "node-a".

**Expected**:
- Conflict disappears from the list
- Sidebar conflict count decreases
- Folder files reflect the chosen version

---

## DI12 — Dismiss Conflict (Keep Both)

Create another conflict:

```bash
echo "Another v-a" > /tmp/murmur-sync-a/another-conflict.txt
echo "Another v-b" > /tmp/murmur-sync-b/another-conflict.txt
```

Wait for sync and detection. Navigate to Conflicts.

Click **"Keep Both (dismiss)"**.

**Expected**: Conflict dismissed — removed from the list without choosing a winner.

---

## DI13 — Bulk Conflict Resolution

Create multiple conflicts in the same folder:

```bash
echo "bulk-a-1" > /tmp/murmur-sync-a/bulk1.txt
echo "bulk-b-1" > /tmp/murmur-sync-b/bulk1.txt
echo "bulk-a-2" > /tmp/murmur-sync-a/bulk2.txt
echo "bulk-b-2" > /tmp/murmur-sync-b/bulk2.txt
```

Wait for sync and detection. Navigate to Conflicts.

**Expected**: Bulk resolution toolbar visible above conflicts.
**"Keep All Newest"** button shown for the Photos folder.

Click **"Keep All Newest"**.

**Expected**: All conflicts in the Photos folder resolved at once. List clears.

**Checkpoint DI13**: All three conflict resolution modes work.

---

## DI14 — Device Approval from Desktop

Stop node-b, wipe its data, and rejoin to create a pending device:

```bash
# Stop daemon B (Ctrl+C in Terminal 2)
rm -rf /tmp/murmur-b
cargo run --bin murmurd -- --data-dir /tmp/murmur-b --name "node-c" --role full --verbose --join "<MNEMONIC>"
```

In the desktop app, navigate to **Devices**.

**Expected**:
- Sidebar shows **"Devices (1)"** (pending count)
- **"Pending Approval" section** visible with "node-c"
- **"Approve"** button next to the pending device

Click **"Approve"**.

**Expected**:
- Device moves from "Pending Approval" to "Other Devices" section
- Sidebar pending count returns to 0
- Device shows as "Online" with "full" role

**Checkpoint DI14**: Device approval works from desktop.

---

## DI15 — Device Presence (Online/Offline)

Stop daemon B (Ctrl+C in Terminal 2).

Wait a few seconds, then check the Devices screen.

**Expected**:
- "node-c" status changes from "Online" to a relative time (e.g., "1 min ago")
- Status indicator changes from green to gray

Restart daemon B:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-b --name "node-c" --role full --verbose
```

Wait a few seconds.

**Expected**: "node-c" returns to "Online" with green indicator.

**Checkpoint DI15**: Device presence updates in real time.

---

## DI16 — Network Folder Discovery

Navigate to **Folders** in the sidebar.

**Expected**: Two sections visible:
- **"My Folders"**: Lists folders you are subscribed to (e.g., "Photos")
- **"Available on Network"**: Lists any unsubscribed folders (if node-c created one)

Create a folder from node-c:

**Terminal 3**:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder create "Music"
```

Wait for sync. Return to Folders screen in the desktop app (navigate away and back).

**Expected**: "Music" appears in the "Available on Network" section with:
- Folder name
- File count
- Subscriber count (e.g., "1 subs")
- **"Subscribe"** button

---

## DI17 — Folder Subscribe from Desktop

Click **"Subscribe"** next to "Music".

**Expected**: A native directory picker dialog opens.

Select a directory (e.g., `/tmp/murmur-sync-a-music`).

**Expected**:
- "Music" moves from "Available on Network" to "My Folders"
- The folder name ("Music") is preserved in the subscription
- Local path shown below the name (e.g., `/tmp/murmur-sync-a-music`)
- **"Rename"** button next to the folder name
- Shows as "read-write" mode with **"Open"** and **"Unsub"** buttons

---

## DI18 — Folder Unsubscribe

Click **"Unsub"** next to "Music".

**Expected**:
- "Music" moves from "My Folders" back to "Available on Network"
- Local files in the sync directory are preserved (not deleted)

**Checkpoint DI18**: Folder subscribe and unsubscribe work from desktop.

---

## DI19 — Folder Detail: Subscribers List

Subscribe to "Music" again, then click **"Open"** on it.

In the folder detail, check the Subscribers section.

**Expected**: "Subscribers: node-a [read-write], node-c [read-write]" (or similar)
listing all devices subscribed to this folder with their sync modes.

---

## Cleanup

Stop both daemons and close the desktop app.

```bash
rm -rf /tmp/murmur-a /tmp/murmur-b /tmp/murmur-sync-a /tmp/murmur-sync-b
```

## Quick Reference

| #    | What                    | Pass criteria                                          |
| ---- | ----------------------- | ------------------------------------------------------ |
| DI1  | Folder detail           | Header, info line, subscribers, search, files shown    |
| DI2  | File list               | CLI file visible with size, type, action buttons       |
| DI3  | File search             | Filters files by substring, clears correctly           |
| DI4  | File sort               | Name/Size/Type sort, toggle direction                  |
| DI5  | File history            | Versions listed with hash, device, HLC, restore btn   |
| DI6  | Version restore         | Older version restored, file still listed              |
| DI7  | File deletion           | File removed from list, confirmed via CLI              |
| DI8  | Folder sync pause       | Pause/resume button toggles correctly                  |
| DI9  | Ignore patterns         | Patterns saved, matching files excluded from sync      |
| DI10 | Conflict detection      | Conflicts shown with versions and action buttons       |
| DI11 | Resolve conflict        | "Keep" picks version, conflict removed                 |
| DI12 | Dismiss conflict        | "Keep Both" dismisses without choosing                 |
| DI13 | Bulk resolve            | "Keep All Newest" resolves all folder conflicts        |
| DI14 | Device approval         | Pending device approved, moves to Other Devices        |
| DI15 | Device presence         | Online/offline status updates with indicators          |
| DI16 | Network folder discover | Unsubscribed folders shown in Available on Network     |
| DI17 | Folder subscribe        | Directory picker, folder moves to My Folders           |
| DI18 | Folder unsubscribe      | Folder moves back, local files preserved               |
| DI19 | Folder subscribers      | Subscriber list shows all devices with modes           |
