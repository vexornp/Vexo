#!/bin/bash
set -e

# Paths
TARGET_DIR="target/aarch64-apple-ios/release"
XCFRAMEWORK_DIR="VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64"
HEADERS_DIR="$XCFRAMEWORK_DIR/Headers"
SWIFT_SRC_DIR="VexoDemo/SharedApp/Sources"
OUT_DIR="out"

# Ensure required directories exist
mkdir -p "$HEADERS_DIR"
mkdir -p "$SWIFT_SRC_DIR"
mkdir -p "$OUT_DIR"

# 1. Build Rust library for iOS
echo "Building Rust library..."
cargo build --target aarch64-apple-ios --release

# 2. Generate Swift bindings
echo "Generating Swift bindings (shared_app.swift)..."
target/debug/uniffi-bindgen-swift --swift-sources "$TARGET_DIR/libshared_app.a" "$OUT_DIR"

echo "Generating C headers (shared_appFFI.h)..."
target/debug/uniffi-bindgen-swift --headers "$TARGET_DIR/libshared_app.a" "$OUT_DIR"

# 3. Copy generated files to xcframework and sources
echo "Copying generated files..."
cp "$TARGET_DIR/libshared_app.a" "$XCFRAMEWORK_DIR/"
cp "$OUT_DIR/shared_appFFI.h" "$HEADERS_DIR/"
cp "$OUT_DIR/shared_app.swift" "$SWIFT_SRC_DIR/"

echo "All tasks completed successfully."
