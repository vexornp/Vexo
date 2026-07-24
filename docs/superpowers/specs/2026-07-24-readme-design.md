# GitHub README Design

**Date:** 2026-07-24
**Status:** Approved

## Context

Vexo has no top-level `README.md`. The repository is at `github.com/vexorsis/vexo`, runs on five platforms (iOS, Android, macOS, Windows, Linux), and implements Flutter's three-tree architecture in pure Rust — but a GitHub visitor lands on an empty repo home page. The user will provide GIFs of the demo app running on all five platforms.

The primary audience is **Rust developers evaluating UI options**. They care about architecture, performance, and "why Rust for UI" — and they decide quickly based on what loads above the fold and what they can run in 30 seconds.

Positioning: **early-stage but real**. Apps run on all five platforms today, but production-critical gaps remain (accessibility, IME, common widgets). The README must set correct expectations — an evaluator who clones it and expects a 1.0 framework will be disappointed; an evaluator who understands it's pre-1.0 but architecturally serious will be intrigued.

## Goal

Create a `README.md` at the repository root that:

1. Makes a strong first impression on Rust devs evaluating UI frameworks (hero + 5 platform GIFs)
2. Lets them run Vexo in one command (quickstart)
3. Shows the API shape in ~10 lines (minimal code sample)
4. Communicates the differentiator (three-tree architecture, pure Rust, wgpu, no webview)
5. Sets honest expectations about maturity (status section reflecting actual source state, not the stale ROADMAP)
6. Provides build paths for all 5 platforms
7. Adds an MIT `LICENSE` file and license section

## Non-Goals

- Updating or fixing the stale `ROADMAP.md` (out of scope — only referenced with a caveat)
- Creating a project website, docs site, or marketing landing page
- Designing a logo or visual brand identity
- Writing API documentation (lives in `CLAUDE.md` and source)
- Comparison tables vs. other Rust UI frameworks (deliberately omitted per user decision)

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Audience focus | Rust devs evaluating UI options | Architecture story is the differentiator for this group |
| Maturity framing | "Early-stage but real" | Honest; matches actual source state (not the stale ROADMAP) |
| Repo URL | `github.com/vexorsis/vexo` | Confirmed via glyphon fork org in `Cargo.toml` |
| Tagline | "A cross-platform UI framework in Rust." | Factual; lets architecture speak |
| License | MIT | Permissive, common in Rust ecosystem; add `LICENSE` file + badge |
| GIF layout | 2 rows: desktop (macOS/Windows/Linux) top, mobile (iOS/Android) bottom | Clear desktop/mobile split; equal-ish column counts |
| GIF hosting | GitHub user-images URLs (placeholders in README) | Zero repo bloat; user fills in real URLs after upload |
| Code sample | Minimal "Hello, Vexo!" (~10 lines) | Gives instant API-shape feel; verified against real API |
| Comparison section | Omitted | User decision; "Why Vexo?" states differentiators without naming competitors |

## Design

### Structure (Approach B — Progressive Disclosure)

The README follows this section order, each scaled to its complexity:

1. **Hero** — title, tagline, badges, positioning paragraph
2. **GIF Showcase** — 5 platform GIFs in 2 rows (desktop / mobile)
3. **Quickstart** — one command to run on desktop
4. **Minimal App** — ~10-line code sample showing `Application` trait
5. **Why Vexo?** — 5 differentiator bullets
6. **Architecture** — three-tree diagram + data-flow paragraph + workspace table
7. **Platform Support** — 5-row matrix with backends and status
8. **Status** — honest ✅/🚧 split reflecting actual source state
9. **Building** — desktop / iOS / Android subsections
10. **Contributing** — short, points to `CLAUDE.md` and `ROADMAP.md`
11. **License** — MIT, links to `LICENSE`

### Section 1: Hero & Badges

```
# Vexo

A cross-platform UI framework in Rust.

[MIT badge] [Rust badge] [platforms: iOS · Android · macOS · Windows · Linux] [status: early-stage]

Vexo brings Flutter's three-tree architecture to pure Rust — one codebase,
GPU-rendered, running natively on five platforms. Early-stage but real:
apps run today, gaps are tracked in the ROADMAP.
```

- One-line tagline, no marketing fluff.
- Positioning paragraph names "three-tree architecture" and "early-stage but real" immediately.
- 4 badges: MIT, Rust, platforms (all 5), status (early-stage). The status badge is the honesty signal.

### Section 2: GIF Showcase

Two tables — desktop row (3 GIFs) on top, mobile row (2 GIFs) below:

```markdown
### Desktop

| macOS | Windows | Linux |
|-------|---------|-------|
| ![macOS](MACOS_GIF_URL) | ![Windows](WINDOWS_GIF_URL) | ![Linux](LINUX_GIF_URL) |

### Mobile

| iOS | Android |
|-----|---------|
| ![iOS](IOS_GIF_URL) | ![Android](ANDROID_GIF_URL) |
```

- GitHub tables give equal column widths and clean captions.
- Placeholder URLs (`MACOS_GIF_URL`, `WINDOWS_GIF_URL`, `LINUX_GIF_URL`, `IOS_GIF_URL`, `ANDROID_GIF_URL`) — user replaces with real `user-images.githubusercontent.com/...` URLs after uploading.
- The 3+2 split reads as "desktop-class + mobile-class," matching how devs think about platform reach.

### Section 3: Quickstart

```markdown
## Quickstart

```bash
cargo run -p desktop_demo
```

That's it — builds and runs the demo app on your desktop. No platform
SDKs required for desktop. (For mobile, see [Building for iOS](#building-for-ios)
and [Building for Android](#building-for-android).)
```

- One command, lowest possible friction. Desktop needs no SDK.
- Mobile build steps deferred to their own section.

### Section 4: Minimal App

```rust
use vexo::{Application, ComponentState, MultiChild, Layout, Text, Widget};

#[derive(ComponentState, Default)]
struct Hello;

impl Application for Hello {
    type State = Self;

    fn new() -> Self { Hello }

    fn view(_state: &mut Self) -> Box<dyn Widget> {
        MultiChild::new(
            vec![ Box::new(Text::new("Hello, Vexo!")) ],
            Layout::column(),
        )
    }
}

fn main() {
    vexo::run_desktop_demo::<Hello>().unwrap();
}
```

**API verification (done during brainstorming):**
- `Application` trait: `type State: ComponentState + Default`, `fn new() -> Self::State`, `fn view(&mut Self::State) -> Box<dyn Widget>` — confirmed at `vexo/src/lib.rs:217`
- `MultiChild::new(children: Vec<Box<dyn Widget>>, layout: Layout)` — confirmed at `vexo/src/widgets/multi_child.rs:40`
- `Layout::column()` — confirmed at `vexo/src/layout/style.rs:653`
- `Text::new(content: impl Into<String>)` — confirmed at `vexo/src/widgets/text.rs:25`
- `run_desktop_demo<A: Application>() -> Result<(), Box<dyn Error>>` — confirmed at `vexo/src/lib.rs:272`
- `Layout`, `MultiChild`, `Text`, `Application`, `Widget`, `ComponentState` all re-exported from `vexo` crate root — confirmed via `pub use` in `vexo/src/lib.rs` (`ComponentState` at line 40)

**Note for implementation:** The `Hello` struct must satisfy `ComponentState + Default + 'static`. The sample above includes `#[derive(ComponentState, Default)]` to meet these bounds. The `component_state_derive` proc-macro is designed for structs with `Signal` fields, so its behavior on a fieldless unit struct needs a compile check during implementation. If the derive rejects a unit struct, fall back to a struct with a `#[signal] _phantom: ()` marker or a manual `ComponentState` impl. Verify the exact code compiles with `cargo check` before finalizing the README.

Closing paragraph after the code:

```markdown
The `Application` trait is the whole contract: `new()` gives you state,
`view()` returns a widget tree. State changes trigger targeted rebuilds via
the reactive `Signal` primitive — no diffing, no virtual DOM.
```

### Section 5: Why Vexo?

Five differentiator bullets, each one sentence naming a concrete thing:

```markdown
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
```

- Leads with "retained-mode" — the single most important positioning signal for Rust devs (distinguishes from egui immediately).
- Each bullet names a concrete technology (wgpu, Taffy, glyphon, Signal) — no vague claims.

### Section 6: Architecture

Simplified vertical diagram (not the full dense ASCII from `CLAUDE.md`) + data-flow paragraph + workspace table.

```markdown
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
```

- Simplified diagram keeps the core idea without the dense `CLAUDE.md` ASCII.
- Data-flow paragraph is plain English — no jargon a Rust dev can't follow.
- Workspace table answers "what's in this repo?" fast. Links to `CLAUDE.md` for depth.

**Workspace table verification:** The 7 crates + 2 project folders match the `members` list in root `Cargo.toml` plus the `VexoDemo/` (iOS) and `VexoDemoAndroid/` (Android Studio) directories confirmed via `ls`.

### Section 7: Platform Support

```markdown
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
```

- Backend column names the actual wgpu backend per platform (concrete for Rust devs).
- All 5 marked ✅ Running (matches reality per the GIFs the user will provide).

### Section 8: Status

**Corrected to reflect actual source state** (the `ROADMAP.md` is stale — many items listed as "missing" already exist). Based on a thorough source audit performed during brainstorming:

```markdown
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
```

**Source audit findings (for implementation reference):**

- **EXISTS (ROADMAP was wrong):** ScrollView + scroll physics + fling + spring bounce (`scroll_view.rs`, 2796-line element), Image widget + decoding (`image.rs`, `image_data.rs`, `image_atlas.rs`), text selection + clipboard (`text_edit.rs:192-249`, `platform/clipboard*.rs`), Button with 4 variants (`vexo_uikit/src/button.rs`), Theme/dark mode (`widgets/theme.rs`, `vexo_uikit/src/theme/`), Stack/Positioned (`stack.rs`, `positioned.rs`), Android support (`android_demo/`, `run_android_demo`), Navigation stack + tabs + transitions (`vexo_uikit/src/navigation.rs`, `transitions.rs`, `tab_bar.rs`), Animation controller/curve/tween/spring/momentum (`animation/`), Shadows (`style.rs`, `shadow_math.rs`)
- **MISSING (ROADMAP accurate):** Accessibility/semantics, IME composition, tab traversal, common widgets (Checkbox/Switch/Slider/Dialog/Tooltip/Menu/Snackbar), RepaintBoundary, ListView virtualization, Rich text (TextSpan), gradients, Error boundaries, Widget test framework (testWidgets/WidgetTester), Web/WASM, CustomPaint, Hero
- **PARTIAL:** Gestures (arena + tap + vertical-drag exist; no pinch/long-press/double-tap/swipe/horizontal-drag), Animation (physics + explicit transitions exist; no implicit animations), Shadows & gradients (shadows yes, gradients no)

The README's ✅/🚧 lists collapse these into audience-facing categories (not the full audit). The last line of the Status section honestly notes the ROADMAP is partly stale — important since we link to it.

### Section 9: Building

Three subsections — desktop (one line), iOS, Android:

```markdown
## Building

### Desktop

```bash
cargo run -p desktop_demo
```

### iOS

First-time only, build the UniFFI bindgen host:
```bash
cargo build -p shared_app
```
Then either run `./build_for_ios.sh` or open `VexoDemo/` in Xcode (the
scheme runs the script automatically via a Build pre-action).

### Android

Requires the Android NDK r25+, the `aarch64-linux-android` Rust target,
and `cargo-ndk`:
```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
./build_for_android.sh
```
Then open `VexoDemoAndroid/` in Android Studio and press Run. See
[`VexoDemoAndroid/README.md`](./VexoDemoAndroid/README.md) for details.
```

**Verification notes:**
- `build_for_ios.sh` exists at repo root (confirmed via `ls`).
- `build_for_android.sh` exists at repo root (confirmed via `ls`).
- The iOS Xcode pre-action claim matches `CLAUDE.md`: "the VexoDemo scheme has a Build pre-action that runs the script automatically."
- Android steps match `VexoDemoAndroid/README.md` (NDK r25+, `aarch64-linux-android` target, `cargo-ndk`, `./build_for_android.sh`, then Android Studio Run).
- During implementation, verify these commands haven't drifted from the actual scripts by reading `build_for_ios.sh` and `build_for_android.sh` before finalizing the README.

### Section 10: Contributing

```markdown
## Contributing

Vexo is early and the roadmap is public. If you'd like to contribute,
start by reading [`CLAUDE.md`](./CLAUDE.md) (architecture) and
[`ROADMAP.md`](./ROADMAP.md) (where help is needed most).
```

- Short — points to the two docs that orient a contributor.
- Doesn't over-promise process (no CONTRIBUTING.md, no issue templates) that doesn't exist yet.

### Section 11: License

```markdown
## License

MIT — see [`LICENSE`](./LICENSE).
```

- A `LICENSE` file must be created at repo root with the standard MIT text (copyright holder TBD — ask user during implementation, or use "Vexo contributors" as a placeholder).
- Single-line section links to it.

## Files to Create/Modify

| File | Action | Notes |
|------|--------|-------|
| `README.md` | **Create** | At repo root. All 11 sections above. GIF URLs as placeholders. |
| `LICENSE` | **Create** | Standard MIT text at repo root. |

No other files modified. `CLAUDE.md`, `ROADMAP.md`, `VexoDemoAndroid/README.md` are referenced but not changed.

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Minimal code sample doesn't compile (bare unit struct + derives) | Verify with `cargo check` during implementation; fall back to manual `Default` impl if `#[derive(ComponentState)]` rejects a unit struct |
| GIF URLs left as placeholders | User replaces 5 `*_GIF_URL` tokens with real `user-images.githubusercontent.com` URLs after uploading |
| Status section drifts as features land | Acceptable for a README; the ROADMAP-caveat line covers this. Update on major merges. |
| `build_for_ios.sh` / `build_for_android.sh` steps drift from scripts | Re-read scripts during implementation before finalizing build commands |
| LICENSE copyright holder unknown | Use "Vexo contributors" placeholder, or ask user during implementation |
| Repo URL `vexorsis/vexo` assumed from glyphon fork org | Confirmed reasonable; user can correct if the vexo repo itself lives elsewhere |

## Out of Scope

- Updating the stale `ROADMAP.md` (separate task)
- Logo / brand identity / visual design system
- Project website or docs site
- API reference docs
- `CONTRIBUTING.md`, issue/PR templates, CI badges
- Comparison table vs. other frameworks
