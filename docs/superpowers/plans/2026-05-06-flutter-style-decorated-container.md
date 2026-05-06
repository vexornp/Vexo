# Flutter-Style Decorated Container Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a Flutter-style DecoratedContainer widget that bundles multiple visual decorations into a single element/render object, reducing overhead by ~66% compared to nested modifiers.

**Architecture:** Style struct holds decoration properties (background, border, corner_radius). DecoratedContainer widget takes a child and Style. Creates single DecoratedContainerElement and DecoratedContainerRenderObject that paints all decorations in one pass.

**Tech Stack:** Rust, existing Vexo retain-mode infrastructure

---

## File Structure

### New Files
- `vexo/src/retain/widgets/decorated_container.rs` - DecoratedContainer widget, DecoratedContainerElement, DecoratedContainerRenderObject
- `vexo/src/retain/style.rs` - Style, Border, CornerRadius structs with builder API

### Modified Files
- `vexo/src/retain/widgets/mod.rs` - Export DecoratedContainer and Style
- `vexo/src/retain/mod.rs` - Export Style from retain module

---

## Task 1: Create Style Struct

**Files:**
- Create: `vexo/src/retain/style.rs`

- [ ] **Step 1: Write the Style struct with Border and CornerRadius**

Create `vexo/src/retain/style.rs`:

```rust
//! Visual decoration properties for containers.
//!
//! Style is analogous to Flutter's BoxDecoration - it holds all visual
//! properties in one place for efficient single-pass rendering.

use crate::core::Color;

/// Visual decoration properties for a DecoratedContainer.
///
/// Holds all decoration properties (background, border, corner radius)
/// in a single struct for efficient rendering. This allows multiple
/// decorations to be applied with a single element and render object.
///
/// # Example
///
/// ```
/// let style = Style::new()
///     .background(Color::RED)
///     .border(Color::BLACK, 2.0)
///     .corner_radius(8.0);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Style {
    /// Background color (drawn behind child).
    pub background: Option<Color>,

    /// Border decoration.
    pub border: Option<Border>,

    /// Corner radius for rounded rectangles.
    pub corner_radius: Option<CornerRadius>,
}

/// Border decoration properties.
#[derive(Clone, Debug)]
pub struct Border {
    /// Border color.
    pub color: Color,
    /// Border width in logical pixels.
    pub width: f32,
}

/// Corner radius for rounded rectangles.
#[derive(Clone, Debug)]
pub struct CornerRadius {
    /// Radius for all corners (uniform).
    pub radius: f32,
}

impl Style {
    /// Create a new empty style.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set background color.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Set border with color and width.
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(Border { color, width });
        self
    }

    /// Set uniform corner radius for all corners.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(CornerRadius { radius });
        self
    }
}
```

- [ ] **Step 2: Export Style from retain module**

Add to `vexo/src/retain/mod.rs` after the existing mod declarations:

```rust
mod style;
```

Add to the exports section:

```rust
pub use style::{Style, Border, CornerRadius};
```

- [ ] **Step 3: Run cargo build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/style.rs vexo/src/retain/mod.rs
git commit -m "feat: add Style struct for decoration properties"
```

---

## Task 2: Create DecoratedContainerRenderObject

**Files:**
- Create: `vexo/src/retain/widgets/decorated_container.rs` (RenderObject part)

- [ ] **Step 1: Write DecoratedContainerRenderObject struct**

Add to `vexo/src/retain/widgets/decorated_container.rs`:

```rust
//! Decorated container widget - applies visual decorations to a child.
//!
//! This widget bundles multiple decorations (background, border, corner radius)
//! into a single element and render object, reducing overhead compared to
//! chaining multiple modifier widgets.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;
use crate::retain::{
    Element, ElementContext, ElementId, ElementRegistry, EventContext,
    HitTestContext, LayoutContext, LayoutResult, PaintContext,
    RenderObject, RenderObjectId, Widget,
};
use crate::retain::key::{GlobalKey, Key, WidgetKey};
use crate::retain::style::Style;
use crate::input::InputEvent;

// ============================================================================
// DecoratedContainerRenderObject
// ============================================================================

/// Render object for DecoratedContainer - handles all decorations in a single pass.
///
/// This render object paints background, border, and corner radius together,
/// avoiding the overhead of multiple nested render objects.
pub struct DecoratedContainerRenderObject {
    /// Current style configuration.
    style: Style,

    /// Child render object ID.
    child: Option<RenderObjectId>,

    /// Computed bounds from layout.
    computed_bounds: Option<Bounds<Logical>>,

    /// Layout node in Taffy.
    layout_node: Option<LayoutNodeId>,
}

impl DecoratedContainerRenderObject {
    /// Create a new decorated container render object with the given style.
    pub fn new(style: Style) -> Self {
        Self {
            style,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the style configuration.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Get the current style.
    pub fn style(&self) -> &Style {
        &self.style
    }
}

impl RenderObject for DecoratedContainerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // DecoratedContainer is a pass-through for layout - uses child's bounds
        match child_nodes.first() {
            Some(child_node) => {
                self.layout_node = Some(*child_node);
                LayoutResult {
                    node: *child_node,
                    size: Size::zero(),
                }
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

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

        // 1. Push corner radius if set (affects all subsequent rects)
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

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeId> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 2: Run cargo build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully (may have unused warnings)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "feat: add DecoratedContainerRenderObject"
```

---

## Task 3: Create DecoratedContainerElement

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs` (Element part)

- [ ] **Step 1: Write DecoratedContainerElement struct and impl**

Add to `vexo/src/retain/widgets/decorated_container.rs` after the RenderObject:

```rust
// ============================================================================
// DecoratedContainerElement
// ============================================================================

/// Element for DecoratedContainer widget.
///
/// Manages a single child element and updates the render object
/// when style changes.
pub struct DecoratedContainerElement<M: Clone + Send + 'static = ()> {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    child_element: Option<ElementId>,
}

impl<M: Clone + Send + 'static> DecoratedContainerElement<M> {
    /// Create a new decorated container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            child_element: None,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget<M>) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }

    /// Get the child element ID.
    pub fn child_element(&self) -> Option<ElementId> {
        self.child_element
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget<M>> {
        self.widget.as_ref()?.child()
    }
}

impl<M: Clone + Send + 'static> Default for DecoratedContainerElement<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Clone + Send + 'static> Element for DecoratedContainerElement<M> {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(context.element_id);

        // Register global key if present
        if let Some(WidgetKey::Global(key)) = &self.key {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

        // Create render object if widget is set
        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);

                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget<M>>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties from the widget
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                }
            }
        }

        // Mark render objects dirty
        if let Some(ro) = self.render_object {
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

        // Remove render object from registry
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // DecoratedContainer doesn't handle events itself
        None
    }

    fn add_child(&mut self, child_id: ElementId) {
        self.child_element = Some(child_id);
    }

    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget<M>>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                }
            }

            // Reconcile single child if present
            if let Some(child_widget) = self.get_child_widget() {
                if let Some(child_id) = self.child_element {
                    // Take the element_registry to avoid double borrow
                    let element_registry = context.element_registry.take();
                    if let Some(registry) = element_registry {
                        if let Some(child_element) = registry.get_mut(child_id) {
                            let widget_any = Box::new(child_widget.clone_box());
                            child_element.rebuild(widget_any, context);
                        }
                        // Restore the registry
                        context.element_registry = Some(registry);
                    }
                }
            }
        }

        // Mark render objects dirty
        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn has_children(&self) -> bool {
        self.child_element.is_some()
    }
}
```

- [ ] **Step 2: Run cargo build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "feat: add DecoratedContainerElement"
```

---

## Task 4: Create DecoratedContainer Widget

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs` (Widget part)

- [ ] **Step 1: Write DecoratedContainer widget struct and impl**

Add to `vexo/src/retain/widgets/decorated_container.rs` after the Element:

```rust
// ============================================================================
// DecoratedContainer Widget
// ============================================================================

/// A widget that decorates a child with visual styling.
///
/// Creates a single element and render object regardless of how many
/// decorations are applied. This is more efficient than chaining
/// multiple modifier widgets (Background, Border, CornerRadius).
///
/// # Performance
///
/// | Approach | Elements | Render Objects |
/// |----------|----------|----------------|
/// | Chained modifiers | N | N |
/// | DecoratedContainer | 1 | 1 |
///
/// # Example
///
/// ```ignore
/// DecoratedContainer::new(Text::new("Hello").boxed())
///     .style(Style::new()
///         .background(Color::RED)
///         .border(Color::BLACK, 2.0)
///         .corner_radius(8.0))
/// ```
pub struct DecoratedContainer<M: Clone + Send + 'static = ()> {
    key: Option<WidgetKey>,
    child: Box<dyn Widget<M>>,
    style: Style,
}

impl<M: Clone + Send + 'static> DecoratedContainer<M> {
    /// Create a new decorated container with a child.
    pub fn new(child: Box<dyn Widget<M>>) -> Self {
        Self {
            key: None,
            child,
            style: Style::default(),
        }
    }

    /// Set the style for this container.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the key for this container.
    ///
    /// Accepts both local keys (strings) and global keys.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget<M> {
        self.child.as_ref()
    }

    /// Get the style.
    pub fn style_ref(&self) -> &Style {
        &self.style
    }
}

impl<M: Clone + Send + 'static> Clone for DecoratedContainer<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            style: self.style.clone(),
        }
    }
}

impl<M: Clone + Send + 'static> Widget<M> for DecoratedContainer<M> {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = DecoratedContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(DecoratedContainerRenderObject::new(self.style.clone()))
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget<M>> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) {
        if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<DecoratedContainerRenderObject>() {
            container_ro.set_style(self.style.clone());
        }
    }
}
```

- [ ] **Step 2: Run cargo build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "feat: add DecoratedContainer widget"
```

---

## Task 5: Export DecoratedContainer and Style

**Files:**
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Add module declaration and exports**

Add to `vexo/src/retain/widgets/mod.rs`:

```rust
mod decorated_container;

pub use decorated_container::DecoratedContainer;
```

- [ ] **Step 2: Run cargo build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/mod.rs
git commit -m "feat: export DecoratedContainer from widgets module"
```

---

## Task 6: Add Unit Tests for Style

**Files:**
- Modify: `vexo/src/retain/style.rs`

- [ ] **Step 1: Add tests module to style.rs**

Add to `vexo/src/retain/style.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_new() {
        let style = Style::new();

        assert!(style.background.is_none());
        assert!(style.border.is_none());
        assert!(style.corner_radius.is_none());
    }

    #[test]
    fn test_style_default() {
        let style = Style::default();

        assert!(style.background.is_none());
        assert!(style.border.is_none());
        assert!(style.corner_radius.is_none());
    }

    #[test]
    fn test_style_builder_background() {
        let style = Style::new().background(Color::RED);

        assert_eq!(style.background, Some(Color::RED));
    }

    #[test]
    fn test_style_builder_border() {
        let style = Style::new().border(Color::BLACK, 2.0);

        let border = style.border.unwrap();
        assert_eq!(border.color, Color::BLACK);
        assert_eq!(border.width, 2.0);
    }

    #[test]
    fn test_style_builder_corner_radius() {
        let style = Style::new().corner_radius(8.0);

        let cr = style.corner_radius.unwrap();
        assert_eq!(cr.radius, 8.0);
    }

    #[test]
    fn test_style_builder_all_properties() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0);

        assert_eq!(style.background, Some(Color::RED));
        assert_eq!(style.border.unwrap().color, Color::BLACK);
        assert_eq!(style.corner_radius.unwrap().radius, 8.0);
    }

    #[test]
    fn test_style_clone() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let cloned = style.clone();

        assert_eq!(cloned.background, Some(Color::RED));
        assert_eq!(cloned.border.unwrap().color, Color::BLACK);
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo style`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/style.rs
git commit -m "test: add unit tests for Style"
```

---

## Task 7: Add Unit Tests for DecoratedContainer

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs`

- [ ] **Step 1: Add tests module to decorated_container.rs**

Add to `vexo/src/retain/widgets/decorated_container.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::Text;

    #[test]
    fn test_decorated_container_creation() {
        let container: DecoratedContainer<()> = DecoratedContainer::new(
            Box::new(Text::new("Hello"))
        );

        assert!(container.key().is_none());
    }

    #[test]
    fn test_decorated_container_with_key() {
        let container: DecoratedContainer<()> = DecoratedContainer::new(
            Box::new(Text::new("Hello"))
        ).with_key("my-container");

        assert_eq!(container.key(), Some(WidgetKey::Local(Key::new("my-container"))));
    }

    #[test]
    fn test_decorated_container_with_global_key() {
        let global_key = GlobalKey::new();
        let container: DecoratedContainer<()> = DecoratedContainer::new(
            Box::new(Text::new("Hello"))
        ).with_key(global_key.clone());

        assert_eq!(container.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_decorated_container_with_style() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let container: DecoratedContainer<()> = DecoratedContainer::new(
            Box::new(Text::new("Hello"))
        ).style(style);

        assert_eq!(container.style_ref().background, Some(Color::RED));
    }

    #[test]
    fn test_decorated_container_render_object_creation() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let container: DecoratedContainer<()> = DecoratedContainer::new(
            Box::new(Text::new("Hello"))
        ).style(style);

        let ro = container.create_render_object();

        // Should be able to downcast to DecoratedContainerRenderObject
        assert!(ro.as_any().downcast_ref::<DecoratedContainerRenderObject>().is_some());
    }

    #[test]
    fn test_decorated_container_render_object_paint() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 2 commands (background + border)
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_decorated_container_render_object_paint_with_corner_radius() {
        let style = Style::new()
            .background(Color::RED)
            .corner_radius(8.0);

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 3 commands (push radius + background + pop radius)
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_decorated_container_render_object_paint_empty() {
        let style = Style::new(); // No decorations

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 0 commands (no decorations)
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn test_decorated_container_render_object_set_style() {
        let style1 = Style::new().background(Color::RED);
        let mut ro = DecoratedContainerRenderObject::new(style1);

        assert_eq!(ro.style().background, Some(Color::RED));

        let style2 = Style::new().background(Color::BLUE);
        ro.set_style(style2);

        assert_eq!(ro.style().background, Some(Color::BLUE));
    }

    #[test]
    fn test_decorated_container_element_default() {
        let element: DecoratedContainerElement<()> = DecoratedContainerElement::default();

        assert!(element.id().is_none());
        assert!(element.child_element().is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo decorated_container`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "test: add unit tests for DecoratedContainer"
```

---

## Task 8: Run Full Test Suite

**Files:**
- None (verification only)

- [ ] **Step 1: Run all vexo tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 2: Run desktop demo to verify**

Run: `cargo run -p desktop_demo`
Expected: Application runs without errors

- [ ] **Step 3: Final commit if needed**

```bash
git status
# If any uncommitted changes:
git add -A
git commit -m "chore: final cleanup for DecoratedContainer"
```

---

## Summary

This implementation adds a Flutter-style DecoratedContainer widget that:

1. **Style struct** - Holds all decoration properties (background, border, corner_radius) in one place
2. **DecoratedContainer widget** - Takes a child and Style, creates single element/render object
3. **DecoratedContainerElement** - Single element managing one child
4. **DecoratedContainerRenderObject** - Single render object painting all decorations in one pass

### Performance Improvement

| Scenario | Before (Nested Modifiers) | After (DecoratedContainer) |
|----------|---------------------------|---------------------------|
| 3 decorations | 3 elements, 3 render objects | 1 element, 1 render object |
| Reconciliation | Update 3 elements | Update 1 element |
| Paint | 3 separate passes | 1 combined pass |

### Migration Example

```rust
// Before (3 elements, 3 render objects)
Text::new("Hello")
    .background(Color::RED)
    .border(Color::BLACK, 2.0)
    .corner_radius(8.0)

// After (1 element, 1 render object)
DecoratedContainer::new(Text::new("Hello").boxed())
    .style(Style::new()
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0))
```