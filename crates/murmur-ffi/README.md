# murmur-ffi

UniFFI FFI bindings that expose `murmur-engine` to Android and iOS.

iroh is an internal implementation detail — mobile code never sees iroh types.
The FFI boundary is thin: bytes in, bytes out, callback for persistence.

## Build — Android

```bash
# Install cargo-ndk and add Android targets
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Build .so files for all Android ABIs
cargo ndk \
  -t arm64-v8a \
  -t armeabi-v7a \
  -t x86_64 \
  -o platforms/android/app/src/main/jniLibs \
  build --release -p murmur-ffi

# Generate Kotlin bindings
uniffi-bindgen generate \
  --library target/aarch64-linux-android/release/libmurmur_ffi.so \
  --language kotlin \
  --out-dir platforms/android/app/src/main/java/net/murmur/generated
```

## Build — iOS (xcframework)

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Build static libraries
cargo build --release --target aarch64-apple-ios -p murmur-ffi
cargo build --release --target aarch64-apple-ios-sim -p murmur-ffi
cargo build --release --target x86_64-apple-ios -p murmur-ffi

# Combine simulator slices
lipo -create \
  target/aarch64-apple-ios-sim/release/libmurmur_ffi.a \
  target/x86_64-apple-ios/release/libmurmur_ffi.a \
  -output target/universal-ios-sim/libmurmur_ffi.a

# Create xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libmurmur_ffi.a \
  -library target/universal-ios-sim/libmurmur_ffi.a \
  -output MurmurCore.xcframework

# Generate Swift bindings
uniffi-bindgen generate \
  --library target/aarch64-apple-ios/release/libmurmur_ffi.a \
  --language swift \
  --out-dir platforms/ios/MurmurApp/Generated
```

## FFI Surface

```
namespace murmur {
    create_network(device_name, mnemonic, callbacks) → MurmurHandle
    join_network(device_name, mnemonic, callbacks) → MurmurHandle
}

interface MurmurHandle {
    load_dag_entry(entry_bytes)
    start() / stop()
    approve_device(device_id_hex, role)
    revoke_device(device_id_hex)
    list_devices() → [DeviceInfoFfi]
    pending_requests() → [DeviceInfoFfi]
    add_file(blob_hash, metadata, data)
    request_access(device_id_hex, scope)
    fetch_blob(blob_hash) → bytes?
    receive_sync_entries(entries)
    device_id_hex() → string
}

callback interface FfiPlatformCallbacks {
    on_dag_entry(entry_bytes)
    on_blob_received(blob_hash, data)
    on_blob_needed(blob_hash) → bytes?
    on_event(event)
}
```
