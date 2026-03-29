//! Integration tests: Folder management scenarios.

#[path = "helpers.rs"]
mod helpers;

use helpers::*;
use murmur_engine::EngineEvent;
use murmur_types::{DeviceRole, SyncMode};

// =========================================================================
// Folder creation and listing
// =========================================================================

/// Creating a folder auto-subscribes the creator.
#[test]
fn test_create_folder_auto_subscribes_creator() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);

    let (folder, entries) = engine.create_folder("Photos").unwrap();
    assert_eq!(folder.name, "Photos");
    assert_eq!(entries.len(), 2); // FolderCreated + FolderSubscribed

    let folders = engine.list_folders();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "Photos");

    let subs = engine.folder_subscriptions(folder.folder_id);
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].device_id, engine.device_id());
    assert_eq!(subs[0].mode, SyncMode::ReadWrite);
}

/// Multiple folders can be created.
#[test]
fn test_create_multiple_folders() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);

    engine.create_folder("Photos").unwrap();
    engine.create_folder("Documents").unwrap();
    engine.create_folder("Music").unwrap();

    let folders = engine.list_folders();
    assert_eq!(folders.len(), 3);

    let names: Vec<&str> = folders.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"Photos"));
    assert!(names.contains(&"Documents"));
    assert!(names.contains(&"Music"));
}

/// Folder creation emits FolderCreated event.
#[test]
fn test_create_folder_emits_event() {
    let (mut engine, cb) = create_engine("NAS", DeviceRole::Full);

    let (folder, _) = engine.create_folder("Backups").unwrap();

    let events = cb.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::FolderCreated { folder_id, name }
            if *folder_id == folder.folder_id && name == "Backups"
    )));
}

// =========================================================================
// Folder subscription
// =========================================================================

/// Remote device subscribes to a folder after sync.
#[test]
fn test_remote_device_subscribes_to_folder() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Source);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    subscribe_test_folder(&mut phone, folder_id);

    // Both should see two subscriptions.
    let subs_on_phone = phone.folder_subscriptions(folder_id);
    assert_eq!(subs_on_phone.len(), 2);

    // Sync phone's subscription back to NAS.
    sync_engines(&phone, &mut nas);
    let subs_on_nas = nas.folder_subscriptions(folder_id);
    assert_eq!(subs_on_nas.len(), 2);
}

/// Subscribe to a nonexistent folder fails.
#[test]
fn test_subscribe_nonexistent_folder_fails() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let fake_folder = murmur_types::FolderId::from_data(b"nonexistent");
    let result = engine.subscribe_folder(fake_folder, SyncMode::ReadWrite);
    assert!(result.is_err());
}

/// Unsubscribe from a folder.
#[test]
fn test_unsubscribe_folder() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let folder_id = create_test_folder(&mut engine);

    // Auto-subscribed after creation.
    assert_eq!(engine.folder_subscriptions(folder_id).len(), 1);

    // Unsubscribe.
    engine.unsubscribe_folder(folder_id).unwrap();

    // Subscription removed.
    assert_eq!(engine.folder_subscriptions(folder_id).len(), 0);
}

/// ReadOnly subscription prevents file addition.
#[test]
fn test_read_only_subscription_prevents_writes() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Source);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);

    // Subscribe as read-only.
    phone
        .subscribe_folder(folder_id, SyncMode::ReadOnly)
        .unwrap();

    // Try to add a file — should fail.
    let (meta, data) = make_file(b"test", "test.txt", phone_id, folder_id);
    let result = phone.add_file(meta, data);
    assert!(result.is_err());
}

// =========================================================================
// Folder removal
// =========================================================================

/// Remove a folder from the network.
#[test]
fn test_remove_folder() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);

    let folder_id = create_test_folder(&mut engine);
    assert_eq!(engine.list_folders().len(), 1);

    engine.remove_folder(folder_id).unwrap();
    assert_eq!(engine.list_folders().len(), 0);
}

/// Remove a nonexistent folder fails.
#[test]
fn test_remove_nonexistent_folder_fails() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let fake_folder = murmur_types::FolderId::from_data(b"nonexistent");
    let result = engine.remove_folder(fake_folder);
    assert!(result.is_err());
}

/// Folder removal syncs to remote devices.
#[test]
fn test_folder_removal_syncs() {
    let (mut nas, _) = create_engine("NAS", DeviceRole::Full);
    let (mut phone, _, phone_id) = join_engine("Phone");
    join_approve_sync(&mut nas, &mut phone, phone_id, DeviceRole::Source);

    let folder_id = create_test_folder(&mut nas);
    sync_engines(&nas, &mut phone);
    assert_eq!(phone.list_folders().len(), 1);

    nas.remove_folder(folder_id).unwrap();
    sync_engines(&nas, &mut phone);
    assert_eq!(phone.list_folders().len(), 0);
}

// =========================================================================
// Folder files listing
// =========================================================================

/// Folder files returns only files in that folder.
#[test]
fn test_folder_files_isolation() {
    let (mut engine, _) = create_engine("NAS", DeviceRole::Full);
    let id = engine.device_id();

    let folder1 = create_test_folder(&mut engine);
    let (folder2, _) = engine.create_folder("other").unwrap();

    let (meta1, data1) = make_file(b"in folder 1", "f1.txt", id, folder1);
    engine.add_file(meta1, data1).unwrap();

    let (meta2, data2) = make_file(b"in folder 2", "f2.txt", id, folder2.folder_id);
    engine.add_file(meta2, data2).unwrap();

    let files1 = engine.folder_files(folder1);
    let files2 = engine.folder_files(folder2.folder_id);

    assert_eq!(files1.len(), 1);
    assert_eq!(files1[0].path, "f1.txt");
    assert_eq!(files2.len(), 1);
    assert_eq!(files2[0].path, "f2.txt");
}
