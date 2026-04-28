# Retain Mode Input Event Handling Design

## Overview

Add message-passing input event handling to the retain-mode three-tree architecture. Elements handle events and return `Option<Box<dyn Any>>`, which `WindowState` downcasts to `A::Message` and passes to `Application::update()`.

## Goals

- Enable input event handling in retain mode
- Maintain consistency with immediate mode's message-passing pattern
- Support focus management for keyboard input
- Keep the design simple and extensible

## Non-Goals

- Event bubbling (can be added later if needed)
- Gesture recognizers (higher-level abstraction)
- Multi-touch handling

## Architecture

### Event Flow

```
winit::WindowEvent
       ↓
InputEvent::from_winit()
       ↓
WindowState.process_input_event()
       ↓
   [retain mode?]
       ↓
ThreeTreePipeline.handle_event(position, event)
       ↓
RenderObjectRegistry.hit_test(position)
       ↓
HitTestResult → ElementId
       ↓
Element.on_event(event, context)
       ↓
Option<Box<dyn Any>>
       ↓
WindowState downcasts to A::Message
       ↓
Application::update(state, message)
```

### Key Components

#### 1. EventContext

Provides context during event handling:

```rust
pub struct EventContext<'a> {
    /// Current pointer position in logical coordinates
    pub pointer_position: Point<Logical>,
    /// Currently focused element (if any)
    pub focused_element: Option<ElementId>,
    /// Bounds of the element receiving the event
    pub bounds: Bounds<Logical>,
    /// State storage for element-local state
    pub state: &'a mut StateStorage,
}
```

#### 2. Element Trait Extension

Add `on_event()` method to the `Element` trait:

```rust
pub trait Element {
    // ... existing methods ...

    /// Handle an input event.
    ///
    /// Returns `Some(message)` if the event was handled and produces a message.
    /// The message is type-erased as `Box<dyn Any>` and will be downcast
    /// by `WindowState` to the application's message type.
    ///
    /// Default implementation returns `None` (no interaction).
    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        None
    }
}
```

#### 3. ThreeTreePipeline Enhancement

Add event handling and focus management:

```rust
pub struct ThreeTreePipeline {
    element_registry: ElementRegistry,
    render_objects: RenderObjectRegistry,
    state: StateStorage,
    dirty: DirtyTracking,

    /// Currently focused element (for keyboard events)
    focused_element: Option<ElementId>,
}

impl ThreeTreePipeline {
    /// Handle an input event.
    ///
    /// For pointer events, performs hit testing to find the target element.
    /// For keyboard events, dispatches to the focused element.
    ///
    /// Returns `Some(message)` if the event was handled.
    pub fn handle_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerMoved { position } => {
                self.handle_pointer_event(*position, event)
            }
            InputEvent::PointerButton { position, .. } => {
                self.handle_pointer_event(*position, event)
            }
            InputEvent::Keyboard { .. } => {
                self.handle_keyboard_event(event)
            }
            _ => None,
        }
    }

    fn handle_pointer_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
    ) -> Option<Box<dyn Any>> {
        // 1. Hit test to find target
        let hit_result = self.render_objects.hit_test(position);

        // 2. Get target element
        let target_element = hit_result.target_element()?;

        // 3. Get render object bounds for context
        let target_render = hit_result.target()?;
        let bounds = self.render_objects.get(target_render)
            .and_then(|obj| obj.computed_bounds())
            .unwrap_or_default();

        // 4. Create event context
        let mut ctx = EventContext {
            pointer_position: position,
            focused_element: self.focused_element,
            bounds,
            state: &mut self.state,
        };

        // 5. Dispatch to element
        self.element_registry.get_mut(target_element)?
            .on_event(event, &mut ctx)
    }

    fn handle_keyboard_event(&mut self, event: &InputEvent) -> Option<Box<dyn Any>> {
        // Get focused element
        let focused = self.focused_element?;

        // Get bounds (not critical for keyboard events)
        let bounds = Bounds::default();

        let mut ctx = EventContext {
            pointer_position: Point::zero(),
            focused_element: self.focused_element,
            bounds,
            state: &mut self.state,
        };

        self.element_registry.get_mut(focused)?
            .on_event(event, &mut ctx)
    }
}
```

#### 4. WindowState Integration

Add retain-mode event processing:

```rust
impl<A: Application + 'static> WindowState<A> {
    fn process_input_event(&mut self, input_event: InputEvent) {
        if self.use_retain_mode && self.view_retain().is_some() {
            self.process_input_event_retain(input_event);
        } else {
            self.process_input_event_immediate(input_event);
        }
    }

    fn process_input_event_retain(&mut self, input_event: InputEvent) {
        let position = match &input_event {
            InputEvent::PointerMoved { position } => *position,
            InputEvent::PointerButton { position, .. } => *position,
            _ => Point::new(0.0, 0.0),
        };

        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return,
        };

        let message = pipeline.handle_event(position, &input_event);

        if let Some(msg) = message {
            // Downcast to A::Message and call update
            if let Some(typed_msg) = msg.downcast_ref::<A::Message>() {
                self.update(typed_msg.clone());
            } else {
                // Type mismatch - log warning
                eprintln!("Warning: Element returned message of wrong type");
            }
        }
    }
}
```

## Element Implementations

### LeafElement (Text, etc.)

Default implementation returns `None` - no interaction:

```rust
impl Element for LeafElement {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        None
    }
}
```

### ContainerElement (Column, Row)

Containers don't handle events themselves. Children are hit-tested individually:

```rust
impl Element for ContainerElement {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // Container itself doesn't handle events
        // Hit testing finds the specific child element
        None
    }
}
```

### ModifierElement (Background, Border, etc.)

Modifiers delegate to their child:

```rust
impl Element for ModifierElement {
    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // Modifiers don't handle events themselves
        // The hit test already found the correct target
        None
    }
}
```

## Focus Management

### Focus State

`ThreeTreePipeline` tracks focus:

```rust
impl ThreeTreePipeline {
    /// Get the currently focused element.
    pub fn focused_element(&self) -> Option<ElementId> {
        self.focused_element
    }

    /// Set focus to an element.
    pub fn set_focus(&mut self, element: Option<ElementId>) {
        self.focused_element = element;
    }
}
```

### Focus Requests

Elements can request focus via `EventContext`:

```rust
impl<'a> EventContext<'a> {
    /// Request focus for an element.
    pub fn request_focus(&mut self, element: ElementId) {
        // Store the request for the pipeline to process
        self.focus_request = Some(element);
    }

    /// Check if this element is focused.
    pub fn is_focused(&self, element: ElementId) -> bool {
        self.focused_element == Some(element)
    }
}
```

### Focus on Click

When a pointer press occurs and no element handles it, focus is cleared:

```rust
fn handle_pointer_event(&mut self, position: Point<Logical>, event: &InputEvent) -> Option<Box<dyn Any>> {
    // ... hit test and dispatch ...

    // If event not handled and it's a press, clear focus
    if message.is_none() {
        if let InputEvent::PointerButton { state: ButtonState::Pressed, .. } = event {
            self.focused_element = None;
        }
    }

    message
}
```

## Type Safety

### Message Type Matching

The `Box<dyn Any>` approach requires runtime type checking:

```rust
// In WindowState
if let Some(msg) = message {
    if let Some(typed_msg) = msg.downcast_ref::<A::Message>() {
        self.update(typed_msg.clone());
    } else {
        // This indicates a bug: an element returned a message
        // that doesn't match the application's message type
        eprintln!("Warning: Element returned message of wrong type. Expected {}", std::any::type_name::<A::Message>());
    }
}
```

### Why Not Generic Element<M>?

A generic `Element<M>` trait would provide compile-time safety but introduces complexity:

1. **Heterogeneous storage**: `ElementRegistry` would need `Box<dyn Element<M>>` for each `M`
2. **Trait bounds**: Every function touching elements would need `<M>` bounds
3. **Message type propagation**: The entire pipeline would need to be generic over `M`

The `Box<dyn Any>` approach keeps the trait simple and isolates type checking to one location.

## Testing Strategy

### Unit Tests

1. **EventContext creation** - Verify context is correctly populated
2. **Element on_event()** - Test each element type returns expected result
3. **Focus management** - Test focus set/clear/is_focused

### Integration Tests

1. **Hit test + dispatch** - Click on element, verify event reaches element
2. **Message emission** - Element returns message, verify it reaches update
3. **Focus tracking** - Click on focusable element, verify focus changes
4. **Keyboard routing** - Press key, verify focused element receives event

### Example Test

```rust
#[test]
fn test_click_emits_message() {
    // Create a clickable element that emits a message
    struct ClickableElement;

    impl Element for ClickableElement {
        fn on_event(&mut self, event: &InputEvent, _: &mut EventContext) -> Option<Box<dyn Any>> {
            match event {
                InputEvent::PointerButton { state: ButtonState::Pressed, .. } => {
                    Some(Box::new("clicked".to_string()))
                }
                _ => None,
            }
        }
        // ... other trait methods ...
    }

    // Create pipeline with clickable element
    let mut pipeline = ThreeTreePipeline::new();
    // ... mount element ...

    // Simulate click
    let event = InputEvent::PointerButton {
        position: Point::new(10.0, 10.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let message = pipeline.handle_event(Point::new(10.0, 10.0), &event);

    assert!(message.is_some());
    assert_eq!(message.unwrap().downcast_ref::<String>().unwrap(), "clicked");
}
```

## File Changes

| File | Change |
|------|--------|
| `vexo/src/retain/event_context.rs` | **New** - `EventContext` struct |
| `vexo/src/retain/mod.rs` | Export `EventContext` |
| `vexo/src/retain/element.rs` | Add `on_event()` to `Element` trait with default impl |
| `vexo/src/retain/pipeline.rs` | Add `handle_event()`, `focused_element` field |
| `vexo/src/retain/elements/leaf.rs` | Implement `on_event()` (returns None) |
| `vexo/src/retain/elements/container.rs` | Implement `on_event()` (returns None) |
| `vexo/src/retain/elements/modifier.rs` | Implement `on_event()` (returns None) |
| `vexo/src/window.rs` | Add `process_input_event_retain()`, modify `process_input_event()` |

## Future Enhancements

1. **Event bubbling** - Allow events to propagate up the tree
2. **Gesture recognizers** - Higher-level abstractions (Tap, LongPress, Drag)
3. **Hover state** - Track pointer enter/leave for elements
4. **Cursor icons** - Allow elements to specify cursor on hover

## References

- Flutter's gesture system: https://docs.flutter.dev/ui/interactivity/gestures
- Immediate mode `Widget::on_event()` pattern in `vexo/src/widgets/mod.rs`
- Hit testing in `vexo/src/retain/hit_test.rs`
