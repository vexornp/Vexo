# VexoDemoAndroid

Android demo app for the Vexo UI framework, built on the `android_demo`
Rust crate and `GameActivity`.

## Prerequisites (one-time)

1. **Android NDK r25+** installed (via Android Studio SDK Manager or
   standalone). Set both environment variables:

   ```sh
   export ANDROID_NDK_HOME="/path/to/ndk/25.x.xxxxx"
   export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
   ```

2. **Rust Android target**:

   ```sh
   rustup target add aarch64-linux-android
   ```

3. **`cargo-ndk`** (handles NDK linker selection — avoids manual
   `.cargo/config.toml` linker wiring):

   ```sh
   cargo install cargo-ndk
   ```

4. **Android Studio** (or the Android SDK command-line tools + Gradle).
   When opening this folder in Android Studio for the first time, the
   Gradle wrapper will be generated automatically.

## Build

From the repository root:

```sh
./build_for_android.sh
```

This cross-compiles `android_demo` to `aarch64-linux-android` and copies
`libmain.so` into `VexoDemoAndroid/app/src/main/jniLibs/arm64-v8a/`.

Then build the APK:

- **Android Studio**: open `VexoDemoAndroid/` and press Run, or
- **Command line**: `cd VexoDemoAndroid && ./gradlew assembleDebug`

The debug APK will be at
`VexoDemoAndroid/app/build/outputs/apk/debug/app-debug.apk`.

## Install & run

```sh
adb install -r VexoDemoAndroid/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.vexo.demo/.com.google.androidgamesdk.GameActivity
```

Watch framework logs:

```sh
adb logcat vexo:V *:S
```

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  APK                                                 │
│  ┌────────────────────────────────────────────────┐  │
│  │ GameActivity (Java, from games-activity AAR)   │  │
│  │   loads libmain.so, spawns native thread       │  │
│  └─────────────────────┬──────────────────────────┘  │
│                        ▼                              │
│  ┌────────────────────────────────────────────────┐  │
│  │ libmain.so (Rust, from android_demo crate)     │  │
│  │   #[no_mangle] fn android_main(app)            │  │
│  └─────────────────────┬──────────────────────────┘  │
│                        ▼                              │
│  ┌────────────────────────────────────────────────┐  │
│  │ vexo::run_android_demo::<State>(app)           │  │
│  │   EventLoop::builder().with_android_app(app)   │  │
│  │   → VexoApp (shared with desktop + iOS)        │  │
│  │   → ThreeTreePipeline (widget/element/RO)      │  │
│  │   → wgpu Vulkan backend (Backends::PRIMARY)    │  │
│  └────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

The Rust-side `android_main` and the winit `VexoApp` event handler are
identical to the desktop path — only `EventLoop` construction differs
(`with_android_app` instead of `EventLoop::new()`).

## Notes

- **Single ABI**: arm64-v8a only for the minimal bring-up.
- **Software keyboard**: works (GameActivity implements `BaseInputConnection`);
  focus a `TextEdit` and the IME will appear.
- **Clipboard**: stubbed (copy/paste not functional yet).
- **Multi-touch**: not supported by `InputEvent` yet (ROADMAP item).
