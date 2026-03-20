/// Build script for murmur-ffi.
///
/// UniFFI with the proc-macro approach does not require any build-time
/// scaffolding generation — the `uniffi::setup_scaffolding!()` macro handles
/// that at compile time.  This file exists so that `uniffi-bindgen` can be
/// invoked separately to emit Kotlin or Swift bindings:
///
///   uniffi-bindgen generate \
///     --library target/release/libmurmur_ffi.so \
///     --language kotlin \
///     --out-dir platforms/android/app/src/main/java/net/murmur/generated
///
///   uniffi-bindgen generate \
///     --library target/aarch64-apple-ios/release/libmurmur_ffi.a \
///     --language swift \
///     --out-dir platforms/ios/MurmurApp/Generated
fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
}
