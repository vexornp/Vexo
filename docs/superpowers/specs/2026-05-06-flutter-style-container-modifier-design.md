# Flutter-Style Container Modifier System Design

**Date:** 2026-05-06
**Status:** Approved
**Author:** Claude

## Summary

Refactor Vexo's retain-mode modifier system from nested wrapper widgets to Flutter's Container + Style pattern. This reduces element and render object count by ~66% for typical decoration chains, improving performance and simplifying reconciliation.

## Motivation

### Current Problem

Vexo's current modifier system uses nested wrapper widgets:

```rust
Text::new("Hello")
    .background(Color::RED)
    .border(Color::BLACK, 2.0)
    .corner_radius(8.0)
```

This creates:
- `CornerRadius<Border<Background<Text>, M>, M>`
- 3 separate elements
- 3 separate render objects
- 3 layout passes
- 3 paint passes

### Flutter's Solution

Flutter bundles decorations into a single `BoxDecoration` object:

```dart
Container(
  decoration: BoxDecoration(
    color: Colors.red,
    border: Border.all(color: Colors.black, width: 2),
    borderRadius: BorderRadius.circular(8),
  ),
  child: Text('Hello'),
)
```

This creates:
- 1 element (`RenderObjectWidget`)
- 1 render object
- 1 layout pass
- 1 paint pass (all decorations drawn together)

### Goal

Adopt Flutter's pattern to minimize element/render object count and improve performance.

## Architecture

### Core Types

```
┌─────────────────────────────────────────────────────────────────┐
│                    Style (Immutable Data)                       │
│  Holds all decoration properties: background, border, radius   │
│  Builder pattern for fluent construction                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ applied to
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Container Widget                             │
│  Takes child + optional Style                                   │
│  Creates ContainerElement                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ create_element()
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ContainerElement                             │
│  Single element managing one child                              │
│  Updates render object when style changes                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ create_render_object()
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ContainerRenderObject                        │
│  Single render object handling all decorations                  │
│  Paints background, border with corner radius in one pass       │
└─────────────────────────────────────────────────────────────────┘
```

### Comparison: Before vs After

| Aspect | Before (Nested Modifiers) | After (Container + Style) |
|--------|---------------------------|---------------------------|
| Element count | N (one per modifier) | 1 |
| Render object count | N (one per modifier) | 1 |
| Layout passes | N | 1 |
| Paint passes | N | 1 |
| Reconciliation | Update N elements | Update 1 element |
| Type complexity | Nested generics | Simple Container<M> |

**Example:** 3 decorations → 75% reduction in element/render object count.

## Design Details

### Style Struct

```rust
/// Visual decoration properties for a Container.
///
/// Analogous to Flutter's BoxDecoration - holds all visual properties
/// in one place for efficient single-pass rendering.
#[derive(Clone, Debug, Default)]
pub struct Style {
    /// Background color (drawn behind child)
    pub background: Option<Color>,

    /// Border decoration
    pub border: Option<Border>,

    /// Corner radius for rounded rectangles
    pub corner_radius: Option<CornerRadius>,
}

/// Border decoration properties.
#[derive(Clone, Debug)]
pub struct Border {
    pub color: Color,
    pub width: f32,
}

/// Corner radius for rounded rectangles.
#[derive(Clone, Debug)]
pub struct CornerRadius {
    /// Radius for all corners (uniform)
    pub radius: f32,
}
```

### Style Builder API

```rust
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

    /// Set border.
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(Border { color, width });
        self
    }

    /// Set uniform corner radius.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(CornerRadius { radius });
        self
    }
}
```

### Container Widget

```rust
/// A widget that decorates a child with visual styling.
///
/// Creates a single element and render object regardless of how many
/// decorations are applied. This is more efficient than chaining
/// multiple modifier widgets.
///
/// # Example
///
/// ```ignore
/// Container::new(Text::new("Hello").boxed())
///     .style(Style::new()
///         .background(Color::RED)
///         .border(Color::BLACK, 2.0)
///         .corner_radius(8.0))
/// ```
pub struct Container<M: Clone + Send + 'static = ()> {
    key: Option<WidgetKey>,
    child: Box<dyn Widget<M>>,
    style: Style,
}

impl<M: Clone + Send + 'static> Container<M> {
    /// Create a new container with a child.
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
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}
```

### ContainerRenderObject

```rust
/// Render object for Container - handles all decorations in a single pass.
pub struct ContainerRenderObject {
    /// Current style configuration
    style: Style,

    /// Child render object ID
    child: Option<RenderObjectId>,

    /// Computed bounds from layout
    computed_bounds: Option<Bounds<Logical>>,

    /// Layout node in Taffy
    layout_node: Option<LayoutNodeId>,
}

impl ContainerRenderObject {
    /// Create a new container render object with the given style.
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
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // Container is a pass-through for layout - uses child's bounds
        match child_nodes.first() {
            Some(child_node) => {
                self.layout_node = Some(*child_node);
                LayoutResult { node: *child_node, size: Size::zero() }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult { node, size: Size::zero() }
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
        let pos = ctx.absolute_position();

        let absolute_bounds = Bounds::new(
            pos.x, pos.y,
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

### ContainerElement

```rust
/// Element for Container widget.
///
/// Manages a single child element and updates the render object
/// when style changes.
pub struct ContainerElement<M: Clone + Send + 'static = ()> {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    child_element: Option<ElementId>,
}

impl<M: Clone + Send + 'static> ContainerElement<M> {
    /// Create a new container element.
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

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget<M>> {
        self.widget.as_ref()?.child()
    }
}

impl<M: Clone + Send + 'static> Element for ContainerElement<M> {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(context.element_id);

        if let Some(WidgetKey::Global(key)) = &self.key {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

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

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                }
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget<M>>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                }
            }

            // Reconcile child
            if let Some(child_widget) = self.get_child_widget() {
                if let Some(child_id) = self.child_element {
                    if let Some(registry) = &mut context.element_registry {
                        if let Some(child_element) = registry.get_mut(child_id) {
                            let widget_any = Box::new(child_widget.clone_box());
                            child_element.rebuild(widget_any, context);
                        }
                    }
                }
            }
        }

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn add_child(&mut self, child_id: ElementId) {
        self.child_element = Some(child_id);
    }

    fn has_children(&self) -> bool {
        self.child_element.is_some()
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(&mut self, _event: &InputEvent, _context: &mut EventContext) -> Option<Box<dyn Any>> {
        None
    }
}
```

### Widget Trait Implementation

```rust
impl<M: Clone + Send + 'static> Widget<M> for Container<M> {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.style.clone()))
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            style: self.style.clone(),
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget<M>> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) {
        if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<ContainerRenderObject>() {
            container_ro.set_style(self.style.clone());
        }
    }
}
```

## File Structure

### New Files

```
vexo/src/retain/widgets/
├── container.rs           # Container widget + ContainerElement + ContainerRenderObject
├── style.rs               # Style, Border, CornerRadius structs
```

### Modified Files

```
vexo/src/retain/widgets/mod.rs    # Export Container and Style
```

## Migration Guide

### Before (Nested Modifiers)

```rust
// Creates 3 elements, 3 render objects
Text::new("Hello")
    .background(Color::RED)
    .border(Color::BLACK, 2.0)
    .corner_radius(8.0)
```

### After (Container + Style)

```rust
// Creates 1 element, 1 render object
Container::new(Text::new("Hello").boxed())
    .style(Style::new()
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0))
```

### Deprecation of Old Modifiers

The old modifier widgets in `vexo/src/widgets/modifiers.rs` will be:

1. Kept for backward compatibility during migration
2. Marked as deprecated with documentation pointing to `Container`
3. Removed in a future version after migration is complete

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_style_builder() {
    let style = Style::new()
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0);

    assert_eq!(style.background, Some(Color::RED));
    assert_eq!(style.border.unwrap().color, Color::BLACK);
    assert_eq!(style.corner_radius.unwrap().radius, 8.0);
}

#[test]
fn test_container_render_object_paint() {
    let mut ro = ContainerRenderObject::new(Style::new()
        .background(Color::RED)
        .border(Color::BLACK, 2.0));

    ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

    let mut commands = Vec::new();
    let mut ctx = PaintContext::new(&mut commands);
    let cmds = ro.paint(&mut ctx);

    // Should have 2 commands (background + border)
    assert_eq!(cmds.len(), 2);
}

#[test]
fn test_container_element_single_render_object() {
    let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

    // Old way: 3 modifiers
    pipeline.reconcile(Box::new(
        Text::new("Hello")
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0)
    ));
    let old_count = pipeline.render_object_count();

    // New way: single container
    pipeline.reconcile(Box::new(
        Container::new(Text::new("Hello").boxed())
            .style(Style::new()
                .background(Color::RED)
                .border(Color::BLACK, 2.0)
                .corner_radius(8.0))
    ));
    let new_count = pipeline.render_object_count();

    // Should have fewer render objects
    assert!(new_count < old_count);
}
```

### Integration Tests

```rust
#[test]
fn test_container_visual_output() {
    // Visual test: Container with all decorations should render correctly
    let mut window = TestWindow::new();

    let widget = Container::new(Text::new("Hello").boxed())
        .style(Style::new()
            .background(Color::WHITE)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0));

    window.render_frame(widget);

    // Verify: background rect drawn, border drawn, corner radius applied
    assert!(window.has_rect_with_color(Color::WHITE));
    assert!(window.has_border_with_color(Color::BLACK));
}
```

## Performance Impact

### Benchmark Scenario

Container with 3 decorations (background, border, corner radius):

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Element count | 3 | 1 | 66% reduction |
| Render object count | 3 | 1 | 66% reduction |
| Reconciliation updates | 3 elements | 1 element | 66% reduction |
| Paint commands | 3 separate passes | 1 combined pass | Batching opportunity |

### Memory Impact

- Each element: ~64 bytes (id, widget, render_object, child_element)
- Each render object: ~128 bytes (style, bounds, layout_node, child)
- Savings: 2 elements + 2 render objects = ~384 bytes per decorated widget

## Future Extensions

### Potential Additions to Style

```rust
pub struct Style {
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub corner_radius: Option<CornerRadius>,
    pub padding: Option<EdgeInsets>,      // Future: affects layout
    pub shadow: Option<BoxShadow>,        // Future: drop shadows
    pub gradient: Option<Gradient>,       // Future: gradient backgrounds
}
```

### Per-Corner Radius

```rust
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}
```

## Scope

**In scope:**
- Style struct with background, border, corner_radius
- Container widget with Style parameter
- ContainerElement and ContainerRenderObject
- Unit tests
- Migration documentation

**Out of scope:**
- Padding (affects layout, separate concern)
- Shadows, gradients (future extensions)
- Removing old modifiers (backward compatibility)

## References

- Flutter Container: https://docs.flutter.dev/ui/widgets/container
- Flutter BoxDecoration: https://api.flutter.dev/flutter/painting/BoxDecoration-class.html
- Flutter DecoratedBox: https://docs.flutter.dev/ui/widgets/painting#decoratedbox