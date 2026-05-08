# Convert Vexo Retain Mode Widgets to Callback-Based System

**Date:** 2026-05-08

## Context

Vexo currently uses an ELM-style typed message system where:
- `Widget<M>` trait has a generic message type parameter `M`
- Widgets return typed messages via `WidgetResponse<M>`
- `Application` trait has `Message` type and `update()` function
- Messages flow up to `Application::update()` for state changes

**Problems with current approach:**
1. Type parameter `M` propagates through all widgets, making the API complex
2. `MapWidget<M1, M2>` needed to compose widgets with different message types
3. Message boxing/cloning has runtime overhead
4. Immediate mode and retain mode have inconsistent message handling

**Goals:**
1. Simplify type system by removing `M` parameter from retain mode widgets
2. Enable dynamic widget composition without message mapping
3. Improve performance by eliminating message passing overhead

**Scope:** Retain mode widgets only. Immediate mode widgets remain unchanged (will be removed later).

---

## Design

### 1. Signal Library: futures-signals

Use `futures-signals` crate for reactive local state.

**API:**
```rust
use futures_signals::signal::Mutable;

let counter = Mutable::new(0);

// Read
let value = *counter.lock_ref();

// Write
counter.replace_with(|v| *v + 1);
```

**Dependency in `vexo/Cargo.toml`:**
```toml
futures-signals = "0.3"
```

### 2. Widget Trait (Simplified)

Remove `M` parameter and `clone_box()`:

```rust
// vexo/src/retain/widgets/mod.rs

pub trait Widget: Any {
    fn key(&self) -> Option<WidgetKey>;
    fn create_element(&self) -> Box<dyn Element>;
    fn create_render_object(&self) -> Box<dyn RenderObject>;
    fn can_update(&self, other: &dyn Widget) -> bool;
    fn as_any(&self) -> &dyn Any;
}
```

### 3. WidgetResponse (No Messages)

```rust
// vexo/src/retain/widgets/mod.rs

pub struct WidgetResponse {
    pub focus_request: Option<WidgetId>,
    pub handled: bool,
    pub clear_focus: bool,
    pub cursor: Option<CursorIcon>,
}

impl WidgetResponse {
    pub fn ignored() -> Self { /* ... */ }
    pub fn handled() -> Self { /* ... */ }
}
```

### 4. Button Widget with Callback

```rust
// vexo/src/retain/widgets/button.rs

pub struct Button {
    pub on_press: Option<Box<dyn FnMut()>>,
    pub child: Box<dyn Widget>,
    pub key: Option<WidgetKey>,
    pub layout: Layout,
}

impl Button {
    pub fn new(child: impl Widget + 'static) -> Self { /* ... */ }
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self { /* ... */ }
}

impl Widget for Button {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ButtonElement {
            on_press: self.on_press.take(),  // Move callback to element
            pressed: false,
        })
    }
}
```

### 5. TextField Widget with Typed Callback

```rust
// vexo/src/retain/widgets/text_edit.rs

pub struct TextEdit {
    pub on_changed: Option<Box<dyn FnMut(&str)>>,
    pub on_submit: Option<Box<dyn FnMut(&str)>>,
    pub key: Option<WidgetKey>,
    pub layout: Layout,
}

impl TextEdit {
    pub fn new() -> Self { /* ... */ }
    pub fn on_changed(mut self, callback: impl FnMut(&str) + 'static) -> Self { /* ... */ }
    pub fn on_submit(mut self, callback: impl FnMut(&str) + 'static) -> Self { /* ... */ }
}
```

### 6. Container Widgets (Column, Row)

```rust
// vexo/src/retain/widgets/container.rs

pub struct Column {
    pub children: Vec<Box<dyn Widget>>,
    pub layout: Layout,
    pub key: Option<WidgetKey>,
}

impl Column {
    pub fn new() -> Self { /* ... */ }
    pub fn push(mut self, child: impl Widget + 'static) -> Self { /* ... */ }
}
```

### 7. Modifier Widgets

```rust
// vexo/src/retain/widgets/modifiers.rs

pub struct Padding {
    pub child: Box<dyn Widget>,
    pub padding: EdgeInsets,
    pub key: Option<WidgetKey>,
}

impl Padding {
    pub fn new(padding: EdgeInsets, child: impl Widget + 'static) -> Self { /* ... */ }
}
```

### 8. Application Structure

No `Application` trait. Applications create a struct with signals:

```rust
// shared_app/src/lib.rs

use futures_signals::signal::Mutable;

struct CounterApp {
    count: Mutable<i32>,
}

impl CounterApp {
    fn new() -> Self {
        Self { count: Mutable::new(0) }
    }

    fn view(&self) -> Box<dyn Widget> {
        let count = self.count.clone();

        Column::new()
            .push(Text::new(move || format!("Count: {}", *count.lock_ref())))
            .push(
                Button::new(Text::new("Increment"))
                    .on_press(move || {
                        count.replace_with(|v| *v + 1);
                    })
            )
    }
}
```

---

## Files to Modify

| File | Change |
|------|--------|
| `vexo/Cargo.toml` | Add `futures-signals` dependency |
| `vexo/src/retain/widgets/mod.rs` | Remove `M` from `Widget` trait, remove `clone_box()` |
| `vexo/src/retain/widgets/button.rs` | Replace `message: M` with `on_press: Box<dyn FnMut()>` |
| `vexo/src/retain/widgets/text.rs` | Remove `M` parameter |
| `vexo/src/retain/widgets/container.rs` | Remove `M` from `Column`, `Row` |
| `vexo/src/retain/widgets/modifiers.rs` | Remove `PhantomData<M>` from all modifiers |
| `vexo/src/retain/element.rs` | Update `on_event` signature |
| `shared_app/src/lib.rs` | Update sample app to use callbacks |

## Files to Keep Unchanged

All files in `vexo/src/widgets/` (immediate mode) — will be removed in future PR.

## New Files to Create

| File | Purpose |
|------|---------|
| `vexo/src/reactive/mod.rs` | Re-export `futures_signals` types |

---

## Verification

### Build
```bash
cargo build -p vexo
```

### Test
```bash
cargo test -p vexo
```

### Run Desktop Demo
```bash
cargo run -p desktop_demo
```

### Manual Testing
- Button `on_press` callback triggers correctly
- TextField `on_changed` callback receives text changes
- Container widgets render children correctly
- Modifier widgets apply styles correctly
- Signal updates with `mark_dirty()` trigger UI rebuild

---

## Migration Order

1. Add `futures-signals` dependency to `vexo/Cargo.toml`
2. Update `Widget` trait in `vexo/src/retain/widgets/mod.rs`
3. Update `Button` widget
4. Update `Text` widget
5. Update container widgets (`Column`, `Row`)
6. Update modifier widgets
7. Update sample app in `shared_app/src/lib.rs`
8. Run tests and fix compilation errors
9. Manual testing
