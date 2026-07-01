# Vexo Production Readiness Roadmap

## What's Already Strong

The foundation is solid:

- Three-tree architecture with proper reconciliation
- Flexbox + Grid layout via Taffy
- Focus tree with lifecycle
- Reactive state management
- Desktop + iOS support
- GPU rendering via wgpu
- Text editing with cursor
- Hit testing, dirty tracking, targeted rebuilds

---

## Critical Blockers (Must Have)

| Priority | Feature | Why Critical |
|----------|---------|--------------|
| 1 | ScrollView | No real app exists without scrolling. Lists, forms, settings—all require scroll. |
| 2 | Image widget + decoding | Every production app displays images. No image support = non-starter. |
| 3 | Accessibility | Required for App Store approval. iOS/Android reject apps without screen reader support. |
| 4 | Text selection + clipboard | TextEdit is cursor-only. Users expect copy/paste, select all. |
| 5 | IME support | Critical for CJK markets (Chinese, Japanese, Korean). Without it, text input is broken for billions of users. **Partial:** iOS software keyboard shows/hides and types via winit's `UIKeyInput`; no composition/marked-text yet. See [iOS Text Input Follow-ups](#ios-text-input-follow-ups). |
| 6 | Tab navigation | Focus tree exists but no tab traversal. Keyboard users can't navigate. |

---

## High Priority (Severely Limited Without)

| Feature | Gap |
|---------|-----|
| Button widget | Currently built manually via GestureDetector + DecoratedContainer + Text |
| Advanced gestures | No drag, pinch, long-press, double-tap, swipe—severely limits interactivity |
| Animation framework | No implicit/explicit animations, no transitions—UI feels static |
| Navigation/routing | No multi-screen support, no Navigator, no page transitions |
| Theme system | No dark mode, no style inheritance, no typography system |
| Common widgets | Checkbox, Switch, Slider, Dialog, Tooltip, Menu, Snackbar, etc. |
| RepaintBoundary | Performance degrades with large UIs—no repaint isolation |

---

## Medium Priority (Expected in Production Framework)

| Feature | Gap |
|---------|-----|
| Error boundaries | One widget panic crashes entire app |
| Widget test framework | No testWidgets(), WidgetTester, finders, matchers |
| Dev tools | No inspector, performance overlay, hot reload |
| Stack/Positioned | No z-ordering, overlapping layouts |
| Shadows & gradients | Limited visual richness |
| ListView virtualization | No lazy loading for large lists |
| Rich text | No TextSpan, inline styling |

---

## Lower Priority (Nice to Have)

| Feature | Gap |
|---------|-----|
| Android support | iOS + desktop only currently |
| Web/WASM support | No web target |
| i18n/l10n | No internationalization |
| Video/audio | No media support |
| Platform channels | No native API bridge |
| CustomPaint | No custom rendering |

---

## Detailed Gap Analysis

### 1. Widget Catalog

**Exists:** Text, Column, Row, DecoratedContainer, GestureDetector, MouseRegion, TextEdit, TextEditContent, Focus

**Missing:**
- Button, Checkbox, Switch, Radio, Slider, Progress indicator
- Scrollbar, ScrollView, ListView, GridView
- Dialog, Modal, BottomSheet, Tooltip
- Tab, TabBar, TabView, Menu, PopupMenu, ContextMenu
- Drawer, Snackbar, Toast, Chip, Badge
- Image, Icon, Card, Divider, Spacer
- Expanded, Flexible, SizedBox, ConstrainedBox, AspectRatio, FittedBox
- Wrap, Stack, Positioned, IndexedStack
- Opacity, Transform, ClipRect, ClipRRect, ClipPath
- Shadow, Gradient, Blur, BackdropFilter
- CustomPaint, Placeholder

### 2. Layout System

**Exists:** Flexbox + Grid via Taffy, box model, positioning, display modes, text measurement

**Missing:**
- Stack/Positioned layout (z-ordered overlapping)
- Wrap layout
- Intrinsic width/height measurement
- Baseline alignment protocol
- LayoutBuilder for custom layout logic
- Overflow handling (clip, scroll, visible)
- Sliver protocol for advanced scrolling

### 3. Input & Gestures

**Exists:** InputEvent enum, GestureDetector (press/release), MouseRegion, MouseTracker, hit testing, system cursors

**Missing:**
- Drag gesture (start/move/end)
- Pinch/Scale gesture
- Long press recognizer
- Double tap recognizer
- Swipe/Pan gesture
- Gesture arena for disambiguation
- Tap vs Drag disambiguation (timeout-based)
- Multi-pointer/touch tracking
- Velocity tracker for fling gestures

### 4. Focus System

**Exists:** FocusManager, FocusNode, FocusAttachment, Focus widget, focus request/unfocus, focus-aware styling

**Missing:**
- Tab navigation (tab_index, FocusTraversalPolicy, next/previous focus)
- Directional focus traversal (arrow keys)
- FocusScope widget
- Focus traversal group
- Focus debug overlay
- Click-to-focus for arbitrary widgets (only TextEdit implements it)

### 5. Accessibility

**Exists:** Nothing

**Missing:**
- Semantics tree
- Screen reader support
- Accessibility labels and roles
- Announce notifications
- Accessibility focus
- Platform accessibility bridge (iOS UIAccessibility, Android AccessibilityNodeInfo)
- Reduced motion / high contrast support

### 6. Theming & Styling

**Exists:** Style struct (background, border, corner_radius, padding), Color with RGBA/presets/hex, DecoratedContainer

**Missing:**
- Theme system (ThemeData, InheritedWidget-style propagation)
- Dark mode
- Style inheritance
- Typography system (font families, sizes, weights, line heights)
- Color scheme
- Elevation / Material design
- Shadow in Style
- Gradient in Style
- Per-side border color/width
- Per-corner radius control
- Shape abstraction (rounded rectangle, circle, stadium)

### 7. Animation

**Exists:** CursorBlinkState (time-based), hover state animation via rebuild

**Missing:**
- Animation framework (Animation<T>, AnimationController, Tween, Curve)
- Implicit animations (AnimatedContainer, AnimatedOpacity, AnimatedPositioned)
- Explicit animations (AnimationController with duration/curve)
- Transitions (FadeTransition, SlideTransition, ScaleTransition)
- Physics animations (spring simulation)
- Animation ticker (per-frame callback)
- Staggered animations
- Hero transitions
- AnimatedBuilder / AnimatedWidget

### 8. Text Handling

**Exists:** Text widget, TextEdit widget, TextEditingController, cursor movement, character insertion/deletion, click-to-position cursor, cursor blink, font size control, glyphon rendering, text cache, embedded font, iOS software keyboard show/hide via `Window::set_ime_allowed` (typed text and Backspace delivered through existing `KeyboardInput` path), iOS Return key → `Action::Enter`

**Missing:**
- Text selection (highlight)
- Copy/paste/cut/select all via software keyboard edit menu (hardware Cmd shortcuts already work via `IosClipboard`)
- IME composition events (preedit / marked text / candidate window)
- Rich text (TextSpan, inline styling)
- Text alignment (left/center/right/justify)
- Text overflow (ellipsis, clip, fade)
- Line limit / max lines
- Text direction (LTR/RTL)
- Font family / weight selection
- Text decoration (underline, strikethrough)
- Password/obscured text
- Text input formatters

### 8a. iOS Text Input Follow-ups

Basic iOS text input shipped (keyboard appears when a `TextEdit` gains focus, dismisses when focus leaves; typed characters, Backspace, and Return all work). The following limitations are accepted as follow-ups — all stem from winit 0.31's iOS backend implementing `UIKeyInput` (insert + delete) rather than the full `UITextInput` protocol, plus a few Vexo-side sync gaps:

| # | Limitation | Root cause | Sketch of fix |
|---|------------|------------|----------------|
| 1 | No CJK IME composition / marked text / candidate window | winit's `WinitView` conforms to `UIKeyInput` only, not `UITextInput`. Committed characters still arrive via `insertText:`, but in-place composition display is impossible. | Upstream winit change (implement `UITextInput` on `WinitView`) + a new `InputEvent::Ime { Preedit, Commit, Clear }` variant in Vexo + a composition render path in `TextEditRenderObject`. Until then, CJK users get commit-only input (characters appear after selecting a candidate, no inline preedit). |
| 2 | No copy/cut/paste from the iOS software keyboard's edit menu | The edit menu requires `UITextInput`/`UIEditMenuInteraction` + a first responder that returns selection rects. winit's `UIKeyInput` view doesn't participate. | Either (a) upstream winit `UITextInput` support, or (b) a Vexo-side native overlay: a hidden `UITextField`/`UITextView` synced with the focused `TextEdit`'s selection, becoming first responder for the edit menu. Hardware-keyboard Cmd+C/X/V already work via `IosClipboard` and the existing `TextEditingController` clipboard methods. |
| 3 | Keyboard does not auto-scroll to avoid covering the focused field | `Window::set_ime_cursor_area` is a documented no-op on iOS (`ImeRequest::Update` is ignored in `winit-uikit/src/window.rs`). | Vexo cannot fix this alone — needs winit to honor cursor area on iOS (likely as part of `UITextInput` support). Workaround: app-level scroll management on focus gain, or reserve bottom padding when a `TextEdit` is focused. |
| 4 | Dismissing the keyboard via the iPad "hide keyboard" key leaves Vexo focus on the `TextEdit` | winit's iOS backend does not emit an event when the system keyboard is dismissed externally (only `resignFirstResponder` from our own `set_ime_allowed(false)` is tracked). The `TextEdit` border stays blue until the user taps elsewhere. | Listen for `UIKeyboardWillHideNotification` (via `objc2-ui-kit` `NotificationCenter`) on the Rust side and call `pipeline.set_focus(None)` when the dismissal wasn't initiated by Vexo. Requires a small `objc2` listener registered at `WindowState::new` on iOS. |

**Verification status:** MVP built and unit-tested on desktop + iOS targets (`cargo test -p vexo` 689 passed, `cargo check --target aarch64-apple-ios` clean). On-device keyboard-appearance verification still pending — run `./build_for_ios.sh` and launch `VexoDemo` in the simulator.

### 9. Scrolling

**Exists:** InputEvent::Scroll variant only

**Missing:**
- ScrollView widget
- ScrollController
- Scroll physics (Bouncing, Clamping, AlwaysScrollable)
- Overscroll
- Scrollbar
- Scroll notification
- Lazy loading / viewport-based rendering
- Sliver protocol
- Scroll position tracking

### 10. Navigation

**Exists:** Nothing

**Missing:**
- Navigator
- Route
- Page transitions
- Deep linking
- URL routing
- NavigationBar / BottomNavigationBar
- PageView
- TabController / TabBarView
- Back button handling
- Navigation stack

### 11. Error Handling & Recovery

**Exists:** LayoutError enum, GlobalKeyError, env_logger, anyhow::Result in WgpuBackend

**Missing:**
- Widget build error recovery (ErrorBoundary)
- Render object error handling / fallback rendering
- Layout error recovery
- Debug error overlays (red error screen)
- Structured error reporting

### 12. Testing Infrastructure

**Exists:** Inline unit tests, e2e_test.rs, integration tests, MockBackend, RenderCommand equality

**Missing:**
- Widget test framework (testWidgets, WidgetTester, pumpWidget)
- Golden/image tests
- Accessibility tests
- Performance benchmarks
- Test utilities (finders, matchers)
- Async test support
- CI test configuration

### 13. Performance

**Exists:** Dirty tracking, incremental reconciliation, targeted rebuilds, UpdateResult flags, cached render commands, text cache, GPU instanced rendering, frame request flag

**Missing:**
- RepaintBoundary widget
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

**Exists:** Desktop (winit + wgpu), iOS (UniFFI + Metal), Swift bindings, scale factor / HiDPI, embedded font

**Missing:**
- Android support
- Web/WASM support
- Platform-specific integration beyond winit defaults
- Platform channels for native API calls
- Platform-aware theming (Material vs Cupertino)
- File picker, share sheet
- Camera/microphone permissions

### 15. Image/Media

**Exists:** Nothing

**Missing:**
- Image widget
- Image decoding (PNG, JPEG, WebP, SVG)
- Asset management / asset bundle
- Network image loading
- Image caching
- Icon support
- Video, audio

### 16. Internationalization

**Exists:** Nothing

**Missing:**
- i18n framework
- l10n / message formatting
- Locale resolution
- Text direction (LTR/RTL)
- Plural rules
- Date/number formatting
- Resource bundles

### 17. Logging & Debugging

**Exists:** env_logger with RUST_LOG

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
