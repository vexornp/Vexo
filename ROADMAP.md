# Vexo Production Readiness Roadmap

> _Last updated: 2026-07-24. Status reflects the actual codebase, verified against
> `vexo/`, `vexo_uikit/`, `vexo_fontawesome/`, `shared_app/`, and `desktop_demo/`.
> Test suite: **~1164 passing** (`cargo test --workspace`), 60 ignored (iOS-gated)._

## What's Already Strong

The foundation has grown well beyond the original core:

- Three-tree architecture with proper reconciliation (`update_child`, `Reconciler`)
- Flexbox + Grid layout via Taffy
- Focus tree with lifecycle (request/unfocus, click-to-focus, autofocus)
- Reactive state management (`Component` + `ComponentState` + `Signal`)
- Desktop + iOS support (UniFFI + Metal); platform-adaptive `Platform::current()`
- GPU rendering via wgpu 30.0, including a texture atlas for images
- Text editing with cursor, **selection highlight, copy/cut/paste/select_all**
- Hit testing, dirty tracking, targeted rebuilds, `UpdateResult` flags
- **ScrollView** with momentum fling, spring-back, rubber-band overscroll, wheel/keyboard
- **Image widget** with PNG/JPEG decode and GPU atlas rendering
- **Theme system** (`Theme`/`ThemeData` `InheritedWidget`, light/dark swap, token layer)
- **Navigation** — SwiftUI-style `NavigationController` + `NavigationStackView` with animated
  push/pop transitions (mobile slide / desktop fade), `IndexedStack` + `Offstage` for
  state-preserving page switching
- **TabBar** (`TabController` + `TabBarView`) for bottom-tab shells
- **Animation primitives** — `AnimationController`, `AnimationTicker`, `Tween` (Color/Float),
  `Curve` (Linear/EaseIn/EaseOut/EaseInOut/CubicBezier), `CurvedAnimation`,
  `SlideTransition`/`FadeTransition`, `SpringSimulation`, `MomentumSimulation`
- **Common layout widgets** — `Stack`, `Positioned`, `IndexedStack`, `Opacity`, `Transform`,
  `FractionalTranslation`, `ClipRRect`, `SafeArea`, `Grid`
- **Button** component (`vexo_uikit`) with Primary/Secondary/Destructive/Ghost variants
- **Icon support** via `vexo_fontawesome` (Solid style, codegen'd `Icons` enum)
- **`BoxShadow`** in `Style` (offset/blur/spread/color), rendered by `painter.rs`
- **Gesture arena** with `TapRecognizer` + `VerticalDragRecognizer` + `VelocityTracker`
- End-to-end demo: a **mocked IM app** (`shared_app`) with 3-tab shell, conversation list,
  chat screen, contacts, iOS-style profile screen with dark-mode picker — proving the full
  retain-mode pipeline on mobile (`TabBarView`) and desktop (`DesktopShell` sidebar)

---

## Critical Blockers (Still Remaining)

The original six blockers have shrunk to three. ScrollView, Image, and Text
selection/clipboard have shipped (gaps tracked in the Detailed Gap Analysis below).

| Priority | Feature | Why Critical | Current State |
|----------|---------|--------------|---------------|
| 1 | Accessibility | Required for App Store approval. iOS/Android reject apps without screen reader support. | **Nothing exists** — no semantics tree, no platform a11y bridge, no labels/roles. See §5. |
| 2 | IME support (CJK) | Critical for Chinese/Japanese/Korean markets. Without it, text input is broken for billions of users. | **Partial** — iOS software keyboard shows/hides and types via winit's `UIKeyInput`; commit-only input works. No composition/marked-text/preedit. See §8a. |
| 3 | Tab navigation | Focus tree exists but no tab traversal. Keyboard users can't navigate. | **Missing** — `skip_traversal` flag is defined but dead; no `FocusTraversalPolicy`, `FocusScope`, next/previous focus. See §4. |

### Recently shipped blockers (gaps remain — see Detailed Gap Analysis)

| Was | Now | Remaining gaps |
|-----|-----|----------------|
| ScrollView (was #1) | **Shipped** — vertical ScrollView with momentum fling, spring-back, rubber-band, wheel/keyboard, `ScrollController` (`jump_to*`) | No `ScrollPhysics` abstraction, no scrollbar, no scroll notifications, no lazy loading/virtualization, no sliver protocol, vertical-only. See §9. |
| Image widget (was #2) | **Shipped** — `Image` widget, PNG/JPEG decode, wgpu texture atlas | No SVG/WebP/GIF, no asset bundle, no network loading, no decode cache. See §15. |
| Text selection + clipboard (was #4) | **Shipped** — selection highlight, copy/cut/paste, select_all, hardware Cmd shortcuts | No drag-to-select, no double/triple-click word/line select, no iOS software-keyboard edit menu. See §8. |

---

## High Priority (Severely Limited Without)

| Feature | Gap |
|---------|-----|
| Advanced gestures | Only `TapRecognizer` + `VerticalDragRecognizer` exist. No drag (horizontal/2D pan), pinch/scale, long-press, double-tap, swipe, or multi-pointer/touch. No gesture-arena disambiguation beyond slop. See §3. |
| Animation framework | Primitives exist (controller/tween/curve/transitions/spring). Missing: implicit animations (`AnimatedContainer`/`AnimatedOpacity`/`AnimatedPositioned`), staggered animations, Hero transitions, `AnimatedBuilder`/`AnimatedWidget`, GlobalKey reparenting for state-preserving page transitions. See §7. |
| Navigation/routing | SwiftUI-style stack nav + `TabBarView` shipped. Missing: Flutter-style `Navigator`/`Route`/`PageRoute`, deep linking, URL routing, standalone `NavigationBar`/`BottomNavigationBar`, `PageView`, system back-button handling, gesture-driven swipe-back, queued multi-push transitions. See §10. |
| Theme system | `Theme`/`ThemeData`/dark mode/`InheritedWidget` propagation shipped. Missing: typography system, elevation/Material, gradient in `Style`, per-side border color/width, per-corner radius, shape abstraction. See §6. |
| Common widgets | Stack/Positioned/IndexedStack/Opacity/Transform/ClipRRect/SafeArea/Button/Icon/TabBar shipped. Still missing: Checkbox, Switch, Radio, Slider, Progress, Scrollbar, Dialog, Modal, BottomSheet, Tooltip, Menu/PopupMenu/ContextMenu, Drawer, Snackbar, Toast, Chip, Badge, Card, Divider, Spacer, Expanded/Flexible, SizedBox, ConstrainedBox, AspectRatio, FittedBox, Wrap, ClipRect, ClipPath, Gradient, Blur, BackdropFilter, CustomPaint. See §1. |
| RepaintBoundary | Performance degrades with large UIs—no repaint isolation, no retained rendering, no raster cache. See §13. |

> Note: "Button widget" and "Theme system" are no longer in this table's gap column —
> both shipped. The rows now describe the *remaining* high-priority gaps within those areas.

---

## Medium Priority (Expected in Production Framework)

| Feature | Gap |
|---------|-----|
| Error boundaries | One widget panic crashes entire app. No `ErrorBoundary`, no fallback rendering, no debug red-error screen. See §11. |
| Widget test framework | `MockBackend` + ~1164 inline/integration tests exist, but no `testWidgets()`/`WidgetTester`/finders/matchers/golden tests/a11y tests. See §12. |
| Dev tools | No inspector, performance overlay, hot reload, layout bounds overlay, rebuild tracking. See §17. |
| Shadows & gradients | **Shadows shipped** (`BoxShadow` in `Style`). Gradients still missing (no `LinearGradient`/`RadialGradient`, no gradient render command). |
| ListView virtualization | No lazy loading for large lists. `ScrollView` paints its entire child subtree every frame. See §9/§13. |
| Rich text | No `TextSpan`, no inline styling. `Text` takes a single string + single color/size/family. See §8. |

> Note: "Stack/Positioned" has shipped and is removed from this table.

---

## Lower Priority (Nice to Have)

| Feature | Gap |
|---------|-----|
| Android support | iOS + desktop only currently (UniFFI scaffolding present for Android, no backend) |
| Web/WASM support | No web target |
| i18n/l10n | No internationalization framework, no locale resolution, no plural/date/number formatting |
| Video/audio | No media support |
| Platform channels | No native API bridge |
| CustomPaint | No custom rendering widget |

---

## Detailed Gap Analysis

### 1. Widget Catalog

**Exists:** `Text`, `Column`/`Row` (`MultiChild`), `DecoratedBox`, `GestureDetector`,
`MouseRegion`, `TextEdit`, `TextEditContent`, `Focus`, `Stack`, `Positioned`, `IndexedStack`,
`Offstage`, `Opacity`, `Transform` (+ `FractionalTranslation`), `ClipRRect`, `SafeArea`,
`Grid`, `ScrollView`, `ScrollController`, `Image`, `Theme`, `WithLayout`,
`Button` (in `vexo_uikit`, variants Primary/Secondary/Destructive/Ghost),
`TabBarView`/`TabController` (in `vexo_uikit`),
`NavigationStackView`/`NavigationController` (in `vexo_uikit`),
`Icon` (in `vexo_fontawesome`, FontAwesome Solid),
`SlideTransition`/`FadeTransition` (in `vexo/widgets/transitions.rs`)

**Missing:**
- Checkbox, Switch, Radio, Slider, Progress indicator _(Checkbox exists only as an inline
  demo helper in `shared_app/src/me/profile_screen.rs`, not a reusable widget)_
- Scrollbar, ListView, GridView (as lazy widgets)
- Dialog, Modal, BottomSheet, Tooltip
- Tab, TabBar (standalone — `TabBarView` covers bottom tabs only), TabView, Menu, PopupMenu, ContextMenu
- Drawer, Snackbar, Toast, Chip, Badge
- Card, Divider, Spacer
- Expanded, Flexible, SizedBox, ConstrainedBox, AspectRatio, FittedBox
- Wrap, ClipRect, ClipPath
- Gradient, Blur, BackdropFilter
- CustomPaint, Placeholder

### 2. Layout System

**Exists:** Flexbox + Grid via Taffy, box model, positioning, display modes, text measurement,
`Stack`/`Positioned` z-ordered overlapping, `Wrap` as a flex enum variant (`FlexWrap::Wrap`
in `layout/style.rs` — not a dedicated widget), `SafeArea` insets, percent sizing

**Missing:**
- Intrinsic width/height measurement
- Baseline alignment protocol
- LayoutBuilder for custom layout logic
- Overflow handling as a first-class widget (clip/scroll/visible exist as `Overflow` enum
  on flex children, but no dedicated `OverflowBox`/`ClipRect` widget)
- Sliver protocol for advanced scrolling

### 3. Input & Gestures

**Exists:** `InputEvent` enum (pointer button/moved, keyboard, scroll), `GestureDetector`
(press/release/tap), `MouseRegion` + `MouseTracker`, hit testing, system cursors,
**`GestureArena`** (per-pointer, slop-based disambiguation, single-winner invariant),
`GestureRecognizer` trait, `TapRecognizer`, `VerticalDragRecognizer`, `VelocityTracker`
(windowed least-squares, 100ms window — used by ScrollView for fling, not exposed to users)

**Missing:**
- Horizontal drag recognizer
- Pan (2D / omnidirectional drag) recognizer
- Pinch/Scale gesture (requires multi-pointer support — arena is single-pointer today)
- Long press recognizer
- Double tap recognizer
- Swipe as a standalone discrete gesture (momentum exists inside `ScrollViewElement` only)
- Tap-vs-drag disambiguation beyond slop (no timeout-based policy)
- Multi-pointer / multi-touch tracking (`InputEvent::PointerButton` has no pointer ID)
- Velocity tracker exposed as a public recognizer for fling gestures outside ScrollView

### 4. Focus System

**Exists:** `FocusManager` (SlotMap-backed tree, root node, deferred `request_focus`,
`unfocus`, `apply_focus_changes`, `reparent`, `remove_node`, `primary_focus` +
`previous_primary_focus`, ancestor `on_focus_change` notification), `FocusNodeData`
(`element_key`, `can_request_focus`, `skip_traversal`, `is_text_input`, `on_focus_change`),
`FocusAttachment`, `Focus` widget + `FocusElement` (autofocus, on_focus_change callback),
click-to-focus for arbitrary widgets (`event_handler.rs:394-401`), keyboard dispatch to
`primary_focus_element()` only

**Missing:**
- Tab navigation (Tab/Shift+Tab traversal — `NamedKey::Tab` is mapped but never handled to
  move focus; `FocusManager` has no `next_focus`/`previous_focus`/`traverse`)
- `tab_index` concept
- `FocusTraversalPolicy`
- Directional focus traversal (arrow keys)
- `FocusScope` widget
- `FocusTraversalGroup`
- Focus debug overlay
- **Note:** the `skip_traversal` flag exists on `FocusNodeData` (`node.rs:30`) but is
  **dead code** — no codepath reads it. It is aspirational only.

### 5. Accessibility

**Exists:** Nothing. Exhaustive source search for `semantics`, `accessibility`, `UIAccessibility`,
`AccessibilityNodeInfo`, `a11y`, `announce`, `reduced motion`, `high contrast` returns zero
implementation hits (only roadmap/README planning text and incidental English uses of
"semantics" in code comments).

**Missing:**
- Semantics tree
- Screen reader support
- Accessibility labels and roles
- Announce notifications
- Accessibility focus
- Platform accessibility bridge (iOS UIAccessibility, Android AccessibilityNodeInfo)
- Reduced motion / high contrast support

### 6. Theming & Styling

**Exists:** `Style` struct (background, border, corner_radius, padding, **`BoxShadow`**),
`Color` with RGBA/presets/hex, `DecoratedBox`, **`Theme` `InheritedWidget` + `ThemeData`**
(`theme.rs:30-160`), **`Brightness` enum + `ThemeData::light()`/`dark()`/`is_dark()`**,
Material-ish color roles (`primary`, `on_primary`, `background`, `surface`,
`surface_variant`, `outline`, `on_surface_variant`, `error`, `grouped_background`),
`InheritedWidget`-based style inheritance with dependent-rebuild propagation,
UIKit theme-token layer (`vexo_uikit/src/theme/tokens.rs` — `ButtonColors`, `NavColors`,
light/dark variants with `Color::lerp`-derived hover/pressed shades). Demo proves live
dark/light theme swap via `Theme::new(theme, inner)`.

**Missing:**
- Typography system (no `Typography`/`TextTheme`/`TextStyle`; only ad-hoc per-widget
  `Text::with_font_size(...)`)
- Elevation / Material design (no `Material` widget, no elevation levels)
- Gradient in `Style` (no `LinearGradient`/`RadialGradient`; `Style` has only flat
  `background: Option<Color>`)
- Per-side border color/width (`Border { color, width }` is uniform on all four sides)
- Per-corner radius control (`CornerRadius { radius: f32 }` is a single uniform value;
  `ClipRRect` likewise takes one radius)
- Shape abstraction (no `Shape`/`OutlinedBorder`/`RoundedRectangleBorder`/`CircleBorder`/`StadiumBorder`)

### 7. Animation

**Exists:** `CursorBlinkState` (time-based), hover state animation via rebuild,
**`AnimationController`** (`animation/controller.rs` — forward/reverse/stop/advance,
duration-based linear progress, ticker registration),
**`AnimationTicker`** (`animation/ticker.rs` — per-frame callback registry),
**`Tween`** (`animation/tween.rs` — `ColorTween` + `FloatTween`),
**`Curve` trait + `LinearCurve`/`EaseInCurve`/`EaseOutCurve`/`EaseInOutCurve`/`CubicBezierCurve`**
(Newton-Raphson + bisection solver, `animation/curve.rs`),
**`CurvedAnimation`** (wraps controller + curve, exposes eased `value()`),
**`SpringSimulation`** (`animation/spring.rs` — critically-damped harmonic oscillator,
stiffness=340, damping-ratio=1.0, semi-implicit Euler substepping, settle detection),
**`MomentumSimulation`** (`animation/momentm.rs` — exponential-decay fling, used by ScrollView),
**`SlideTransition` / `FadeTransition`** `Component`s (`widgets/transitions.rs`),
navigation page transitions (mobile slide + desktop fade) via `NavigationStackView` +
`TransitionCtx` builder, two-phase push/pop with deferred path mutation

**Missing:**
- Implicit animations (`AnimatedContainer`, `AnimatedOpacity`, `AnimatedPositioned`)
- Staggered animations
- Hero transitions (explicitly out-of-scope per design docs)
- `AnimatedBuilder` / `AnimatedWidget` (currently transitions are plain `Component`s
  reading `controller.value()` in `render()` — see
  `docs/superpowers/specs/2026-07-07-navigation-animation-design.md` §3.2)
- **TODO (Path A): GlobalKey reparenting in the reconciler.** Required for
  state-preserving page transitions. Currently `NavigationStackView` works around the
  remount problem with type-stable `Opacity(FractionalTranslation(...))` wrapper trees so
  the reconciler updates in place rather than remounting — no actual GlobalKey-based
  reparenting exists. See `docs/superpowers/specs/2026-07-07-navigation-animation-design.md` §5.

### 8. Text Handling

**Exists:** `Text` widget (`color`, `font_size`, `font_family` — icon-font path),
`TextEdit` widget, `TextEditingController`, cursor movement (arrows/Home/End/Up/Down),
character insertion/deletion, click-to-position cursor, cursor blink, **selection
(shift+arrow, anchor+cursor)**, **selection highlight painting** (per-line `RenderCommand::Rect`,
multi-line aware, `SELECTION_COLOR`), **select_all / copy / cut / paste**, **clipboard
integration** (Ctrl/Cmd+A/C/X/V via `ctx.clipboard()`), multi-line input (Enter / `\n`),
glyphon rendering, text cache, embedded font, wrapping via `max_width`, vertical centering,
`line_height` multiplier on `TextRenderObject` (not exposed on `Text` widget API),
iOS software keyboard show/hide via `Window::set_ime_allowed`, iOS Return key → `Action::Enter`

**Missing:**
- Mouse drag-to-select, double/triple-click word/line select (only single-click cursor positioning)
- Copy/paste/cut/select all via iOS software keyboard edit menu (hardware Cmd shortcuts work;
  edit menu requires `UITextInput`/`UIEditMenuInteraction` — see §8a)
- IME composition events (preedit / marked text / candidate window)
- Rich text (`TextSpan`, inline styling)
- Text alignment (left/center/right/justify) — no field on `Text` widget
- Text overflow (ellipsis, clip, fade) — `RenderCommand::Text` has `max_width` for wrapping
  but no overflow mode
- Line limit / max lines
- Text direction (LTR/RTL) — explicit comment at `render_objects/text_edit.rs:295`:
  "RTL special-casing is skipped for v1"
- Font weight selection (`vexo_fontawesome/README.md:98` confirms `font_weight` not exposed)
- Text decoration (underline, strikethrough)
- Password/obscured text
- Text input formatters
- `line_height` exposed on the `Text` widget API (currently render-object only)

### 8a. iOS Text Input Follow-ups

Basic iOS text input shipped (keyboard appears when a `TextEdit` gains focus, dismisses when
focus leaves; typed characters, Backspace, and Return all work). The following limitations are
accepted as follow-ups — all stem from winit 0.31's iOS backend implementing `UIKeyInput`
(insert + delete) rather than the full `UITextInput` protocol, plus a few Vexo-side sync gaps:

| # | Limitation | Root cause | Sketch of fix |
|---|------------|------------|----------------|
| 1 | No CJK IME composition / marked text / candidate window | winit's `WinitView` conforms to `UIKeyInput` only, not `UITextInput`. Committed characters still arrive via `insertText:`, but in-place composition display is impossible. | Upstream winit change (implement `UITextInput` on `WinitView`) + a new `InputEvent::Ime { Preedit, Commit, Clear }` variant in Vexo + a composition render path in `TextEditRenderObject`. Until then, CJK users get commit-only input (characters appear after selecting a candidate, no inline preedit). |
| 2 | No copy/cut/paste from the iOS software keyboard's edit menu | The edit menu requires `UITextInput`/`UIEditMenuInteraction` + a first responder that returns selection rects. winit's `UIKeyInput` view doesn't participate. | Either (a) upstream winit `UITextInput` support, or (b) a Vexo-side native overlay: a hidden `UITextField`/`UITextView` synced with the focused `TextEdit`'s selection, becoming first responder for the edit menu. Hardware-keyboard Cmd+C/X/V already work via `IosClipboard` and the existing `TextEditingController` clipboard methods. |
| 3 | Keyboard does not auto-scroll to avoid covering the focused field | `Window::set_ime_cursor_area` is a documented no-op on iOS (`ImeRequest::Update` is ignored in `winit-uikit/src/window.rs`). | Vexo cannot fix this alone — needs winit to honor cursor area on iOS (likely as part of `UITextInput` support). Workaround: app-level scroll management on focus gain, or reserve bottom padding when a `TextEdit` is focused. |
| 4 | Dismissing the keyboard via the iPad "hide keyboard" key leaves Vexo focus on the `TextEdit` | winit's iOS backend does not emit an event when the system keyboard is dismissed externally (only `resignFirstResponder` from our own `set_ime_allowed(false)` is tracked). The `TextEdit` border stays blue until the user taps elsewhere. | Listen for `UIKeyboardWillHideNotification` (via `objc2-ui-kit` `NotificationCenter`) on the Rust side and call `pipeline.set_focus(None)` when the dismissal wasn't initiated by Vexo. Requires a small `objc2` listener registered at `WindowState::new` on iOS. |

**Verification status:** MVP built and unit-tested on desktop + iOS targets (`cargo test -p vexo`
now ~1011 passed in the core crate, ~1164 across the workspace; `cargo check --target
aarch64-apple-ios` clean). On-device keyboard-appearance verification still pending — run
`./build_for_ios.sh` and launch `VexoDemo` in the simulator.

### 9. Scrolling

**Exists:** `ScrollView` widget (vertical-only — `FlexDirection::Column` +
`overflow_y(Scroll)` hardcoded), `ScrollController` (`current_offset()`, `jump_to(offset)`,
`jump_to_bottom()`, deferred-apply pattern, `Rc`-shared, `Clone`), `ScrollViewElement` with
**rubber-band overscroll** (`apply_rubber_band` — iOS-style asymptotic resistance, touch-only),
**momentum fling** (`MomentumSimulation`, exponential decay, iOS `TAU=0.325`, staleness guard,
min-velocity gate), **spring-back** (`SpringSimulation`, critically-damped, stiffness=340),
fling-to-edge handoff to spring, wheel/keyboard (Arrow/Page/Home/End, hard-clamped),
`InputEvent::Scroll` (wheel), scroll-offset dispatch in `event_handler.rs`, comprehensive
test suite (37 element tests + 7 controller + 12 momentum + 11 spring)

**Missing:**
- `ScrollPhysics` abstraction (`Bouncing`/`Clamping`/`AlwaysScrollable`/`NeverScrollable`)
  — physics is hardcoded inline in `ScrollViewElement` (touch → rubber-band, wheel/kbd → clamp)
  with no caller-selectable policy
- Horizontal scroll view (vertical-only)
- Scrollbar widget
- Scroll notification / `ScrollMetrics` (descendants can only poll `current_offset()`,
  no listener/observer API, no `min_scroll_extent`/`max_scroll_extent`/`viewport_dimension`)
- Lazy loading / viewport-based rendering (`ScrollViewRenderObject::paint` returns `vec![]`;
  entire child subtree is laid out + painted every frame)
- Sliver protocol (no `Sliver` trait, no `RenderSliver`, no `CustomScrollView`/`SliverList`)
- Animated programmatic scroll (`animate_to`) — only instant `jump_to`

### 10. Navigation

**Exists:** `NavigationController<Dest>` + `NavigationStackView<Dest>` (SwiftUI-style stack
navigator in `vexo_uikit/src/navigation.rs`, LIFO stack with `push`/`pop`/`pop_to_root`/
`replace`, `Rc<RefCell>`-shared path + dirty callback, two-phase pending-op capture),
`IndexedStack` + `Offstage` for state-preserving page switching, animated page transitions
(mobile horizontal slide with dual-view dim/offset + drop shadow / desktop fade) with
`TransitionCtx` builder, type-stable wrapper tree to preserve state across transition swaps,
`TabController` + `TabBarView` (bottom-tab shell using `IndexedStack`), NavBar built into
`NavigationStackView` (title + back button), Platform-adaptive shell (`TabBarView` mobile /
`DesktopShell` sidebar desktop)

**Missing:**
- Navigator (Flutter-style)
- Route / PageRoute (destinations are caller-supplied `Hash+Eq+Clone` enum values, not
  `Route` objects with transition builders)
- Deep linking (explicitly out-of-scope per design docs)
- URL routing
- Standalone `NavigationBar` / `BottomNavigationBar` (chrome is private to
  `NavigationStackView`; bottom-bar case covered by `TabBarView`)
- `PageView` (swipeable horizontal pager)
- Back button handling (system back gesture — only UI back button exists; no
  Android hardware-back, no system BackRequested, no Escape→pop)
- Gesture-driven swipe-back (needs edge-pan gesture tied to transition progress)
- Queued multi-push transitions (rapid pushes coalesce to the latest — each `push`/`pop`
  overwrites `pending` with no queue)

### 11. Error Handling & Recovery

**Exists:** `LayoutError` enum, `GlobalKeyError`, env_logger, `anyhow::Result` in `WgpuBackend`

**Missing:**
- Widget build error recovery (`ErrorBoundary`)
- Render object error handling / fallback rendering
- Layout error recovery
- Debug error overlays (red error screen)
- Structured error reporting

### 12. Testing Infrastructure

**Exists:** Inline unit tests (~1011 in `vexo` core crate, ~1164 across workspace), `e2e_test.rs`,
integration tests (61 in `shared_app`, 8 pass / 53 ignored — iOS-gated), `MockBackend`,
`RenderCommand` equality, dedicated suites for scroll physics (momentum/spring/rubber-band),
navigation, button, tokens, platform

**Missing:**
- Widget test framework (`testWidgets`, `WidgetTester`, `pumpWidget`)
- Golden/image tests
- Accessibility tests
- Performance benchmarks
- Test utilities (finders, matchers)
- Async test support
- CI test configuration

### 13. Performance

**Exists:** Dirty tracking, incremental reconciliation, targeted rebuilds, `UpdateResult` flags,
cached render commands, text cache, GPU instanced rendering, frame request flag, image atlas
with slot reuse (free-list, fixes iOS push/pop atlas leak), per-widget image change detection

**Missing:**
- `RepaintBoundary` widget
- Retained rendering between frames
- Lazy loading / virtualization
- Offscreen rendering / compositing layers
- Raster cache
- Picture recording / replay
- Layout cache across frames
- GPU resource lifecycle management
- Frame timing / jank detection
- Occlusion culling

### 14. Platform Integration

**Exists:** Desktop (winit + wgpu), iOS (UniFFI + Metal), Swift bindings, scale factor / HiDPI,
embedded font, `Platform::current()` runtime detection, `SafeArea` for notches/home indicator,
platform-adaptive layouts (mobile `TabBarView` / desktop sidebar)

**Missing:**
- Android support (UniFFI scaffolding present, no backend)
- Web/WASM support
- Platform-specific integration beyond winit defaults
- Platform channels for native API calls
- Platform-aware theming (Material vs Cupertino)
- File picker, share sheet
- Camera/microphone permissions

### 15. Image/Media

**Exists:** `Image` widget (`Image::new(image_data)` / `Image::from_bytes(bytes)`),
`ImageRenderObject` (layout fills parent via `flex_grow(1.0)`, hit testing, emits
`RenderCommand::Image`), wgpu texture atlas (`image_atlas.rs` — `ShelfAllocator` with
free-list slot reuse, `ImageKey = u64`), `WgpuBackend::register_image`/`unregister_image`
(RGBA upload via `queue.write_texture`), per-frame image register/unregister with orphaned-key
draining (pop nav reclaims atlas slots), `image` crate decode for **PNG + JPEG** (workspace
`default-features = false` + `["jpeg"]`; `["png"]` in shared_app), `Icon` support via
`vexo_fontawesome` (FontAwesome Solid, font-glyph based, codegen'd `Icons` enum), demo usage
(runtime-generated PNG avatars via `Image::from_bytes` + `ClipRRect` for circular crop)

**Missing:**
- WebP decoding
- GIF decoding
- SVG decoding/rendering (no `usvg`/`resvg`/`tiny-skia`)
- BMP / TIFF / ICO
- Asset management / asset bundle (no `AssetBundle`/`AssetManager`; only `include_bytes!`)
- Network image loading (no HTTP client dependency anywhere)
- File-system image loading (no `std::fs` reads in `vexo/src` or `shared_app/src`)
- Image decode cache (`ImageData::from_bytes` re-decodes every call; only GPU atlas slot
  reuse + per-widget change detection exist)
- Regular/Brands icon styles (FontAwesome Solid only)
- Video, audio

### 16. Internationalization

**Exists:** Nothing. No `i18n`/`l10n`/`intl`/`fluent`/`gettext` dependencies anywhere in the
workspace.

**Missing:**
- i18n framework
- l10n / message formatting
- Locale resolution
- Text direction (LTR/RTL) — also tracked in §8
- Plural rules
- Date/number formatting
- Resource bundles

### 17. Logging & Debugging

**Exists:** env_logger with `RUST_LOG`, `log::debug!` instrumentation (recently added for
scroll bounce tuning)

**Missing:**
- Widget inspector
- Layout inspector (visual bounds overlay)
- Performance overlay (FPS counter, frame timing)
- Debug paint mode (baselines, hit test regions)
- Rebuild tracking
- Repaint region highlighting
- Timeline/trace
- Remote debugging / DevTools protocol
- Hot reload / hot restart
- Debug banner
