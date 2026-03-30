# Local Development Guide

## Prerequisites

### Rust Toolchain

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify
rustc --version
cargo --version
```

### Android Targets (for Android builds only)

```bash
# Add cross-compilation targets
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Install cargo-ndk
cargo install cargo-ndk
```

### Android SDK & NDK (for Android builds only)

Your SDK path is set in `platforms/android/local.properties`.

You need:
- **SDK Platform 35** (Android 15)
- **NDK r27+**
- **Build Tools 35.x**
- **Java 17+**

Install missing components via `sdkmanager`:

```bash
export ANDROID_HOME=$HOME/android-sdk

# Install what you need (skip what you already have)
$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager \
  "platforms;android-35" \
  "build-tools;35.0.0" \
  "ndk;27.2.12479018"
```

### uniffi-bindgen (for Android builds only)

```bash
cargo install uniffi_bindgen_cli --version 0.31
```

Verify: `uniffi-bindgen --version`

---

## Building & Running

### CLI (`murmur-cli`)

```bash
# Build
cargo build -p murmur-cli

# Or install to PATH
cargo install --path crates/murmur-cli

# Join an existing network (use mnemonic printed when murmurd first ran)
murmur-cli join "word1 word2 ... word24" --name "my-phone"

# Once murmurd is running, manage it
murmur-cli status
murmur-cli devices
murmur-cli pending
murmur-cli approve <device_id>
murmur-cli files
murmur-cli add /path/to/file

# JSON output for scripting
murmur-cli status --json
```

Data directory defaults to `~/.murmur`. Override with `--data-dir`.

### Daemon (`murmurd`)

```bash
# Build
cargo build -p murmurd

# Or install to PATH
cargo install --path crates/murmurd

# Run (reads config from ~/.murmur/config.toml, listens on ~/.murmur/murmurd.sock)
murmurd

# With options
murmurd --data-dir /tmp/murmur-test --name "test-node" --verbose
```

murmurd runs in the foreground. Use a second terminal with `murmur-cli` to manage it.

**Typical first-run workflow:**

```bash
# Terminal 1: start daemon (auto-initializes on first run, prints mnemonic)
murmurd --name "my-server" --role backup

# Terminal 2: check status
murmur-cli status
```

### Desktop App (`murmur-desktop`)

Built with [iced](https://iced.rs) v0.14. Runs on Linux, macOS, and Windows.

```bash
# Build
cargo build -p murmur-desktop

# Run
cargo run -p murmur-desktop
```

**Linux note:** iced uses wgpu by default. You may need GPU drivers or mesa installed. If you get rendering errors, try:

```bash
# Force software rendering if GPU has issues
WGPU_BACKEND=gl cargo run -p murmur-desktop
```

### Android App

#### Full build (Gradle handles everything)

```bash
cd platforms/android

# Build Rust FFI + generate Kotlin bindings + assemble debug APK
./gradlew cargoBuildRelease
./gradlew uniffiBindgen
./gradlew assembleDebug
```

#### Step-by-step (if you prefer manual control)

```bash
# 1. Build Rust FFI for Android targets
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o platforms/android/app/src/main/jniLibs \
  build --release -p murmur-ffi

# 2. Generate Kotlin bindings
uniffi-bindgen generate \
  --library target/aarch64-linux-android/release/libmurmur_ffi.so \
  --language kotlin \
  --out-dir platforms/android/app/src/main/java/net/murmur/generated

# 3. Build APK
cd platforms/android
./gradlew assembleDebug
```

The debug APK lands at `platforms/android/app/build/outputs/apk/debug/app-debug.apk`.

#### Deploy to emulator

```bash
# Start an emulator (must have x86_64 system image)
$ANDROID_HOME/emulator/emulator -avd <avd_name> &

# Or create one first
$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager create avd \
  -n murmur-test \
  -k "system-images;android-35;google_apis;x86_64" \
  -d pixel_6

# Install the system image if needed
$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager \
  "system-images;android-35;google_apis;x86_64"

# Install and run
adb install platforms/android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n net.murmur.app/.ui.MainActivity
```

#### Deploy to physical device

```bash
# Enable USB debugging on the phone (Settings → Developer options → USB debugging)
# Connect via USB, then:

adb devices          # verify device shows up
adb install platforms/android/app/build/outputs/apk/debug/app-debug.apk

# Or build + install in one step
cd platforms/android
./gradlew installDebug
```

`./gradlew installDebug` builds and pushes to the connected device/emulator automatically.

#### View logs

```bash
# Filter to Murmur logs
adb logcat -s MurmurService MurmurEngine

# Or broader filter
adb logcat | grep -i murmur
```

---

## Running Tests

```bash
cargo test                          # all tests
cargo test -p murmur-types          # single crate
cargo test -p murmur-cli            # CLI tests
cargo test -p murmurd               # daemon tests
cargo test --test two_device_sync   # integration test
cargo clippy -- -D warnings         # lint (must be clean)
cargo fmt --check                   # format check
```

---

## Data Directories

### Desktop (`~/.murmur/`)

```
~/.murmur/
├── config.toml        # device config (name, role, paths)
├── mnemonic           # BIP39 seed phrase (24 words)
├── device.key         # ed25519 signing key
├── murmurd.sock       # Unix socket (runtime only)
├── db/                # DAG storage (dag.bin) + Fjall push queue
│   ├── dag.bin        # append-only DAG entry file
│   └── fjall/         # Fjall DB (transient push queue only)
└── blobs/             # content-addressed file storage
```

### Android

```
/data/data/net.murmur.app/
├── shared_prefs/murmur.xml   # mnemonic, device name, creator flag
├── databases/murmur_db.db    # Room database (DAG entries)
└── files/blobs/              # blob storage
```

---

## Recommended Setup

### Shell aliases

Add to `~/.zshrc`:

```bash
alias mb='cargo build'
alias mt='cargo test'
alias mc='cargo clippy -- -D warnings'
alias mf='cargo fmt'
alias mcheck='cargo clippy -- -D warnings && cargo fmt --check && cargo test'

# Android shortcuts (adjust paths to your checkout)
alias mand='cd ~/dev/murmur/platforms/android && ./gradlew'
alias madb='adb install ~/dev/murmur/platforms/android/app/build/outputs/apk/debug/app-debug.apk'
```

### Environment variables

Add to `~/.zshrc`:

```bash
export ANDROID_HOME=$HOME/android-sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018  # adjust version
export PATH=$PATH:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin
```

### cargo-watch (rebuild on save) — installed

```bash
# Already installed. Usage:

# Auto-rebuild on file changes
cargo watch -x 'build -p murmurd'

# Auto-test on file changes
cargo watch -x 'test -p murmur-engine'
```

### Multiple murmurd instances (local testing)

To test sync between two devices on the same machine:

```bash
# Terminal 1: first node (auto-creates network on first run)
murmurd --data-dir /tmp/murmur-a --name "node-a" --verbose

# Terminal 2: get mnemonic, then join second node
murmur-cli --data-dir /tmp/murmur-a mnemonic
murmur-cli --data-dir /tmp/murmur-b join "the mnemonic words..." --name "node-b"
murmurd --data-dir /tmp/murmur-b --verbose

# Terminal 3: manage either node
murmur-cli --data-dir /tmp/murmur-a status
murmur-cli --data-dir /tmp/murmur-b devices
```

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `cargo ndk` fails with "NDK not found" | Set `ANDROID_NDK_HOME` or install NDK via sdkmanager |
| `uniffi-bindgen` not found | `cargo install uniffi_bindgen_cli --version 0.31` |
| Gradle fails with "SDK not found" | Check `platforms/android/local.properties` has the correct `sdk.dir` path |
| `adb: no devices` | Enable USB debugging, or start emulator first |
| Desktop app crashes on launch | Try `WGPU_BACKEND=gl` for software rendering |
| Socket already in use | Remove stale socket: `rm ~/.murmur/murmurd.sock` |
| Permission denied on socket | Check `~/.murmur/` directory permissions |
