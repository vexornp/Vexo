# iOS Simulator Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ARM64 iOS simulator support to the VexoDemo build pipeline so the app can build and run on the iOS Simulator.

**Architecture:** Extend the existing xcframework to include a second slice (`ios-arm64-simulator`) alongside the existing device slice (`ios-arm64`). The build script compiles Rust for both targets and copies artifacts to the appropriate xcframework directories.

**Tech Stack:** Rust (cargo), UniFFI, Xcode xcframework, bash scripting

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `build_for_ios.sh` | Modify | Add simulator target build and copy steps |
| `VexoDemo/SharedApp/shared_appFFI.xcframework/Info.plist` | Modify | Add simulator slice entry to AvailableLibraries |
| `VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator/` | Create | New directory for simulator slice |

---

### Task 1: Update build_for_ios.sh for Simulator Support

**Files:**
- Modify: `build_for_ios.sh`

- [ ] **Step 1: Read current build_for_ios.sh**

Read the file to understand the current structure before modifying.

- [ ] **Step 2: Add simulator target variables**

Add these variables after the existing variable definitions (around line 9):

```bash
SIMULATOR_DIR="VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator"
SIMULATOR_HEADERS_DIR="$SIMULATOR_DIR/Headers"
SIMULATOR_TARGET_DIR="target/aarch64-apple-ios-sim/release"
```

- [ ] **Step 3: Add simulator directory creation**

After the existing `mkdir` commands (around line 14), add:

```bash
mkdir -p "$SIMULATOR_HEADERS_DIR"
```

- [ ] **Step 4: Add simulator Rust build step**

After the existing device build (around line 18), add:

```bash
# 1b. Build Rust library for iOS Simulator (ARM64)
echo "Building Rust library for iOS Simulator..."
cargo build --target aarch64-apple-ios-sim --release
```

- [ ] **Step 5: Add simulator artifact copy step**

After the existing copy commands (around line 31), add:

```bash
# 3b. Copy generated files to simulator xcframework slice
echo "Copying simulator artifacts..."
cp "$SIMULATOR_TARGET_DIR/libshared_app.a" "$SIMULATOR_DIR/"
cp "$OUT_DIR/shared_appFFI.h" "$SIMULATOR_HEADERS_DIR/"
cp "$XCFRAMEWORK_DIR/Headers/module.modulemap" "$SIMULATOR_HEADERS_DIR/"
```

- [ ] **Step 6: Verify script syntax**

Run: `bash -n build_for_ios.sh`
Expected: No output (syntax OK)

- [ ] **Step 7: Commit build script changes**

```bash
git add build_for_ios.sh
git commit -m "feat: add iOS simulator (ARM64) build support to build_for_ios.sh

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Update xcframework Info.plist for Simulator Slice

**Files:**
- Modify: `VexoDemo/SharedApp/shared_appFFI.xcframework/Info.plist`

- [ ] **Step 1: Read current Info.plist**

Read the file to see the current structure.

- [ ] **Step 2: Add simulator slice entry to AvailableLibraries array**

After the closing `</dict>` of the existing device slice entry (around line 22), add:

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

- [ ] **Step 3: Verify plist is valid XML**

Run: `plutil -lint VexoDemo/SharedApp/shared_appFFI.xcframework/Info.plist`
Expected: `VexoDemo/SharedApp/shared_appFFI.xcframework/Info.plist: OK`

- [ ] **Step 4: Commit Info.plist changes**

```bash
git add VexoDemo/SharedApp/shared_appFFI.xcframework/Info.plist
git commit -m "feat: add simulator slice to xcframework Info.plist

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Run Build Script and Verify Output

**Files:**
- None (verification only)

- [ ] **Step 1: Ensure Rust target is installed**

Run: `rustup target add aarch64-apple-ios-sim`
Expected: Either "installed" or no output (already installed)

- [ ] **Step 2: Run the build script**

Run: `./build_for_ios.sh`
Expected: Script completes with "All tasks completed successfully."

- [ ] **Step 3: Verify xcframework structure**

Run: `ls VexoDemo/SharedApp/shared_appFFI.xcframework/`
Expected output contains: `ios-arm64-simulator`

Run: `ls VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator/`
Expected output contains: `Headers` and `libshared_app.a`

- [ ] **Step 4: Verify simulator library architecture**

Run: `lipo -info VexoDemo/SharedApp/shared_appFFI.xcframework/ios-arm64-simulator/libshared_app.a`
Expected: Contains `arm64` architecture

---

### Task 4: Verify Xcode Build for Simulator

**Files:**
- None (verification only)

- [ ] **Step 1: Build for simulator using xcodebuild**

Run:
```bash
cd VexoDemo && xcodebuild -project VexoDemo.xcodeproj -scheme VexoDemo -destination 'platform=iOS Simulator,name=iPhone 15' build
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 2: Final commit if any uncommitted changes**

Run: `git status`
If clean, no action needed. If changes exist, commit them.