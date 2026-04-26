# ScrollView Widget Design

**Date:** 2026-04-26
**Status:** Draft
**Author:** Claude

## Context

Vexo currently lacks a scrollable container widget. Users cannot view content that exceeds the viewport bounds. This design adds a `ScrollView` widget following the container widget pattern used by `Column` and `Row`.

## Requirements

- **Vertical scrolling only** - content scrolls up/down
- **Static content** - all children rendered (no lazy loading)
- **Scrollbar indicator** - visual indicator of scroll position
- **User-driven scrolling** - no programmatic control initially
- **Multiple input methods**:
  - Scroll wheel (desktop)
  - Drag/pan gesture (touch)
  - Keyboard navigation (arrow keys, page up/down)

## Architecture

### Overview

`ScrollView<M>` is a container widget that:
1. Manages children as `Vec<Box<dyn Widget<M>>>`
2. Tracks scroll state in `ComponentStateStorage`
3. Clips content to viewport bounds during painting
4. Handles scroll events to update scroll offset
5. Renders scrollbar indicator

### File Location

`vexo/src/widgets/scroll_view.rs`

### Data Structures

```rust
/// Scroll state stored in ComponentStateStorage
#[derive(Default, Clone)]
pub struct ScrollState {
    /// Current vertical scroll offset (0 = top, positive = scrolled down)
    pub offset_y: f32,
    /// Whether user is currently dragging to scroll
    pub is_dragging: bool,
    /// Y position where drag started
    pub drag_start_y: f32,
    /// Scroll offset when drag started
    pub drag_start_offset: f32,
}

/// ScrollView widget - a vertical scrollable container
pub struct ScrollView<M: Clone + std::fmt::Debug + Send> {
    /// Child widgets
    children: Vec<Box<dyn Widget<M>>>,
    /// Optional key for state persistence
    key: Option<String>,
    /// Layout properties for the viewport
    layout: Layout,
    /// Computed viewport bounds
    computed_layout: Option<ComputedLayout>,
    /// Computed content height (sum of children heights)
    content_height: f32,
    /// Scrollbar width in logical pixels
    scrollbar_width: f32,
    /// Whether scrollbar should always be visible
    always_show_scrollbar: bool,
    _marker: PhantomData<M>,
}
```

### Builder API

```rust
impl<M: Clone + std::fmt::Debug + Send> ScrollView<M> {
    pub fn new() -> Self;

    pub fn push(mut self, child: impl Widget<M> + 'static) -> Self;
    pub fn with_key(mut self, key: impl Into<String>) -> Self;
    pub fn with_layout(mut self, layout: Layout) -> Self;
    pub fn scrollbar_width(mut self, width: f32) -> Self;
}
```

## Implementation Details

### Layout Phase

1. Layout all children recursively, collecting `LayoutNodeId`s
2. Create container node with `FlexDirection::Column`
3. Store computed viewport bounds in `apply_layout()`
4. Calculate `content_height` from sum of children's heights

```rust
fn layout(&mut self, layout_ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
    let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
    for child in self.children.iter_mut() {
        child_nodes.push(child.layout(layout_ctx, widget_ctx));
    }

    let layout = Layout {
        flex_direction: Some(FlexDirection::Column),
        ..self.layout.clone()
    };

    layout_ctx.create_container(&layout, &child_nodes)
}
```

### Paint Phase

1. Push clip to viewport bounds
2. Push scroll offset (negative Y)
3. Paint children
4. Pop offset
5. Pop clip
6. Draw scrollbar if content exceeds viewport

```rust
fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
    let viewport_bounds = self.computed_layout?.bounds;
    let scroll_state = ctx.get_scroll_state(self.key.as_deref());
    let offset_y = scroll_state.offset_y;

    let mut commands = Vec::new();

    commands.push(RenderCommand::PushClip { bounds: viewport_bounds });
    commands.push(RenderCommand::PushOffset { offset: Point::new(0.0, -offset_y) });

    for child in &self.children {
        commands.extend(child.paint(ctx));
    }

    commands.push(RenderCommand::PopOffset);
    commands.push(RenderCommand::PopClip);

    if self.content_height > viewport_bounds.height() {
        commands.extend(self.paint_scrollbar(viewport_bounds, offset_y));
    }

    commands
}
```

### Scrollbar Rendering

- Position: right edge of viewport
- Height: proportional to viewport/content ratio
- Color: semi-transparent gray
- Shape: rounded corners

```rust
fn paint_scrollbar(&self, viewport: Rect, offset_y: f32) -> Vec<RenderCommand> {
    let max_scroll = self.content_height - viewport.height();
    let scroll_ratio = offset_y / max_scroll;
    let scrollbar_height = (viewport.height() * viewport.height() / self.content_height).min(viewport.height());
    let scrollbar_y = viewport.y() + scroll_ratio * (viewport.height() - scrollbar_height);

    vec![RenderCommand::Rect {
        bounds: Rect::from_xywh(
            viewport.x() + viewport.width() - self.scrollbar_width,
            scrollbar_y,
            self.scrollbar_width,
            scrollbar_height,
        ),
        fill: Color::rgba(0.5, 0.5, 0.5, 0.5),
        stroke: None,
        corner_radius: self.scrollbar_width / 2.0,
    }]
}
```

### Event Handling

#### Scroll Wheel

```rust
InputEvent::Scroll { delta } => {
    if max_scroll > 0.0 {
        scroll_state.offset_y = (scroll_state.offset_y + delta.y).clamp(0.0, max_scroll);
        return WidgetResponse { handled: true, ..Default::default() };
    }
}
```

#### Drag Gesture

```rust
// Start drag
InputEvent::PointerButton { position, state: ButtonState::Pressed, .. } => {
    if viewport_bounds.contains(position) && max_scroll > 0.0 {
        scroll_state.is_dragging = true;
        scroll_state.drag_start_y = position.y;
        scroll_state.drag_start_offset = scroll_state.offset_y;
        return WidgetResponse { handled: true, ..Default::default() };
    }
}

// Move during drag
InputEvent::PointerMoved { position } => {
    if scroll_state.is_dragging {
        let drag_delta = scroll_state.drag_start_y - position.y;
        scroll_state.offset_y = (scroll_state.drag_start_offset + drag_delta).clamp(0.0, max_scroll);
        return WidgetResponse { handled: true, ..Default::default() };
    }
}

// End drag
InputEvent::PointerButton { state: ButtonState::Released, .. } => {
    scroll_state.is_dragging = false;
}
```

#### Keyboard Navigation

```rust
InputEvent::Keyboard { key, state: ButtonState::Pressed, .. } => {
    match key {
        Key::Named(NamedKey::ArrowDown) => {
            scroll_state.offset_y = (scroll_state.offset_y + 20.0).clamp(0.0, max_scroll);
        }
        Key::Named(NamedKey::ArrowUp) => {
            scroll_state.offset_y = (scroll_state.offset_y - 20.0).clamp(0.0, max_scroll);
        }
        Key::Named(NamedKey::PageDown) => {
            scroll_state.offset_y = (scroll_state.offset_y + viewport_bounds.height()).clamp(0.0, max_scroll);
        }
        Key::Named(NamedKey::PageUp) => {
            scroll_state.offset_y = (scroll_state.offset_y - viewport_bounds.height()).clamp(0.0, max_scroll);
        }
        _ => {}
    }
}
```

### Event Propagation to Children

Events are propagated to children with scroll offset applied. Children outside the viewport are skipped for performance.

```rust
let adjusted_offset = Point::new(
    offset.x + viewport_bounds.x(),
    offset.y + viewport_bounds.y() - scroll_state.offset_y,
);

for (child, child_node) in self.children.iter_mut().zip(child_ids.iter()) {
    if let Some(child_layout) = layout_view.get_layout(*child_node) {
        let child_top = child_layout.bounds.y() - scroll_state.offset_y;
        let child_bottom = child_top + child_layout.bounds.height();

        if child_bottom >= 0.0 && child_top <= viewport_bounds.height() {
            let response = child.on_event(...);
            if response.handled {
                return response;
            }
        }
    }
}
```

## Usage Example

```rust
use vexo::widgets::{ScrollView, Text, Button, Column};

impl Application for MyApp {
    type Message = MyMessage;
    type State = Self;

    fn view(state: &Self) -> Box<dyn Widget<Self::Message>> {
        Box::new(
            ScrollView::new()
                .with_key("main-scroll")
                .with_layout(Layout::default().width(300.0).height(400.0))
                .push(Text::new("Header").with_font_size(24.0))
                .push(Column::new()
                    .push(Text::new("Item 1"))
                    .push(Text::new("Item 2"))
                    .push(Text::new("Item 3"))
                    .boxed())
                .push(Button::new("Load More", MyMessage::LoadMore))
                .background(Color::WHITE)
        )
    }
}
```

## Testing Strategy

### Unit Tests (Separated Traits)

```rust
#[test]
fn test_scroll_view_layout_constraints() {
    let scroll = ScrollView::<()>::new()
        .with_layout(Layout::default().width(200.0).height(300.0));

    let constraints = Layout::constraints(&scroll);
    assert_eq!(constraints.width, Some(200.0));
    assert_eq!(constraints.height, Some(300.0));
}

#[test]
fn test_scroll_view_paint_clips_content() {
    let mut scroll = ScrollView::<()>::new()
        .push(Text::new("Item 1"))
        .push(Text::new("Item 2"));

    scroll.apply_layout(ComputedLayout::new(0.0, 0.0, 100.0, 50.0));

    let mut ctx = PaintContext::new(Point::origin());
    let commands = scroll.paint(&mut ctx);

    assert!(commands.iter().any(|c| matches!(c, RenderCommand::PushClip { .. })));
    assert!(commands.iter().any(|c| matches!(c, RenderCommand::PopClip)));
}
```

### Integration Tests

```rust
#[test]
fn test_scroll_wheel_updates_offset() {
    let mut scroll = ScrollView::<()>::new()
        .push(Text::new("Long content"));

    scroll.apply_layout(ComputedLayout::new(0.0, 0.0, 100.0, 50.0));
    scroll.content_height = 200.0;

    let event = InputEvent::Scroll { delta: Point::new(0.0, 30.0) };
    // Verify offset is updated correctly
}
```

## Files to Modify

| File | Change |
|------|--------|
| `vexo/src/widgets/mod.rs` | Export `ScrollView` and `ScrollState` |
| `vexo/src/widgets/scroll_view.rs` | New file - ScrollView implementation |
| `vexo/src/lib.rs` | Re-export ScrollView if needed |

## Future Enhancements

Not in scope for initial implementation:

1. **Horizontal scrolling** - Add `ScrollDirection` enum
2. **Programmatic control** - `scroll_to()`, `animate_to()` methods
3. **Lazy loading** - `LazyScrollView` that only renders visible children
4. **Custom scrollbar styling** - Allow custom scrollbar widget
5. **Overscroll effects** - Bounce/glow effects at scroll boundaries
6. **Momentum scrolling** - Continue scrolling after gesture ends

## References

- SwiftUI `ScrollView` - declarative scroll container
- Flutter `SingleChildScrollView` - static content scrolling
- Jetpack Compose `verticalScroll` modifier - scroll via modifier
