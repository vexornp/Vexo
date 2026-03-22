# iOS Simulator Support for VexoDemo

**Date:** 2026-03-22
**Status:** Draft

## Overview

Add ARM64 iOS simulator support to the VexoDemo build pipeline, enabling developers to build and run the app on the iOS Simulator (ARM64 Macs only).

## Goals

- VexoDemo builds and runs successfully on iOS Simulator (ARM64)
- Single xcframework contains both device and simulator slices
- No changes required to Xcode project or Swift code

## Non-Goals

- x86_64 simulator support (Intel Macs)
- Dynamic framework approach

## Prerequisites

Ensure the Rust target is installed:

```bash
rustup target add aarch64-apple-ios-sim
```

## Current State

```
shared_appFFI.xcframework/
├── Info.plist
└── ios-arm64/
    ├── libshared_app.a
    └── Headers/
        ├── shared_appFFI.h
        └── module.modulemap
```

The current `build_for_ios.sh` only builds for `aarch64-apple-ios` (physical device).

## Target State

```
shared_appFFI.xcframework/
├── Info.plist                          # Updated with both slices
├── ios-arm64/
│   ├── libshared_app.a                 # Device (unchanged)
│   └── Headers/
│       ├── shared_appFFI.h
│       └── module.modulemap
└── ios-arm64-simulator/
    ├── libshared_app.a                 # Simulator (new)
    └── Headers/
        ├── shared_appFFI.h
        └── module.modulemap
```

## Implementation

### 1. Update build_for_ios.sh

Add simulator target build step:

```bash
# Build for device (existing)
cargo build --target aarch64-apple-ios --release

# Build for simulator (new)
cargo build --target aarch64-apple-ios-sim --release
```

### 2. Create simulator slice directory

```bash
SIMULATOR_DIR="VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator"
mkdir -p "$SIMULATOR_DIR/Headers"
```

### 3. Copy simulator artifacts

```bash
OUT_DIR="out"
SIMULATOR_TARGET_DIR="target/aarch64-apple-ios-sim/release"

cp "$SIMULATOR_TARGET_DIR/libshared_app.a" "$SIMULATOR_DIR/"
cp "$OUT_DIR/shared_appFFI.h" "$SIMULATOR_DIR/Headers/"
# Copy modulemap from device slice (same content works for simulator)
cp "$XCFRAMEWORK_DIR/Headers/module.modulemap" "$SIMULATOR_DIR/Headers/"
```

Note: The device slice's `module.modulemap` is reused for the simulator since both use the same static library structure.

### 4. Update Info.plist

Add simulator slice entry to `AvailableLibraries` array in `shared_appFFI.xcframework/Info.plist`:

```xml
<dict>
    <key>BinaryPath</key>
    <string>libshared_app.a</string>
    <key>HeadersPath</key>
    <string>Headers</string>
    <key>LibraryIdentifier</key>
    <string>ios-arm64-simulator</string>
    <key>LibraryPath</key>
    <string>libshared_app.a</string>
    <key>SupportedArchitectures</key>
    <array>
        <string>arm64</string>
    </array>
    <key>SupportedPlatform</key>
    <string>ios</string>
    <key>SupportedPlatformVariant</key>
    <string>simulator</string>
</dict>
```

Key difference from device slice: `SupportedPlatformVariant` set to `simulator`.

This is a one-time manual edit to the Info.plist file. The script does not need to modify this file dynamically.

## Files Changed

| File | Change |
|------|--------|
| `build_for_ios.sh` | Add simulator build and copy steps |
| `shared_appFFI.xcframework/Info.plist` | Add simulator slice entry (one-time manual edit) |

## Verification

1. Run `./build_for_ios.sh`
2. Verify xcframework structure:
   ```bash
   ls VexoDemo/SharedApp/shared_appFFI.xcframework/
   # Should show: Info.plist  ios-arm64  ios-arm64-simulator
   ```
3. Open `VexoDemo.xcodeproj` in Xcode
4. Select an iOS Simulator destination (e.g., iPhone 15)
5. Build and run — should launch in simulator

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Missing Rust target | Script should check for target availability and provide clear error message |
| Intel Mac users | Only ARM64 simulator supported; Intel Mac users must use real devices. Document this limitation. |
| Different compile flags | Currently no flags differ between device/simulator. If flags are added later, ensure they're applied to both targets. |