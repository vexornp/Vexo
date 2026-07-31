# Vexo

A cross-platform UI framework in Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org)
[![Platforms](https://img.shields.io/badge/platforms-iOS%20%C2%B7%20Android%20%C2%B7%20macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-blue.svg)](#platform-support)
[![Status](https://img.shields.io/badge/status-early--stage-orange.svg)](#status)

Vexo brings [Flutter's](https://github.com/flutter/flutter) three-tree architecture to pure Rust — one codebase,
GPU-rendered, running natively on five platforms. Early-stage but real:
apps run today, gaps are tracked in the ROADMAP.

### Desktop

| macOS | Windows | Linux |
|-------|---------|-------|
| ![macOS](MACOS_GIF_URL) | ![Windows](WINDOWS_GIF_URL) | ![Linux](LINUX_GIF_URL) |

### Mobile

| iOS | Android |
|-----|---------|
| ![iOS](IOS_GIF_URL) | ![Android](ANDROID_GIF_URL) |

## Quickstart

```bash
cargo run -p desktop_demo
```

That's it — builds and runs the demo app on your desktop. No platform
SDKs required for desktop. (For mobile, see [Building for iOS](#building-for-ios)
and [Building for Android](#building-for-android).)

## A minimal app

```rust
use vexo::{Application, ComponentState, Layout, MultiChild, Text, Widget};

#[derive(Default)]
struct Hello;

impl ComponentState for Hello {}

impl Application for Hello {
    type State = Self;

    fn new() -> Self {
        Hello
    }

    fn view(_state: &mut Self) -> Box<dyn Widget> {
        Box::new(MultiChild::new(
            vec![Box::new(Text::new("Hello, Vexo!"))],
            Layout::column(),
        ))
    }
}

fn main() {
    vexo::run_desktop_demo::<Hello>().unwrap();
}
```

The `Application` trait is the whole contract: `new()` gives you state,
`view()` returns a widget tree. State changes trigger targeted rebuilds via
the reactive `Signal` primitive — no diffing, no virtual DOM.

## Why Vexo?

Vexo isn't another immediate-mode library or a webview wrapper. It's a
**retained-mode** framework that brings Flutter's battle-tested rendering
architecture to Rust:

- **Three-tree architecture** — widget, element, and render-object trees
  separate *description* from *lifecycle* from *painting*. Only what changed
  gets rebuilt, reconciled, and repainted.
- **Pure Rust, GPU-rendered** — wgpu backend, no webview, no JS/HTML/CSS,
  no system widget bridge. The same Rust code renders via Metal, Vulkan,
  DX12, or OpenGL.
- **One codebase, five platforms** — iOS, Android, macOS, Windows, and
  Linux from a single `Application::view()` implementation.
- **Reactive state** — `Signal<T>` primitives drive targeted rebuilds. No
  virtual DOM, no diffing, no re-render storms.
- **Flutter-grade layout** — Taffy flexbox + grid, proper text layout via
  glyphon, dirty tracking, and incremental reconciliation.

## Architecture

Vexo uses Flutter's three-tree architecture for efficient UI updates:

```
Widget tree        →  what to show (immutable descriptions)
    │ build()
    ▼
Element tree       →  manages state & lifecycle (mutable)
    │ mount() / rebuild()
    ▼
Render object tree →  layout (Taffy) + paint (RenderCommands)
    │
    ▼
GPU (wgpu: Metal · Vulkan · DX12 · OpenGL)
```

**The data flow:** `Application::view()` produces a widget tree →
`Widget::create_element()` builds the element tree → `Element::mount()`
builds the render-object tree → `RenderObject::layout()` (Taffy) and
`RenderObject::paint()` (RenderCommands) → `WgpuBackend.render()`.

Only dirty subtrees are reconciled and repainted — a state change in one
widget doesn't rebuild the whole tree.

### Workspace layout

| Crate | Role |
|-------|------|
| `vexo/` | Core framework: widgets, elements, render objects, layout, wgpu backend |
| `vexo_uikit/` | High-level widgets (TabBarView, NavigationStackView, Platform shell) |
| `vexo_fontawesome/` | Icon widget library (FontAwesome Solid) |
| `shared_app/` | Platform-agnostic app logic (the demo IM app) |
| `desktop_demo/` | Desktop entry point (winit) |
| `android_demo/` | Android cdylib (GameActivity) |
| `VexoDemo/` | iOS Xcode project (UniFFI + Metal) |
| `VexoDemoAndroid/` | Android Studio project |

For the full module-level architecture, see [`CLAUDE.md`](./CLAUDE.md).

## Platform support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS    | wgpu (Metal)    | ✅ Running |
| Windows  | wgpu (DX12)     | ✅ Running |
| Linux    | wgpu (Vulkan)   | ✅ Running |
| iOS      | wgpu (Metal) + UniFFI    | ✅ Running |
| Android  | wgpu (Vulkan) + GameActivity | ✅ Running |

Single codebase — the same `Application::view()` drives every platform.
Desktop builds need only Rust; mobile builds require the platform SDK
(see below).

## Status

Vexo is **early-stage but real**. Apps run on all five platforms today, with
a working widget set, navigation, theming, scrolling, and animations.

- ✅ Three-tree architecture, flexbox/grid layout, text & text editing with
  selection, reactive state, focus tree, scroll physics (rubber-band, fling,
  spring), image decoding, theming & dark mode, navigation, animations,
  shadows, desktop + mobile rendering on 5 platforms
- 🚧 Accessibility, IME composition (CJK), tab traversal, common widgets
  (Checkbox/Switch/Slider/Dialog/...), RepaintBoundary, ListView
  virtualization, rich text, gradients, widget test framework

See [`ROADMAP.md`](./ROADMAP.md) for the full gap analysis (note: some
items there are already implemented — trust the source).

## Building

### Desktop

```bash
cargo run -p desktop_demo
```

### Building for iOS

First-time only, build the UniFFI bindgen host:
```bash
cargo build -p shared_app
```
Then either run `./build_for_ios.sh` or open `VexoDemo/` in Xcode (the
scheme runs the script automatically via a Build pre-action).

### Building for Android

Requires the Android NDK r25+, the `aarch64-linux-android` Rust target,
and `cargo-ndk`:
```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
./build_for_android.sh
```
Then open `VexoDemoAndroid/` in Android Studio and press Run. See
[`VexoDemoAndroid/README.md`](./VexoDemoAndroid/README.md) for details.

## Contributing

Vexo is early and the roadmap is public. If you'd like to contribute,
start by reading [`CLAUDE.md`](./CLAUDE.md) (architecture) and
[`ROADMAP.md`](./ROADMAP.md) (where help is needed most).

## Acknowledgments

### Inspiration

Vexo's architecture — the three-tree model, reactive widget composition,
element reconciliation, and rendering pipeline — is deeply inspired by
[Flutter](https://github.com/flutter/flutter). Many core concepts and
design patterns are referenced directly from the Flutter framework.
We're grateful to the Flutter team for the foundational ideas that made
Vexo possible.

### Built With

Vexo is built on these open-source projects — thanks to their authors
and maintainers.

- [wgpu](https://github.com/gfx-rs/wgpu) — GPU rendering backend (Metal/Vulkan/DX12/OpenGL)
- [winit](https://github.com/rust-windowing/winit) — cross-platform windowing & event loop
- [taffy](https://github.com/DioxusLabs/taffy) — flexbox & grid layout engine
- [glyphon](https://github.com/grovesnl/glyphon) — text layout & rendering (Vexo uses a [fork](https://github.com/vexorsis/glyphon) with per-textarea depth)
- [uniffi](https://github.com/mozilla/uniffi-rs) — Rust-to-Swift FFI for iOS

- [android-activity](https://github.com/rust-mobile/android-activity) — Android activity glue
- [objc2](https://github.com/madsmtm/objc2) — iOS UIKit/Foundation bindings
- [image](https://github.com/image-rs/image) — image decoding

## License

MIT — see [`LICENSE`](./LICENSE).
