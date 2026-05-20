# Eliminate Phase 2 Hit Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace EmptyRenderObject with ProxyRenderObject so StatefulElement participates in the render tree hit path, eliminating the Phase 2 ancestor walk in event dispatch.

**Architecture:** StatefulElement creates a ProxyRenderObject (pass-through layout, invisible paint, bounds-based hit test) instead of EmptyRenderObject. The ProxyRenderObject sits in the render tree between the parent and child, making StatefulElement visible to hit testing. Phase 2 in EventHandler is removed — all event dispatch happens via single-phase bubbling through the hit test path.

**Tech Stack:** Rust, Taffy layout engine, slotmap generational keys

---

## File Structure

| File | Responsibility |
|------|---------------|
| `vexo/src/retain/stateful_widget.rs` | ProxyRenderObject definition, StatefulElement trait impls, Widget blanket impl |
| `vexo/src/retain/event_handler.rs` | Remove Phase 2 ancestor walk |
| `vexo/src/retain/mod.rs` | Update public exports (EmptyRenderObject → ProxyRenderObject) |

---

### Task 1: Create ProxyRenderObject

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs:530-566`

- [ ] **Step 1: Replace EmptyRenderObject with ProxyRenderObject**

Replace the `EmptyRenderObject` struct and its `RenderObject` impl (lines 530-566) with `ProxyRenderObject`:

```rust
/// Proxy render object for StatefulElement.
///
/// StatefulElement doesn't render itself - it delegates painting to its child.
/// But unlike EmptyRenderObject, ProxyRenderObject participates in the render tree:
/// - Pass-through layout (wraps child's Taffy node)
/// - No paint commands (invisible)
/// - Bounds-based hit test (enables StatefulElement to appear in hit test path)
///
/// This eliminates the need for Phase 2 ancestor walking in event dispatch.
pub struct ProxyRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<crate::core::Bounds<crate::core::Logical>>,
    layout_node: Option<crate::layout::LayoutNodeKey>,
}

impl ProxyRenderObject {
    /// Create a new ProxyRenderObject.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for ProxyRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for ProxyRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[crate::layout::LayoutNodeKey]) -> LayoutResult {
        let layout = crate::layout::Layout::default();
        let node = ctx.engine().create_container(&layout, child_nodes);
        self.layout_node = Some(node);
        LayoutResult {
            node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        Vec::new()
    }

    fn hit_test(&self, position: crate::core::Point<crate::core::Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
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

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<crate::layout::LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 2: Update Widget blanket impl to create ProxyRenderObject**

In the same file, change the `create_render_object` method in the `impl<W: StatefulWidget + Clone + 'static> Widget for W` block (line 586):

```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(ProxyRenderObject::new())
}
```

- [ ] **Step 3: Update public exports in mod.rs**

In `vexo/src/retain/mod.rs`, line 96, change:

```rust
pub use stateful_widget::{StatefulWidget, BuildContext, StatefulElement, EmptyRenderObject, State, StateContext, SimpleState};
```

to:

```rust
pub use stateful_widget::{StatefulWidget, BuildContext, StatefulElement, ProxyRenderObject, State, StateContext, SimpleState};
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -50`

Expected: May have warnings or errors about `EmptyRenderObject` references elsewhere. Fix any remaining references.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs vexo/src/retain/mod.rs
git commit -m "feat: replace EmptyRenderObject with ProxyRenderObject"
```

---

### Task 2: Make StatefulElement implement RenderObjectElement + SingleChildRenderObjectElement

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs:282-528`

- [ ] **Step 1: Add necessary imports**

At the top of `stateful_widget.rs`, add imports for the element traits:

```rust
use super::elements::{RenderObjectElement, SingleChildRenderObjectElement};
use crate::layout::LayoutNodeKey;
```

- [ ] **Step 2: Implement RenderObjectElement trait for StatefulElement**

Add after the `StatefulElement<W>` struct definition (before the `impl<W: StatefulWidget + Clone> Element for StatefulElement<W>` block):

```rust
impl<W: StatefulWidget + Clone> RenderObjectElement for StatefulElement<W> {
    fn widget(&self) -> Option<&dyn Widget> {
        Some(&self.widget)
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Ok(w) = widget.downcast::<W>() {
            self.widget = *w;
        }
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object_id
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object_id = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}
```

- [ ] **Step 3: Implement SingleChildRenderObjectElement trait for StatefulElement**

```rust
impl<W: StatefulWidget + Clone> SingleChildRenderObjectElement for StatefulElement<W> {
    fn child_element(&self) -> Option<ElementKey> {
        None
    }

    fn set_child_element(&mut self, _child: Option<ElementKey>) {
        // No-op: child tracking is done via ElementRegistry::children_map
    }
}
```

- [ ] **Step 4: Refactor StatefulElement::mount() to use mount_render_object()**

Replace the current `mount()` implementation. The key changes:
1. Use `self.mount_render_object(context)` instead of manual ID/key setup
2. Keep the state initialization, dirty callback wiring, and child inflation
3. Remove the manual `self.id = Some(context.element_id)` and key registration (handled by `mount_render_object`)

New `mount()`:

```rust
fn mount(&mut self, context: &mut ElementContext) {
    // Use RenderObjectElement's default mount for render object creation
    // This creates the ProxyRenderObject and stores the element ID + key
    self.mount_render_object(context);

    let element_id = context.element_id;

    // Initialize state with Default
    let mut state = W::State::default();

    // Wire up dirty callback using channel sender.
    let tx = context.dirty_sender.clone();
    let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let _ = tx.send(element_id);
    });
    state.set_dirty_callback(dirty_callback);

    // Call State::init() lifecycle hook
    let mut state_ctx = StateContext::new(element_id, context.build_owner);
    state.init(&mut state_ctx);

    // Store state in StateStorage
    context.insert_state(element_id, state);

    // Wire controller dirty callback for TextEdit widgets.
    if let Some(text_edit) = (&mut self.widget as &mut dyn Any).downcast_mut::<TextEdit>() {
        let tx = context.dirty_sender.clone();
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(element_id);
        });
        text_edit.wire_controller_dirty_callback(dirty_callback);
    }

    // Build the child widget tree using BuildContext
    let child_widget = {
        let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
        self.build_child_widget(
            element_id,
            state_ref,
            context.dirty,
            context.render_objects,
            context.build_owner,
        )
    };

    // Mount the child element tree via child_ops
    context.inflate_child(None, child_widget);
}
```

- [ ] **Step 5: Refactor StatefulElement::update() to use update_render_object()**

Replace the current `update()` implementation:

```rust
fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
    // Downcast to the concrete widget type
    if let Ok(widget) = new_widget.downcast::<W>() {
        self.widget = *widget;
    }

    // Update the render object (no-op for ProxyRenderObject, but follows pattern)
    if let Some(ro_id) = self.render_object_id {
        // ProxyRenderObject has no properties to update from widget config
        // Just mark it as needing layout in case child changed
        context.mark_needs_layout(ro_id);
    }

    let element_id = context.element_id;

    // Build the child widget tree using BuildContext
    let child_widget = {
        let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
        self.build_child_widget(
            element_id,
            state_ref,
            context.dirty,
            context.render_objects,
            context.build_owner,
        )
    };

    // Reconcile child via child_ops
    let old_child = context.children().first().copied();
    match old_child {
        Some(old_child_key) => {
            context.update_child(old_child_key, child_widget);
        }
        None => {
            context.inflate_child(None, child_widget);
        }
    }
}
```

- [ ] **Step 6: Refactor StatefulElement::unmount() to use unmount_render_object()**

Replace the current `unmount()` implementation:

```rust
fn unmount(&mut self, context: &mut ElementContext) {
    // Call State::dispose() lifecycle hook before removing state
    if let Some(id) = self.id {
        if let Some(state) = context.state.get_mut::<W::State>(id) {
            state.dispose();
        }
    }

    // Use RenderObjectElement's default unmount for render object removal
    // This unregisters global key, removes render object, and removes state
    self.unmount_render_object(context);

    // Unmount child element via child_ops
    if let Some(child_key) = context.children().first().copied() {
        context.unmount_child(child_key);
    }
}
```

Note: `unmount_render_object()` already handles global key unregistration and state removal, so we can remove the duplicate code. But we need to call `State::dispose()` before `unmount_render_object()` since the latter removes the state. The `unmount_render_object()` call will handle `remove_state()`, so we should NOT call `context.remove_state()` separately anymore.

- [ ] **Step 7: Change child_mounted() to link child via insert_child_render_object()**

Replace the current `child_mounted()`:

```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    // Link the child's render object to our ProxyRenderObject
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);
    }
}
```

This is the critical change: instead of `self.render_object_id = child_ro` (delegating to child's render object), we link the child's render object as a child of our ProxyRenderObject via `insert_child_render_object()`. This means `self.render_object_id` stays as the ProxyRenderObject's key, and the ProxyRenderObject has the child's render object as its child in the render tree.

- [ ] **Step 8: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -80`

Expected: Clean compilation. If errors, fix them.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat: StatefulElement implements RenderObjectElement + SingleChildRenderObjectElement"
```

---

### Task 3: Remove Phase 2 from EventHandler

**Files:**
- Modify: `vexo/src/retain/event_handler.rs:170-229`

- [ ] **Step 1: Remove the Phase 2 ancestor walk code**

Remove lines 170-229 (the entire Phase 2 block starting with the comment "4. Continue bubbling up through ancestor elements..." and ending with the closing brace of the `if any_message.is_none()` block).

The `handle_pointer_event()` method should now look like this after Phase 1 bubbling (line 168):

```rust
// If no element handled the event and it's a press, clear focus
if any_message.is_none() {
    if let InputEvent::PointerButton {
        state: ButtonState::Pressed,
        ..
    } = event
    {
        *focused_element = None;
    }
}

any_message
```

- [ ] **Step 2: Update the doc comment on handle_pointer_event()**

Update the doc comment (lines 92-98) to reflect the single-phase approach:

```rust
/// Handle a pointer event (moved or button).
///
/// Events are dispatched using single-phase bubbling: the event is sent
/// to each element in the hit test path from deepest (innermost) to
/// shallowest (root). The first element that handles the event stops
/// propagation. This allows modifier elements like GestureDetector to
/// intercept events before they reach the child element.
///
/// All elements (including StatefulElement) appear in the hit test path
/// because they own ProxyRenderObjects that participate in the render tree.
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -50`

Expected: Clean compilation.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/event_handler.rs
git commit -m "feat: remove Phase 2 ancestor walk from EventHandler"
```

---

### Task 4: Run existing tests to verify no regressions

**Files:**
- None (test only)

- [ ] **Step 1: Run all vexo tests**

Run: `cargo test -p vexo 2>&1 | tail -80`

Expected: All tests pass. If any fail, investigate and fix.

- [ ] **Step 2: Run the desktop demo to verify visual behavior**

Run: `cargo run -p desktop_demo 2>&1 &`

Expected: Window opens, TextEdit is clickable and focusable, GestureDetector works. Kill the process after visual verification.

- [ ] **Step 3: Commit any test fixes if needed**

If any test fixes were required:

```bash
git add -u
git commit -m "fix: update tests for ProxyRenderObject"
```

---

### Task 5: Add integration test verifying StatefulElement appears in hit test path

**Files:**
- Modify: `vexo/src/retain/stateful_integration_test.rs`

- [ ] **Step 1: Write test that verifies StatefulElement is in the hit test path**

Add a new test to `stateful_integration_test.rs`:

```rust
/// Test that StatefulElement appears in the hit test path via ProxyRenderObject.
/// This verifies that Phase 2 ancestor walking is no longer needed.
#[test]
fn test_stateful_element_in_hit_test_path() {
    use crate::retain::StatefulWidget;
    use crate::core::Position;

    #[derive(Clone)]
    struct SimpleStateful;

    impl StatefulWidget for SimpleStateful {
        type State = SimpleState<()>;

        fn build(&self, _state: &mut Self::State, _ctx: &mut BuildContext) -> Box<dyn Widget> {
            Box::new(Text::new("Stateful"))
        }
    }

    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile with a stateful widget
    pipeline.reconcile(Box::new(SimpleStateful));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // Hit test inside the text bounds
    let result = pipeline.hit_test(Position::new(5.0, 5.0));

    // Should hit something
    assert!(result.is_hit(), "Hit test should find a target");

    // The element path should contain the StatefulElement
    // The root element IS the StatefulElement for SimpleStateful
    let root_id = pipeline.element_registry().root().unwrap();
    let element_path = result.element_path();

    // The StatefulElement (root) should be in the element path
    assert!(element_path.contains(&root_id),
        "StatefulElement should appear in hit test element path. Path: {:?}", element_path);

    // The element path should have at least 2 entries:
    // [StatefulElement, LeafElement (for Text)]
    assert!(element_path.len() >= 2,
        "Element path should have at least StatefulElement + child. Got: {:?}", element_path);
}
```

- [ ] **Step 2: Run the new test**

Run: `cargo test -p vexo test_stateful_element_in_hit_test_path 2>&1 | tail -30`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_integration_test.rs
git commit -m "test: verify StatefulElement appears in hit test path"
```

---

### Task 6: Add integration test verifying TextEdit click-to-focus works without Phase 2

**Files:**
- Modify: `vexo/src/retain/stateful_integration_test.rs`

- [ ] **Step 1: Write test that verifies TextEdit focus via Phase 1 bubbling**

Add a new test:

```rust
/// Test that TextEdit click-to-focus works via Phase 1 bubbling
/// (without the Phase 2 ancestor walk that was removed).
#[test]
fn test_textedit_click_to_focus_without_phase2() {
    use crate::retain::{TextEdit, TextEditState, TextEditingController};
    use crate::core::Position;

    let controller = TextEditingController::new("editable");

    let mut pipeline = ThreeTreePipeline::new();

    // Create a TextEdit widget
    let text_edit = TextEdit::new(controller.clone());
    pipeline.reconcile(Box::new(text_edit));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // Initially no focus
    assert!(pipeline.focused_element().is_none(),
        "No element should be focused initially");

    // Click inside the TextEdit bounds
    let click_position = Point::new(5.0, 5.0);
    let event = InputEvent::PointerButton {
        position: click_position,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let mut font_system = create_test_font_system();
    let _result = pipeline.handle_event(click_position, &event, crate::input::Modifiers::default(), &mut font_system);

    // After clicking, the TextEdit's StatefulElement should be focused
    // (This works because StatefulElement is now in the hit test path via ProxyRenderObject)
    assert!(pipeline.focused_element().is_some(),
        "TextEdit should be focused after click (via Phase 1 bubbling)");
}
```

- [ ] **Step 2: Run the new test**

Run: `cargo test -p vexo test_textedit_click_to_focus_without_phase2 2>&1 | tail -30`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_integration_test.rs
git commit -m "test: verify TextEdit click-to-focus works without Phase 2"
```

---

### Task 7: Final verification — run all tests and desktop demo

**Files:**
- None (verification only)

- [ ] **Step 1: Run all vexo tests**

Run: `cargo test -p vexo 2>&1 | tail -40`

Expected: All tests pass.

- [ ] **Step 2: Run all workspace tests**

Run: `cargo test 2>&1 | tail -40`

Expected: All tests pass.

- [ ] **Step 3: Build in release mode**

Run: `cargo build -p vexo --release 2>&1 | tail -20`

Expected: Clean build.

- [ ] **Step 4: Final commit if any remaining fixes**

If any fixes were needed:

```bash
git add -u
git commit -m "fix: final adjustments for ProxyRenderObject migration"
```
