# StatefulWidget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Flutter-style stateful widgets for Vexo's retain mode with persistent mutable state.

**Architecture:** Add a `StatefulWidget` trait and `StatefulElement` wrapper that manages state in the existing `StateStorage`. StatefulElement delegates rendering to a child widget produced by `build()`, preserving state across rebuilds.

**Tech Stack:** Rust, existing retain-mode infrastructure (Element, StateStorage, BuildOwner)

---

## File Structure

| File | Purpose |
|------|---------|
| `vexo/src/retain/stateful_widget.rs` | StatefulWidget trait, BuildContext, StatefulElement |
| `vexo/src/retain/mod.rs` | Export new types |
| `vexo/src/retain/build_owner.rs` | Add rebuild scheduling (minor changes) |

---

### Task 1: Define StatefulWidget Trait and BuildContext

**Files:**
- Create: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Write the StatefulWidget trait and BuildContext struct**

```rust
//! StatefulWidget trait for widgets with persistent mutable state.

use std::any::Any;

use super::id::ElementId;
use super::state::StateStorage;
use super::dirty::DirtyTracking;
use super::render_object::RenderObjectRegistry;
use super::build_owner::BuildOwner;
use super::widgets::Widget;

/// Context provided to StatefulWidget::build().
pub struct BuildContext<'a> {
    /// The element ID for this stateful element.
    pub element_id: ElementId,

    /// State storage for accessing element state.
    pub state_storage: &'a mut StateStorage,

    /// Dirty tracking for marking layout/paint dirty.
    pub dirty: &'a mut DirtyTracking,

    /// Render object registry.
    pub render_objects: &'a mut RenderObjectRegistry,

    /// Build owner for scheduling rebuilds.
    pub build_owner: &'a mut BuildOwner,
}

impl<'a> BuildContext<'a> {
    /// Request a rebuild of this element.
    ///
    /// The element will be rebuilt during the next frame.
    pub fn request_rebuild(&mut self) {
        self.build_owner.mark_needs_build(self.element_id);
    }

    /// Mark the element's render object as needing layout.
    pub fn mark_needs_layout(&mut self, render_object_id: super::id::RenderObjectId) {
        self.dirty.mark_needs_layout(render_object_id);
    }

    /// Mark the element's render object as needing paint.
    pub fn mark_needs_paint(&mut self, render_object_id: super::id::RenderObjectId) {
        self.dirty.mark_needs_paint(render_object_id);
    }
}

/// Trait for widgets that have persistent mutable state.
///
/// StatefulWidget is the Vexo equivalent of Flutter's StatefulWidget.
/// The state persists across widget tree rebuilds, allowing the widget
/// to maintain mutable data that survives reconciliation.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone)]
/// struct Counter {
///     label: String,
/// }
///
/// struct CounterState {
///     count: u32,
/// }
///
/// impl Default for CounterState {
///     fn default() -> Self {
///         Self { count: 0 }
///     }
/// }
///
/// impl StatefulWidget for Counter {
///     type State = CounterState;
///
///     fn build(&self, state: &mut CounterState, ctx: &mut BuildContext) -> Box<dyn Widget> {
///         Column::new()
///             .push(Text::new(format!("{}: {}", self.label, state.count)))
///             .push(Button::new("Increment", || {
///                 state.count += 1;
///                 ctx.request_rebuild();
///             }))
///             .boxed()
///     }
/// }
/// ```
pub trait StatefulWidget: Sized + 'static {
    /// The mutable state type that persists across rebuilds.
    ///
    /// Must implement Default for initialization.
    type State: Default;

    /// Build the widget tree using current state.
    ///
    /// Called during mount and update. The state is passed mutably
    /// so the widget can modify it. Call `ctx.request_rebuild()`
    /// after modifying state to trigger a rebuild.
    fn build(&self, state: &mut Self::State, ctx: &mut BuildContext) -> Box<dyn Widget>;
}
```

- [ ] **Step 2: Run cargo check to verify the trait compiles**

Run: `cargo check -p vexo`
Expected: Compilation succeeds or shows only missing import errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat(retain): add StatefulWidget trait and BuildContext"
```

---

### Task 2: Implement StatefulElement

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Add StatefulElement struct and implementation**

Add to `vexo/src/retain/stateful_widget.rs`:

```rust
use super::element::{Element, ElementRegistry};
use super::element_context::ElementContext;
use super::id::RenderObjectId;
use super::key::WidgetKey;
use super::UpdateResult;

/// Element for StatefulWidget widgets.
///
/// StatefulElement wraps a StatefulWidget and:
/// - Stores the widget configuration
/// - Manages state in StateStorage (keyed by element ID)
/// - Builds a child widget tree on mount and update
/// - Delegates rendering to the child element
pub struct StatefulElement<W: StatefulWidget> {
    /// The widget configuration.
    widget: W,

    /// The element ID (set during mount).
    id: Option<ElementId>,

    /// The widget key (if any).
    key: Option<WidgetKey>,

    /// The child element ID (from build()).
    child_element_id: Option<ElementId>,

    /// The render object ID (from child, if any).
    render_object_id: Option<RenderObjectId>,
}

impl<W: StatefulWidget> StatefulElement<W> {
    /// Create a new StatefulElement from a widget.
    pub fn new(widget: W) -> Self {
        let key = None; // StatefulWidget widgets can have keys via Widget trait
        Self {
            widget,
            id: None,
            key,
            child_element_id: None,
            render_object_id: None,
        }
    }
}

impl<W: StatefulWidget + Clone> Element for StatefulElement<W> {
    fn mount(&mut self, context: &mut ElementContext) {
        // Store the element ID
        self.id = Some(context.element_id);

        // Register global key if present
        if let Some(WidgetKey::Global(key)) = &self.key {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

        // Initialize state with Default
        let state = W::State::default();
        context.insert_state(context.element_id, state);

        // Build the child widget tree
        let child_widget = {
            let state_mut = context.get_state_mut::<W::State>(context.element_id).unwrap();

            // Create BuildContext
            let mut build_ctx = BuildContext {
                element_id: context.element_id,
                state_storage: context.state,
                dirty: context.dirty,
                render_objects: context.render_objects.as_mut().unwrap(),
                build_owner: context.build_owner.as_mut().unwrap(),
            };

            self.widget.build(state_mut, &mut build_ctx)
        };

        // Mount the child element
        if let Some(registry) = &mut context.element_registry {
            let child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
            self.child_element_id = Some(child_id);

            // Get the child's render object
            self.render_object_id = registry.get(child_id)
                .and_then(|el| el.render_object());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast to the concrete widget type
        if let Ok(widget) = new_widget.downcast::<W>() {
            self.widget = widget;
        }

        // Retrieve existing state
        let child_widget = {
            let state_mut = context.get_state_mut::<W::State>(context.element_id).unwrap();

            // Create BuildContext
            let mut build_ctx = BuildContext {
                element_id: context.element_id,
                state_storage: context.state,
                dirty: context.dirty,
                render_objects: context.render_objects.as_mut().unwrap(),
                build_owner: context.build_owner.as_mut().unwrap(),
            };

            self.widget.build(state_mut, &mut build_ctx)
        };

        // Reconcile child element
        if let Some(child_id) = self.child_element_id {
            if let Some(registry) = &mut context.element_registry {
                if registry.contains(child_id) {
                    // Update existing child
                    let widget_any: Box<dyn Any> = Box::new(child_widget.clone_boxed());
                    registry.update_element(child_id, widget_any, context);
                } else {
                    // Mount new child
                    let new_child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
                    self.child_element_id = Some(new_child_id);
                }
            }
        } else if let Some(registry) = &mut context.element_registry {
            // No existing child, mount new
            let child_id = registry.mount(child_widget.create_element(), Some(context.element_id));
            self.child_element_id = Some(child_id);
        }

        // Update render object reference
        if let Some(child_id) = self.child_element_id {
            self.render_object_id = context.element_registry.as_ref()
                .and_then(|r| r.get(child_id))
                .and_then(|el| el.render_object());
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

        // Unmount child element
        if let Some(child_id) = self.child_element_id {
            if let Some(registry) = &mut context.element_registry {
                registry.unmount(child_id);
            }
        }

        // Remove state from storage
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element_id {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object_id
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<W>().is_some()
    }

    fn has_children(&self) -> bool {
        self.child_element_id.is_some()
    }
}
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat(retain): implement StatefulElement with lifecycle methods"
```

---

### Task 3: Add Widget Trait Implementation for StatefulWidget

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Add Widget trait implementation and EmptyRenderObject**

Add to `vexo/src/retain/stateful_widget.rs`:

```rust
use super::render_object::{RenderObject, LayoutContext, LayoutResult, PaintContext, HitTestContext};
use crate::core::Logical;
use crate::render::RenderCommand;

/// Empty render object for StatefulElement.
///
/// StatefulElement doesn't render itself - it delegates to its child.
/// This render object exists only to satisfy the Widget trait.
pub struct EmptyRenderObject;

impl RenderObject for EmptyRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _children: &[crate::layout::LayoutNodeId]) -> LayoutResult {
        let node = ctx.engine().create_leaf(&crate::layout::Layout::default());
        LayoutResult {
            node,
            size: crate::core::Size::new(0.0, 0.0),
        }
    }

    fn apply_layout(&mut self, _ctx: &LayoutContext) {}

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        Vec::new()
    }

    fn hit_test(&self, _position: crate::core::Point<Logical>, _ctx: &HitTestContext) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Blanket Widget implementation for StatefulWidget types.
///
/// This allows StatefulWidget implementations to be used anywhere
/// a Widget is expected.
impl<W: StatefulWidget + Clone + 'static> Widget for W {
    fn key(&self) -> Option<WidgetKey> {
        None // StatefulWidget widgets can override this if needed
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(StatefulElement::new(self.clone()))
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(EmptyRenderObject)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat(retain): add Widget trait impl for StatefulWidget"
```

---

### Task 4: Export New Types from Module

**Files:**
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Add module and exports**

Modify `vexo/src/retain/mod.rs` to add:

```rust
mod stateful_widget;
```

And add to the exports:

```rust
pub use stateful_widget::{StatefulWidget, BuildContext, StatefulElement, EmptyRenderObject};
```

- [ ] **Step 2: Run cargo check to verify exports work**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/mod.rs
git commit -m "feat(retain): export StatefulWidget types from module"
```

---

### Task 5: Write Unit Tests for StatefulElement

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Add test module with comprehensive tests**

Add to `vexo/src/retain/stateful_widget.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, ElementRegistry, ElementContext, Text, BuildOwner};

    #[derive(Clone)]
    struct TestCounter {
        label: String,
    }

    struct TestCounterState {
        count: u32,
    }

    impl Default for TestCounterState {
        fn default() -> Self {
            Self { count: 0 }
        }
    }

    impl StatefulWidget for TestCounter {
        type State = TestCounterState;

        fn build(&self, state: &mut TestCounterState, _ctx: &mut BuildContext) -> Box<dyn Widget> {
            // Return a simple text widget showing the count
            Box::new(Text::new(format!("{}: {}", self.label, state.count)))
        }
    }

    fn create_test_context() -> (
        ElementId,
        StateStorage,
        DirtyTracking,
        RenderObjectRegistry,
        ElementRegistry,
        BuildOwner,
    ) {
        (
            ElementId::new(),
            StateStorage::new(),
            DirtyTracking::new(),
            RenderObjectRegistry::new(),
            ElementRegistry::new(),
            BuildOwner::new(),
        )
    }

    #[test]
    fn test_stateful_element_mount_creates_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, mut element_registry, mut build_owner) = create_test_context();

        // Mount the element
        let mut ctx = ElementContext::full(
            element_id,
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
            &mut element_registry,
        );
        ctx.build_owner = Some(&mut build_owner);

        let mut element = element;
        Element::mount(&mut element, &mut ctx);

        // State should be created with default value
        assert!(state.get::<TestCounterState>(element_id).is_some());
        assert_eq!(state.get::<TestCounterState>(element_id).unwrap().count, 0);
    }

    #[test]
    fn test_stateful_element_update_preserves_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let mut element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, mut element_registry, mut build_owner) = create_test_context();

        // Mount
        {
            let mut ctx = ElementContext::full(
                element_id,
                None,
                &mut state,
                &mut dirty,
                &mut render_objects,
                &mut element_registry,
            );
            ctx.build_owner = Some(&mut build_owner);
            Element::mount(&mut element, &mut ctx);
        }

        // Modify state
        state.get_mut::<TestCounterState>(element_id).unwrap().count = 5;

        // Update with new widget
        let new_widget = TestCounter { label: "Updated".to_string() };
        {
            let mut ctx = ElementContext::full(
                element_id,
                None,
                &mut state,
                &mut dirty,
                &mut render_objects,
                &mut element_registry,
            );
            ctx.build_owner = Some(&mut build_owner);
            Element::update(&mut element, Box::new(new_widget), &mut ctx);
        }

        // State should be preserved
        assert_eq!(state.get::<TestCounterState>(element_id).unwrap().count, 5);
    }

    #[test]
    fn test_stateful_element_unmount_removes_state() {
        let widget = TestCounter { label: "Count".to_string() };
        let mut element = StatefulElement::new(widget);

        let (element_id, mut state, mut dirty, mut render_objects, mut element_registry, mut build_owner) = create_test_context();

        // Mount
        {
            let mut ctx = ElementContext::full(
                element_id,
                None,
                &mut state,
                &mut dirty,
                &mut render_objects,
                &mut element_registry,
            );
            ctx.build_owner = Some(&mut build_owner);
            Element::mount(&mut element, &mut ctx);
        }

        // Verify state exists
        assert!(state.get::<TestCounterState>(element_id).is_some());

        // Unmount
        {
            let mut ctx = ElementContext::full(
                element_id,
                None,
                &mut state,
                &mut dirty,
                &mut render_objects,
                &mut element_registry,
            );
            ctx.build_owner = Some(&mut build_owner);
            Element::unmount(&mut element, &mut ctx);
        }

        // State should be removed
        assert!(state.get::<TestCounterState>(element_id).is_none());
    }

    #[test]
    fn test_stateful_element_can_update_same_type() {
        let widget = TestCounter { label: "Count".to_string() };
        let element = StatefulElement::new(widget);

        let new_widget = TestCounter { label: "Updated".to_string() };
        let widget_any: Box<dyn Any> = Box::new(new_widget);

        assert!(element.can_update(&widget_any));
    }

    #[test]
    fn test_build_context_request_rebuild() {
        let (element_id, mut state, mut dirty, mut render_objects, _, mut build_owner) = create_test_context();

        let mut ctx = BuildContext {
            element_id,
            state_storage: &mut state,
            dirty: &mut dirty,
            render_objects: &mut render_objects,
            build_owner: &mut build_owner,
        };

        ctx.request_rebuild();

        assert!(build_owner.is_dirty(element_id));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo stateful_widget::tests`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "test(retain): add unit tests for StatefulElement"
```

---

### Task 6: Add Integration Test with Pipeline

**Files:**
- Create: `vexo/src/retain/stateful_integration_test.rs`

- [ ] **Step 1: Add integration test file**

Create `vexo/src/retain/stateful_integration_test.rs`:

```rust
//! Integration tests for StatefulWidget with ThreeTreePipeline.

#[cfg(test)]
mod tests {
    use vexo::retain::{StatefulWidget, BuildContext, ThreeTreePipeline, Widget, Text, Column};
    use vexo::core::Size;
    use vexo::layout::TaffyLayoutEngine;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = vexo::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[derive(Clone)]
    struct Counter {
        label: String,
    }

    struct CounterState {
        count: u32,
    }

    impl Default for CounterState {
        fn default() -> Self {
            Self { count: 0 }
        }
    }

    impl StatefulWidget for Counter {
        type State = CounterState;

        fn build(&self, state: &mut CounterState, _ctx: &mut BuildContext) -> Box<dyn Widget> {
            Box::new(Text::new(format!("{}: {}", self.label, state.count)))
        }
    }

    #[test]
    fn test_stateful_widget_in_pipeline() {
        let mut pipeline = ThreeTreePipeline::new();

        // Create a stateful widget
        let counter = Counter { label: "Count".to_string() };

        // Reconcile with the stateful widget
        pipeline.reconcile(Box::new(counter));

        // Should have elements
        assert!(!pipeline.element_registry().is_empty());
    }

    #[test]
    fn test_stateful_widget_state_persists_across_rebuild() {
        let mut pipeline = ThreeTreePipeline::new();

        // Initial reconcile
        let counter = Counter { label: "Count".to_string() };
        pipeline.reconcile(Box::new(counter));

        // Get the root element ID
        let root_id = pipeline.element_registry().root().unwrap();

        // Update with new widget (same type, different label)
        let counter_updated = Counter { label: "Updated".to_string() };
        pipeline.reconcile(Box::new(counter_updated));

        // Root element should be the same (updated, not remounted)
        assert_eq!(pipeline.element_registry().root(), Some(root_id));
    }

    #[test]
    fn test_stateful_widget_layout_and_paint() {
        let mut pipeline = ThreeTreePipeline::new();

        let counter = Counter { label: "Count".to_string() };
        pipeline.reconcile(Box::new(counter));

        // Layout
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Paint
        let commands = pipeline.paint();

        // Should have generated render commands from the child Text widget
        assert!(!commands.is_empty());
    }
}
```

- [ ] **Step 2: Add the test module to mod.rs**

Add to `vexo/src/retain/mod.rs`:

```rust
#[cfg(test)]
mod stateful_integration_test;
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p vexo stateful_integration_test`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs vexo/src/retain/stateful_integration_test.rs vexo/src/retain/mod.rs
git commit -m "test(retain): add integration tests for StatefulWidget with pipeline"
```

---

### Task 7: Add Counter Example to Demo App

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add a Counter StatefulWidget example**

Read the current `shared_app/src/lib.rs` and add a Counter example that demonstrates StatefulWidget usage.

- [ ] **Step 2: Run the demo app to verify**

Run: `cargo run -p desktop_demo`
Expected: App runs without errors

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "demo: add Counter StatefulWidget example"
```

---

### Task 8: Run Full Test Suite

- [ ] **Step 1: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p vexo`
Expected: No warnings

- [ ] **Step 3: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: resolve test and clippy issues"
```
