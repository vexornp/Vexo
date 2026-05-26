# Pipeline SRP Decomposition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract ThreeTreePipeline into 4 zero-sized structs (Reconciler, Layouter, Painter, EventHandler) to respect SRP, while keeping the pipeline's public API unchanged.

**Architecture:** Zero-sized structs with associated functions. Pipeline owns all state and delegates by passing `&mut` references. No behavioral changes — pure restructuring.

**Tech Stack:** Rust, existing vexo retain mode infrastructure

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `vexo/src/retain/reconciler.rs` | Create | Reconciler struct + impl (reconciliation, lifecycle, rebuilds) |
| `vexo/src/retain/layouter.rs` | Create | Layouter struct + impl (3-phase layout) |
| `vexo/src/retain/painter.rs` | Create | Painter struct + impl (render command generation) |
| `vexo/src/retain/event_handler.rs` | Create | EventHandler struct + impl (hit test, event dispatch, focus) |
| `vexo/src/retain/pipeline.rs` | Modify | Thin orchestrator — remove extracted method bodies, delegate to structs |
| `vexo/src/retain/mod.rs` | Modify | Add new module declarations |

---

### Task 1: Create Layouter

**Why first:** Layouter has the cleanest boundary — it only needs `render_objects` and `dirty`. No cross-dependencies with other structs. Validates the pattern before extracting more complex structs.

**Files:**
- Create: `vexo/src/retain/layouter.rs`
- Modify: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create `layouter.rs` with the Layouter struct and extracted methods**

Read `pipeline.rs` and copy these methods into `layouter.rs` as associated functions on `pub struct Layouter;`:
- `layout()` → `Layouter::layout()`
- `layout_build_recursive()` → `Layouter::layout_build_recursive()`
- `apply_layout_recursive()` → `Layouter::apply_layout_recursive()`
- `get_layout_node()` → `Layouter::get_layout_node()`

Each method's `&self` / `&mut self` parameters become explicit parameters. For example:

```rust
// Before (on ThreeTreePipeline):
pub fn layout(&mut self, available_size: Size<Logical>, engine: &mut dyn LayoutEngine, font_system: &mut glyphon::FontSystem) {
    // body...
}

// After (on Layouter):
pub fn layout(
    render_objects: &mut RenderObjectRegistry,
    dirty: &mut DirtyTracking,
    available_size: Size<Logical>,
    engine: &mut dyn LayoutEngine,
    font_system: &mut glyphon::FontSystem,
) {
    // same body, but self.render_objects → render_objects, self.dirty → dirty
}
```

Replace all `self.render_objects` with `render_objects` and `self.dirty` with `dirty` in the method bodies. For private helper methods (`layout_build_recursive`, `apply_layout_recursive`, `get_layout_node`), make them `pub(crate)` associated functions with the same parameter transformation.

Add necessary `use` statements at the top of `layouter.rs` (copy from pipeline.rs, keep only what's needed).

- [ ] **Step 2: Add `pub mod layouter;` to `mod.rs`**

In `vexo/src/retain/mod.rs`, add:
```rust
pub mod layouter;
```

- [ ] **Step 3: Replace method bodies in `pipeline.rs` with delegation**

Replace the body of each extracted method in `ThreeTreePipeline` with a delegation call:

```rust
pub fn layout(&mut self, available_size: Size<Logical>, engine: &mut dyn LayoutEngine, font_system: &mut glyphon::FontSystem) {
    Layouter::layout(&mut self.render_objects, &mut self.dirty, available_size, engine, font_system)
}
```

Remove the now-unused private helper methods (`layout_build_recursive`, `apply_layout_recursive`, `get_layout_node`) from `ThreeTreePipeline` — they now live in `Layouter`.

Add `use super::layouter::Layouter;` at the top of `pipeline.rs`.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/layouter.rs vexo/src/retain/pipeline.rs vexo/src/retain/mod.rs
git commit -m "refactor: extract Layouter from ThreeTreePipeline"
```

---

### Task 2: Create Painter

**Why second:** Painter also has a clean boundary — only `render_objects` and `dirty`. Same pattern as Layouter, reinforces it.

**Files:**
- Create: `vexo/src/retain/painter.rs`
- Modify: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create `painter.rs` with the Painter struct and extracted methods**

Read `pipeline.rs` and copy these methods into `painter.rs` as associated functions on `pub struct Painter;`:
- `paint()` → `Painter::paint()`
- `paint_recursive()` → `Painter::paint_recursive()`

Transform `self.render_objects` → `render_objects`, `self.dirty` → `dirty`. Make `paint_recursive` `pub(crate)`.

- [ ] **Step 2: Add `pub mod painter;` to `mod.rs`**

- [ ] **Step 3: Replace method bodies in `pipeline.rs` with delegation**

```rust
pub fn paint(&mut self) -> Vec<RenderCommand> {
    Painter::paint(&mut self.render_objects, &mut self.dirty)
}
```

Remove `paint_recursive` from `ThreeTreePipeline`. Add `use super::painter::Painter;`.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/painter.rs vexo/src/retain/pipeline.rs vexo/src/retain/mod.rs
git commit -m "refactor: extract Painter from ThreeTreePipeline"
```

---

### Task 3: Create EventHandler

**Why third:** EventHandler has a wider boundary (element_registry, render_objects, state, build_owner, dirty_sender, focused_element) but is still self-contained — it doesn't call back into reconciliation or layout.

**Files:**
- Create: `vexo/src/retain/event_handler.rs`
- Modify: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create `event_handler.rs` with the EventHandler struct and extracted methods**

Read `pipeline.rs` and copy these methods into `event_handler.rs` as associated functions on `pub struct EventHandler;`:
- `handle_event()` → `EventHandler::handle_event()`
- `handle_pointer_event()` → `EventHandler::handle_pointer_event()`
- `handle_keyboard_event()` → `EventHandler::handle_keyboard_event()`
- `hit_test()` → `EventHandler::hit_test()`

Transform self-field references to explicit parameters. `focused_element` becomes `&mut Option<ElementKey>` since event handling mutates focus. Make `handle_pointer_event` and `handle_keyboard_event` `pub(crate)`.

- [ ] **Step 2: Add `pub mod event_handler;` to `mod.rs`**

- [ ] **Step 3: Replace method bodies in `pipeline.rs` with delegation**

```rust
pub fn handle_event(&mut self, position: Point<Logical>, event: &InputEvent, modifiers: Modifiers) -> Option<Box<dyn Any>> {
    EventHandler::handle_event(
        &mut self.element_registry, &self.render_objects,
        &mut self.state, &self.build_owner, &self.dirty_sender,
        &mut self.focused_element, position, event, modifiers,
    )
}

pub fn hit_test(&self, position: Position<Logical, Absolute>) -> HitTestResult {
    EventHandler::hit_test(&self.render_objects, position)
}
```

Remove the private helper methods from `ThreeTreePipeline`. Add `use super::event_handler::EventHandler;`.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/event_handler.rs vexo/src/retain/pipeline.rs vexo/src/retain/mod.rs
git commit -m "refactor: extract EventHandler from ThreeTreePipeline"
```

---

### Task 4: Create Reconciler

**Why last:** Reconciler is the most complex — it touches the most state and its methods call each other. Doing it last means the other 3 structs are already validated, and the remaining methods in the pipeline are clearly the reconciler's.

**Files:**
- Create: `vexo/src/retain/reconciler.rs`
- Modify: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create `reconciler.rs` with the Reconciler struct and extracted methods**

Read `pipeline.rs` and copy ALL remaining non-trivial methods into `reconciler.rs` as associated functions on `pub struct Reconciler;`:
- `reconcile()` → `Reconciler::reconcile()`
- `reconcile_element()` → `Reconciler::reconcile_element()`
- `rebuild_root()` → `Reconciler::rebuild_root()`
- `rebuild_element()` → `Reconciler::rebuild_element()`
- `mount_element_tree()` → `Reconciler::mount_element_tree()`
- `unmount_element_tree()` → `Reconciler::unmount_element_tree()`
- `execute_child_ops()` → `Reconciler::execute_child_ops()`
- `perform_rebuilds()` → `Reconciler::perform_rebuilds()`
- `drain_dirty_channel()` → `Reconciler::drain_dirty_channel()`
- `update()` → `Reconciler::update()`
- `update_state_only()` → `Reconciler::update_state_only()`

Transform all `self.field` references to explicit parameters. The parameter list will be large but consistent across methods — they all need some combination of: `element_registry`, `render_objects`, `state`, `dirty`, `build_owner`, `child_ops`, `dirty_sender`, `dirty_receiver`, `needs_full_reconcile`.

Make internal methods (`reconcile_element`, `rebuild_root`, `rebuild_element`, `mount_element_tree`, `unmount_element_tree`, `execute_child_ops`, `drain_dirty_channel`) `pub(crate)`. Keep `reconcile`, `update`, `perform_rebuilds`, `update_state_only` as `pub`.

- [ ] **Step 2: Add `pub mod reconciler;` to `mod.rs`**

- [ ] **Step 3: Replace method bodies in `pipeline.rs` with delegation**

For each extracted method, replace the body with a delegation call. Example:

```rust
pub fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
    Reconciler::reconcile(
        &mut self.element_registry, &mut self.render_objects,
        &mut self.state, &mut self.dirty, &mut self.build_owner,
        &mut self.child_ops, &self.dirty_sender, &self.dirty_receiver,
        &mut self.needs_full_reconcile, root_widget,
    )
}
```

Remove all the private helper methods from `ThreeTreePipeline`. Add `use super::reconciler::Reconciler;`.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/reconciler.rs vexo/src/retain/pipeline.rs vexo/src/retain/mod.rs
git commit -m "refactor: extract Reconciler from ThreeTreePipeline"
```

---

### Task 5: Clean up pipeline.rs

**Why:** After extraction, pipeline.rs should only contain the struct definition, `new()`, delegation methods, and simple accessors. Verify it's clean and remove any unused imports.

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

- [ ] **Step 1: Remove unused imports from pipeline.rs**

After all extractions, many `use` statements in pipeline.rs will be unused. Remove them. The file should only import what's needed for:
- The struct field types
- The 4 struct imports (`use super::reconciler::Reconciler;` etc.)
- Types used in delegation signatures

- [ ] **Step 2: Verify pipeline.rs is a thin orchestrator**

Read the final `pipeline.rs`. It should contain:
- Struct definition with all fields
- `new()` constructor
- Delegation methods (`reconcile`, `update`, `layout`, `paint`, `handle_event`, `hit_test`, `perform_rebuilds`, `update_state_only`)
- Simple accessors (`focused_element`, `set_focus`, `element_registry`, `render_objects`, `build_owner`, `needs_layout`, `needs_paint`, `clear_dirty`, `mark_all_needs_layout`, `mark_needs_build`, `has_pending_rebuilds`)

No method should have more than ~10 lines (just the delegation call).

- [ ] **Step 3: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: All builds and tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "refactor: clean up pipeline.rs imports after SRP extraction"
```

---

### Task 6: Final verification

- [ ] **Step 1: Run full build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: All builds and tests pass

- [ ] **Step 2: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: App launches and behaves identically to before the refactoring

- [ ] **Step 3: Verify no behavioral changes**

Confirm:
- All public methods on `ThreeTreePipeline` still exist with the same signatures
- No new public API was added to the pipeline
- The 4 new structs are `pub` but their methods are `pub` or `pub(crate)` as appropriate
- `mod.rs` exports all 4 new modules
