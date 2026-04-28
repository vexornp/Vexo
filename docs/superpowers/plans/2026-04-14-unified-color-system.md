# Unified Color System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a unified `Color` type to replace the multiple color representations used across the Vexo framework.

**Architecture:** New `Color` struct in `vexo/src/color.rs` with `From` trait implementations for seamless conversion to/from wgpu::Color, [f32; 3], [f32; 4], and cosmic_text::Color. Widget APIs updated to accept Color, renderer converts internally.

**Tech Stack:** Rust, wgpu, cosmic-text

---

## File Structure

| File | Purpose |
|------|---------|
| `vexo/src/color.rs` | **New** - Color struct, constructors, presets, conversion traits |
| `vexo/src/lib.rs` | Export Color, update CLEAR_COLOR |
| `vexo/src/widgets/rectangle.rs` | Use Color in Rectangle struct |
| `vexo/src/widgets/button.rs` | Use Color in Button struct |
| `vexo/src/widgets/text.rs` | Use Color in Text and TextEdit structs |
| `vexo/src/renderer.rs` | Accept Color, convert to internal formats |
| `vexo/src/macros.rs` | Update macro docs (no code changes needed) |
| `shared_app/src/lib.rs` | Update demo to use Color |

---

### Task 1: Create Color Struct

**Files:**
- Create: `vexo/src/color.rs`

- [ ] **Step 1: Create color.rs with Color struct and core implementations**

```rust
// vexo/src/color.rs

/// A unified color representation using RGBA f32 values (0.0-1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a new color with RGBA components (0.0-1.0).
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create an opaque color with RGB components (alpha = 1.0).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a color from a hex value.
    /// - 0xRRGGBB creates an opaque color
    /// - 0xRRGGBBAA creates a color with alpha
    pub fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 24) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let b = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let a = (hex & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }

    /// Convert to [f32; 4] array.
    pub const fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Create a new color with a different alpha value.
    pub const fn with_alpha(&self, a: f32) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }
}

// Preset colors
impl Color {
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Color = Color::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Color = Color::rgb(1.0, 0.0, 1.0);
    pub const GRAY: Color = Color::rgb(0.5, 0.5, 0.5);
}

// Conversion from [f32; 3] (RGB, alpha defaults to 1.0)
impl From<[f32; 3]> for Color {
    fn from(rgb: [f32; 3]) -> Self {
        Self::rgb(rgb[0], rgb[1], rgb[2])
    }
}

// Conversion from [f32; 4] (RGBA)
impl From<[f32; 4]> for Color {
    fn from(rgba: [f32; 4]) -> Self {
        Self::new(rgba[0], rgba[1], rgba[2], rgba[3])
    }
}

// Conversion to [f32; 4]
impl From<Color> for [f32; 4] {
    fn from(color: Color) -> Self {
        color.to_array()
    }
}

// Conversion from wgpu::Color (f64)
impl From<wgpu::Color> for Color {
    fn from(color: wgpu::Color) -> Self {
        Self::new(color.r as f32, color.g as f32, color.b as f32, color.a as f32)
    }
}

// Conversion to wgpu::Color
impl From<Color> for wgpu::Color {
    fn from(color: Color) -> Self {
        Self {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        }
    }
}

// Conversion from cosmic_text::Color (u8 RGBA)
impl From<cosmic_text::Color> for Color {
    fn from(color: cosmic_text::Color) -> Self {
        let (r, g, b, a) = color.as_rgba();
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }
}

// Conversion to cosmic_text::Color
impl From<Color> for cosmic_text::Color {
    fn from(color: Color) -> Self {
        cosmic_text::Color::rgba(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        )
    }
}
```

- [ ] **Step 2: Verify color.rs compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors about missing module (expected - not yet added to lib.rs)

---

### Task 2: Export Color from lib.rs

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Add color module and export Color**

Find the module declarations section in `vexo/src/lib.rs` (around line 1-30) and add:

```rust
mod color;
```

Find the public exports section and add:

```rust
pub use color::Color;
```

- [ ] **Step 2: Update CLEAR_COLOR constant**

Find line 22 in `vexo/src/lib.rs`:
```rust
const CLEAR_COLOR: wgpu::Color = wgpu::Color::BLUE;
```

Change to:
```rust
const CLEAR_COLOR: wgpu::Color = Color::BLUE.into();
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 4: Commit**

```bash
git add vexo/src/color.rs vexo/src/lib.rs
git commit -m "feat: add unified Color type with conversion traits"
```

---

### Task 3: Update Rectangle Widget

**Files:**
- Modify: `vexo/src/widgets/rectangle.rs`

- [ ] **Step 1: Update Rectangle struct to use Color**

Find in `vexo/src/widgets/rectangle.rs`:
```rust
pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
    pub key: Option<String>,
}
```

Change to:
```rust
use crate::Color;

pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub key: Option<String>,
}
```

- [ ] **Step 2: Update Rectangle::new signature**

Find:
```rust
pub fn new(width: f32, height: f32, color: [f32; 3]) -> Self {
```

Change to:
```rust
pub fn new(width: f32, height: f32, color: impl Into<Color>) -> Self {
```

And update the body:
```rust
Self {
    width,
    height,
    color: color.into(),
    key: None,
}
```

- [ ] **Step 3: Update draw method to use Color::to_array()**

Find in the `draw` method:
```rust
// Convert [f32; 3] to [f32; 4] (assuming alpha is 1.0)
let color = [self.color[0], self.color[1], self.color[2], 1.0];
```

Change to:
```rust
let color = self.color.to_array();
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/rectangle.rs
git commit -m "feat: update Rectangle widget to use Color type"
```

---

### Task 4: Update Button Widget

**Files:**
- Modify: `vexo/src/widgets/button.rs`

- [ ] **Step 1: Update Button struct to use Color**

Find in `vexo/src/widgets/button.rs`:
```rust
pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: [f32; 3],
    pub padding: f32,
    pub key: Option<String>,
}
```

Change to:
```rust
use crate::Color;

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: Color,
    pub padding: f32,
    pub key: Option<String>,
}
```

- [ ] **Step 2: Update Button::new default color**

Find:
```rust
impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: [0.2, 0.2, 0.2],
            ...
        }
    }
```

Change to:
```rust
impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: Color::rgb(0.2, 0.2, 0.2),
            ...
        }
    }
```

- [ ] **Step 3: Update color setter to accept Into<Color>**

Find:
```rust
pub fn color(mut self, color: [f32; 3]) -> Self {
    self.background_color = color;
    self
}
```

Change to:
```rust
pub fn color(mut self, color: impl Into<Color>) -> Self {
    self.background_color = color.into();
    self
}
```

- [ ] **Step 4: Update draw method**

Find:
```rust
// Assuming alpha = 1.0 for now
let color = [
    self.background_color[0],
    self.background_color[1],
    self.background_color[2],
    1.0,
];
```

Change to:
```rust
let color = self.background_color.to_array();
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/button.rs
git commit -m "feat: update Button widget to use Color type"
```

---

### Task 5: Update Text Widget

**Files:**
- Modify: `vexo/src/widgets/text.rs`

- [ ] **Step 1: Update Text struct to use Color**

Find in `vexo/src/widgets/text.rs`:
```rust
pub struct Text {
    pub content: String,
    pub size: f32,
    pub color: [f32; 3],
    pub style: taffy::Style,
    pub key: Option<String>,
}
```

Change to:
```rust
use crate::Color;

pub struct Text {
    pub content: String,
    pub size: f32,
    pub color: Color,
    pub style: taffy::Style,
    pub key: Option<String>,
}
```

- [ ] **Step 2: Update Text::new default color**

Find:
```rust
impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            size: 24.0,
            color: [0.0, 0.0, 0.0],
            ...
        }
    }
```

Change to:
```rust
impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            size: 24.0,
            color: Color::BLACK,
            ...
        }
    }
```

- [ ] **Step 3: Update TextEdit struct to use Color**

Find:
```rust
pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: [f32; 3],
    pub style: taffy::Style,
    pub key: Option<String>,
}
```

Change to:
```rust
pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub style: taffy::Style,
    pub key: Option<String>,
}
```

- [ ] **Step 4: Update TextEdit::new default color**

Find:
```rust
impl TextEdit {
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            ...
            text_color: [1.0, 1.0, 1.0],
            ...
        }
    }
```

Change to:
```rust
impl TextEdit {
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            ...
            text_color: Color::WHITE,
            ...
        }
    }
```

- [ ] **Step 5: Update TextEdit draw method**

Find the hardcoded colors in the draw method:
```rust
let debug_color = [1.0, 0.0, 0.0, 1.0];
renderer.add_rect(pos, size, [0.0, 0.0, 0.0, 1.0], debug_color, 1.0);
```

Change to:
```rust
let debug_color = Color::RED.to_array();
renderer.add_rect(pos, size, Color::BLACK.to_array(), debug_color, 1.0);
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/text.rs
git commit -m "feat: update Text and TextEdit widgets to use Color type"
```

---

### Task 6: Update Renderer

**Files:**
- Modify: `vexo/src/renderer.rs`

- [ ] **Step 1: Update add_text signature to accept Color**

Find in `vexo/src/renderer.rs`:
```rust
pub fn add_text(&mut self, content: String, x: f32, y: f32, size: f32, color: [f32; 3]) {
    let color_rgba = [color[0], color[1], color[2], 1.0];

    self.text_requests.push(TextRequest {
        content,
        position: (x, y),
        size,
        color: color_rgba,
    });
}
```

Change to:
```rust
use crate::Color;

pub fn add_text(&mut self, content: String, x: f32, y: f32, size: f32, color: impl Into<Color>) {
    let color: Color = color.into();

    self.text_requests.push(TextRequest {
        content,
        position: (x, y),
        size,
        color: color.to_array(),
    });
}
```

- [ ] **Step 2: Update add_rect signature to accept Color**

Find:
```rust
pub fn add_rect(
    &mut self,
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
) {
```

Change to:
```rust
pub fn add_rect(
    &mut self,
    pos: [f32; 2],
    size: [f32; 2],
    color: impl Into<Color>,
    border_color: impl Into<Color>,
    border_width: f32,
) {
    let color: Color = color.into();
    let border_color: Color = border_color.into();
```

And update the body:
```rust
    self.quad_instances.push(quad_instance::QuadInstance {
        position: pos,
        size,
        color: color.to_array(),
        border_color: border_color.to_array(),
        border_width,
        _padding: [0.0; 3],
    });
}
```

- [ ] **Step 3: Update EditorRequest color**

Find:
```rust
pub struct EditorRequest {
    pub id: String,
    pub bounds: Bounds,
    pub color: [f32; 4],
}
```

And in `add_editor_request`:
```rust
self.editor_requests.push(EditorRequest {
    id: id.into(),
    bounds,
    color: [1.0, 1.0, 1.0, 1.0],
});
```

Change to:
```rust
self.editor_requests.push(EditorRequest {
    id: id.into(),
    bounds,
    color: Color::WHITE.to_array(),
});
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 5: Commit**

```bash
git add vexo/src/renderer.rs
git commit -m "feat: update renderer to accept Color type"
```

---

### Task 7: Update Macros Documentation

**Files:**
- Modify: `vexo/src/macros.rs`

- [ ] **Step 1: Update rect! macro documentation**

Find:
```rust
/// Create a Rectangle widget wrapped in Box.
///
/// # Example
/// ```
/// rect!(60.0, 70.0, [1.0, 0.0, 0.0])  // width, height, RGB color
/// ```
```

Change to:
```rust
/// Create a Rectangle widget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::Color;
/// rect!(60.0, 70.0, Color::RED)           // width, height, Color
/// rect!(60.0, 70.0, [1.0, 0.0, 0.0])      // width, height, RGB array (also works)
/// ```
```

- [ ] **Step 2: Update button! macro documentation**

Find:
```rust
/// Create a Button widget wrapped in Box.
///
/// # Examples
/// ```
/// button!(text!("Click"), Message::Clicked)
/// button!(text!("Click"), Message::Clicked, color: [0.1, 0.4, 0.1])
/// ```
```

Change to:
```rust
/// Create a Button widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::Color;
/// button!(text!("Click"), Message::Clicked)
/// button!(text!("Click"), Message::Clicked, color: Color::rgb(0.1, 0.4, 0.1))
/// button!(text!("Click"), Message::Clicked, color: [0.1, 0.4, 0.1])  // RGB array also works
/// ```
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vexo`
Expected: Success

- [ ] **Step 4: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "docs: update macro docs to show Color usage"
```

---

### Task 8: Update shared_app Demo

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Update demo to use Color**

Find in `shared_app/src/lib.rs`:
```rust
use vexo::{column, rect, row, text, text_edit, button, widgets::Widget, AlignItems, Application};
```

Change to:
```rust
use vexo::{column, rect, row, text, text_edit, button, widgets::Widget, AlignItems, Application, Color};
```

- [ ] **Step 2: Update rect! calls to use Color**

Find:
```rust
column![
    align: AlignItems::Center,
    rect!(400.0, 150.0, [0.0, 0.1, 0.0]),
    text_edit!("editor_id_input", "Type here...", size: (100.0, 50.0)),
    button!(text!(text_content, size: 24.0), Message::Clicked, color: [0.1, 0.4, 0.1]),
    rect!(150.0, 50.0, [0.0, 0.0, 1.0]),
    rect!(110.0, 30.0, [0.0, 1.0, 1.0]),
    row![
        rect!(60.0, 70.0, [1.0, 0.0, 0.0]),
        rect!(90.0, 40.0, [1.0, 1.0, 0.0]),
    ],
]
```

Change to:
```rust
column![
    align: AlignItems::Center,
    rect!(400.0, 150.0, Color::rgb(0.0, 0.1, 0.0)),
    text_edit!("editor_id_input", "Type here...", size: (100.0, 50.0)),
    button!(text!(text_content, size: 24.0), Message::Clicked, color: Color::rgb(0.1, 0.4, 0.1)),
    rect!(150.0, 50.0, Color::BLUE),
    rect!(110.0, 30.0, Color::CYAN),
    row![
        rect!(60.0, 70.0, Color::RED),
        rect!(90.0, 40.0, Color::YELLOW),
    ],
]
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p shared_app`
Expected: Success

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: update demo app to use Color type"
```

---

### Task 9: Final Verification

- [ ] **Step 1: Build entire workspace**

Run: `cargo build`
Expected: Success with no errors

- [ ] **Step 2: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens with colored rectangles displayed correctly

- [ ] **Step 3: Build iOS**

Run: `./build_for_ios.sh`
Expected: Build succeeds with Swift bindings generated

- [ ] **Step 4: Final commit (if any fixes needed)**

```bash
git status
# If any uncommitted changes:
git add -A
git commit -m "fix: resolve remaining Color integration issues"
```

---

## Summary

This plan creates a unified `Color` type that:
1. Stores RGBA as f32 (0.0-1.0) internally
2. Provides `From` traits for seamless conversion to/from wgpu::Color, [f32; 3], [f32; 4], and cosmic_text::Color
3. Updates all widget APIs to accept `impl Into<Color>` for backward compatibility
4. Provides preset colors like `Color::RED`, `Color::WHITE`, etc.
