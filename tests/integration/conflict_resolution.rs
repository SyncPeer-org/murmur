//! Integration tests: Conflict detection and resolution.

#[path = "helpers.rs"]
mod helpers;

use helpers::*;
use murmur_engine::EngineEvent;

// =========================================================================
// Conflict detection
// =========================================================================

/// Concurrent modifications to the same file produce a conflict.
#[test]
fn test_concurrent_modification_creates_conflict() {
    let (mut nas, _) = create_engine("NAS");
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // NAS adds original file.
    let (meta, data) = make_file(b"original content", "readme.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    bidirectional_sync(&mut nas, &mut phone);

    // Both modify the same file independently.
    let (new_meta_nas, new_data_nas) = make_file(b"nas version", "readme.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "readme.txt", new_meta_nas, new_data_nas)
        .unwrap();

    let (new_meta_phone, new_data_phone) =
        make_file(b"phone version", "readme.txt", phone_id, folder_id);
    phone
        .modify_file(folder_id, "readme.txt", new_meta_phone, new_data_phone)
        .unwrap();

    // Sync: both see each other's modification.
    bidirectional_sync(&mut nas, &mut phone);

    // Rebuild conflicts.
    nas.rebuild_conflicts();
    phone.rebuild_conflicts();

    // Both should detect a conflict.
    let nas_conflicts = nas.list_conflicts();
    let phone_conflicts = phone.list_conflicts();
    assert!(!nas_conflicts.is_empty(), "NAS should detect conflict");
    assert!(!phone_conflicts.is_empty(), "Phone should detect conflict");

    // Conflict is for the right file.
    let conflict = &nas_conflicts[0];
    assert_eq!(conflict.path, "readme.txt");
    assert_eq!(conflict.folder_id, folder_id);
    assert!(conflict.versions.len() >= 2);
}

/// Conflict detection emits ConflictDetected event.
#[test]
fn test_conflict_emits_event() {
    let (mut nas, cb_nas) = create_engine("NAS");
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // Add file, sync.
    let (meta, data) = make_file(b"v1", "conflict.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    bidirectional_sync(&mut nas, &mut phone);

    // Concurrent modifications.
    let (m1, d1) = make_file(b"nas-v2", "conflict.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "conflict.txt", m1, d1).unwrap();
    let (m2, d2) = make_file(b"phone-v2", "conflict.txt", phone_id, folder_id);
    phone
        .modify_file(folder_id, "conflict.txt", m2, d2)
        .unwrap();

    cb_nas.events.lock().unwrap().clear();
    bidirectional_sync(&mut nas, &mut phone);
    nas.rebuild_conflicts();

    let events = cb_nas.events.lock().unwrap();
    // A DagSynced event should have fired.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::DagSynced { .. }))
    );
}

// =========================================================================
// Conflict resolution
// =========================================================================

/// Resolving a conflict removes it from the active list.
#[test]
fn test_resolve_conflict_removes_from_list() {
    let (mut nas, _) = create_engine("NAS");
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // Add, sync, modify concurrently.
    let (meta, data) = make_file(b"v1", "resolve_me.txt", nas_id, folder_id);
    nas.add_file(meta, data).unwrap();
    bidirectional_sync(&mut nas, &mut phone);

    let (m1, d1) = make_file(b"nas-v2", "resolve_me.txt", nas_id, folder_id);
    nas.modify_file(folder_id, "resolve_me.txt", m1, d1)
        .unwrap();
    let (m2, d2) = make_file(b"phone-v2", "resolve_me.txt", phone_id, folder_id);
    phone
        .modify_file(folder_id, "resolve_me.txt", m2, d2)
        .unwrap();

    bidirectional_sync(&mut nas, &mut phone);
    nas.rebuild_conflicts();

    let conflicts = nas.list_conflicts();
    assert!(!conflicts.is_empty());

    // Pick the first version to keep.
    let chosen_hash = conflicts[0].versions[0].blob_hash;
    nas.resolve_conflict(folder_id, "resolve_me.txt", chosen_hash)
        .unwrap();

    // Conflict should be gone.
    let conflicts_after = nas.list_conflicts();
    assert!(
        !conflicts_after.iter().any(|c| c.path == "resolve_me.txt"),
        "resolved conflict should be removed"
    );
}

/// Resolving a nonexistent conflict fails.
#[test]
fn test_resolve_nonexistent_conflict_fails() {
    let (mut engine, _) = create_engine("NAS");
    let folder_id = create_test_folder(&mut engine);

    let fake_hash = murmur_types::BlobHash::from_data(b"fake");
    let result = engine.resolve_conflict(folder_id, "no_such_file.txt", fake_hash);
    assert!(result.is_err());
}

// =========================================================================
// Conflict list filtering
// =========================================================================

/// List conflicts in a specific folder filters correctly.
#[test]
fn test_list_conflicts_in_folder_filters() {
    let (mut nas, _) = create_engine("NAS");
    let (mut phone, _, phone_id) = join_engine("Phone");
    let nas_id = nas.device_id();
    join_approve_sync(&mut nas, &mut phone, phone_id);

    let folder1 = create_test_folder(&mut nas);
    let (f2, _) = nas.create_folder("other").unwrap();
    let folder2 = f2.folder_id;
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder1);
    phone
        .subscribe_folder(folder2, murmur_types::SyncMode::Full)
        .unwrap();

    // Add files in both folders, sync.
    let (m1, d1) = make_file(b"v1", "file.txt", nas_id, folder1);
    nas.add_file(m1, d1).unwrap();
    let (m2, d2) = make_file(b"v1", "file.txt", nas_id, folder2);
    nas.add_file(m2, d2).unwrap();
    bidirectional_sync(&mut nas, &mut phone);

    // Concurrent mods in folder1 only.
    let (m3, d3) = make_file(b"nas-v2", "file.txt", nas_id, folder1);
    nas.modify_file(folder1, "file.txt", m3, d3).unwrap();
    let (m4, d4) = make_file(b"phone-v2", "file.txt", phone_id, folder1);
    phone.modify_file(folder1, "file.txt", m4, d4).unwrap();

    bidirectional_sync(&mut nas, &mut phone);
    nas.rebuild_conflicts();

    let all_conflicts = nas.list_conflicts();
    let folder1_conflicts = nas.list_conflicts_in_folder(folder1);
    let folder2_conflicts = nas.list_conflicts_in_folder(folder2);

    assert!(!folder1_conflicts.is_empty());
    assert!(folder2_conflicts.is_empty());
    // All conflicts are in folder1.
    assert_eq!(all_conflicts.len(), folder1_conflicts.len());
}
