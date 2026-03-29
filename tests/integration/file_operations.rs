//! Integration tests: File modification, deletion, and version history.

#[path = "helpers.rs"]
mod helpers;

use helpers::*;
use murmur_engine::EngineEvent;
use murmur_types::DeviceRole;

// =========================================================================
// File modification
// =========================================================================

/// Modify a file and verify version history grows.
#[test]
fn test_file_modification_creates_version() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    // Add initial version.
    let (meta, data) = make_file(b"version 1", "doc.txt", id, folder_id);
    engine.add_file(meta, data).unwrap();
    assert_eq!(engine.file_history(folder_id, "doc.txt").len(), 1);

    // Modify.
    let (new_meta, new_data) = make_file(b"version 2", "doc.txt", id, folder_id);
    engine
        .modify_file(folder_id, "doc.txt", new_meta, new_data)
        .unwrap();

    // History should have 2 versions.
    let history = engine.file_history(folder_id, "doc.txt");
    assert_eq!(history.len(), 2);

    // Current file should have the latest hash.
    let current = engine
        .state()
        .files
        .get(&(folder_id, "doc.txt".to_string()))
        .unwrap();
    let expected_hash = murmur_types::BlobHash::from_data(b"version 2");
    assert_eq!(current.blob_hash, expected_hash);
}

/// Multiple modifications build a full version chain.
#[test]
fn test_multiple_modifications_version_chain() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    let (meta, data) = make_file(b"v1", "chain.txt", id, folder_id);
    engine.add_file(meta, data).unwrap();

    for i in 2..=5 {
        let content = format!("v{i}");
        let (new_meta, new_data) = make_file(content.as_bytes(), "chain.txt", id, folder_id);
        engine
            .modify_file(folder_id, "chain.txt", new_meta, new_data)
            .unwrap();
    }

    let history = engine.file_history(folder_id, "chain.txt");
    assert_eq!(history.len(), 5);
}

/// Modification of a nonexistent file fails.
#[test]
fn test_modify_nonexistent_file_fails() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    let (meta, data) = make_file(b"content", "ghost.txt", id, folder_id);
    let result = engine.modify_file(folder_id, "ghost.txt", meta, data);
    assert!(result.is_err());
}

/// Modification emits FileModified event.
#[test]
fn test_modification_emits_event() {
    let (mut engine, cb) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    let (meta, data) = make_file(b"v1", "event.txt", id, folder_id);
    engine.add_file(meta, data).unwrap();

    cb.events.lock().unwrap().clear();

    let (new_meta, new_data) = make_file(b"v2", "event.txt", id, folder_id);
    engine
        .modify_file(folder_id, "event.txt", new_meta, new_data)
        .unwrap();

    let events = cb.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::FileModified { path, .. } if path == "event.txt"
    )));
}

/// File modification syncs to remote device.
#[test]
fn test_file_modification_syncs() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Full);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // NAS adds and modifies.
    let (meta, data) = make_file(b"v1", "synced.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    sync_engines(&nas, &mut phone);

    let (new_meta, new_data) = make_file(b"v2", "synced.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "synced.txt", new_meta, new_data)
        .unwrap();
    sync_engines(&nas, &mut phone);

    // Phone should see the latest version.
    let phone_file = phone
        .state()
        .files
        .get(&(folder_id, "synced.txt".to_string()))
        .unwrap();
    let expected = murmur_types::BlobHash::from_data(b"v2");
    assert_eq!(phone_file.blob_hash, expected);

    // Phone should have both versions in history.
    let history = phone.file_history(folder_id, "synced.txt");
    assert_eq!(history.len(), 2);
}

// =========================================================================
// File deletion
// =========================================================================

/// Delete a file removes it from the files map.
#[test]
fn test_delete_file() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    let (meta, data) = make_file(b"content", "deleteme.txt", id, folder_id);
    engine.add_file(meta, data).unwrap();
    assert_eq!(engine.folder_files(folder_id).len(), 1);

    engine.delete_file(folder_id, "deleteme.txt").unwrap();
    assert_eq!(engine.folder_files(folder_id).len(), 0);
}

/// Deleting a nonexistent file fails.
#[test]
fn test_delete_nonexistent_file_fails() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let folder_id = create_test_folder(&mut engine);

    let result = engine.delete_file(folder_id, "nonexistent.txt");
    assert!(result.is_err());
}

/// File deletion syncs to remote device.
#[test]
fn test_delete_file_syncs() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Full);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // NAS adds a file.
    let (meta, data) = make_file(b"temp", "temp.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    sync_engines(&nas, &mut phone);
    assert_eq!(phone.folder_files(folder_id).len(), 1);

    // NAS deletes the file.
    nas.delete_file(folder_id, "temp.txt").unwrap();
    sync_engines(&nas, &mut phone);
    assert_eq!(phone.folder_files(folder_id).len(), 0);
}

/// Cannot delete from a read-only folder.
#[test]
fn test_delete_from_readonly_fails() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Source);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);

    // Phone subscribes as read-only.
    phone
        .subscribe_folder(folder_id, murmur_types::SyncMode::ReadOnly)
        .unwrap();

    // NAS adds a file and syncs.
    let (meta, data) = make_file(b"protected", "protected.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    sync_engines(&nas, &mut phone);

    // Phone tries to delete — should fail.
    let result = phone.delete_file(folder_id, "protected.txt");
    assert!(result.is_err());
}

// =========================================================================
// File history
// =========================================================================

/// File history is empty for nonexistent file.
#[test]
fn test_file_history_empty() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let folder_id = create_test_folder(&mut engine);

    let history = engine.file_history(folder_id, "nonexistent.txt");
    assert!(history.is_empty());
}

/// File history tracks blob hashes correctly.
#[test]
fn test_file_history_blob_hashes() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();
    let folder_id = create_test_folder(&mut engine);

    let (meta, data) = make_file(b"alpha", "tracked.txt", id, folder_id);
    let hash1 = meta.blob_hash;
    engine.add_file(meta, data).unwrap();

    let (new_meta, new_data) = make_file(b"beta", "tracked.txt", id, folder_id);
    let hash2 = new_meta.blob_hash;
    engine
        .modify_file(folder_id, "tracked.txt", new_meta, new_data)
        .unwrap();

    let history = engine.file_history(folder_id, "tracked.txt");
    assert_eq!(history.len(), 2);

    let hashes: Vec<_> = history.iter().map(|(h, _)| *h).collect();
    assert!(hashes.contains(&hash1));
    assert!(hashes.contains(&hash2));
}

/// File history is preserved across sync.
#[test]
fn test_file_history_syncs() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Full);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // NAS creates 3 versions.
    let (m1, d1) = make_file(b"v1", "history.txt", nas_id, folder_id);
    nas.add_file(m1, d1).unwrap();
    let (m2, d2) = make_file(b"v2", "history.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "history.txt", m2, d2).unwrap();
    let (m3, d3) = make_file(b"v3", "history.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "history.txt", m3, d3).unwrap();

    // Sync all to phone.
    sync_engines(&nas, &mut phone);

    let phone_history = phone.file_history(folder_id, "history.txt");
    assert_eq!(phone_history.len(), 3);
}
