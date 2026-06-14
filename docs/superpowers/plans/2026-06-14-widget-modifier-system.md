# Widget Modifier System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SwiftUI/Compose-style modifier chains to the Widget trait, eliminating nested wrapper widget syntax.

**Architecture:** Two-strategy modifier system — decoration/layout modifiers set Style/Layout fields on the widget itself (zero extra nodes), behavioral/transform modifiers wrap in crate-private wrapper widgets (one node each). Each concrete widget has inherent modifier methods returning `Self`; the Widget trait provides fallback defaults returning `Box<dyn Widget>` for type-erased contexts.

**Tech Stack:** Rust, Taffy (layout), glyphon (text), existing three-tree architecture

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `vexo/src/widgets/mod.rs` | Modify | Remove `pub use` for DecoratedContainer, WithLayout, Transform, GestureDetector, MouseRegion |
| `vexo/src/widgets/text.rs` | Modify | Add `style: Style`, `layout: Layout` fields; add inherent modifier methods |
| `vexo/src/widgets/container.rs` | Modify | Add `style: Style` field to Flex; add inherent decoration modifier methods |
| `vexo/src/widgets/grid.rs` | Modify | Add `style: Style` field to Grid; add inherent decoration modifier methods |
| `vexo/src/widgets/decorated_container.rs` | Modify | Change to `pub(crate)` visibility; no structural changes |
| `vexo/src/widgets/with_layout.rs` | Modify | Change to `pub(crate)` visibility; no structural changes |
| `vexo/src/widgets/transform.rs` | Modify | Change to `pub(crate)` visibility; no structural changes |
| `vexo/src/widgets/gesture_detector.rs` | Modify | Change to `pub(crate)` visibility; no structural changes |
| `vexo/src/widgets/mouse_region.rs` | Modify | Change to `pub(crate)` visibility; no structural changes |
| `vexo/src/widgets/mod.rs` (internals) | Modify | Keep `mod X` declarations but move `pub use` lines to `pub(crate) use` |
| `vexo/src/render_objects/text.rs` | Modify | Add `style: Style`, `layout: Layout` fields; extend `paint` for decoration; update `update_render_object` |
| `vexo/src/render_objects/container.rs` | Modify | Add `style: Style` field; extend `paint` for decoration; merge DecoratedContainerRenderObject logic |
| `vexo/src/render_objects/text_edit.rs` | Modify | Add `style: Style`,`layout: Layout` fields; extend `paint` for decoration |
| `vexo/src/render_objects/decorated_container.rs` | Delete | Merged into `container.rs` |
| `vexo/src/render_objects/mod.rs` | Modify | Remove `decorated_container` module; update re-exports |
| `vexo/src/macros.rs` | Modify | Add `modifier_fields!()` and `modifier_methods!()` macros |
| `vexo/src/lib.rs` | Modify | Update public re-exports (remove DecoratedContainer, WithLayout, Transform, GestureDetector, MouseRegion from root; add MouseCursor re-export) |
| `vexo/src/style.rs` | Modify | No changes (Style struct stays the same) |
| `shared_app/src/lib.rs` | Modify | Migrate to modifier syntax |

---

### Task 1: Add modifier macros to `macros.rs`

**Files:**
- Modify: `vexo/src/macros.rs`
- Test: `vexo/src/macros.rs` (inline `#[cfg(test)]`)

The `modifier_fields!()` macro adds `style: Style` and `layout: Layout` fields to a widget struct. The `modifier_methods!()` macro generates inherent methods on a concrete widget type that return `Self`, setting individual style/layout properties.

- [ ] **Step 1: Write the failing tests**

Add test module at bottom of `vexo/src/macros.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::{Color, Layout, Style};
    use crate::layout::{FlexDirection, AlignItems};

    // Dummy widget to test macros
    struct TestWidget {
        content: String,
        modifier_fields!();
    }

    impl TestWidget {
        fn new(content: &str) -> Self {
            Self {
                content: content.to_string(),
                style: Style::default(),
                layout: Layout::default(),
            }
        }

        modifier_methods!();
    }

    #[test]
    fn test_modifier_fields_macro_generates_fields() {
        let w = TestWidget::new("hello");
        assert_eq!(w.style.background, None);
        assert!(w.layout.padding.is_none());
    }

    #[test]
    fn test_modifier_methods_background_returns_self() {
        let w = TestWidget::new("hello").background(Color::RED);
        assert_eq!(w.style.background, Some(Color::RED));
        assert_eq!(w.content, "hello"); // other fields preserved
    }

    #[test]
    fn test_modifier_methods_padding_returns_self() {
        let w = TestWidget::new("hello").padding(8.0);
        assert!(w.layout.padding.is_some());
        assert_eq!(w.content, "hello");
    }

    #[test]
    fn test_modifier_methods_chain_preserves_all() {
        let w = TestWidget::new("hello")
            .background(Color::RED)
            .padding(8.0)
            .margin(4.0);
        assert_eq!(w.style.background, Some(Color::RED));
        assert!(w.layout.padding.is_some());
        assert!(w.layout.margin.is_some());
        assert_eq!(w.content, "hello");
    }

    #[test]
    fn test_modifier_methods_corner_radius_returns_self() {
        let w = TestWidget::new("hello").corner_radius(8.0);
        assert!(w.style.corner_radius.is_some());
        assert_eq!(w.style.background, None); // other style fields preserved
    }

    #[test]
    fn test_modifier_methods_border_returns_self() {
        let w = TestWidget::new("hello").border(Color::BLACK, 2.0);
        assert!(w.style.border.is_some());
        assert_eq!(w.style.border.as_ref().unwrap().width, 2.0);
    }

    #[test]
    fn test_modifier_methods_clip_returns_self() {
        let w = TestWidget::new("hello").clip();
        assert!(w.style.clip);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- macros::tests`
Expected: Compile error — `modifier_fields!` and `modifier_methods!` macros not defined

- [ ] **Step 3: Implement the macros**

Add to `vexo/src/macros.rs` (before the test module):

```rust
/// Generates `style: Style` and `layout: Layout` fields on a struct.
///
/// Place inside the struct definition where fields would go:
/// ```ignore
/// struct MyWidget {
///     content: String,
///     modifier_fields!();
/// }
/// ```
#[macro_export]
macro_rules! modifier_fields {
    () => {
        style: $crate::Style,
        layout: $crate::Layout
    };
}

/// Generates inherent modifier methods on a concrete widget type that return `Self`.
///
/// Each method sets one Style or Layout property, preserving all others.
/// Requires `self.style: Style` and `self.layout: Layout` fields.
#[macro_export]
macro_rules! modifier_methods {
    () => {
        // Style property methods
        pub fn background(mut self, color: $crate::Color) -> Self {
            self.style = self.style.background(color);
            self
        }

        pub fn border(mut self, color: $crate::Color, width: f32) -> Self {
            self.style = self.style.border(color, width);
            self
        }

        pub fn corner_radius(mut self, radius: f32) -> Self {
            self.style = self.style.corner_radius(radius);
            self
        }

        pub fn clip(mut self) -> Self {
            self.style = self.style.clip();
            self
        }

        // Layout property methods (delegating to Layout builder)
        pub fn padding(mut self, value: f32) -> Self {
            self.layout = self.layout.padding(value);
            self
        }

        pub fn padding_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
            self.layout = self.layout.padding_each(top, right, bottom, left);
            self
        }

        pub fn margin(mut self, value: f32) -> Self {
            self.layout = self.layout.margin(value);
            self
        }

        pub fn margin_each(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
            self.layout = self.layout.margin_each(top, right, bottom, left);
            self
        }

        pub fn width(mut self, value: f32) -> Self {
            self.layout = self.layout.width(value);
            self
        }

        pub fn height(mut self, value: f32) -> Self {
            self.layout = self.layout.height(value);
            self
        }

        pub fn min_width(mut self, value: f32) -> Self {
            self.layout = self.layout.min_width(value);
            self
        }

        pub fn min_height(mut self, value: f32) -> Self {
            self.layout = self.layout.min_height(value);
            self
        }

        pub fn max_width(mut self, value: f32) -> Self {
            self.layout = self.layout.max_width(value);
            self
        }

        pub fn max_height(mut self, value: f32) -> Self {
            self.layout = self.layout.max_height(value);
            self
        }

        pub fn flex_grow(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_grow(value);
            self
        }

        pub fn flex_shrink(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_shrink(value);
            self
        }

        pub fn flex_basis(mut self, value: f32) -> Self {
            self.layout = self.layout.flex_basis(value);
            self
        }

        pub fn align_self(mut self, value: $crate::layout::AlignSelf) -> Self {
            self.layout = self.layout.align_self(value);
            self
        }

        pub fn position(mut self, value: $crate::layout::Position) -> Self {
            self.layout = self.layout.position(value);
            self
        }

        pub fn absolute(mut self) -> Self {
            self.layout = self.layout.absolute();
            self
        }

        pub fn relative(mut self) -> Self {
            self.layout = self.layout.relative();
            self
        }

        pub fn inset(mut self, value: f32) -> Self {
            self.layout = self.layout.inset(value);
            self
        }

        pub fn top(mut self, value: f32) -> Self {
            self.layout = self.layout.top(value);
            self
        }

        pub fn right(mut self, value: f32) -> Self {
            self.layout = self.layout.right(value);
            self
        }

        pub fn bottom(mut self, value: f32) -> Self {
            self.layout = self.layout.bottom(value);
            self
        }

        pub fn left(mut self, value: f32) -> Self {
            self.layout = self.layout.left(value);
            self
        }

        pub fn aspect_ratio(mut self, value: f32) -> Self {
            self.layout = self.layout.aspect_ratio(value);
            self
        }

        pub fn overflow(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow(value);
            self
        }

        pub fn overflow_x(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow_x(value);
            self
        }

        pub fn overflow_y(mut self, value: $crate::layout::Overflow) -> Self {
            self.layout = self.layout.overflow_y(value);
            self
        }
    };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- macros::tests`
Expected: All 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "feat: add modifier_fields and modifier_methods macros"
```

---

### Task 2: Add Style/Layout fields and inherent modifier methods to Text widget

**Files:**
- Modify: `vexo/src/widgets/text.rs`
- Test: `vexo/src/widgets/text.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

Add tests to the existing `#[cfg(test)] mod tests` block in `vexo/src/widgets/text.rs`:

```rust
#[test]
fn test_text_modifier_background_returns_self() {
    let w = Text::new("Hello").background(Color::RED);
    assert_eq!(w.style.background, Some(Color::RED));
    assert_eq!(w.content(), "Hello");
}

#[test]
fn test_text_modifier_padding_returns_self() {
    let w = Text::new("Hello").padding(8.0);
    assert!(w.layout.padding.is_some());
    assert_eq!(w.content(), "Hello");
}

#[test]
fn test_text_modifier_chain_preserves_all() {
    let w = Text::new("Hello")
        .background(Color::RED)
        .padding(8.0)
        .margin(4.0)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0)
        .clip();
    assert_eq!(w.style.background, Some(Color::RED));
    assert!(w.style.border.is_some());
    assert!(w.style.corner_radius.is_some());
    assert!(w.style.clip);
    assert!(w.layout.padding.is_some());
    assert!(w.layout.margin.is_some());
    assert_eq!(w.content(), "Hello");
}

#[test]
fn test_text_modifier_preserves_font_size() {
    let w = Text::new("Hello").with_font_size(32.0).padding(8.0);
    assert_eq!(w.font_size(), 32.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- widgets::text::tests::test_text_modifier`
Expected: Compile error — Text has no `style` or `layout` field, no `background()` method

- [ ] **Step 3: Add modifier_fields and modifier_methods to Text**

Modify `vexo/src/widgets/text.rs`:

1. Add imports at top:
```rust
use crate::style::Style;
use crate::layout::Layout;
use crate::modifier_fields;
use crate::modifier_methods;
```

2. Change struct to use `modifier_fields!()`:
```rust
pub struct Text {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    modifier_fields!(),
}
```

3. Update `new()` to initialize `style: Style::default(), layout: Layout::default()`:
```rust
pub fn new(content: impl Into<String>) -> Self {
    Self {
        key: None,
        content: content.into(),
        font_size: 24.0,
        style: Style::default(),
        layout: Layout::default(),
    }
}
```

4. Update `with_key()`, `with_font_size()` to preserve style/layout:
```rust
pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
    self.key = Some(key.into());
    self
}

pub fn with_font_size(mut self, size: f32) -> Self {
    self.font_size = size;
    self
}
```

5. Add `modifier_methods!()` in an `impl Text` block (after the existing methods):
```rust
impl Text {
    modifier_methods!();
}
```

6. Update `Clone` impl to include style/layout:
```rust
impl Clone for Text {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}
```

7. Update `Widget` impl — `create_render_object` must pass style and layout:
```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(TextRenderObject::new(&self.content)
        .with_font_size(self.font_size)
        .with_style(self.style.clone())
        .with_layout(self.layout.clone()))
}
```

8. Update `update_render_object` to also check style/layout changes:
```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(text_ro) = render_object.as_any_mut().downcast_mut::<TextRenderObject>() {
        let content_changed = text_ro.set_content(&self.content);
        let font_size_changed = text_ro.set_font_size(self.font_size);
        let style_changed = text_ro.set_style(self.style.clone());
        let layout_changed = text_ro.set_layout(self.layout.clone());

        if layout_changed {
            UpdateResult::LAYOUT
        } else if content_changed || font_size_changed || style_changed {
            UpdateResult::PAINT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

Note: `set_style` returning true means PAINT (style changes only affect painting). `set_layout` returning true means LAYOUT (layout changes need re-layout).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- widgets::text::tests::test_text_modifier`
Expected: All 4 new tests PASS. All existing tests still PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/text.rs
git commit -m "feat: add Style/Layout fields and modifier methods to Text widget"
```

---

### Task 3: Extend TextRenderObject with Style and Layout support

**Files:**
- Modify: `vexo/src/render_objects/text.rs`
- Test: `vexo/src/render_objects/text.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

Add to the test module in `vexo/src/render_objects/text.rs`:

```rust
#[test]
fn test_text_render_object_with_style_background_paint() {
    let style = crate::Style::new().background(crate::core::Color::RED);
    let mut ro = TextRenderObject::new("Hello").with_style(style);
    ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

    let mut commands = Vec::new();
    let mut ctx = PaintContext::new(&mut commands);
    let cmds = ro.paint(&mut ctx);

    // Should have background rect + text command
    assert!(cmds.len() >= 2, "expected at least 2 commands, got {}", cmds.len());
}

#[test]
fn test_text_render_object_set_style_change_detection() {
    let style1 = crate::Style::new().background(crate::core::Color::RED);
    let style2 = crate::Style::new().background(crate::core::Color::BLUE);
    let mut ro = TextRenderObject::new("Hello").with_style(style1);

    assert!(ro.set_style(style2)); // different = changed
    assert!(!ro.set_style(style2.clone())); // same = no change
}

#[test]
fn test_text_render_object_set_layout_change_detection() {
    let layout1 = crate::Layout::default().padding(8.0);
    let layout2 = crate::Layout::default().padding(16.0);
    let mut ro = TextRenderObject::new("Hello").with_layout(layout1);

    assert!(ro.set_layout(layout2)); // different = changed
    assert!(!ro.set_layout(layout2.clone())); // same = no change
}

#[test]
fn test_text_render_object_layout_with_custom_padding() {
    let layout = crate::Layout::default().padding(8.0);
    let mut ro = TextRenderObject::new("Hello").with_layout(layout);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

    let result = ro.layout(&mut ctx, &[]);

    assert!(ro.layout_node.is_some());
    // The layout node should use the custom padding layout
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- render_objects::text::tests`
Expected: Compile error — TextRenderObject has no `style`, `layout` fields, no `with_style`, `with_layout`, `set_style`, `set_layout` methods

- [ ] **Step 3: Add Style, Layout fields and methods to TextRenderObject**

Modify `vexo/src/render_objects/text.rs`:

1. Add imports:
```rust
use crate::style::Style;
use crate::layout::Layout;
```

2. Add fields to struct:
```rust
pub struct TextRenderObject {
    content: String,
    font_size: f32,
    style: Style,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

3. Update `new()`:
```rust
pub fn new(content: &str) -> Self {
    Self {
        content: content.to_string(),
        font_size: 24.0,
        style: Style::default(),
        layout: Layout::default(),
        computed_bounds: None,
        layout_node: None,
    }
}
```

4. Add builder methods:
```rust
pub fn with_style(mut self, style: Style) -> Self {
    self.style = style;
    self
}

pub fn with_layout(mut self, layout: Layout) -> Self {
    self.layout = layout;
    self
}
```

5. Add setter methods:
```rust
pub fn set_style(&mut self, style: Style) -> bool {
    if self.style != style {
        self.style = style;
        true
    } else {
        false
    }
}

pub fn set_layout(&mut self, layout: Layout) -> bool {
    if self.layout != layout {
        self.layout = layout;
        true
    } else {
        false
    }
}
```

6. Update `layout()` to use `self.layout` instead of hardcoded `Layout::default().flex_shrink(0.0)`:
```rust
fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    let measure_ctx = MeasureContext::Text(TextMeasureContext {
        content: self.content.clone(),
        font_size: self.font_size,
        line_height: 1.2,
    });

    match self.layout_node {
        Some(existing) => {
            ctx.engine().set_context(existing, measure_ctx);
            // Update the style on the existing node in case layout changed
            let leaf_style = self.layout.clone().flex_shrink(0.0);
            ctx.engine().set_style(existing, &leaf_style);
            LayoutResult {
                node: existing,
                size: Size::new(0.0, 0.0),
            }
        }
        None => {
            let leaf_style = self.layout.clone().flex_shrink(0.0);
            let node = ctx.engine().create_leaf_with_context(
                &leaf_style,
                measure_ctx,
            );
            self.layout_node = Some(node);
            LayoutResult {
                node,
                size: Size::new(0.0, 0.0),
            }
        }
    }
}
```

7. Update `paint()` to paint decoration before text:
```rust
fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
    let bounds = match &self.computed_bounds {
        Some(b) => b,
        None => return vec![],
    };

    let mut commands = Vec::new();
    let pos: Position<Logical, Absolute> = ctx.absolute_position();

    let absolute_bounds = Bounds::new(
        pos.x,
        pos.y,
        pos.x + bounds.width(),
        pos.y + bounds.height(),
    );

    // 1. Push corner radius if set
    if let Some(ref cr) = self.style.corner_radius {
        commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
    }

    // 2. Draw background
    if let Some(bg_color) = self.style.background {
        commands.push(RenderCommand::rect(absolute_bounds, bg_color));
    }

    // 3. Draw border
    if let Some(ref border) = self.style.border {
        commands.push(RenderCommand::rect_with_border(
            absolute_bounds,
            Color::TRANSPARENT,
            border.color,
            border.width,
        ));
    }

    // 4. Pop corner radius
    if self.style.corner_radius.is_some() {
        commands.push(RenderCommand::PopCornerRadius);
    }

    // 5. Draw text
    commands.push(RenderCommand::Text {
        content: self.content.clone(),
        position: pos.to_point(),
        font_size: self.font_size,
        color: crate::core::Color::BLACK,
        max_width: Some(bounds.width()),
    });

    commands
}
```

8. Add `clip_bounds()` implementation for clip support:
```rust
fn clip_bounds(&self) -> Option<Bounds<Logical>> {
    if self.style.clip {
        self.computed_bounds
    } else {
        None
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- render_objects::text::tests`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render_objects/text.rs
git commit -m "feat: extend TextRenderObject with Style and Layout support"
```

---

### Task 4: Add Style field and decoration modifier methods to Flex and Grid widgets

**Files:**
- Modify: `vexo/src/widgets/container.rs`
- Modify: `vexo/src/widgets/grid.rs`
- Test: inline tests in each file

- [ ] **Step 1: Write the failing tests for Flex**

Add to `vexo/src/widgets/container.rs` test module:

```rust
#[test]
fn test_flex_modifier_background_returns_self() {
    let w = Flex::column().background(Color::RED);
    assert_eq!(w.style.background, Some(Color::RED));
}

#[test]
fn test_flex_modifier_chain() {
    let w = Flex::column()
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0)
        .clip()
        .padding(8.0);
    assert_eq!(w.style.background, Some(Color::RED));
    assert!(w.style.border.is_some());
    assert!(w.style.corner_radius.is_some());
    assert!(w.style.clip);
    assert!(w.layout.padding.is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- widgets::container::tests::test_flex_modifier`
Expected: Compile error — Flex has no `style` field, no `background()` inherent method

- [ ] **Step 3: Add `style: Style` field and decoration modifier methods to Flex**

Modify `vexo/src/widgets/container.rs`:

1. Add imports:
```rust
use crate::style::Style;
use crate::modifier_methods;
```

2. Add `style: Style` to Flex struct:
```rust
pub struct Flex {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
    style: Style,
}
```

3. Update constructors to initialize `style: Style::default()`:
```rust
pub fn new() -> Self {
    Self {
        key: None,
        children: Vec::new(),
        layout: Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch),
        style: Style::default(),
    }
}

pub fn column() -> Self {
    Self {
        key: None,
        children: Vec::new(),
        layout: Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch),
        style: Style::default(),
    }
}

pub fn row() -> Self {
    Self::new()
}
```

4. Update `push()`, `with_key()`, `layout()` to preserve `style`:
```rust
pub fn push(mut self, child: impl Widget + 'static) -> Self {
    self.children.push(Box::new(child));
    self
}

pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
    self.key = Some(key.into());
    self
}

pub fn layout(mut self, layout: Layout) -> Self {
    self.layout = layout;
    self
}
```

5. Add an `impl Flex` block with decoration modifier methods (style-only, since layout methods already exist from `layout_builder_methods!()`):
```rust
impl Flex {
    pub fn background(mut self, color: Color) -> Self {
        self.style = self.style.background(color);
        self
    }

    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.style = self.style.border(color, width);
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.style = self.style.corner_radius(radius);
        self
    }

    pub fn clip(mut self) -> Self {
        self.style = self.style.clip();
        self
    }
}
```

6. Update `Clone` impl:
```rust
impl Clone for Flex {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
            style: self.style.clone(),
        }
    }
}
```

7. Update `Widget::create_render_object` to pass style:
```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(ContainerRenderObject::new_with_style(self.layout.clone(), self.style.clone()))
}
```

8. Update `Widget::update_render_object` to check style changes:
```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<ContainerRenderObject>() {
        let layout_changed = container_ro.set_layout(self.layout.clone());
        let style_changed = container_ro.set_style(self.style.clone());

        if layout_changed {
            UpdateResult::LAYOUT
        } else if style_changed {
            UpdateResult::PAINT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 4: Run Flex tests to verify they pass**

Run: `cargo test -p vexo -- widgets::container::tests::test_flex_modifier`
Expected: All PASS

- [ ] **Step 5: Write the failing tests for Grid**

Add to `vexo/src/widgets/grid.rs` test module:

```rust
#[test]
fn test_grid_modifier_background_returns_self() {
    let w = Grid::new().background(Color::RED);
    assert_eq!(w.style.background, Some(Color::RED));
}

#[test]
fn test_grid_modifier_chain() {
    let w = Grid::new()
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0)
        .clip()
        .padding(8.0);
    assert_eq!(w.style.background, Some(Color::RED));
    assert!(w.style.border.is_some());
    assert!(w.style.corner_radius.is_some());
    assert!(w.style.clip);
    assert!(w.layout.padding.is_some());
}
```

- [ ] **Step 6: Run Grid tests to verify they fail**

Run: `cargo test -p vexo -- widgets::grid::tests::test_grid_modifier`
Expected: Compile error — Grid has no `style` field

- [ ] **Step 7: Add `style: Style` field and decoration modifier methods to Grid**

Apply the same pattern as Flex to `vexo/src/widgets/grid.rs`:
1. Add `use crate::style::Style;`
2. Add `style: Style` field
3. Initialize in `new()` with `style: Style::default()`
4. Add `background()`, `border()`, `corner_radius()`, `clip()` inherent methods
5. Update `Clone` impl
6. Update `create_render_object` and `update_render_object`

- [ ] **Step 8: Run Grid tests to verify they pass**

Run: `cargo test -p vexo -- widgets::grid::tests::test_grid_modifier`
Expected: All PASS

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/container.rs vexo/src/widgets/grid.rs
git commit -m "feat: add Style field and decoration modifier methods to Flex and Grid"
```

---

### Task 5: Extend ContainerRenderObject with Style support (merge DecoratedContainerRenderObject)

**Files:**
- Modify: `vexo/src/render_objects/container.rs`
- Delete: `vexo/src/render_objects/decorated_container.rs`
- Modify: `vexo/src/render_objects/mod.rs`
- Test: `vexo/src/render_objects/container.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

Add to `vexo/src/render_objects/container.rs` test module:

```rust
#[test]
fn test_container_render_object_paint_with_background() {
    let style = crate::Style::new().background(crate::core::Color::RED);
    let layout = column_layout();
    let mut ro = ContainerRenderObject::new_with_style(layout, style);
    ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

    let mut commands = Vec::new();
    let mut ctx = PaintContext::new(&mut commands);
    let cmds = ro.paint(&mut ctx);

    assert!(!cmds.is_empty(), "should emit background command");
}

#[test]
fn test_container_render_object_paint_with_border() {
    let style = crate::Style::new().border(crate::core::Color::BLACK, 2.0);
    let layout = column_layout();
    let mut ro = ContainerRenderObject::new_with_style(layout, style);
    ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

    let mut commands = Vec::new();
    let mut ctx = PaintContext::new(&mut commands);
    let cmds = ro.paint(&mut ctx);

    assert!(!cmds.is_empty(), "should emit border command");
}

#[test]
fn test_container_render_object_set_style_change_detection() {
    let style1 = crate::Style::new().background(crate::core::Color::RED);
    let style2 = crate::Style::new().background(crate::core::Color::BLUE);
    let layout = column_layout();
    let mut ro = ContainerRenderObject::new_with_style(layout, style1);

    assert!(ro.set_style(style2)); // different = changed
    assert!(!ro.set_style(style2.clone())); // same = no change
}

#[test]
fn test_container_render_object_clip_bounds() {
    let style = crate::Style::new().clip();
    let layout = column_layout();
    let mut ro = ContainerRenderObject::new_with_style(layout, style);
    ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

    assert!(ro.clip_bounds().is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- render_objects::container::tests`
Expected: Compile error — no `new_with_style`, `set_style`, `style` field on ContainerRenderObject

- [ ] **Step 3: Add Style support to ContainerRenderObject**

Modify `vexo/src/render_objects/container.rs`:

1. Add imports:
```rust
use crate::core::Color;
use crate::style::Style;
use crate::render::RenderCommand;
```

2. Add `style: Style` field:
```rust
pub struct ContainerRenderObject {
    children: Vec<RenderObjectKey>,
    layout: Layout,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

3. Update `new()` and add `new_with_style()`:
```rust
pub fn new(layout: Layout) -> Self {
    Self::new_with_style(layout, Style::default())
}

pub fn new_with_style(layout: Layout, style: Style) -> Self {
    Self {
        children: Vec::new(),
        layout,
        style,
        computed_bounds: None,
        layout_node: None,
    }
}
```

4. Add `set_style()` method:
```rust
pub fn set_style(&mut self, style: Style) -> bool {
    if self.style != style {
        self.style = style;
        true
    } else {
        false
    }
}
```

5. Update `paint()` to handle decoration (copy the logic from DecoratedContainerRenderObject):
```rust
fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
    let bounds = match &self.computed_bounds {
        Some(b) => b,
        None => return vec![],
    };

    let mut commands = Vec::new();
    let pos: Position<Logical, Absolute> = ctx.absolute_position();

    let absolute_bounds = Bounds::new(
        pos.x,
        pos.y,
        pos.x + bounds.width(),
        pos.y + bounds.height(),
    );

    // 1. Push corner radius if set
    if let Some(ref cr) = self.style.corner_radius {
        commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
    }

    // 2. Draw background first (behind child)
    if let Some(bg_color) = self.style.background {
        commands.push(RenderCommand::rect(absolute_bounds, bg_color));
    }

    // 3. Draw border on top (after background)
    if let Some(ref border) = self.style.border {
        commands.push(RenderCommand::rect_with_border(
            absolute_bounds,
            Color::TRANSPARENT,
            border.color,
            border.width,
        ));
    }

    // 4. Pop corner radius
    if self.style.corner_radius.is_some() {
        commands.push(RenderCommand::PopCornerRadius);
    }

    commands
}
```

6. Add `clip_bounds()`:
```rust
fn clip_bounds(&self) -> Option<Bounds<Logical>> {
    if self.style.clip {
        self.computed_bounds
    } else {
        None
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- render_objects::container::tests`
Expected: All PASS

- [ ] **Step 5: Delete DecoratedContainerRenderObject file and update module**

1. Delete `vexo/src/render_objects/decorated_container.rs`
2. Modify `vexo/src/render_objects/mod.rs` — remove `mod decorated_container;` and `pub use decorated_container::DecoratedContainerRenderObject;`
3. Update `vexo/src/widgets/decorated_container.rs` — change its `create_render_object` to use `ContainerRenderObject::new_with_style` instead of `DecoratedContainerRenderObject`:

```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(crate::render_objects::ContainerRenderObject::new_with_style(
        self.layout.clone(),
        self.style.clone(),
    ))
}
```

4. Update its `update_render_object` similarly:
```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<crate::render_objects::ContainerRenderObject>() {
        let layout_changed = container_ro.set_layout(self.layout.clone());
        let style_changed = container_ro.set_style(self.style.clone());

        if layout_changed {
            UpdateResult::LAYOUT
        } else if style_changed {
            UpdateResult::PAINT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 6: Run full build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Build succeeds, all tests pass

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: merge DecoratedContainerRenderObject into ContainerRenderObject with Style support"
```

---

### Task 6: Extend TextEditRenderObject with Style and Layout support

**Files:**
- Modify: `vexo/src/render_objects/text_edit.rs`
- Test: inline tests

- [ ] **Step 1: Write the failing tests**

Add to test module in `vexo/src/render_objects/text_edit.rs`:

```rust
#[test]
fn test_text_edit_render_object_set_style_change_detection() {
    let editor = create_test_editor("Hello");
    let style1 = crate::Style::new().background(crate::core::Color::WHITE);
    let style2 = crate::Style::new().background(crate::core::Color::BLUE);
    let mut ro = TextEditRenderObject::new("Hello", editor).with_style(style1);

    assert!(ro.set_style(style2));
    assert!(!ro.set_style(style2.clone()));
}

#[test]
fn test_text_edit_render_object_set_layout_change_detection() {
    let editor = create_test_editor("Hello");
    let layout1 = crate::Layout::default().padding(8.0);
    let layout2 = crate::Layout::default().padding(16.0);
    let mut ro = TextEditRenderObject::new("Hello", editor).with_layout(layout1);

    assert!(ro.set_layout(layout2));
    assert!(!ro.set_layout(layout2.clone()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- render_objects::text_edit::tests`
Expected: Compile error — no `with_style`, `set_style`, `with_layout`, `set_layout` on TextEditRenderObject

- [ ] **Step 3: Add Style and Layout support to TextEditRenderObject**

Apply same pattern as TextRenderObject:
1. Add `style: Style` and `layout: Layout` fields
2. Add `with_style()`, `with_layout()` builder methods
3. Add `set_style()`, `set_layout()` setter methods with change detection
4. Update `paint()` to draw decoration before text content
5. Add `clip_bounds()` implementation
6. Update `layout()` to use `self.layout` instead of hardcoded defaults

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- render_objects::text_edit::tests`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render_objects/text_edit.rs
git commit -m "feat: extend TextEditRenderObject with Style and Layout support"
```

---

### Task 7: Add Widget trait modifier default methods

**Files:**
- Modify: `vexo/src/widgets/mod.rs`
- Test: `vexo/src/widgets/mod.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

Add to test module in `vexo/src/widgets/mod.rs`:

```rust
#[test]
fn test_widget_trait_on_press_wraps() {
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let called_clone = called.clone();
    let widget = Text::new("Click")
        .on_press(move || called_clone.set(true));
    // Should produce a widget with GestureDetector wrapping
    assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
}

#[test]
fn test_widget_trait_cursor_wraps() {
    let widget = Text::new("Hover")
        .cursor(MouseCursor::System(SystemCursorKind::Pointer));
    assert!(widget.as_any().downcast_ref::<MouseRegion>().is_some());
}

#[test]
fn test_widget_trait_translate_wraps() {
    let widget = Text::new("Shift")
        .translate(10.0, 20.0);
    assert!(widget.as_any().downcast_ref::<Transform>().is_some());
}

#[test]
fn test_widget_trait_on_press_chain() {
    let widget = Text::new("Click")
        .background(Color::RED)
        .padding(8.0)
        .on_press(|| {});
    // Text with style/layout set, then wrapped in GestureDetector
    // The outer widget should be GestureDetector
    assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo -- widgets::tests::test_widget_trait`
Expected: Compile error — no `on_press`, `cursor`, `translate` methods on Widget trait(or on Text after boxing)

- [ ] **Step 3: Add modifier default methods to Widget trait**

Modify `vexo/src/widgets/mod.rs` — add to the `Widget` trait definition:

```rust
use crate::input::MouseCursor;

// In the Widget trait, add these default methods:

    // Decoration modifiers (fallback: wrap in DecoratedContainer)
    fn background(self, color: Color) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).background(color))
    }

    fn border(self, color: Color, width: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).border(color, width))
    }

    fn corner_radius(self, radius: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).corner_radius(radius))
    }

    fn clip(self) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(DecoratedContainer::new(self).clip())
    }

    // Layout modifiers (fallback: wrap in WithLayout)
    fn padding(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().padding(value)))
    }

    fn margin(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().margin(value)))
    }

    fn width(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().width(value)))
    }

    fn height(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().height(value)))
    }

    fn flex_grow(self, value: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().flex_grow(value)))
    }

    fn align_self(self, value: crate::layout::AlignSelf) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().align_self(value)))
    }

    fn absolute(self) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(WithLayout::new(self, Layout::default().absolute()))
    }

    // Behavioral modifiers (always wrap)
    fn on_press(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(GestureDetector::new(self).on_press(callback))
    }

    fn on_release(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(GestureDetector::new(self).on_release(callback))
    }

    fn cursor(self, cursor: MouseCursor) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).cursor(cursor))
    }

    fn on_enter(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).on_enter(callback))
    }

    fn on_exit(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(MouseRegion::new(self).on_exit(callback))
    }

    // Transform modifiers (always wrap)
    fn translate(self, dx: f32, dy: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::translate(self, dx, dy))
    }

    fn rotate(self, radians: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::rotate(self, radians))
    }

    fn scale(self, sx: f32, sy: f32) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(Transform::scale(self, sx, sy))
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo -- widgets::tests::test_widget_trait`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/mod.rs
git commit -m "feat: add modifier default methods to Widget trait"
```

---

### Task 8: Make wrapper widgets pub(crate) and update re-exports

**Files:**
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Change widget module re-exports**

In `vexo/src/widgets/mod.rs`, change the public re-exports to crate-only for wrapper widgets:

```rust
// Public API - leaf and container widgets
pub use container::Flex;
pub use grid::Grid;
pub use text::Text;
pub use text_edit::{TextEdit, TextEditState, TextEditingController};
pub use super::Focus;

// Crate-internal modifier widgets (not part of public API)
pub(crate) use decorated_container::DecoratedContainer;
pub(crate) use with_layout::WithLayout;
pub(crate) use transform::Transform;
pub(crate) use gesture_detector::GestureDetector;
pub(crate) use mouse_region::MouseRegion;
```

- [ ] **Step 2: Update lib.rs re-exports**

In `vexo/src/lib.rs`, remove the public re-exports for wrapper widgets:

```rust
// Remove these from the public widgets re-export:
// DecoratedContainer, GestureDetector, MouseRegion, Transform, WithLayout

// The line becomes:
pub use widgets::{Widget, Text, Flex, Grid, TextEdit, TextEditState, TextEditingController};
```

Keep `MouseCursor` and `SystemCursorKind` public since they're needed for the `.cursor()` modifier:

```rust
pub use input::{MouseCursor, SystemCursorKind};
```

- [ ] **Step 3: Run full build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Build succeeds. Some shared_app import errors expected (fixed in Task 10).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "refactor: make wrapper widgets pub(crate), update public re-exports"
```

---

### Task 9: Add .boxed() convenience method, add Style/Layout to TextEditContent, update TextEdit build()

**Files:**
- Modify: `vexo/src/widgets/mod.rs` — add `.boxed()` to Widget trait
- Modify: `vexo/src/widgets/text_edit_content.rs` — add modifier fields/methods
- Modify: `vexo/src/widgets/text_edit.rs` — update build() to use modifiers

- [ ] **Step 1: Add .boxed() method to Widget trait**

In `vexo/src/widgets/mod.rs`, add to the Widget trait:

```rust
    /// Box this widget into a `Box<dyn Widget>`.
    /// Useful at the end of a modifier chain when you need to return `Box<dyn Widget>`.
    fn boxed(self) -> Box<dyn Widget>
    where Self: Sized + 'static {
        Box::new(self)
    }
```

- [ ] **Step 2: Add modifier fields and methods to TextEditContent**

In `vexo/src/widgets/text_edit_content.rs`:

1. Add imports:
```rust
use crate::style::Style;
use crate::layout::Layout;
use crate::modifier_fields;
use crate::modifier_methods;
```

2. Change the struct to use `modifier_fields!()`:
```rust
pub struct TextEditContent {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
    modifier_fields!(),
}
```

3. Update `new()` to initialize style and layout:
```rust
pub fn new(content: impl Into<String>, editor: Rc<RefCell<Editor>>) -> Self {
    Self {
        key: None,
        content: content.into(),
        font_size: 24.0,
        editor,
        is_focused: false,
        cursor_blink_visible: false,
        style: Style::default(),
        layout: Layout::default(),
    }
}
```

4. Existing builder methods (`with_font_size`, `with_focused`, `with_cursor_blink_visible`) already take `mut self` and return `Self`, so they'll preserve the new fields automatically — no changes needed.

5. Add `modifier_methods!()` in an `impl TextEditContent` block:
```rust
impl TextEditContent {
    modifier_methods!();
}
```

6. Update `Clone` impl to include style and layout:
```rust
impl Clone for TextEditContent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            editor: self.editor.clone(),
            is_focused: self.is_focused,
            cursor_blink_visible: self.cursor_blink_visible,
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}
```

7. Update `Widget::create_render_object` to pass style and layout:
```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    let mut ro = TextEditRenderObject::new(&self.content, self.editor.clone())
        .with_font_size(self.font_size)
        .with_style(self.style.clone())
        .with_layout(self.layout.clone());
    ro.set_focused(self.is_focused);
    ro.set_cursor_blink_visible(self.cursor_blink_visible);
    Box::new(ro)
}
```

8. Update `update_render_object` to check style/layout changes:
```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(ro) = render_object.as_any_mut().downcast_mut::<TextEditRenderObject>() {
        let content_changed = ro.set_content(&self.content);
        let font_size_changed = ro.set_font_size(self.font_size);
        let style_changed = ro.set_style(self.style.clone());
        let layout_changed = ro.set_layout(self.layout.clone());
        let focused_changed = ro.set_focused(self.is_focused);
        let blink_changed = ro.set_cursor_blink_visible(self.cursor_blink_visible);

        if layout_changed {
            UpdateResult::LAYOUT
        } else if content_changed || font_size_changed || style_changed || focused_changed || blink_changed {
            UpdateResult::PAINT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 3: Update TextEdit's build() to use modifiers**

In `vexo/src/widgets/text_edit.rs`, find the `StatefulWidget::build()` method and replace the DecoratedContainer + MouseRegion composition with modifier syntax.

The current pattern:
```rust
Box::new(
    MouseRegion::new(
        DecoratedContainer::new(
            TextEditContent::new(self.controller.text(), self.controller.editor())
                .with_font_size(self.controller.font_size())
                .with_focused(is_focused)
                .with_cursor_blink_visible(false),
        )
        .style(style)
        .layout(layout),
    )
    .cursor(MouseCursor::System(SystemCursorKind::Text)),
)
```

Replace with:
```rust
TextEditContent::new(self.controller.text(), self.controller.editor())
    .with_font_size(self.controller.font_size())
    .with_focused(is_focused)
    .with_cursor_blink_visible(false)
    .background(crate::core::Color::WHITE)
    .border(border_color, border_width)
    .corner_radius(4.0)
    .padding(8.0)
    .cursor(MouseCursor::System(SystemCursorKind::Pointer))
    .boxed()
```

Where `border_color` and `border_width` are the values currently computed in the `build()` method (focused = blue `Color::rgb(0.2, 0.4, 0.8)` with width 2.0, unfocused = gray `Color::rgb(0.6, 0.6, 0.6)` with width 1.0).

The `style` and `layout` local variables in `build()` can be removed — they're replaced by the chained modifiers.

- [ ] **Step 4: Run build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Build succeeds, all tests pass

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/mod.rs vexo/src/widgets/text_edit.rs vexo/src/widgets/text_edit_content.rs
git commit -m "feat: add .boxed() method, migrate TextEdit to modifier syntax"
```

---

### Task 10: Migrate shared_app to modifier syntax

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Update imports**

Remove `DecoratedContainer`, `GestureDetector`, `MouseRegion`, `Transform`, `WithLayout` from imports. Keep `Text`, `Flex`, `Grid`, `TextEdit`, `Widget`, `Color`, `Focus`, etc.

```rust
use vexo::{
    column, input::MouseCursor, reactive::StatefulMutable, row, run_desktop_demo, Application,
    BuildContext, Color, Focus, State as RetainState, StatefulWidget, SystemCursorKind, Text,
    TextEdit, TextEditingController, Widget,
};
```

- [ ] **Step 2: Rewrite tap_button helper**

```rust
fn tap_button(label: &str, on_press: impl FnMut() + 'static) -> Box<dyn Widget> {
    Text::new(label)
        .background(Color::rgb(0.9, 0.9, 0.9))
        .border(Color::rgb(0.6, 0.6, 0.6), 1.0)
        .corner_radius(8.0)
        .padding(24.0)
        .on_press(on_press)
}
```

- [ ] **Step 3: Rewrite HoverableCard build method**

Replace `MouseRegion::new(DecoratedContainer::new(column)...)` with:

```rust
column
    .background(Color::rgb(0.95, 0.95, 1.0))
    .border(border_color, border_width)
    .corner_radius(8.0)
    .padding(8.0)
    .cursor(MouseCursor::System(SystemCursorKind::Pointer))
    .on_enter(...)
    .on_exit(...)
    .boxed()
```

- [ ] **Step 4: Rewrite Group B in view()**

Replace `DecoratedContainer::new(column![...]).background(...).border(...)...` with:

```rust
column![
    Text::new("Group B"),
    Focus::new(TextEdit::new(b1.clone())),
    Focus::new(TextEdit::new(b2.clone()))
]
.gap(10.0)
.background(Color::rgb(1.0, 0.95, 0.95))
.border(Color::rgb(0.8, 0.5, 0.5), 1.0)
.corner_radius(8.0)
.padding(8.0)
.boxed()
```

- [ ] **Step 5: Rewrite Transform section**

Replace `Transform::rotate(DecoratedContainer::new(...), ...)` with:

```rust
Text::new("Rotated 15\u{00b0}")
    .background(Color::rgb(0.85, 1.0, 0.85))
    .border(Color::rgb(0.3, 0.6, 0.3), 2.0)
    .corner_radius(12.0)
    .padding(8.0)
    .rotate(15.0_f32.to_radians())
```

And similarly for scale and translate:

```rust
Text::new("1.5x")
    .background(Color::rgb(1.0, 0.9, 0.85))
    .padding(8.0)
    .scale(1.5, 1.5)
```

```rust
Text::new("Shifted")
    .background(Color::rgb(0.85, 0.9, 1.0))
    .padding(8.0)
    .translate(100.0, 100.0)
```

And the 45-degree rotation:

```rust
Text::new("45\u{00b0} rounded")
    .background(Color::rgb(1.0, 0.85, 0.85))
    .border(Color::rgb(0.8, 0.3, 0.3), 2.0)
    .corner_radius(16.0)
    .padding(12.0)
    .rotate(45.0_f32.to_radians())
```

- [ ] **Step 6: Rewrite clip demo**

Replace `DecoratedContainer::new(column![...]).width(150.0).height(60.0).padding(8.0).background(...)...` with:

```rust
column![
    Text::new("Line 1"),
    Text::new("Line 2"),
    Text::new("Line 3"),
    Text::new("Line 4"),
    Text::new("Line 5"),
]
.width(150.0)
.height(60.0)
.padding(8.0)
.background(Color::rgb(1.0, 0.95, 0.9))
.border(Color::rgb(0.8, 0.6, 0.4), 1.0)
.corner_radius(8.0)
.clip()
```

- [ ] **Step 7: Build and run desktop demo**

Run: `cargo build -p desktop_demo && cargo run -p desktop_demo`
Expected: Builds and runs, visual output identical to before

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "refactor: migrate shared_app from wrapper widgets to modifier syntax"
```

---

### Task 11: Delete WithLayoutRenderObject and WithLayout widget (absorbed by other widgets)

**Files:**
- Modify: `vexo/src/widgets/with_layout.rs` — delete file or make it a thin type alias
- Modify: `vexo/src/render_objects/` — remove WithLayoutRenderObject if it exists as separate file
- Modify: `vexo/src/widgets/mod.rs` — update module declarations

Since `WithLayout` is now only used as a fallback wrapper (from Widget trait default methods) and `ContainerRenderObject` already handles layout + style, `WithLayout` can be simplified to just create a `ContainerRenderObject` with default style.

- [ ] **Step 1: Simplify WithLayout to use ContainerRenderObject**

Modify `vexo/src/widgets/with_layout.rs`:

Replace `WithLayoutRenderObject` creation with `ContainerRenderObject::new(self.layout.clone())`. Remove the `WithLayoutRenderObject` type entirely.

Update `WithLayout::create_render_object`:
```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(crate::render_objects::ContainerRenderObject::new(self.layout.clone()))
}
```

Update `WithLayout::update_render_object`:
```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(ro) = render_object.as_any_mut().downcast_mut::<crate::render_objects::ContainerRenderObject>() {
        if ro.set_layout(self.layout.clone()) { UpdateResult::LAYOUT } else { UpdateResult::NONE }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 2: Delete WithLayoutRenderObject from render_objects module**

If `WithLayoutRenderObject` exists as a separate file or re-export in `vexo/src/render_objects/`, remove it. Check `vexo/src/render_objects/mod.rs` for any reference.

- [ ] **Step 3: Run build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Build succeeds, all tests pass

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: simplify WithLayout to use ContainerRenderObject, remove WithLayoutRenderObject"
```

---

### Task 12: Final integration test and cleanup

**Files:**
- Test: full workspace build and test

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: Success

- [ ] **Step 2: Full workspace test**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Run desktop demo and verify visually**

Run: `cargo run -p desktop_demo`
Expected: App renders correctly, all sections (transforms, clip, hover) work as before

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A
git commit -m "chore: final integration test and cleanup for modifier system"
```
