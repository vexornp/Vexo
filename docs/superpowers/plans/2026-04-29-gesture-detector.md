# GestureDetector Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GestureDetector widget that wraps a child and emits typed messages on pointer press and release events.

**Architecture:** GestureDetector is a modifier widget following the same pattern as Background/Border. It has three components: widget (configuration), element (event handling), and render object (pass-through layout, invisible paint).

**Tech Stack:** Rust, existing retain mode infrastructure (Widget trait, Element trait, RenderObject trait)

---

### Task 1: Create GestureDetector Widget

**Files:**
- Create: `vexo/src/retain/widgets/gesture_detector.rs`

- [ ] **Step 1: Write the widget struct and builder methods**

```rust
//! Gesture detector widget - detects pointer press/release events.
//!
//! A modifier widget that wraps a child and emits typed messages
//! for pointer press and release events. Invisible (no visual rendering).

use std::any::Any;

use crate::core::{Bounds, Logical, Point};
use crate::input::{ButtonState, InputEvent};
use crate::retain::{
    Element, ElementContext, ElementId, ElementRegistry, EventContext,
    HitTestContext, Key, LayoutContext, LayoutResult, PaintContext,
    RenderObject, RenderObjectId, Widget,
};
use crate::layout::LayoutNodeId;

/// Gesture detector - emits messages on pointer press/release.
///
/// A modifier widget that wraps a child and detects pointer events.
/// Invisible (no visual rendering).
pub struct GestureDetector<M: Clone + Send + 'static> {
    key: Option<Key>,
    child: Box<dyn Widget<M>>,
    on_press: Option<M>,
    on_release: Option<M>,
}

impl<M: Clone + Send + 'static> GestureDetector<M> {
    /// Create a new gesture detector wrapping a child.
    pub fn new(child: Box<dyn Widget<M>>) -> Self {
        Self {
            key: None,
            child,
            on_press: None,
            on_release: None,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the message to emit on pointer press.
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// Set the message to emit on pointer release.
    pub fn on_release(mut self, message: M) -> Self {
        self.on_release = Some(message);
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget<M> {
        self.child.as_ref()
    }
}

impl<M: Clone + Send + 'static> Clone for GestureDetector<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
        }
    }
}
```

- [ ] **Step 2: Implement Widget trait for GestureDetector**

```rust
impl<M: Clone + Send + 'static> Widget<M> for GestureDetector<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = GestureDetectorElement::new(self.on_press.clone(), self.on_release.clone());
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(GestureDetectorRenderObject::new())
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
}
```

- [ ] **Step 3: Write the GestureDetectorElement struct**

```rust
/// Element for GestureDetector - handles press/release events.
pub struct GestureDetectorElement<M: Clone + Send + 'static> {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    on_press: Option<M>,
    on_release: Option<M>,
}

impl<M: Clone + Send + 'static> GestureDetectorElement<M> {
    /// Create a new gesture detector element.
    pub fn new(on_press: Option<M>, on_release: Option<M>) -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            on_press,
            on_release,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget<M>) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
    }
}
```

- [ ] **Step 4: Implement Element trait for GestureDetectorElement**

```rust
impl<M: Clone + Send + 'static> Element for GestureDetectorElement<M> {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(context.element_id);

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
        }

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        // GestureDetector has no direct children in the element tree
        // The child widget is mounted separately by the pipeline
        let _ = (registry, visitor);
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
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
}
```

- [ ] **Step 5: Write the GestureDetectorRenderObject**

```rust
/// Render object for GestureDetector - pass-through, invisible.
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}

impl GestureDetectorRenderObject {
    /// Create a new gesture detector render object.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for GestureDetectorRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for GestureDetectorRenderObject {
    fn layout(
        &mut self,
        ctx: &mut LayoutContext,
        child_nodes: &[LayoutNodeId],
    ) -> LayoutResult {
        // Pass-through: use child's layout node
        match child_nodes.first() {
            Some(child_node) => {
                self.layout_node = Some(*child_node);
                LayoutResult {
                    node: *child_node,
                    size: crate::core::Size::new(0.0, 0.0),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&crate::layout::Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: crate::core::Size::new(0.0, 0.0),
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        // Invisible - no render commands
        Vec::new()
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

- [ ] **Step 6: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::Text;

    #[derive(Clone, Debug, PartialEq)]
    enum TestMessage {
        Pressed,
        Released,
    }

    #[test]
    fn test_gesture_detector_creation() {
        let child = Box::new(Text::new("Hello"));
        let detector: GestureDetector<TestMessage> = GestureDetector::new(child);
        assert!(detector.key().is_none());
    }

    #[test]
    fn test_gesture_detector_with_key() {
        let child = Box::new(Text::new("Hello"));
        let detector: GestureDetector<TestMessage> = GestureDetector::new(child)
            .with_key("my-detector");
        assert_eq!(detector.key(), Some(Key::new("my-detector")));
    }

    #[test]
    fn test_gesture_detector_on_press() {
        let child = Box::new(Text::new("Hello"));
        let detector: GestureDetector<TestMessage> = GestureDetector::new(child)
            .on_press(TestMessage::Pressed);
        assert_eq!(detector.on_press, Some(TestMessage::Pressed));
        assert_eq!(detector.on_release, None);
    }

    #[test]
    fn test_gesture_detector_on_release() {
        let child = Box::new(Text::new("Hello"));
        let detector: GestureDetector<TestMessage> = GestureDetector::new(child)
            .on_release(TestMessage::Released);
        assert_eq!(detector.on_press, None);
        assert_eq!(detector.on_release, Some(TestMessage::Released));
    }

    #[test]
    fn test_gesture_detector_both_messages() {
        let child = Box::new(Text::new("Hello"));
        let detector: GestureDetector<TestMessage> = GestureDetector::new(child)
            .on_press(TestMessage::Pressed)
            .on_release(TestMessage::Released);
        assert_eq!(detector.on_press, Some(TestMessage::Pressed));
        assert_eq!(detector.on_release, Some(TestMessage::Released));
    }

    #[test]
    fn test_render_object_paint_returns_empty() {
        let ro = GestureDetectorRenderObject::new();
        let mut ctx = PaintContext::new(&mut Vec::new());
        let commands = ro.paint(&mut ctx);
        assert!(commands.is_empty());
    }
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p vexo --lib gesture_detector`
Expected: All tests pass

- [ ] **Step 8: Commit the widget file**

```bash
git add vexo/src/retain/widgets/gesture_detector.rs
git commit -m "feat: add GestureDetector widget for press/release events"
```

---

### Task 2: Export GestureDetector from widgets module

**Files:**
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Add module declaration and export**

Add at the top with other modules:
```rust
mod gesture_detector;
```

Add to the exports:
```rust
pub use gesture_detector::GestureDetector;
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo --lib`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/widgets/mod.rs
git commit -m "feat: export GestureDetector from widgets module"
```

---

### Task 3: Verify integration with existing code

**Files:**
- None (verification only)

- [ ] **Step 1: Build full workspace**

Run: `cargo build`
Expected: Build succeeds

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 3: Build desktop demo**

Run: `cargo build -p desktop_demo`
Expected: Build succeeds
