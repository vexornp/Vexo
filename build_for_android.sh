#!/usr/bin/env bash
# Builds the `android_demo` Rust crate for `aarch64-linux-android` and
# copies `libmain.so` into the Gradle project's `jniLibs/` folder so that
# a subsequent `./gradlew assembleDebug` (or Android Studio build) packages
# it into the APK.
#
# Prerequisites (one-time):
#   1. Android NDK r25+ installed, with ANDROID_NDK_HOME / ANDROID_NDK_ROOT set.
#   2. `rustup target add aarch64-linux-android`
#   3. `cargo install cargo-ndk`
#
# See VexoDemoAndroid/README.md for full setup instructions.
set -euo pipefail

# Always operate relative to this script's location so it can be invoked
# from anywhere (Xcode-style pre-build hooks, CI, etc.).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ---- 1. Environment checks --------------------------------------------------

if [[ -z "${ANDROID_NDK_HOME:-}" && -z "${ANDROID_NDK_ROOT:-}" ]]; then
    echo "ERROR: ANDROID_NDK_HOME (or ANDROID_NDK_ROOT) is not set." >&2
    echo "       Install the NDK (r25+) and export ANDROID_NDK_HOME=/path/to/ndk." >&2
    exit 1
fi

if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "ERROR: cargo-ndk not found on PATH." >&2
    echo "       Install it with: cargo install cargo-ndk" >&2
    exit 1
fi

# `cargo ndk` requires the rustc target to be installed.
if ! rustup target list --installed | grep -q '^aarch64-linux-android$'; then
    echo "ERROR: Rust target aarch64-linux-android is not installed." >&2
    echo "       Install it with: rustup target add aarch64-linux-android" >&2
    exit 1
fi

# ---- 2. Build the Rust cdylib -----------------------------------------------

# `-P 24` sets the API level (Android 7.0), matching the Gradle `minSdk`
# and ensuring Vulkan symbols are available at link time. Note: capital `-P`
# is cargo-ndk's platform flag; lowercase `-p` is cargo's `--package`.
# Release mode keeps the .so small and matches how iOS ships.
echo "==> Building android_demo for aarch64-linux-android (release)…"
cargo ndk -t arm64-v8a -P 24 build -p android_demo --release

# ---- 3. Copy libmain.so into the Gradle project -----------------------------

SOURCE_SO="target/aarch64-linux-android/release/libmain.so"
DEST_DIR="VexoDemoAndroid/app/src/main/jniLibs/arm64-v8a"

if [[ ! -f "$SOURCE_SO" ]]; then
    echo "ERROR: build did not produce $SOURCE_SO" >&2
    exit 1
fi

mkdir -p "$DEST_DIR"
cp "$SOURCE_SO" "$DEST_DIR/libmain.so"

echo "==> Copied $SOURCE_SO → $DEST_DIR/libmain.so"
echo ""
echo "Next steps:"
echo "  cd VexoDemoAndroid && ./gradlew assembleDebug"
echo "  # or open VexoDemoAndroid/ in Android Studio and press Run."
echo ""
echo "All tasks completed successfully."
