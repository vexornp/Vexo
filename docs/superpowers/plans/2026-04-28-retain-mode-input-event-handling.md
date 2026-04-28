# Retain Mode Input Event Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add message-passing input event handling to the retain-mode three-tree architecture.

**Architecture:** Elements handle events via `on_event()` returning `Option<Box<dyn Any>>`. `ThreeTreePipeline.handle_event()` performs hit testing and dispatches to elements. `WindowState` downcasts messages to `A::Message` and calls `Application::update()`.

**Tech Stack:** Rust, existing retain-mode infrastructure (Element, RenderObject, ThreeTreePipeline)

---

## File Structure

| File | Responsibility |
|------|----------------|
| `vexo/src/retain/event_context.rs` | **New** - Context for event handling (position, focus, bounds) |
| `vexo/src/retain/mod.rs` | Export `EventContext` |
| `vexo/src/retain/element.rs` | Add `on_event()` to `Element` trait with default impl |
| `vexo/src/retain/render_object.rs` | Add `computed_bounds()` to `RenderObject` trait |
| `vexo/src/retain/pipeline.rs` | Add `handle_event()`, `focused_element` field, focus management |
| `vexo/src/retain/elements/leaf.rs` | Implement `on_event()` (returns None) |
| `vexo/src/retain/elements/container.rs` | Implement `on_event()` (returns None) |
| `vexo/src/retain/elements/modifier.rs` | Implement `on_event()` (returns None) |
| `vexo/src/window.rs` | Add `process_input_event_retain()`, modify `process_input_event()` |

---

### Task 1: Add computed_bounds() to RenderObject trait

**Files:**
- Modify: `vexo/src/retain/render_object.rs:156-220`

**Why:** The `EventContext` needs the bounds of the element receiving the event. `computed_bounds()` exists on concrete render object types but not on the trait. Adding it to the trait allows `handle_event()` to get bounds from `dyn RenderObject`.

- [ ] **Step 1: Add computed_bounds() method to RenderObject trait**

Add the method with a default implementation returning `None`:

```rust
// In vexo/src/retain/render_object.rs, in the RenderObject trait
// Add after the layout_node() method (around line 218)

    /// Get the computed bounds after layout.
    ///
    /// Returns `None` if layout has not been applied yet.
    /// Used by event handling to determine element bounds.
    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        None
    }
```

- [ ] **Step 2: Override computed_bounds() in TextRenderObject**

```rust
// In vexo/src/retain/render_objects/text.rs
// Add after the computed_bounds() method (around line 57)
// Change the existing method to satisfy the trait:

impl RenderObject for TextRenderObject {
    // ... existing methods ...

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 3: Override computed_bounds() in ContainerRenderObject**

```rust
// In vexo/src/retain/render_objects/container.rs
// Add after the computed_bounds() method (around line 66)
// Change the existing method to satisfy the trait:

impl RenderObject for ContainerRenderObject {
    // ... existing methods ...

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 4: Override computed_bounds() in BackgroundRenderObject**

```rust
// In vexo/src/retain/widgets/background.rs
// In the impl RenderObject block, add:

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
```

- [ ] **Step 5: Override computed_bounds() in BorderRenderObject**

```rust
// In vexo/src/retain/widgets/border.rs
// In the impl RenderObject block, add:

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
```

- [ ] **Step 6: Override computed_bounds() in CornerRadiusRenderObject**

```rust
// In vexo/src/retain/widgets/corner_radius.rs
// In the impl RenderObject block, add:

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
```

- [ ] **Step 7: Run tests to verify no breakage**

Run: `cargo test -p vexo --lib retain::`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add vexo/src/retain/render_object.rs vexo/src/retain/render_objects/ vexo/src/retain/widgets/
git commit -m "feat: add computed_bounds() to RenderObject trait"
```

---

### Task 2: Create EventContext struct

**Files:**
- Create: `vexo/src/retain/event_context.rs`
- Modify: `vexo/src/retain/mod.rs`

**Why:** `EventContext` provides context during event handling: pointer position, focus state, bounds, and state storage access.

- [ ] **Step 1: Create the EventContext struct**

Create file `vexo/src/retain/event_context.rs`:

```rust
//! Event context for input event handling.
//!
//! Provides context during element event handling.

use crate::core::{Bounds, Logical, Point};
use crate::input::Modifiers;

use super::{ElementId, StateStorage};

// ============================================================================
// EVENT CONTEXT
// ============================================================================

/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - Pointer position for hit testing
/// - Focus state for keyboard event routing
/// - Element bounds for position calculations
/// - State storage for element-local state
pub struct EventContext<'a> {
    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,

    /// Currently focused element (if any).
    pub focused_element: Option<ElementId>,

    /// Bounds of the element receiving the event.
    pub bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    pub modifiers: Modifiers,

    /// State storage for element-local state.
    pub state: &'a mut StateStorage,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementId>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,
}

impl<'a> EventContext<'a> {
    /// Create a new event context.
    pub fn new(
        pointer_position: Point<Logical>,
        focused_element: Option<ElementId>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        state: &'a mut StateStorage,
    ) -> Self {
        Self {
            pointer_position,
            focused_element,
            bounds,
            modifiers,
            state,
            focus_request: None,
            clear_focus_request: false,
        }
    }

    /// Check if the pointer is inside the element bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Check if this element is currently focused.
    pub fn is_focused(&self, element: ElementId) -> bool {
        self.focused_element == Some(element)
    }

    /// Check if any element has focus.
    pub fn has_focus(&self) -> bool {
        self.focused_element.is_some()
    }

    /// Request focus for an element.
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementId) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
    }

    /// Request to clear focus from the currently focused element.
    pub fn clear_focus(&mut self) {
        self.clear_focus_request = true;
        self.focus_request = None;
    }

    /// Get the focus request (if any).
    pub fn focus_request(&self) -> Option<ElementId> {
        self.focus_request
    }

    /// Check if the element requested to clear focus.
    pub fn should_clear_focus(&self) -> bool {
        self.clear_focus_request
    }

    /// Check if the control key is pressed.
    pub fn is_control_pressed(&self) -> bool {
        self.modifiers.control
    }

    /// Check if the shift key is pressed.
    pub fn is_shift_pressed(&self) -> bool {
        self.modifiers.shift
    }

    /// Check if the alt key is pressed.
    pub fn is_alt_pressed(&self) -> bool {
        self.modifiers.alt
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;

    #[test]
    fn test_event_context_is_pointer_inside() {
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::new(50.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
        );

        assert!(ctx.is_pointer_inside());

        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::new(150.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
        );

        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_event_context_focus() {
        let element = ElementId::new();
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::zero(),
            Some(element),
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        assert!(ctx.is_focused(element));
        assert!(ctx.has_focus());
        assert!(!ctx.is_focused(ElementId::new()));
    }

    #[test]
    fn test_event_context_focus_request() {
        let mut state = StateStorage::new();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        let element = ElementId::new();
        ctx.request_focus(element);

        assert_eq!(ctx.focus_request(), Some(element));
        assert!(!ctx.should_clear_focus());
    }

    #[test]
    fn test_event_context_clear_focus_request() {
        let mut state = StateStorage::new();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        ctx.clear_focus();

        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }

    #[test]
    fn test_event_context_modifiers() {
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::control(),
            &mut state,
        );

        assert!(ctx.is_control_pressed());
        assert!(!ctx.is_shift_pressed());
        assert!(!ctx.is_alt_pressed());
    }
}
```

- [ ] **Step 2: Export EventContext from mod.rs**

Add to `vexo/src/retain/mod.rs`:

```rust
// In vexo/src/retain/mod.rs
// Add the module declaration after other module declarations (around line 49):

mod event_context;

// Add to the pub use section (around line 72):

pub use event_context::EventContext;
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p vexo --lib retain::event_context`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/event_context.rs vexo/src/retain/mod.rs
git commit -m "feat: add EventContext for retain mode event handling"
```

---

### Task 3: Add on_event() to Element trait

**Files:**
- Modify: `vexo/src/retain/element.rs`

**Why:** Elements need to handle input events. Adding `on_event()` with a default implementation that returns `None` allows all existing elements to compile without changes.

- [ ] **Step 1: Add on_event() method to Element trait**

Add to `vexo/src/retain/element.rs` in the `Element` trait (after the `can_update` method):

```rust
// In vexo/src/retain/element.rs, in the Element trait
// Add after the can_update() method (around line 45)

    /// Handle an input event.
    ///
    /// Returns `Some(message)` if the event was handled and produces a message.
    /// The message is type-erased as `Box<dyn Any>` and will be downcast
    /// by `WindowState` to the application's message type.
    ///
    /// Default implementation returns `None` (no interaction).
    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut super::EventContext,
    ) -> Option<Box<dyn std::any::Any>> {
        None
    }
```

- [ ] **Step 2: Add necessary imports**

Add at the top of `vexo/src/retain/element.rs` if not already present:

```rust
// The imports should already include std::any::Any
// Verify the file has:
use std::any::Any;
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p vexo --lib retain::element`
Expected: All tests pass (default impl means no changes needed in element impls)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/element.rs
git commit -m "feat: add on_event() to Element trait with default impl"
```

---

### Task 4: Add handle_event() to ThreeTreePipeline

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

**Why:** `ThreeTreePipeline` needs to dispatch events to elements. This includes hit testing for pointer events and routing keyboard events to the focused element.

- [ ] **Step 1: Add focused_element field to ThreeTreePipeline**

Modify `vexo/src/retain/pipeline.rs`:

```rust
// In vexo/src/retain/pipeline.rs
// Find the ThreeTreePipeline struct (around line 87)

pub struct ThreeTreePipeline {
    /// Registry of live elements (middle tree).
    element_registry: ElementRegistry,

    /// Registry of render objects (third tree).
    render_objects: RenderObjectRegistry,

    /// State storage for elements.
    state: StateStorage,

    /// Dirty tracking for incremental updates.
    dirty: DirtyTracking,

    /// Currently focused element (for keyboard events).
    focused_element: Option<ElementId>,
}
```

- [ ] **Step 2: Initialize focused_element in new()**

```rust
// In the impl ThreeTreePipeline block, in the new() method (around line 101)

    pub fn new() -> Self {
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
            focused_element: None,  // Add this line
        }
    }
```

- [ ] **Step 3: Add imports for event handling**

Add to the imports at the top of `vexo/src/retain/pipeline.rs`:

```rust
// At the top of vexo/src/retain/pipeline.rs
// Add to existing imports:

use crate::input::{ButtonState, InputEvent, Modifiers};
use crate::core::Bounds;
use super::EventContext;
use std::any::Any;
```

- [ ] **Step 4: Add handle_event() method**

Add to the `impl ThreeTreePipeline` block (after the `hit_test()` method, around line 475):

```rust
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
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerMoved { position } => {
                self.handle_pointer_event(*position, event, modifiers)
            }
            InputEvent::PointerButton { position, .. } => {
                self.handle_pointer_event(*position, event, modifiers)
            }
            InputEvent::Keyboard { .. } => {
                self.handle_keyboard_event(event, modifiers)
            }
            _ => None,
        }
    }

    /// Handle a pointer event (moved or button).
    fn handle_pointer_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
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
        let mut ctx = EventContext::new(
            position,
            self.focused_element,
            bounds,
            modifiers,
            &mut self.state,
        );

        // 5. Dispatch to element
        let message = self.element_registry.get_mut(target_element)?
            .on_event(event, &mut ctx);

        // 6. Handle focus requests
        if let Some(focus) = ctx.focus_request() {
            self.focused_element = Some(focus);
        } else if ctx.should_clear_focus() {
            self.focused_element = None;
        } else if message.is_none() {
            // If event not handled and it's a press, clear focus
            if let InputEvent::PointerButton { state: ButtonState::Pressed, .. } = event {
                self.focused_element = None;
            }
        }

        message
    }

    /// Handle a keyboard event.
    fn handle_keyboard_event(
        &mut self,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Get focused element
        let focused = self.focused_element?;

        // Bounds not critical for keyboard events
        let bounds = Bounds::default();

        let mut ctx = EventContext::new(
            Point::zero(),
            self.focused_element,
            bounds,
            modifiers,
            &mut self.state,
        );

        let message = self.element_registry.get_mut(focused)?
            .on_event(event, &mut ctx);

        // Handle focus requests
        if let Some(focus) = ctx.focus_request() {
            self.focused_element = Some(focus);
        } else if ctx.should_clear_focus() {
            self.focused_element = None;
        }

        message
    }

    /// Get the currently focused element.
    pub fn focused_element(&self) -> Option<ElementId> {
        self.focused_element
    }

    /// Set focus to an element.
    pub fn set_focus(&mut self, element: Option<ElementId>) {
        self.focused_element = element;
    }
```

- [ ] **Step 5: Run tests to verify**

Run: `cargo test -p vexo --lib retain::pipeline`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "feat: add handle_event() to ThreeTreePipeline"
```

---

### Task 5: Integrate retain mode event handling in WindowState

**Files:**
- Modify: `vexo/src/window.rs`

**Why:** `WindowState` needs to route events to the retain-mode pipeline when retain mode is active.

- [ ] **Step 1: Add process_input_event_retain() method**

Add to `vexo/src/window.rs` in the `impl WindowState<A>` block (after the `process_input_event()` method, around line 332):

```rust
// In vexo/src/window.rs
// Add after the process_input_event() method

    /// Process an input event through the retain-mode pipeline.
    fn process_input_event_retain(&mut self, input_event: InputEvent) {
        let position = match &input_event {
            InputEvent::PointerMoved { position } => *position,
            InputEvent::PointerButton { position, .. } => *position,
            _ => Point::new(0.0, 0.0),
        };

        // Get current modifiers from widget_context
        let modifiers = self.widget_context.modifiers.clone().unwrap_or_default();

        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return,
        };

        let message = pipeline.handle_event(position, &input_event, modifiers);

        if let Some(msg) = message {
            // Downcast to A::Message and call update
            if let Some(typed_msg) = msg.downcast_ref::<A::Message>() {
                self.update(typed_msg.clone());
            } else {
                // Type mismatch - log warning
                eprintln!(
                    "Warning: Element returned message of wrong type. Expected {}",
                    std::any::type_name::<A::Message>()
                );
            }
        }
    }
```

- [ ] **Step 2: Modify process_input_event() to route to retain mode**

Modify the existing `process_input_event()` method in `vexo/src/window.rs`:

```rust
// In vexo/src/window.rs
// Replace the existing process_input_event() method (around line 319)

    /// Process an InputEvent through the widget tree and handle responses.
    fn process_input_event(&mut self, input_event: InputEvent) {
        // Check if we should use retain mode
        if self.use_retain_mode && self.view_retain().is_some() {
            self.process_input_event_retain(input_event);
            return;
        }

        // Otherwise use immediate mode
        let layout_view = LayoutView::new(self.layout_engine.as_ref());
        let widget_response = self.root_widget.on_event(
            &layout_view,
            self.root_node_id,
            Point::new(0.0, 0.0),
            &input_event,
            self.focused_widget_id,
            &mut self.widget_context,
        );

        self.handle_widget_response(&widget_response, &input_event);
    }
```

- [ ] **Step 3: Add necessary imports**

Add to the imports at the top of `vexo/src/window.rs`:

```rust
// At the top of vexo/src/window.rs
// Add to existing imports:

use crate::input::Modifiers;
```

- [ ] **Step 4: Store modifiers in WidgetContext**

We need to track modifiers for retain mode. Check if `WidgetContext` has a `modifiers` field. If not, we need to add it or use a different approach.

First, let's check the current WidgetContext:

```rust
// Check vexo/src/widgets/mod.rs for WidgetContext definition
// If it doesn't have modifiers, we'll need to track it in WindowState
```

For now, use `Modifiers::default()` if WidgetContext doesn't have modifiers:

```rust
// In process_input_event_retain(), replace the modifiers line with:

        let modifiers = Modifiers::default(); // TODO: Track modifiers properly
```

- [ ] **Step 5: Build to verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 6: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: integrate retain mode event handling in WindowState"
```

---

### Task 6: Add explicit on_event() implementations to element types

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs`
- Modify: `vexo/src/retain/elements/container.rs`
- Modify: `vexo/src/retain/elements/modifier.rs`

**Why:** While the default implementation returns `None`, explicit implementations make the intent clear and serve as documentation for future implementers.

- [ ] **Step 1: Add on_event() to LeafElement**

Add to `vexo/src/retain/elements/leaf.rs` in the `impl Element for LeafElement` block:

```rust
// In vexo/src/retain/elements/leaf.rs
// Add after the can_update() method (around line 110)

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::retain::EventContext,
    ) -> Option<Box<dyn std::any::Any>> {
        // Leaf elements (like Text) don't handle events by default
        None
    }
```

- [ ] **Step 2: Add on_event() to ContainerElement**

Add to `vexo/src/retain/elements/container.rs` in the `impl Element for ContainerElement` block:

```rust
// In vexo/src/retain/elements/container.rs
// Add after the can_update() method (around line 123)

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::retain::EventContext,
    ) -> Option<Box<dyn std::any::Any>> {
        // Container elements don't handle events themselves
        // Hit testing finds the specific child element
        None
    }
```

- [ ] **Step 3: Add on_event() to ModifierElement**

Add to `vexo/src/retain/elements/modifier.rs` in the `impl Element for ModifierElement` block:

```rust
// In vexo/src/retain/elements/modifier.rs
// Add after the can_update() method (around line 219)

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::retain::EventContext,
    ) -> Option<Box<dyn std::any::Any>> {
        // Modifier elements don't handle events themselves
        // The hit test already found the correct target
        None
    }
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p vexo --lib retain::elements`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/
git commit -m "feat: add explicit on_event() implementations to element types"
```

---

### Task 7: Add integration tests for event handling

**Files:**
- Modify: `vexo/src/retain/integration_tests.rs`

**Why:** Verify the full event flow works: hit test → dispatch → message return → focus management.

- [ ] **Step 1: Add test for hit test and event dispatch**

Add to `vexo/src/retain/integration_tests.rs`:

```rust
// In vexo/src/retain/integration_tests.rs
// Add at the end of the file

#[cfg(test)]
mod event_handling_tests {
    use super::*;
    use crate::input::{ButtonState, InputEvent, PointerButton, Modifiers};
    use crate::core::Point;
    use crate::retain::{Element, EventContext, ThreeTreePipeline, Text, Column};
    use std::any::Any;

    #[test]
    fn test_pipeline_handle_event_no_root() {
        let mut pipeline = ThreeTreePipeline::new();

        let event = InputEvent::PointerButton {
            position: Point::new(10.0, 10.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let message = pipeline.handle_event(Point::new(10.0, 10.0), &event, Modifiers::default());
        assert!(message.is_none());
    }

    #[test]
    fn test_pipeline_handle_event_with_text_widget() {
        let mut pipeline = ThreeTreePipeline::new();

        // Reconcile a text widget
        pipeline.reconcile(Box::new(Text::new("Hello")));

        // Layout
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(
            crate::core::Size::new(800.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Click on the text (position depends on layout)
        let event = InputEvent::PointerButton {
            position: Point::new(5.0, 5.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        // Text element doesn't handle events, so should return None
        let message = pipeline.handle_event(Point::new(5.0, 5.0), &event, Modifiers::default());

        // Text element returns None by default
        assert!(message.is_none());
    }

    #[test]
    fn test_pipeline_focus_management() {
        let mut pipeline = ThreeTreePipeline::new();

        // Initially no focus
        assert!(pipeline.focused_element().is_none());

        // Set focus
        let element_id = crate::retain::ElementId::new();
        pipeline.set_focus(Some(element_id));
        assert_eq!(pipeline.focused_element(), Some(element_id));

        // Clear focus
        pipeline.set_focus(None);
        assert!(pipeline.focused_element().is_none());
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        use std::sync::Arc;
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo --lib retain::integration_tests::event_handling`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/integration_tests.rs
git commit -m "test: add integration tests for retain mode event handling"
```

---

### Task 8: Final verification and build

**Files:**
- None (verification only)

**Why:** Ensure everything compiles and tests pass.

- [ ] **Step 1: Run all retain mode tests**

Run: `cargo test -p vexo --lib retain::`
Expected: All tests pass

- [ ] **Step 2: Build the entire project**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 3: Run desktop demo to verify runtime**

Run: `cargo run -p desktop_demo`
Expected: App launches, can toggle retain mode with 'R' key, no crashes

- [ ] **Step 4: Final commit (if any changes)**

```bash
git status
# If any uncommitted changes:
git add -A
git commit -m "chore: final cleanup for retain mode event handling"
```

---

## Summary

This plan implements input event handling for retain mode in 8 tasks:

1. Add `computed_bounds()` to `RenderObject` trait
2. Create `EventContext` struct
3. Add `on_event()` to `Element` trait
4. Add `handle_event()` to `ThreeTreePipeline`
5. Integrate with `WindowState`
6. Add explicit `on_event()` implementations to element types
7. Add integration tests
8. Final verification

Each task follows TDD principles with tests and commits after each logical unit.
