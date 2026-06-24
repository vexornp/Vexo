# vexo_uikit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `vexo_uikit` crate with a platform-adaptive Button component built on vexo's public widget API.

**Architecture:** Separate crate (`vexo_uikit`) depending only on `vexo`. Button is a `Component` that composes base widgets (Text, DecoratedContainer via modifiers, GestureDetector via modifiers, MouseRegion via modifiers) in its `render()` method. Platform adaptation via runtime `Platform::current()` match. Theme tokens as constants.

**Tech Stack:** Rust, vexo (Component, Signal, Widget trait modifiers, Color, Layout)

## Global Constraints

- vexo_uikit depends only on vexo's public API — no `pub(crate)` types from vexo
- All components implement `Component + Clone` (blanket `Widget` impl)
- Callbacks use `Rc<RefCell<dyn FnMut()>>` matching vexo's GestureDetector pattern
- State structs use `#[derive(ComponentState)]` for auto-wiring Signal dirty callbacks
- No custom Element or RenderObject types in vexo_uikit
- Disabled state uses per-color alpha (`Color::with_alpha(0.5)`) — vexo has no subtree opacity
- Asymmetric padding via `padding_each(left, right, top, bottom)` on concrete widget types (Text)
- Widget trait `.padding()` only takes `f32` — use concrete type builders for asymmetric padding
- `theme` module is `pub` so integration tests in `tests/` can access tokens
- `ButtonState` is `pub` in the `button` module so integration tests can construct it

---

### Task 1: Scaffold vexo_uikit crate

**Files:**
- Create: `vexo_uikit/Cargo.toml`
- Create: `vexo_uikit/src/lib.rs`
- Modify: `Cargo.toml` (add `vexo_uikit` to workspace members)

**Interfaces:**
- Consumes: vexo (workspace dependency)
- Produces: Empty `vexo_uikit` crate that compiles

- [ ] **Step 1: Create Cargo.toml for vexo_uikit**

```toml
[package]
name = "vexo_uikit"
version = "0.1.0"
edition = "2021"

[dependencies]
vexo = { path = "../vexo" }
```

- [ ] **Step 2: Create src/lib.rs**

```rust
//! Rich UI component library built on vexo.
//!
//! Provides platform-adaptive widgets like Button, Toggle, TabView, etc.
//! Components compose vexo's base widgets and adapt their appearance
//! based on the current platform (Desktop vs Mobile).

// Re-exports from vexo that uikit consumers commonly need
pub use vexo::Component;
pub use vexo::ComponentState;
pub use vexo::Signal;
pub use vexo::Color;
pub use vexo::Widget;
```

- [ ] **Step 3: Add vexo_uikit to workspace members**

In root `Cargo.toml`, add `"vexo_uikit"` to the `members` array:

```toml
members = [
    "vexo",
    "vexo/component_state_derive",
    "shared_app",
    "desktop_demo",
    "vexo_uikit",
]
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/ Cargo.toml
git commit -m "feat: scaffold vexo_uikit crate"
```

---

### Task 2: Add Platform enum and detection

**Files:**
- Create: `vexo_uikit/src/platform.rs`
- Modify: `vexo_uikit/src/lib.rs` (add `pub mod platform;` and re-export)
- Create: `vexo_uikit/tests/platform_tests.rs`

**Interfaces:**
- Consumes: Nothing
- Produces: `pub enum Platform` with `Platform::current()`, re-exported as `vexo_uikit::Platform`

- [ ] **Step 1: Write the failing test**

Create `vexo_uikit/tests/platform_tests.rs`:

```rust
use vexo_uikit::Platform;

#[test]
fn platform_current_returns_a_variant() {
    let platform = Platform::current();
    match platform {
        Platform::Desktop | Platform::Mobile => {}
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test platform_tests`
Expected: FAIL — `Platform` not found

- [ ] **Step 3: Write implementation**

Create `vexo_uikit/src/platform.rs`:

```rust
/// The platform the application is running on.
///
/// Components use this to adapt their appearance and behavior.
/// Detected automatically via `Platform::current()`, or overridden
/// per-component via builder methods like `Button::platform()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Mobile,
}

impl Platform {
    /// Detect the current platform at runtime.
    pub fn current() -> Self {
        #[cfg(target_os = "ios")]
        {
            Platform::Mobile
        }
        #[cfg(not(target_os = "ios"))]
        {
            Platform::Desktop
        }
    }
}
```

Update `vexo_uikit/src/lib.rs` — add after the `pub use vexo::` block:

```rust
pub mod platform;
pub use platform::Platform;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test platform_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/platform.rs vexo_uikit/src/lib.rs vexo_uikit/tests/platform_tests.rs
git commit -m "feat: add Platform enum with runtime detection"
```

---

### Task 3: Add theme tokens

**Files:**
- Create: `vexo_uikit/src/theme/mod.rs`
- Create: `vexo_uikit/src/theme/tokens.rs`
- Modify: `vexo_uikit/src/lib.rs` (add `pub mod theme;`)
- Create: `vexo_uikit/tests/token_tests.rs`

**Interfaces:**
- Consumes: `vexo::Color`
- Produces: `theme::tokens::button` module with color and spacing constants, used internally by Button and accessible in tests

- [ ] **Step 1: Write the failing test**

Create `vexo_uikit/tests/token_tests.rs`:

```rust
use vexo_uikit::Color;

#[test]
fn button_primary_bg_is_blue() {
    let bg = vexo_uikit::theme::tokens::button::PRIMARY_BG;
    assert_eq!(bg, Color::rgb(0.0, 0.478, 1.0));
}

#[test]
fn button_disabled_alpha_is_half() {
    let alpha = vexo_uikit::theme::tokens::button::DISABLED_ALPHA;
    assert!((alpha - 0.5).abs() < 0.01);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test token_tests`
Expected: FAIL — `theme` module not found

- [ ] **Step 3: Write implementation**

Create `vexo_uikit/src/theme/mod.rs`:

```rust
pub mod tokens;
```

Create `vexo_uikit/src/theme/tokens.rs`:

```rust
use vexo::Color;

pub mod button {
    use vexo::Color;

    // Primary variant
    pub const PRIMARY_BG: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const PRIMARY_BG_HOVER: Color = Color::rgb(0.224, 0.612, 1.0);
    pub const PRIMARY_BG_PRESSED: Color = Color::rgb(0.0, 0.353, 0.85);
    pub const PRIMARY_TEXT: Color = Color::WHITE;

    // Secondary variant
    pub const SECONDARY_BG: Color = Color::TRANSPARENT;
    pub const SECONDARY_BORDER: Color = Color::rgb(0.78, 0.78, 0.8);
    pub const SECONDARY_TEXT: Color = Color::rgb(0.0, 0.478, 1.0);

    // Destructive variant
    pub const DESTRUCTIVE_BG: Color = Color::rgb(1.0, 0.231, 0.188);
    pub const DESTRUCTIVE_BG_HOVER: Color = Color::rgb(1.0, 0.388, 0.341);
    pub const DESTRUCTIVE_BG_PRESSED: Color = Color::rgb(0.88, 0.18, 0.14);
    pub const DESTRUCTIVE_TEXT: Color = Color::WHITE;

    // Ghost variant
    pub const GHOST_BG: Color = Color::TRANSPARENT;
    pub const GHOST_TEXT: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const GHOST_TEXT_HOVER: Color = Color::rgb(0.224, 0.612, 1.0);

    // Disabled
    pub const DISABLED_ALPHA: f32 = 0.5;

    // Desktop sizing
    pub const CORNER_RADIUS_DESKTOP: f32 = 6.0;
    pub const PADDING_H_DESKTOP: f32 = 16.0;
    pub const PADDING_V_DESKTOP: f32 = 8.0;

    // Mobile sizing
    pub const CORNER_RADIUS_MOBILE: f32 = 12.0;
    pub const PADDING_H_MOBILE: f32 = 20.0;
    pub const PADDING_V_MOBILE: f32 = 12.0;
}
```

Update `vexo_uikit/src/lib.rs` — add after `pub mod platform;`:

```rust
pub mod theme;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test token_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/theme/ vexo_uikit/src/lib.rs vexo_uikit/tests/token_tests.rs
git commit -m "feat: add Button theme tokens for platform-adaptive styling"
```

---

### Task 4: Implement Button component

**Files:**
- Create: `vexo_uikit/src/button.rs`
- Modify: `vexo_uikit/src/lib.rs` (add `pub mod button;` and re-exports)
- Create: `vexo_uikit/tests/button_tests.rs`

**Interfaces:**
- Consumes: `Platform`, `theme::tokens::button`, `vexo::{Component, ComponentState, Signal, Color, Widget, Text, RenderContext}`
- Produces: `pub struct Button`, `pub enum ButtonVariant`, `pub struct ButtonState` — re-exported from `vexo_uikit`

- [ ] **Step 1: Write the failing test**

Create `vexo_uikit/tests/button_tests.rs`:

```rust
use vexo_uikit::{Button, ButtonVariant, Platform, Widget, Component, Signal, ComponentState};
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

#[test]
fn button_implements_widget() {
    let button = Button::new("Click me");
    let boxed: Box<dyn Widget> = button.boxed();
    assert!(boxed.as_any().downcast_ref::<Button>().is_some());
}

#[test]
fn button_default_variant_is_primary() {
    let button = Button::new("Test");
    assert_eq!(button.variant(), &ButtonVariant::Primary);
}

#[test]
fn button_builder_methods_work() {
    let button = Button::new("Delete")
        .variant(ButtonVariant::Destructive)
        .disabled(true)
        .platform(Platform::Mobile);
    assert_eq!(button.variant(), &ButtonVariant::Destructive);
    assert!(button.disabled());
}

#[test]
fn button_on_press_callback_fires() {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let button = Button::new("Press")
        .on_press(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
    button.press();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn button_disabled_does_not_fire_callback() {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let button = Button::new("Press")
        .disabled(true)
        .on_press(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
    button.press();
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test button_tests`
Expected: FAIL — `Button` not found

- [ ] **Step 3: Write implementation**

Create `vexo_uikit/src/button.rs`:

```rust
use std::cell::RefCell;
use std::rc::Rc;

use vexo::{
    Color, Component, ComponentState, RenderContext, Signal, Text, Widget,
};

use crate::platform::Platform;
use crate::theme::tokens;

/// Visual style variant for a Button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Filled background, white text.
    Primary,
    /// Outlined border, no fill, blue text.
    Secondary,
    /// Red filled background, white text.
    Destructive,
    /// No border, no fill, blue text.
    Ghost,
}

impl Default for ButtonVariant {
    fn default() -> Self {
        ButtonVariant::Primary
    }
}

/// State for the Button component.
///
/// Tracks hover and press state via reactive Signals.
/// Auto-wired by `#[derive(ComponentState)]`.
#[derive(ComponentState, Default)]
pub struct ButtonState {
    is_pressed: Signal<bool>,
    is_hovered: Signal<bool>,
}

/// A platform-adaptive button component.
///
/// # Example
///
/// ```ignore
/// Button::new("Submit")
///     .variant(ButtonVariant::Primary)
///     .on_press(|| submit())
///     .boxed()
/// ```
#[derive(Clone)]
pub struct Button {
    label: String,
    on_press: Rc<RefCell<dyn FnMut()>>,
    variant: ButtonVariant,
    disabled: bool,
    platform: Option<Platform>,
}

impl Button {
    /// Create a new button with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_press: Rc::new(RefCell::new(|| {})),
            variant: ButtonVariant::Primary,
            disabled: false,
            platform: None,
        }
    }

    /// Set the visual variant.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the press callback.
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_press = Rc::new(RefCell::new(callback));
        self
    }

    /// Set whether the button is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Override the platform for this button.
    ///
    /// If not set, uses `Platform::current()`.
    pub fn platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Get the variant.
    pub fn variant(&self) -> &ButtonVariant {
        &self.variant
    }

    /// Get whether the button is disabled.
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Trigger the press callback programmatically. No-op if disabled.
    ///
    /// Primarily useful for testing.
    pub fn press(&self) {
        if !self.disabled {
            (self.on_press.borrow_mut())();
        }
    }

    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }

    fn resolve_bg(&self, is_pressed: bool, is_hovered: bool) -> Color {
        let alpha = if self.disabled { tokens::button::DISABLED_ALPHA } else { 1.0 };
        let base = match self.variant {
            ButtonVariant::Primary => {
                if is_pressed {
                    tokens::button::PRIMARY_BG_PRESSED
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::PRIMARY_BG_HOVER
                } else {
                    tokens::button::PRIMARY_BG
                }
            }
            ButtonVariant::Secondary => tokens::button::SECONDARY_BG,
            ButtonVariant::Destructive => {
                if is_pressed {
                    tokens::button::DESTRUCTIVE_BG_PRESSED
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::DESTRUCTIVE_BG_HOVER
                } else {
                    tokens::button::DESTRUCTIVE_BG
                }
            }
            ButtonVariant::Ghost => tokens::button::GHOST_BG,
        };
        base.with_alpha(alpha)
    }

    fn resolve_border(&self) -> (Color, f32) {
        let alpha = if self.disabled { tokens::button::DISABLED_ALPHA } else { 1.0 };
        match self.variant {
            ButtonVariant::Secondary => {
                (tokens::button::SECONDARY_BORDER.with_alpha(alpha), 1.0)
            }
            _ => (Color::TRANSPARENT, 0.0),
        }
    }

    fn resolve_text_color(&self, is_hovered: bool) -> Color {
        let alpha = if self.disabled { tokens::button::DISABLED_ALPHA } else { 1.0 };
        let base = match self.variant {
            ButtonVariant::Primary | ButtonVariant::Destructive => tokens::button::PRIMARY_TEXT,
            ButtonVariant::Secondary => tokens::button::SECONDARY_TEXT,
            ButtonVariant::Ghost => {
                if is_hovered && self.effective_platform() == Platform::Desktop {
                    tokens::button::GHOST_TEXT_HOVER
                } else {
                    tokens::button::GHOST_TEXT
                }
            }
        };
        base.with_alpha(alpha)
    }

    fn resolve_corner_radius(&self) -> f32 {
        match self.effective_platform() {
            Platform::Desktop => tokens::button::CORNER_RADIUS_DESKTOP,
            Platform::Mobile => tokens::button::CORNER_RADIUS_MOBILE,
        }
    }

    fn resolve_padding(&self) -> (f32, f32, f32, f32) {
        match self.effective_platform() {
            Platform::Desktop => (
                tokens::button::PADDING_H_DESKTOP,
                tokens::button::PADDING_H_DESKTOP,
                tokens::button::PADDING_V_DESKTOP,
                tokens::button::PADDING_V_DESKTOP,
            ),
            Platform::Mobile => (
                tokens::button::PADDING_H_MOBILE,
                tokens::button::PADDING_H_MOBILE,
                tokens::button::PADDING_V_MOBILE,
                tokens::button::PADDING_V_MOBILE,
            ),
        }
    }
}

impl Component for Button {
    type State = ButtonState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_pressed = state.is_pressed.get();
        let is_hovered = state.is_hovered.get();

        let bg = self.resolve_bg(is_pressed, is_hovered);
        let (border_color, border_width) = self.resolve_border();
        let _text_color = self.resolve_text_color(is_hovered);
        let corner_radius = self.resolve_corner_radius();
        let (pl, pr, pt, pb) = self.resolve_padding();

        let disabled = self.disabled;
        let on_press_cb = self.on_press.clone();
        let is_pressed_signal = state.is_pressed.clone();
        let is_hovered_signal = state.is_hovered.clone();

        let mut text = Text::new(&self.label)
            .background(bg)
            .corner_radius(corner_radius)
            .padding_each(pl, pr, pt, pb);

        if border_width > 0.0 {
            text = text.border(border_color, border_width);
        }

        text.boxed()
            .on_press(move || {
                if !disabled {
                    is_pressed_signal.set(true);
                    (on_press_cb.borrow_mut())();
                }
            })
            .on_release(move || {
                is_pressed_signal.set(false);
            })
            .on_enter(move || {
                if !disabled {
                    is_hovered_signal.set(true);
                }
            })
            .on_exit(move || {
                is_hovered_signal.set(false);
                is_pressed_signal.set(false);
            })
    }
}
```

Update `vexo_uikit/src/lib.rs` — add after `pub mod theme;`:

```rust
pub mod button;
pub use button::{Button, ButtonVariant, ButtonState};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo_uikit --test button_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/button.rs vexo_uikit/src/lib.rs vexo_uikit/tests/button_tests.rs
git commit -m "feat: implement Button component with platform-adaptive styling"
```

---

### Task 5: Integration test — Button renders widget tree

**Files:**
- Create: `vexo_uikit/tests/button_render_tests.rs`

**Interfaces:**
- Consumes: `Button`, `ButtonVariant`, `Platform`, `vexo_uikit::ButtonState`, `vexo::{RenderContext, BuildOwner, StateStorage, DirtyTracking, RenderObjectRegistry, ElementKey}`

- [ ] **Step 1: Write the failing test**

Create `vexo_uikit/tests/button_render_tests.rs`:

```rust
use vexo_uikit::{Button, ButtonVariant, Platform, Component, Widget, ButtonState};
use vexo::{RenderContext, BuildOwner, StateStorage, DirtyTracking, RenderObjectRegistry, ElementKey};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
    build_owner: &'a BuildOwner,
) -> RenderContext<'a> {
    RenderContext {
        element_id,
        dirty,
        render_objects,
        build_owner,
    }
}

#[test]
fn button_primary_render_does_not_panic() {
    let button = Button::new("Click").variant(ButtonVariant::Primary);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _widget = button.render(&mut state, &mut ctx);
}

#[test]
fn button_disabled_render_does_not_panic() {
    let button = Button::new("Save").disabled(true);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _widget = button.render(&mut state, &mut ctx);
}

#[test]
fn button_hover_state_produces_different_render() {
    let button = Button::new("Hover").variant(ButtonVariant::Primary).platform(Platform::Desktop);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    // Render without hover
    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _unhovered = button.render(&mut state, &mut ctx);

    // Simulate hover
    state.is_hovered.set(true);

    // Render with hover
    let mut ctx2 = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _hovered = button.render(&mut state, &mut ctx2);
}

#[test]
fn button_all_variants_render() {
    for variant in [ButtonVariant::Primary, ButtonVariant::Secondary, ButtonVariant::Destructive, ButtonVariant::Ghost] {
        let button = Button::new("Test").variant(variant);
        let mut state = ButtonState::default();
        let element_id = make_element_key();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();

        let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
        let _widget = button.render(&mut state, &mut ctx);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo_uikit --test button_render_tests`
Expected: FAIL — `ButtonState` not found or import issues

- [ ] **Step 3: Verify ButtonState is publicly accessible**

The `ButtonState` struct is `pub` in `vexo_uikit/src/button.rs` and re-exported via `pub use button::ButtonState` in `lib.rs`. The test imports it as `vexo_uikit::ButtonState`. No code change needed if Task 4 was implemented correctly.

Run: `cargo test -p vexo_uikit --test button_render_tests`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo_uikit/tests/button_render_tests.rs
git commit -m "test: add Button render integration tests"
```

---

### Task 6: Wire up desktop_demo with a Button

**Files:**
- Modify: `shared_app/Cargo.toml` (add vexo_uikit dependency)
- Modify: `shared_app/src/lib.rs` (add a Button to the demo app)

**Interfaces:**
- Consumes: `vexo_uikit::Button`, `vexo_uikit::ButtonVariant`, current shared_app structure
- Produces: Working desktop demo with a Button widget

- [ ] **Step 1: Add vexo_uikit dependency to shared_app**

In `shared_app/Cargo.toml`, add to `[dependencies]`:

```toml
vexo_uikit = { path = "../vexo_uikit" }
```

- [ ] **Step 2: Add a Button to the shared_app demo**

Read `shared_app/src/lib.rs` to understand the current demo structure. Then add:

At the top, add import:

```rust
use vexo_uikit::{Button, ButtonVariant};
```

In the demo app's state struct, add a counter field:

```rust
click_count: Signal<u32>,
```

Initialize in `Default` impl:

```rust
click_count: Signal::new(0),
```

In the `render()` method, add a Button as a child in the main Column:

```rust
Button::new(format!("Clicked {} times", state.click_count.get()))
    .on_press({
        let count = state.click_count.clone();
        move || count.set(count.get() + 1)
    })
    .boxed()
```

- [ ] **Step 3: Build the desktop demo**

Run: `cargo build -p desktop_demo`
Expected: Compiles with no errors

- [ ] **Step 4: Run the desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens with a Button visible. Clicking it increments the counter.

- [ ] **Step 5: Commit**

```bash
git add shared_app/Cargo.toml shared_app/src/lib.rs
git commit -m "feat: add Button widget to desktop demo via vexo_uikit"
```
