# vexo_uikit — Rich Component Library

## Problem

Vexo provides base widgets (Text, Column, Row, DecoratedContainer, GestureDetector, Focus, ScrollView, Image) but no rich interactive components. Every app must manually compose Button, Toggle, TabView, Dialog, etc. from primitives. This is tedious, error-prone, and produces inconsistent UIs.

## Decision

Create a separate `vexo_uikit` crate that builds rich, platform-adaptive UI components on top of vexo's public widget API.

**Why a separate crate:** Rich components carry opinionated behavior (platform conventions, animation policies, accessibility contracts) that doesn't belong in a low-level rendering framework. A separate crate keeps vexo minimal and lets vexo_uikit evolve independently.

**Why not feature flags or shared_app:** Feature flags make vexo grow large. shared_app couples the component library to one application.

## Crate Structure

```
vexo_uikit/
  Cargo.toml          # depends on vexo only
  src/
    lib.rs            # Re-exports, Platform enum
    platform.rs       # Platform::current() detection
    theme/
      mod.rs          # pub mod tokens;
      tokens.rs       # Button tokens, shared spacing/radii constants
    button.rs         # Button, ButtonVariant, ButtonState
```

**Dependency:** `vexo_uikit` depends only on `vexo`. It uses the public API surface: `Component`, `Signal`, `Column`, `Row`, `Text`, `Focus`, `Widget` trait modifiers (`.on_press()`, `.background()`, etc.), `AnimationController`, `ColorTween`, `Layout`, `Style`.

**No internal types.** vexo_uikit does NOT depend on `vexo`'s `pub(crate)` types (`DecoratedContainer`, `GestureDetector`, `MouseRegion`). It accesses them exclusively through the `Widget` trait's modifier methods.

## Platform Adaptation

Runtime detection via a `Platform` enum:

```rust
pub enum Platform {
    Desktop,   // macOS, Windows, Linux
    Mobile,    // iOS, Android
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "ios")]
        { Platform::Mobile }
        #[cfg(not(target_os = "ios"))]
        { Platform::Desktop }
    }
}
```

Each component uses `match Platform::current()` inside `render()` to select visual tokens and behavioral differences. No generics, no trait objects. Override is available via builder method (e.g., `Button::new("x").platform(Platform::Mobile)`).

## Component Architecture Pattern

Every rich component follows this pattern:

1. **Component struct** — holds configuration (label, variant, callbacks). Implements `Clone` (required by blanket `Widget` impl). Callbacks use `Rc<RefCell<dyn FnMut()>>` matching GestureDetector's existing pattern.

2. **State struct** — holds reactive UI state (`Signal<bool>` for pressed/hovered, `Signal<f32>` for animation progress). `#[derive(ComponentState)]` auto-wires dirty callbacks.

3. **render()** — pure composition of base widgets. No custom Elements, no custom RenderObjects. If a component needs rendering capability that base widgets can't provide, that's a signal to add a base widget to vexo, not to bypass the layer.

4. **Platform branching** — `match Platform::current()` in `render()` selects visual tokens. Keeps platform logic local and auditable.

5. **State is private** — `ButtonState` etc. are internal. Users interact through the component struct's builder methods.

## Button Component (Initial Implementation)

### Public API

```rust
Button::new("Submit")
    .on_press(|| do_thing())
    .variant(ButtonVariant::Primary)

Button::new("Delete")
    .variant(ButtonVariant::Destructive)
    .on_press(|| delete())

Button::new("Save")
    .disabled(true)
    .on_press(|| save())
```

### Types

```rust
#[derive(Clone)]
pub struct Button {
    label: String,
    on_press: Rc<RefCell<dyn FnMut()>>,
    variant: ButtonVariant,
    disabled: bool,
}

pub enum ButtonVariant {
    Primary,       // Filled background
    Secondary,     // Outlined, no fill
    Destructive,   // Red fill
    Ghost,         // No border, no fill (text only)
}

#[derive(ComponentState)]
pub struct ButtonState {
    is_pressed: Signal<bool>,
    is_hovered: Signal<bool>,
}
```

### render() Logic

Builds via Widget trait modifier chain:

```
Text (label)
  .background(bg_color)
  .border(border_color, border_width)
  .corner_radius(radius)
  .padding(horizontal, vertical)
  .on_press(press_callback)
  .on_enter(|| state.is_hovered.set(true))
  .on_exit(|| state.is_hovered.set(false))
  .boxed()
```

### Platform Differences

| Aspect | Desktop | Mobile |
|--------|---------|--------|
| Corner radius | 6px | 12px |
| Padding | 8px V, 16px H | 12px V, 20px H |
| Primary bg | System blue | System blue (lighter) |
| Hover | Lighten background | N/A |
| Press | Darken background | Darken background |
| Disabled | Per-color alpha (0.5) | Per-color alpha (0.5) |

### Disabled Behavior

When `disabled` is true, the press callback is not invoked. `is_pressed` and `is_hovered` state changes are suppressed. Visual feedback uses per-color alpha (e.g., `Color::with_alpha(0.5)`) on background, border, and text — vexo does not support subtree opacity, so each color is individually desaturated rather than applying a compositing opacity to the whole widget.

### Theme Tokens

```rust
pub mod button {
    pub const PRIMARY_BG: Color = Color::rgb(0.0, 0.478, 1.0);
    pub const PRIMARY_BG_HOVER: Color = Color::rgb(0.224, 0.612, 1.0);
    pub const PRIMARY_BG_PRESSED: Color = Color::rgb(0.0, 0.353, 0.85);
    pub const DESTRUCTIVE_BG: Color = Color::rgb(1.0, 0.231, 0.188);
    pub const SECONDARY_BORDER: Color = Color::rgb(0.78, 0.78, 0.8);
    pub const DISABLED_ALPHA: f32 = 0.5;

    pub const CORNER_RADIUS_DESKTOP: f32 = 6.0;
    pub const CORNER_RADIUS_MOBILE: f32 = 12.0;
    pub const PADDING_H_DESKTOP: f32 = 16.0;
    pub const PADDING_V_DESKTOP: f32 = 8.0;
    pub const PADDING_H_MOBILE: f32 = 20.0;
    pub const PADDING_V_MOBILE: f32 = 12.0;
}
```

## Component Catalog & Priority

### Tier 1 — Core Interactive

| Component | New concepts introduced |
|-----------|------------------------|
| Button | Component pattern, hover/press state, platform tokens |
| Toggle | Boolean state, animated thumb position |
| Checkbox | Boolean state, custom check drawing |
| Slider | Continuous drag gesture, Float Signal |

### Tier 2 — Containers & Navigation

| Component | New concepts introduced |
|-----------|------------------------|
| TabView | Multi-child switching, bar layout |
| NavigationStack | Push/pop screen stack, transition animation |
| Dialog | Modal overlay, dismiss-on-backdrop |
| Snackbar | Timed auto-dismiss, bottom floating |

### Tier 3 — Display & Feedback

| Component | New concepts introduced |
|-----------|------------------------|
| ProgressIndicator | Indeterminate animation, determinate progress |
| Tooltip | Delayed show on hover, positioned relative to child |
| Badge | Small count/dot overlay |

Initial implementation scope: Button only. Remaining components follow the same Component pattern and will be designed individually.

## Integration

- `shared_app` adds `vexo_uikit` as dependency: `Button::new("Click").on_press(|| ...).boxed()`
- `desktop_demo` same
- No changes to `vexo` crate itself
