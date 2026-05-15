# ThreeTreePipeline SRP Decomposition Design

**Date:** 2026-05-15
**Status:** Approved

## Problem

`ThreeTreePipeline` handles 6 distinct responsibilities in a single 1073-line struct:

1. Reconciliation (widget-to-element diffing)
2. Element lifecycle (mount/unmount/child ops)
3. State-driven rebuilds (perform_rebuilds, drain dirty channel)
4. Layout (3-phase Taffy layout)
5. Paint (render command generation)
6. Event handling (hit test, pointer/keyboard dispatch, focus)

This violates SRP, making the pipeline hard to understand, test, and extend independently.

## Decision

Extract into **4 zero-sized structs** with associated functions. The pipeline retains ownership of all shared state and delegates to the structs by passing `&mut` references.

**Why 4, not 6:** Reconciliation, lifecycle, and state rebuilds form a single call graph (`perform_rebuilds` → `rebuild_element` → `execute_child_ops` → `mount_element_tree`). They share the same context and are tightly coupled — separating them would create artificial boundaries and awkward cross-struct calls.

**Why zero-sized structs with associated functions:** Stateful structs holding `&mut` refs would create lifetime complexity (multiple simultaneous borrows from the pipeline). Associated functions avoid this — the pipeline passes the needed refs as arguments. Zero runtime overhead, idiomatic Rust.

**Why not modules:** Modules work but are less extensible (can't implement traits later) and less idiomatic for this pattern.

## Structs

### Reconciler

**Responsibility:** Widget-to-element reconciliation, element lifecycle, child op execution, state-driven rebuilds.

**Methods extracted from pipeline:**
- `reconcile()`, `reconcile_element()`, `rebuild_root()`, `rebuild_element()`
- `mount_element_tree()`, `unmount_element_tree()`, `execute_child_ops()`
- `perform_rebuilds()`, `drain_dirty_channel()`
- `update()`, `update_state_only()`

**Context needed:** `element_registry`, `render_objects`, `state`, `dirty`, `build_owner`, `child_ops`, `dirty_sender`, `dirty_receiver`, `needs_full_reconcile`

**Signature pattern:**
```rust
pub struct Reconciler;

impl Reconciler {
    pub fn reconcile(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
        needs_full_reconcile: &mut bool,
        root_widget: Box<dyn Widget>,
    ) { ... }

    pub fn update(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
        needs_full_reconcile: &mut bool,
        root_widget: Box<dyn Widget>,
    ) { ... }

    pub fn perform_rebuilds(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
    ) { ... }

    pub fn update_state_only(/* same as perform_rebuilds */) { ... }
}
```

### Layouter

**Responsibility:** Three-phase layout: build Taffy tree, compute layout, apply computed layouts.

**Methods extracted from pipeline:**
- `layout()`, `layout_build_recursive()`, `apply_layout_recursive()`, `get_layout_node()`

**Context needed:** `render_objects`, `dirty`

**Signature pattern:**
```rust
pub struct Layouter;

impl Layouter {
    pub fn layout(
        render_objects: &mut RenderObjectRegistry,
        dirty: &mut DirtyTracking,
        available_size: Size<Logical>,
        engine: &mut dyn LayoutEngine,
        font_system: &mut glyphon::FontSystem,
    ) { ... }
}
```

**Clean boundary:** Only touches `render_objects` and `dirty`. No interaction with element tree or state.

### Painter

**Responsibility:** Generate render commands from the render object tree.

**Methods extracted from pipeline:**
- `paint()`, `paint_recursive()`

**Context needed:** `render_objects`, `dirty`

**Signature pattern:**
```rust
pub struct Painter;

impl Painter {
    pub fn paint(
        render_objects: &mut RenderObjectRegistry,
        dirty: &mut DirtyTracking,
    ) -> Vec<RenderCommand> { ... }
}
```

**Clean boundary:** Only touches `render_objects` and `dirty`.

### EventHandler

**Responsibility:** Hit testing, pointer/keyboard event dispatch, focus management.

**Methods extracted from pipeline:**
- `handle_event()`, `handle_pointer_event()`, `handle_keyboard_event()`
- `hit_test()` (delegates to `render_objects.hit_test()`)
- `focused_element()`, `set_focus()`

**Context needed:** `element_registry`, `render_objects`, `state`, `build_owner`, `dirty_sender`, `focused_element`

**Signature pattern:**
```rust
pub struct EventHandler;

impl EventHandler {
    pub fn handle_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focused_element: &mut Option<ElementKey>,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> { ... }

    pub fn hit_test(
        render_objects: &RenderObjectRegistry,
        position: Position<Logical, Absolute>,
    ) -> HitTestResult { ... }
}
```

**Note:** `focused_element` is passed as `&mut Option<ElementKey>` because event handling mutates focus state.

## Pipeline After Extraction

`ThreeTreePipeline` becomes a thin orchestrator that owns all state and delegates:

```rust
pub struct ThreeTreePipeline {
    element_registry: ElementRegistry,
    render_objects: RenderObjectRegistry,
    state: StateStorage,
    dirty: DirtyTracking,
    focused_element: Option<ElementKey>,
    build_owner: BuildOwner,
    child_ops: ChildOps,
    dirty_sender: mpsc::Sender<ElementKey>,
    dirty_receiver: mpsc::Receiver<ElementKey>,
    needs_full_reconcile: bool,
}

impl ThreeTreePipeline {
    pub fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
        Reconciler::reconcile(
            &mut self.element_registry, &mut self.render_objects,
            &mut self.state, &mut self.dirty, &mut self.build_owner,
            &mut self.child_ops, &self.dirty_sender, &self.dirty_receiver,
            &mut self.needs_full_reconcile, root_widget,
        );
    }

    pub fn update(&mut self, root_widget: Box<dyn Widget>) {
        Reconciler::update(...);
    }

    pub fn layout(&mut self, available_size: Size<Logical>,
                  engine: &mut dyn LayoutEngine,
                  font_system: &mut glyphon::FontSystem) {
        Layouter::layout(&mut self.render_objects, &mut self.dirty,
                         available_size, engine, font_system);
    }

    pub fn paint(&mut self) -> Vec<RenderCommand> {
        Painter::paint(&mut self.render_objects, &mut self.dirty)
    }

    pub fn handle_event(&mut self, position: Point<Logical>,
                        event: &InputEvent, modifiers: Modifiers)
                        -> Option<Box<dyn Any>> {
        EventHandler::handle_event(
            &mut self.element_registry, &self.render_objects,
            &mut self.state, &self.build_owner, &self.dirty_sender,
            &mut self.focused_element, position, event, modifiers,
        );
    }

    pub fn hit_test(&self, position: Position<Logical, Absolute>) -> HitTestResult {
        EventHandler::hit_test(&self.render_objects, position)
    }

    // Accessors remain on pipeline
    pub fn focused_element(&self) -> Option<ElementKey> { self.focused_element }
    pub fn set_focus(&mut self, element: Option<ElementKey>) { self.focused_element = element }
    pub fn element_registry(&self) -> &ElementRegistry { &self.element_registry }
    pub fn render_objects(&self) -> &RenderObjectRegistry { &self.render_objects }
    pub fn build_owner(&self) -> &BuildOwner { &self.build_owner }
    pub fn needs_layout(&self) -> bool { !self.dirty.is_layout_empty() }
    pub fn needs_paint(&self) -> bool { !self.dirty.is_paint_empty() }
    pub fn clear_dirty(&mut self) { self.dirty.clear() }
    pub fn mark_all_needs_layout(&mut self) { ... }
    pub fn mark_needs_build(&mut self, element_id: ElementKey) { self.build_owner.mark_needs_build(element_id) }
    pub fn has_pending_rebuilds(&self) -> bool { self.build_owner.has_pending_rebuilds() }
}
```

**Public API unchanged.** All existing call sites work without modification.

## File Organization

New files in `vexo/src/retain/`:

```
retain/
├── reconciler.rs      # Reconciler struct + impl
├── layouter.rs        # Layouter struct + impl
├── painter.rs         # Painter struct + impl
├── event_handler.rs   # EventHandler struct + impl
├── pipeline.rs        # ThreeTreePipeline (thin orchestrator, ~100 lines)
├── ...                # existing files unchanged
```

`mod.rs` adds: `pub mod reconciler; pub mod layouter; pub mod painter; pub mod event_handler;`

## Testing

- Existing pipeline tests remain in `pipeline.rs` and continue to test the public API
- Each new struct can have its own focused unit tests in its file
- No behavioral changes — this is pure restructuring

## Scope

This is a single refactoring task. No new features, no behavioral changes. The only goal is SRP compliance for `ThreeTreePipeline`.