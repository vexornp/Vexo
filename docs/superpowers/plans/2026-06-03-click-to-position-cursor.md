# Click-to-Position Cursor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the blink cursor to the mouse-clicked position within a focused TextEdit widget.

**Architecture:** Add `inner_bounds` (deepest hit target bounds) to `HitTestResult`, expose `local_position` and `scale` in `EventContext`, then use them in `TextEditState::on_event()` to call `Action::Click` via a new `TextEditingController::click_at()` method. Follows Flutter's `globalToLocal` pattern.

**Tech Stack:** Rust, glyphon (Action::Click), Taffy layout

---

### Task 1: Add `inner_bounds` to `HitTestResult`

**Files:**
- Modify: `vexo/src/hit_test.rs`

- [ ] **Step 1: Add `inner_bounds` field to `HitTestResult` struct**

In `HitTestResult` struct (line 42-54), add after `absolute_bounds`:

```rust
/// Absolute bounds of the deepest (innermost) hit target in the hit path.
/// The deepest target's bounds exclude ancestor offsets (e.g., padding),
/// so `pointer_position - inner_bounds.origin` gives local coordinates
/// relative to the innermost render object.
inner_bounds: Option<Bounds<Logical>>,
```

- [ ] **Step 2: Update `HitTestResult` constructors**

Update `miss()` (line 57-65):
```rust
pub fn miss() -> Self {
    Self {
        path: Vec::new(),
        element_path: Vec::new(),
        absolute_bounds: None,
        inner_bounds: None,
        annotations: Vec::new(),
    }
}
```

Update `hit()` (line 68-70):
```rust
pub fn hit(path: Vec<RenderObjectKey>, element_path: Vec<ElementKey>) -> Self {
    Self { path, element_path, absolute_bounds: None, inner_bounds: None, annotations: Vec::new() }
}
```

Update `hit_with_bounds()` (line 73-84):
```rust
pub fn hit_with_bounds(
    path: Vec<RenderObjectKey>,
    element_path: Vec<ElementKey>,
    absolute_bounds: Bounds<Logical>,
) -> Self {
    Self {
        path,
        element_path,
        absolute_bounds: Some(absolute_bounds),
        inner_bounds: Some(absolute_bounds),
        annotations: Vec::new(),
    }
}
```

- [ ] **Step 3: Add `inner_bounds()` accessor**

After `absolute_bounds()` (line 122-124), add:

```rust
/// Get the absolute bounds of the deepest hit target.
///
/// Returns None if nothing was hit or bounds are not available.
pub fn inner_bounds(&self) -> Option<Bounds<Logical>> {
    self.inner_bounds
}
```

- [ ] **Step 4: Update `hit_test()` to include `inner_bounds` in result construction**

In `RenderObjectRegistry::hit_test()` (line 169-208), add `inner_bounds` variable and pass it through.

Add after line 172 (`let mut absolute_bounds`):
```rust
let mut inner_bounds: Option<Bounds<Logical>> = None;
```

Pass `inner_bounds` as a new parameter to `hit_test_recursive`:
```rust
self.hit_test_recursive(
    root,
    position,
    root_absolute_position,
    &mut path,
    &mut element_path,
    &mut absolute_bounds,
    &mut inner_bounds,
);
```

Update result construction (line 187-192):
```rust
let mut result = HitTestResult {
    path,
    element_path,
    absolute_bounds,
    inner_bounds,
    annotations: Vec::new(),
};
```

- [ ] **Step 5: Update `hit_test_recursive` to track deepest target bounds**

Add `inner_bounds` parameter to `hit_test_recursive` (line 225-297). The signature becomes:
```rust
fn hit_test_recursive(
    &self,
    id: RenderObjectKey,
    pointer_position: Position<Logical, Absolute>,
    parent_absolute_position: Position<Logical, Absolute>,
    path: &mut Vec<RenderObjectKey>,
    element_path: &mut Vec<ElementKey>,
    absolute_bounds: &mut Option<Bounds<Logical>>,
    inner_bounds: &mut Option<Bounds<Logical>>,
) -> bool
```

In the body (inside the `if is_inside` block), after computing the current object's absolute bounds (line 270-276), add:
```rust
// Track the deepest hit target's bounds.
// On each hit, we update inner_bounds. If a deeper child also hits,
// it will overwrite this with its own bounds. If no child hits,
// this value remains — it's the deepest target.
*inner_bounds = Some(Bounds::from_xywh(
    object_absolute_position.x,
    object_absolute_position.y,
    size.width,
    size.height,
));
```

Pass `inner_bounds` through to recursive calls (line 281-288):
```rust
for child in obj.children().iter().rev() {
    if self.hit_test_recursive(
        *child,
        pointer_position,
        object_absolute_position,
        path,
        element_path,
        absolute_bounds,
        inner_bounds,
    ) {
        return true;
    }
}
```

- [ ] **Step 6: Run `cargo build` to verify compilation**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEED

- [ ] **Step 7: Commit**

```bash
git add vexo/src/hit_test.rs
git commit -m "feat: add inner_bounds to HitTestResult for local coordinate conversion"
```

---

### Task 2: Add `local_position` and `scale` to `EventContext`

**Files:**
- Modify: `vexo/src/event_context.rs`

- [ ] **Step 1: Add fields to `EventContext` struct**

In `EventContext` struct (line 23-66), add after `clear_focus_request` (line 65):

```rust
/// Pointer position in the deepest hit target's local coordinate space.
/// Equivalent to Flutter's `localPosition` — computed as
/// `pointer_position - inner_bounds.origin`.
local_position: Point<Logical>,

/// DPI scale factor for converting logical to physical coordinates.
scale: Scale,
```

- [ ] **Step 2: Update `EventContext::new()`**

Update `new()` (line 68-90) to accept `local_position` and `scale`:

```rust
pub fn new(
    element_id: ElementKey,
    pointer_position: Point<Logical>,
    local_position: Point<Logical>,
    focused_element: Option<ElementKey>,
    bounds: Bounds<Logical>,
    modifiers: Modifiers,
    scale: Scale,
    font_system: &'a mut glyphon::FontSystem,
) -> Self {
    Self {
        element_id,
        pointer_position,
        local_position,
        focused_element,
        bounds,
        modifiers,
        scale,
        font_system,
        build_owner: None,
        dirty_sender: None,
        focus_request: None,
        clear_focus_request: false,
    }
}
```

- [ ] **Step 3: Update `EventContext::with_build_owner()`**

Update `with_build_owner()` (line 92-115) to accept `local_position` and `scale`:

```rust
pub fn with_build_owner(
    element_id: ElementKey,
    pointer_position: Point<Logical>,
    local_position: Point<Logical>,
    focused_element: Option<ElementKey>,
    bounds: Bounds<Logical>,
    modifiers: Modifiers,
    scale: Scale,
    font_system: &'a mut glyphon::FontSystem,
    build_owner: &'a BuildOwner,
    dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
) -> Self {
    Self {
        element_id,
        pointer_position,
        local_position,
        focused_element,
        bounds,
        modifiers,
        scale,
        font_system,
        build_owner: Some(build_owner),
        dirty_sender: Some(dirty_sender),
        focus_request: None,
        clear_focus_request: false,
    }
}
```

- [ ] **Step 4: Add public accessors**

After `is_pointer_inside()` (line 123-125), add:

```rust
/// Get the pointer position in the deepest hit target's local space.
/// Equivalent to Flutter's `localPosition`.
pub fn local_position(&self) -> Point<Logical> {
    self.local_position
}

/// Get the DPI scale factor.
pub fn scale(&self) -> Scale {
    self.scale
}
```

- [ ] **Step 5: Update existing tests to pass new parameters**

In `#[cfg(test)] mod tests` (line 190-335), update all `EventContext::new()` calls to include `local_position` and `scale`. For each test, set `local_position` equal to `pointer_position` (since these tests don't have hit test results) and `scale` to `Scale::default()`.

Example for `test_event_context_element_id` (line 214-227):
```rust
let ctx = EventContext::new(
    element,
    Point::zero(),
    Point::zero(), // local_position
    None,
    Bounds::default(),
    Modifiers::default(),
    Scale::default(),
    &mut font_system,
);
```

Apply the same pattern to all other test calls in the file. For `with_build_owner` test calls that don't exist yet in the test file, they don't need changes.

- [ ] **Step 6: Add `Scale` import**

Add to the imports at top of file (line 9):
```rust
use crate::core::{Bounds, Logical, Point, Scale};
```

- [ ] **Step 7: Run `cargo build` to check**

Run: `cargo build -p vexo`
Expected: compile errors at call sites in `event_handler.rs` — that's expected, we'll fix in the next task. The test errors should be fixed by Step 5.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/event_context.rs
git commit -m "feat: add local_position and scale fields to EventContext"
```

---

### Task 3: Wire `local_position` and `scale` through `EventHandler`, `Pipeline`, and `Window`

**Files:**
- Modify: `vexo/src/event_handler.rs`
- Modify: `vexo/src/pipeline.rs`
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Update `EventHandler::handle_event()` to accept `scale`**

In `EventHandler::handle_event()` (line 38-87), add `scale: Scale` parameter after `modifiers: Modifiers`:

```rust
pub fn handle_event(
    element_registry: &mut ElementRegistry,
    render_objects: &RenderObjectRegistry,
    state: &mut StateStorage,
    font_system: &mut glyphon::FontSystem,
    build_owner: &BuildOwner,
    dirty_sender: &mpsc::Sender<ElementKey>,
    focus_manager: &mut FocusManager,
    _position: Point<Logical>,
    event: &InputEvent,
    modifiers: Modifiers,
    scale: Scale,
) -> Option<Box<dyn Any>>
```

Pass `scale` to `handle_pointer_event()` and `handle_keyboard_event()`.

- [ ] **Step 2: Update `EventHandler::handle_pointer_event()` to accept `scale` and compute `local_position`**

Add `scale: Scale` parameter to `handle_pointer_event()` (line 99-181). After `modifiers: Modifiers`:

```rust
pub(crate) fn handle_pointer_event(
    ...
    modifiers: Modifiers,
    scale: Scale,
) -> Option<Box<dyn Any>>
```

After line 130 (`let bounds = hit_result.absolute_bounds().unwrap_or_default();`), compute `local_position`:

```rust
let local_position = hit_result
    .inner_bounds()
    .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
    .unwrap_or(position);
```

Update `EventContext::with_build_owner()` call (line 139-148) to include `local_position` and `scale`:

```rust
let mut ctx = EventContext::with_build_owner(
    element_id,
    position,
    local_position,
    focus_manager.primary_focus_element(),
    bounds,
    modifiers,
    scale,
    font_system,
    build_owner,
    dirty_sender,
);
```

- [ ] **Step 3: Update `EventHandler::handle_keyboard_event()` to accept `scale`**

Add `scale: Scale` parameter. Update `EventContext::with_build_owner()` call (line 200-209) to include `local_position` and `scale`. For keyboard events, `local_position` is `Point::zero()` since there's no pointer position:

```rust
let mut ctx = EventContext::with_build_owner(
    focused,
    Point::zero(),
    Point::zero(), // no pointer position for keyboard events
    focus_manager.primary_focus_element(),
    bounds,
    modifiers,
    scale,
    font_system,
    build_owner,
    dirty_sender,
);
```

- [ ] **Step 4: Update `ThreeTreePipeline::handle_event()` to accept and forward `scale`**

In `pipeline.rs`, change `handle_event()` signature (line 445-451) to include `scale`:

```rust
pub fn handle_event(
    &mut self,
    position: Point<Logical>,
    event: &InputEvent,
    modifiers: Modifiers,
    font_system: &mut glyphon::FontSystem,
    scale: Scale,
) -> Option<Box<dyn Any>>
```

Pass `scale` to `EventHandler::handle_event()` (line 452-463):

```rust
let result = EventHandler::handle_event(
    &mut self.element_registry,
    &self.render_objects,
    &mut self.state,
    font_system,
    &self.build_owner,
    &self.dirty_sender,
    &mut self.focus_manager,
    position,
    event,
    modifiers,
    scale,
);
```

- [ ] **Step 5: Update `window.rs` call site**

In `WindowState::process_input_event()` (line 186), pass `self.scale`:

```rust
let _message = pipeline.handle_event(position, &input_event, modifiers, &mut self.font_system, self.scale);
```

- [ ] **Step 6: Run `cargo build` to verify full compilation**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEED

- [ ] **Step 7: Commit**

```bash
git add vexo/src/event_handler.rs vexo/src/pipeline.rs vexo/src/window.rs
git commit -m "feat: wire local_position and scale through event handling pipeline"
```

---

### Task 4: Add `click_at()` to `TextEditingController`

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs`

- [ ] **Step 1: Add `click_at()` method to `TextEditingController`**

Add after `insert_newline()` (line 169-176) in `impl TextEditingController`:

```rust
/// Position the cursor at the given buffer-relative pixel coordinates.
///
/// Converts the click location to a cursor position using glyphon's
/// `Action::Click`. The x and y are in physical pixels relative to the
/// text buffer's top-left corner.
pub fn click_at(&self, x: i32, y: i32, font_system: &mut glyphon::FontSystem) {
    let mut editor = self.editor.borrow_mut();
    editor.action(font_system, Action::Click { x, y });
    editor.shape_as_needed(font_system, true);
    drop(editor);
    self.notify();
}
```

- [ ] **Step 2: Add the `Action` import**

At the top of the file, the imports already include `glyphon::Action` via `use glyphon::{Action, Attrs, Buffer, Edit, Metrics, Shaping};` (check line ~5). If `Action` is not imported, add it.

- [ ] **Step 3: Run `cargo build` to verify compilation**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEED

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/text_edit.rs
git commit -m "feat: add click_at() to TextEditingController for cursor positioning"
```

---

### Task 5: Update `TextEditState::on_event()` to position cursor on click

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs`
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Update `on_event()` to call `click_at()` on pointer press**

In `TextEditState::on_event()` (line 258-350), update the `InputEvent::PointerButton` match arm (line 270-276). Currently:

```rust
InputEvent::PointerButton {
    state: ButtonState::Pressed,
    ..
} => {
    ctx.request_focus(ctx.element_id());
    Some(Box::new(()))
}
```

Change to:

```rust
InputEvent::PointerButton {
    state: ButtonState::Pressed,
    ..
} => {
    ctx.request_focus(ctx.element_id());

    // Position cursor at click location (Flutter's selectPositionAt pattern)
    let local = ctx.local_position();
    let scale = ctx.scale();
    let physical_x = (local.x * scale.factor()) as i32;
    let physical_y = (local.y * scale.factor()) as i32;
    text_edit.controller.click_at(physical_x, physical_y, ctx.font_system);

    Some(Box::new(()))
}
```

- [ ] **Step 2: Add cursor blink reset on pointer click in `window.rs`**

In `WindowState::process_input_input()` (line 172-237), the existing code resets cursor blink for keyboard events (line 189-193). Add similar logic for pointer button events:

```rust
// Reset cursor blink on pointer click so cursor is visible immediately at new position
if matches!(input_event, InputEvent::PointerButton { state: ButtonState::Pressed, .. }) {
    if pipeline.reset_cursor_blink() {
        pipeline.mark_focus_subtree_needs_paint();
    }
}
```

Add this after the keyboard-input blink reset block (after line 193).

- [ ] **Step 3: Add `ButtonState` import to `window.rs`**

Add `use crate::input::ButtonState;` to the imports in `window.rs` if not already present. Check existing imports first.

- [ ] **Step 4: Run `cargo build` to verify compilation**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEED

- [ ] **Step 5: Run `cargo test` to verify existing tests pass**

Run: `cargo test -p vexo`
Expected: All existing tests PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/text_edit.rs vexo/src/window.rs
git commit -m "feat: position cursor at click location in TextEdit"
```

---

### Task 6: Manual testing

**Files:**
- None (manual verification)

- [ ] **Step 1: Run desktop demo**

Run: `cargo run -p desktop_demo`

- [ ] **Step 2: Verify click-to-position cursor behavior**

1. Click on a text field to focus it
2. Type some text
3. Click in the middle of the text — cursor should move to the clicked position
4. Click at the beginning of the text — cursor should move there
5. Click at the end of the text — cursor should move there
6. Click on an empty text field — cursor should appear at click position
7. Type after repositioning — text should insert at the new cursor position