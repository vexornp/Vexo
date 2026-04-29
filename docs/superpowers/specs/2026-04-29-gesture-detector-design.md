# GestureDetector Widget for Retain Mode

**Date:** 2026-04-29
**Status:** Design Approved

## Context

The retain mode `Button<M>` widget stores a single message for click actions. For use cases requiring separate press and release events (e.g., drag start/end, visual feedback on press), we need a widget that can emit different messages for each event.

Flutter solves this with `GestureDetector`, a modifier widget that wraps any child and provides fine-grained gesture callbacks (`onTapDown`, `onTapUp`, `onTap`, `onTapCancel`). This design follows that pattern.

## Decision

Create `GestureDetector<M>` as a modifier widget that wraps a child and emits typed messages for pointer press and release events. It is invisible (no visual rendering) and follows the existing modifier widget pattern.

## Architecture

### Widget

```rust
pub struct GestureDetector<M: Clone + Send + 'static> {
    key: Option<Key>,
    child: Box<dyn Widget<M>>,
    on_press: Option<M>,
    on_release: Option<M>,
}
```

**Builder API:**
```rust
impl<M: Clone + Send + 'static> GestureDetector<M> {
    pub fn new(child: Box<dyn Widget<M>>) -> Self;

    pub fn with_key(mut self, key: impl Into<Key>) -> Self;
    pub fn on_press(mut self, message: M) -> Self;
    pub fn on_release(mut self, message: M) -> Self;
}
```

**Usage examples:**
```rust
// Press-only tracking
GestureDetector::new(child)
    .on_press(Message::Pressed)

// Press and release (drag pattern)
GestureDetector::new(child)
    .on_press(Message::DragStart)
    .on_release(Message::DragEnd)

// Release-only (tap pattern)
GestureDetector::new(child)
    .on_release(Message::Tapped)
```

### Element

`GestureDetectorElement<M>` handles event dispatch:

```rust
pub struct GestureDetectorElement<M: Clone + Send + 'static> {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    on_press: Option<M>,
    on_release: Option<M>,
}
```

**Event handling logic:**
```rust
fn on_event(&mut self, event: &InputEvent, context: &mut EventContext) -> Option<Box<dyn Any>> {
    match event {
        InputEvent::PointerButton { state, .. } => {
            if context.is_pointer_inside() {
                match state {
                    ButtonState::Pressed => {
                        if let Some(msg) = &self.on_press {
                            return Some(Box::new(msg.clone()));
                        }
                    }
                    ButtonState::Released => {
                        if let Some(msg) = &self.on_release {
                            return Some(Box::new(msg.clone()));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}
```

### Render Object

`GestureDetectorRenderObject` is a pass-through modifier (invisible):

```rust
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}
```

**Behavior:**
- `layout()` - Returns child's layout node (pass-through)
- `paint()` - Returns empty `Vec<RenderCommand>` (invisible)
- `hit_test()` - Uses computed bounds from child's layout

### Widget Trait Implementation

```rust
impl<M: Clone + Send + 'static> Widget<M> for GestureDetector<M> {
    fn key(&self) -> Option<Key>;
    fn create_element(&self) -> Box<dyn Element>;
    fn create_render_object(&self) -> Box<dyn RenderObject>;
    fn clone_box(&self) -> Box<dyn Widget<M>>;
    fn as_any(&self) -> &dyn Any;
    fn child(&self) -> Option<&dyn Widget<M>> { Some(self.child.as_ref()) }
}
```

## Integration with Existing Code

### Modifier Widget Pattern

GestureDetector follows the same pattern as existing modifiers (`Background`, `Border`, `CornerRadius`):
- Wraps a single child: `Box<dyn Widget<M>>`
- Uses `ModifierElement` or custom element for state
- Render object is pass-through for layout, custom for paint/hit_test

### Message Type Consistency

Child and GestureDetector share the same message type `M`. This is consistent with how `Background`, `Border` work (they use `Widget<()>` for non-interactive, but the pattern is the same).

### Pipeline Integration

No changes needed to `ThreeTreePipeline`. The element's `on_event()` returns `Option<Box<dyn Any>>`, which the pipeline already handles.

## Files to Create/Modify

| File | Change |
|------|--------|
| `vexo/src/retain/widgets/gesture_detector.rs` | New file - widget, element, render object |
| `vexo/src/retain/widgets/mod.rs` | Add `mod gesture_detector;` and `pub use gesture_detector::GestureDetector;` |

## Testing

Unit tests for:
1. Widget creation with builder methods
2. Element emits correct message on press
3. Element emits correct message on release
4. Element returns None when pointer is outside bounds
5. Render object layout pass-through
6. Render object paint returns empty

## Out of Scope

- `on_tap` (press + release on same element) - user can implement via press/release
- `on_tap_cancel` (pointer moved away) - not needed for current use cases
- `on_long_press`, `on_double_tap`, drag events - future enhancement
