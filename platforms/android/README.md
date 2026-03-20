# Murmur Android App

Kotlin Android application wrapping `murmur-ffi` via UniFFI-generated bindings.

## Prerequisites

- Android SDK 35, NDK r27+
- Rust toolchain with `cargo-ndk`:
  ```bash
  cargo install cargo-ndk
  rustup target add aarch64-linux-android armeabi-v7a-linux-androideabi x86_64-linux-android
  ```
- Java 17+

## Building

### 1. Build the Rust FFI library

```bash
# From the repo root
cargo ndk \
  -t arm64-v8a \
  -t armeabi-v7a \
  -t x86_64 \
  -o platforms/android/app/src/main/jniLibs \
  build --release -p murmur-ffi
```

### 2. Generate Kotlin bindings

```bash
uniffi-bindgen generate \
  --library target/aarch64-linux-android/release/libmurmur_ffi.so \
  --language kotlin \
  --out-dir platforms/android/app/src/main/java/net/murmur/generated
```

### 3. Build the Android APK

```bash
cd platforms/android
./gradlew assembleDebug
```

Or via Gradle tasks:

```bash
./gradlew cargoBuildRelease    # Rust + bindings
./gradlew assembleRelease      # APK
```

## Running Tests

```bash
# Instrumented tests (requires connected device or emulator)
./gradlew connectedAndroidTest
```

## Project Structure

```
app/
  src/
    main/
      java/net/murmur/app/
        MurmurApp.kt              Application subclass
        MurmurEngine.kt           Kotlin wrapper around MurmurHandle (FFI)
        MurmurService.kt          Foreground Service — owns the engine
        BootReceiver.kt           Restart service on boot
        MurmurDocumentsProvider.kt  Expose files in Android Files app
        MediaStoreObserver.kt     Auto-upload new photos
        DeviceViewModel.kt        StateFlow ViewModel for device list
        FileViewModel.kt          StateFlow ViewModel for file list
        db/
          AppDatabase.kt          Room database
          DagEntryEntity.kt       DAG entry table
          DagEntryDao.kt          DAO
        storage/
          BlobStore.kt            Content-addressed blob storage
        ui/
          MainActivity.kt         Single-activity host
          MurmurTheme.kt          Material3 theme
          SetupScreen.kt          First-run setup
          DeviceScreen.kt         Device management
          FileScreen.kt           File browser
          StatusScreen.kt         Status / event log
      jniLibs/                    .so files from cargo-ndk
      AndroidManifest.xml
    androidTest/                  Instrumented tests
```
