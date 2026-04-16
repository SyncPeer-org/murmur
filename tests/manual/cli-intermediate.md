# CLI: Intermediate Tests — Folders, Sync, Transfers & History

Folder management, file versioning, conflict handling, transfer status, large
file sync, and DAG delta sync. Run the **basic tests** first — this suite
assumes a two-node network is already established and both daemons are running.

This test requires **4 terminal windows**: Terminals 1 and 2 run the daemons;
Terminals 3 and 4 run CLI commands against each daemon.

## Prerequisites

Complete the basic test suite (cli-basic.md) or set up the equivalent state:

```bash
cargo build -p murmurd -p murmur-cli
rm -rf /tmp/murmur-a /tmp/murmur-b
```

Start both daemons, join and approve Node B (as in basic tests B1–B16).
Both nodes should be running with one file synced in the "default" folder.

---

## I1 — Folder List (Default Folder Exists)

The first `add` command auto-creates a "default" folder.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list
```

**Expected**: at least one folder — "default". Shows folder ID, name, local path (if
subscribed), file count, subscription status.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list
```

**Expected**: same "default" folder visible on Node B.

---

## I2 — Folder List JSON

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list --json
```

**Expected**: valid JSON `{"Folders": {"folders": [ ... ]}}`. Each entry has:
`folder_id` (64 hex), `name`, `created_by` (64 hex), `file_count`,
`subscribed` (bool), `mode` (string or null), `local_path` (string or null).

**Save the default folder ID** from this output.

---

## I3 — Create a New Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder create "photos"
```

**Expected**: confirmation message (e.g., "Folder created").

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list
```

**Expected**: two folders — "default" and "photos".

Wait 5-10 seconds, then verify on Node B:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list
```

**Expected**: same two folders on Node B.

**Save the photos folder ID**.

---

## I4 — Create a Second Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder create "documents"
```

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list
```

**Expected**: three folders — "default", "photos", "documents".

---

## I5 — Folder Status

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder status <DEFAULT_FOLDER_ID>
```

**Expected**:
- `Folder: default`
- `Folder ID:` matching the default folder ID
- `Files:` file count (>= 0)
- `Conflicts: 0`
- `Status:` a sync status string

---

## I6 — Folder Status JSON

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder status <DEFAULT_FOLDER_ID> --json
```

**Expected**: valid JSON `{"FolderStatus": { ... }}` with `folder_id`, `name`,
`file_count`, `conflict_count`, `sync_status` under the `FolderStatus` key.

---

## I7 — Folder Status for Non-existent Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder status "aa".repeat(32)
```

(Use a 64-character hex string that doesn't match any folder.)

**Expected**: error response (e.g., "folder not found"). Non-zero exit code.

---

## I8 — Folder Status Invalid Hex

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder status "not-a-hex-id"
```

**Expected**: error about invalid folder ID format. Non-zero exit code.

---

## I9 — Subscribe to Folder

Subscribe Node B to the "photos" folder:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder subscribe <PHOTOS_FOLDER_ID> /tmp/murmur-b-photos --name "photos"
```

**Expected**: confirmation message including the folder name (e.g., "Subscribed to folder photos (…)").

Verify:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list --json
```

**Expected**: "photos" folder shows `subscribed: true`, `mode: "full"`
(the default — accepts the legacy alias `read-write` too).

---

## I10 — Subscribe Receive-Only

Subscribe Node B to "documents" in receive-only mode:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder subscribe <DOCUMENTS_FOLDER_ID> /tmp/murmur-b-docs --name "documents" --mode receive-only
```

**Expected**: confirmation including folder name.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list --json
```

**Expected**: "documents" folder shows `subscribed: true`, `mode: "receive-only"`.
(`read-only` is accepted as a legacy alias on the `mode` subcommand but the
canonical value shown in output is `receive-only`.)

---

## I11 — Subscribe to Non-existent Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder subscribe aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa /tmp/murmur-b-fake --name "fake"
```

**Expected**: error — folder not found. Non-zero exit code.

---

## I12 — Change Folder Sync Mode

Change the "photos" subscription from full to receive-only:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder mode <PHOTOS_FOLDER_ID> receive-only
```

**Expected**: confirmation (e.g., "Mode changed"). `read-only` is accepted as
a legacy alias for `receive-only`.

Verify:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list --json
```

**Expected**: "photos" folder now shows `mode: "receive-only"`.

Change it back:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder mode <PHOTOS_FOLDER_ID> full
```

**Expected**: confirmation. Mode reverts to "full" (the legacy alias
`read-write` is also accepted).

---

## I12a — Rename Folder

Rename the "photos" folder on Node B:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder rename <PHOTOS_FOLDER_ID> "My Photos"
```

**Expected**: confirmation (e.g., "Folder renamed to My Photos.").

Verify:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list
```

**Expected**: folder now displays as "My Photos" with its local path. The rename is local
to this device — Node A still sees "photos".

---

## I13 — Folder Files (Default Folder)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <DEFAULT_FOLDER_ID>
```

**Expected**: lists files belonging to the "default" folder only. Files from other
folders are not shown.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <DEFAULT_FOLDER_ID> --json
```

**Expected**: valid JSON `{"Files": {"files": [ ... ]}}`.

---

## I14 — Folder Files (Empty Folder)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <PHOTOS_FOLDER_ID>
```

**Expected**: "No synced files." (or empty list — photos folder has no files yet).

---

## I15 — Add File Lands in "default" Folder

Add a file. With `photos` and `documents` already created, verify `add`
still targets the folder named "default" (not the first folder in BTreeMap
order):

```bash
echo "Hello from Node A — $(date)" > /tmp/test-file-a.txt
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-file-a.txt
```

**Expected**: "File added" with blob hash.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <DEFAULT_FOLDER_ID>
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder files <PHOTOS_FOLDER_ID>
```

**Expected**: `test-file-a.txt` appears in the `default` folder, and the
`photos` folder still returns "No synced files.".

---

## I16 — MIME Type Detection

Add files with different extensions:

```bash
echo '{"key": "value"}' > /tmp/test.json
echo '<html><body>hi</body></html>' > /tmp/test.html
dd if=/dev/urandom of=/tmp/test.bin bs=64 count=1 2>/dev/null
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test.json
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test.html
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test.bin
```

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a files --json
```

**Expected**: MIME types reflect the extension — `application/json` for
`.json`, `text/html` for `.html`, `application/octet-stream` for `.bin`
(the fallback for unknown/extension-less files). No entry should show
`mime_type: null`.

---

## I17 — File Deduplication

Add the same file again:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-file-a.txt
```

**Expected**: either dedup detected (same blob hash, no new entry) or a second entry
with the same blob hash. The file should not appear twice with different hashes.

---

## I18 — Large File Transfer (Chunked)

Create and add a 5 MB file to exercise chunked transfer:

```bash
dd if=/dev/urandom of=/tmp/test-large.bin bs=1M count=5 2>/dev/null
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-large.bin
```

**Expected**: "File added" with blob hash.

---

## I19 — Transfer Status

Immediately after adding the large file (while sync may still be in progress):

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a transfers
```

**Expected**: either shows an in-flight transfer with bytes_transferred/total_bytes
and percentage, or "No active transfers." if already complete.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a transfers --json
```

**Expected**: valid JSON `{"TransferStatus": {"transfers": [ ... ]}}`. Each
entry has `blob_hash`, `bytes_transferred`, `total_bytes`.

---

## I20 — Large File Arrives on Node B

Wait 10-15 seconds:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b files
```

**Expected**: `test-large.bin` appears with size ~5242880 bytes.

Verify integrity:

```bash
b3sum /tmp/test-large.bin
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b files --json | jq '.Files.files[] | select(.path | contains("test-large.bin")) | .blob_hash'
```

**Expected**: hashes match.

---

## I21 — DAG Sync on Reconnect (Delta Sync)

**Terminal 2** — stop Node B (Ctrl+C).

Add a file while Node B is offline:

```bash
echo "Added while B was offline — $(date)" > /tmp/test-offline.txt
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-offline.txt
```

Restart Node B:

```bash
# Terminal 2
cargo run --bin murmurd -- --data-dir /tmp/murmur-b --name "node-b" --verbose
```

Watch verbose logs for `DagSyncRequest`/`DagSyncResponse` messages (delta sync).

Wait a few seconds:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b files
```

**Expected**: `test-offline.txt` appears on Node B.

---

## I22 — Delta Sync: Multiple Offline Entries

**Terminal 2** — stop Node B again.

Add multiple files while offline:

```bash
echo "Offline file 1" > /tmp/test-off1.txt
echo "Offline file 2" > /tmp/test-off2.txt
echo "Offline file 3" > /tmp/test-off3.txt
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-off1.txt
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-off2.txt
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a add /tmp/test-off3.txt
```

Restart Node B. Wait a few seconds:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b files
```

**Expected**: all three offline files appear on Node B. DAG entry counts match
across both nodes.

---

## I23 — Conflict Detection (Empty)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a conflicts
```

**Expected**: "No active conflicts."

---

## I24 — Conflicts JSON

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a conflicts --json
```

**Expected**: valid JSON `{"Conflicts": {"conflicts": []}}`.

---

## I25 — Conflicts Filtered by Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a conflicts --folder <DEFAULT_FOLDER_ID>
```

**Expected**: "No active conflicts."

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a conflicts --folder <PHOTOS_FOLDER_ID>
```

**Expected**: "No active conflicts."

---

## I26 — Conflict Resolution (Non-existent)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a resolve <DEFAULT_FOLDER_ID> no-such-file.txt aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```

**Expected**: error (e.g., "conflict not found" or "no conflict for this file").
Non-zero exit code.

---

## I27 — File History

Check version history for a file:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a history <DEFAULT_FOLDER_ID> test-file-a.txt
```

**Expected**: at least one version showing:
- Blob hash (64 hex chars)
- Size in bytes
- Device name ("node-a")
- Device ID (64 hex chars)
- Modification timestamp

---

## I28 — File History JSON

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a history <DEFAULT_FOLDER_ID> test-file-a.txt --json
```

**Expected**: valid JSON `{"FileVersions": {"versions": [ ... ]}}`. Each entry
has `blob_hash`, `device_id`, `device_name`, `modified_at`, `size`.

---

## I29 — File History (Non-existent File)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a history <DEFAULT_FOLDER_ID> no-such-file.txt
```

**Expected**: "No version history."

---

## I30 — File History on Node B (Cross-Node Consistency)

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b history <DEFAULT_FOLDER_ID> test-file-a.txt
```

**Expected**: same version history as Node A for the same file.

---

## I31 — Unsubscribe from Folder

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder unsubscribe <DOCUMENTS_FOLDER_ID>
```

**Expected**: confirmation.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list --json
```

**Expected**: "documents" folder shows `subscribed: false`.

---

## I32 — Unsubscribe with Keep-Local

Subscribe first, then unsubscribe with `--keep-local`:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder subscribe <DOCUMENTS_FOLDER_ID> /tmp/murmur-b-docs2 --name "documents"
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder unsubscribe <DOCUMENTS_FOLDER_ID> --keep-local
```

**Expected**: unsubscription succeeds. If local files existed in `/tmp/murmur-b-docs2`,
they are preserved on disk.

---

## I33 — Folder Remove

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder remove <DOCUMENTS_FOLDER_ID>
```

**Expected**: confirmation.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list
```

**Expected**: "documents" folder no longer appears.

Wait 5-10 seconds. On Node B:

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b folder list
```

**Expected**: "documents" folder removed on Node B as well.

---

## I33a — Restart Daemon: Folders & Subscriptions Persist

Stop Node A (Ctrl+C). Restart it:

```bash
cargo run --bin murmurd -- --data-dir /tmp/murmur-a --name node-a
```

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list
```

**Expected**: remaining folders are present with correct names, local paths, file
counts, and subscription states.

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a folder list --json
```

**Expected**: each subscribed folder has `local_path` set (not null), `name` non-empty,
`subscribed: true`.

**Checkpoint I33a**: Folders, names, and subscriptions survive daemon restart.

---

## I34 — DAG Consistency After All Operations

```bash
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-a status --json | jq .Status.dag_entries
cargo run --bin murmur-cli -- --data-dir /tmp/murmur-b status --json | jq .Status.dag_entries
```

**Expected**: both nodes report the same DAG entry count.

---

## Cleanup

```bash
rm -rf /tmp/murmur-a /tmp/murmur-b /tmp/murmur-b-photos /tmp/murmur-b-docs /tmp/murmur-b-docs2
rm -f /tmp/test-file-a.txt /tmp/test-file-b.txt /tmp/test.json /tmp/test.html /tmp/test.bin
rm -f /tmp/test-large.bin /tmp/test-offline.txt /tmp/test-off1.txt /tmp/test-off2.txt /tmp/test-off3.txt
```

## Quick Reference

| # | What | Pass criteria |
|---|------|---------------|
| I1 | Folder list | Default folder visible on both nodes |
| I2 | Folder list JSON | Valid JSON, all fields present |
| I3 | Create folder | "photos" folder created, synced to Node B |
| I4 | Create second folder | "documents" created, three folders total |
| I5 | Folder status | Correct name, file count, conflict count |
| I6 | Folder status JSON | Valid JSON with all fields |
| I7 | Status non-existent folder | Error response |
| I8 | Status invalid hex | Error about invalid folder ID |
| I9 | Subscribe | Node B subscribed to "photos" |
| I10 | Subscribe receive-only | "documents" subscribed in receive-only mode |
| I11 | Subscribe non-existent | Error response |
| I12 | Change mode | Mode switches between full and receive-only |
| I13 | Folder files | Lists only files in specified folder |
| I14 | Folder files (empty) | Empty result for folder with no files |
| I15 | Add lands in "default" | File appears under the folder named "default" |
| I16 | MIME detection | Known extensions map correctly; unknown/no extension → `application/octet-stream`, never `null` |
| I17 | Dedup | Same file content produces same blob hash |
| I18 | Large file add | 5 MB file added successfully |
| I19 | Transfer status | Shows active/completed transfers, valid JSON |
| I20 | Large file sync | 5 MB file arrives on Node B with correct hash |
| I21 | Delta sync | Offline file synced after reconnect |
| I22 | Multi-entry delta | Multiple offline files all sync |
| I23 | Conflicts (empty) | "No active conflicts." |
| I24 | Conflicts JSON | Valid JSON with empty array |
| I25 | Conflicts by folder | Folder filter works |
| I26 | Resolve non-existent | Error response |
| I27 | File history | Version chain with device info |
| I28 | File history JSON | Valid JSON with versions array |
| I29 | History non-existent | "No version history." |
| I30 | History cross-node | Same history on both nodes |
| I31 | Unsubscribe | Subscription removed |
| I32 | Unsubscribe keep-local | Local files preserved |
| I33 | Folder remove | Folder removed from both nodes |
| I34 | DAG consistency | Same entry count after all operations |
