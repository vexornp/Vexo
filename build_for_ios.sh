#!/bin/bash
set -e

# Always run from the directory containing this script so relative paths
# resolve regardless of where it was invoked from (repo root, Xcode, etc.).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Ensure cargo is on PATH. Xcode pre-actions run with a minimal environment
# that doesn't source shell profiles, so ~/.cargo/bin isn't included by default.
export PATH="$HOME/.cargo/bin:$PATH"

# Ensure the host-side bindgen binary exists before doing anything else.
# It's the [[bin]] of the shared_app crate and is produced by a host build
# (NOT by the iOS cross-build below). Build it once with: cargo build -p shared_app
UNIFFI_BINDGEN="target/debug/uniffi-bindgen-swift"
if [ ! -x "$UNIFFI_BINDGEN" ]; then
    echo "ERROR: $UNIFFI_BINDGEN not found." >&2
    echo "Build it once on the host with:" >&2
    echo "  cargo build -p shared_app" >&2
    exit 1
fi

# Paths
TARGET_DIR="target/aarch64-apple-ios/release"
XCFRAMEWORK_DIR="VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64"
HEADERS_DIR="$XCFRAMEWORK_DIR/Headers"
SWIFT_SRC_DIR="VexoDemo/SharedApp/Sources"
OUT_DIR="out"
SIMULATOR_DIR="VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator"
SIMULATOR_HEADERS_DIR="$SIMULATOR_DIR/Headers"
SIMULATOR_TARGET_DIR="target/aarch64-apple-ios-sim/release"

# Ensure required directories exist
mkdir -p "$HEADERS_DIR"
mkdir -p "$SWIFT_SRC_DIR"
mkdir -p "$OUT_DIR"
mkdir -p "$SIMULATOR_HEADERS_DIR"

# 1. Build Rust library for iOS (device + simulator in parallel)
# Pin to the rustup-managed stable toolchain, which has the iOS targets
# installed (rustup target add aarch64-apple-ios aarch64-apple-ios-sim).
# The default `stable-1.97.1` toolchain is a custom (non-rustup) install
# that rustup cannot add components to.
echo "Building Rust library for iOS device and simulator..."
cargo +stable-aarch64-apple-darwin build --target aarch64-apple-ios --target aarch64-apple-ios-sim --release

# 2. Generate Swift bindings
echo "Generating Swift bindings (shared_app.swift)..."
"$UNIFFI_BINDGEN" --swift-sources "$TARGET_DIR/libshared_app.a" "$OUT_DIR"

echo "Generating C headers (shared_appFFI.h)..."
"$UNIFFI_BINDGEN" --headers "$TARGET_DIR/libshared_app.a" "$OUT_DIR"

# 3. Copy to xcframework slices
echo "Copying device artifacts..."
cp "$TARGET_DIR/libshared_app.a" "$XCFRAMEWORK_DIR/"
cp "$OUT_DIR/shared_appFFI.h" "$HEADERS_DIR/"
cp "$OUT_DIR/shared_app.swift" "$SWIFT_SRC_DIR/"

echo "Copying simulator artifacts..."
cp "$SIMULATOR_TARGET_DIR/libshared_app.a" "$SIMULATOR_DIR/"
cp "$OUT_DIR/shared_appFFI.h" "$SIMULATOR_HEADERS_DIR/"
cp "$XCFRAMEWORK_DIR/Headers/module.modulemap" "$SIMULATOR_HEADERS_DIR/"

echo "All tasks completed successfully."
