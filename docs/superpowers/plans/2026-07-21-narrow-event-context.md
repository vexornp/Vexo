# Narrow EventContext Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove 9 dead methods and 3 dead fields from `EventContext`, make the 6 externally-read `pub` fields private with accessors, and migrate all callers. Single-commit mechanical refactor, no behavior change.

**Architecture:** Single-phase narrowing in 5 files: `vexo/src/event_context.rs` (struct/constructor/methods/tests/docstring), `vexo/src/widgets/text_edit.rs` (4 field→accessor migrations, ~20 sites), `vexo/src/elements/scroll_view.rs` (2 field→accessor migrations), `vexo/src/event_handler.rs` (7 constructor callsites drop 3 args), `vexo/src/widgets/gesture_detector.rs` (3 test constructor callsites drop 3 args). The crate does not compile until all 5 files are updated, so the plan is structured as: edit `event_context.rs` first (struct + constructors + accessors + delete dead methods/tests), then update the 4 caller files, then build + test + commit.

**Tech Stack:** Rust workspace (`vexo`, `vexo_uikit`, `shared_app` crates). Standard `cargo build` / `cargo test --workspace` workflow per `CLAUDE.md`.

## Global Constraints

- All `EventContext` fields end up private (3 removed, 6 made private, 5 already private).
- `EventContext::new()` and `EventContext::with_build_owner()` stay `pub fn`. Do not narrow visibility.
- No changes to `RenderContext`, `LifecycleContext`, `ElementContext`, `LayoutContext`, `PaintContext`, or `BuildOwner`.
- No changes to `Element::on_event` or `ComponentState::on_event` signatures.
- No changes to user-visible behavior. The removed methods are dead (zero callers in production code); the removed fields only fed those methods; the fields-made-private are accessed through accessors that return the same value.
- Single-phase: ONE commit at the end.
- Do NOT run `cargo run -p desktop_demo` (per CLAUDE.md).
- After all edits, run: `cargo build -p vexo && cargo build -p vexo_uikit && cargo build -p shared_app && cargo test --workspace`.
- The 2 `setState` references in `vexo/src/build_owner.rs:30` and `vexo/src/pipeline.rs:692` are Flutter-model comparisons in a DIFFERENT context (LifecycleContext narrowing, already done). LEAVE THEM ALONE.

---

### Task 1: Narrow `EventContext` struct, constructors, and methods

**Files:**
- Modify: `vexo/src/event_context.rs:1-242` (struct docstring, struct fields, both constructors, all methods)

**Interfaces:**
- Consumes: `BuildOwner`, `RenderObjectRegistry`, `glyphon::FontSystem`, `crate::platform::Clipboard`, `ScaleSource`, `Modifiers`, `Bounds`, `Point`, `ElementKey` (all unchanged types from existing imports).
- Produces:
  - `EventContext::new(element_id, local_position, bounds, modifiers, font_system, render_objects, clipboard)` — 7 args (was 10)
  - `EventContext::with_build_owner(element_id, local_position, bounds, modifiers, font_system, build_owner, dirty_sender, render_objects, clipboard)` — 9 args (was 12)
  - New accessors: `bounds()`, `modifiers()`, `font_system()`, `clipboard()`, `build_owner()`, `dirty_sender()`
  - Removed methods: `is_pointer_inside`, `is_focused` (arg form), `is_focused_self`, `has_focus`, `scale`, `is_control_pressed`, `is_shift_pressed`, `is_alt_pressed`, `mark_needs_build`

- [ ] **Step 1: Reword the struct docstring**

In `vexo/src/event_context.rs`, find the docstring at the top of the file (lines 16-23):

```rust
/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - The element's own ID (for focus requests and focus checks)
/// - Pointer position for hit testing
/// - Focus state for keyboard event routing
/// - Font system for text editing operations
/// - Build owner for marking elements dirty from event handlers
```

Replace with:

```rust
/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - The element's own ID (for focus requests)
/// - The pointer position in the hit target's local space
/// - Bounds, keyboard modifiers, and font system for text editing
/// - Clipboard for copy/paste/cut
/// - Build owner and dirty sender for marking elements dirty from event handlers
```

- [ ] **Step 2: Narrow the struct fields**

Find the struct definition (currently lines 24-88):

```rust
pub struct EventContext<'a> {
    /// The element receiving this event.
    element_id: ElementKey,

    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,

    /// Currently focused element (if any).
    pub focused_element: Option<ElementKey>,

    /// Bounds of the element receiving the event.
    pub bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    pub modifiers: Modifiers,

    /// Font system for text editing operations.
    ///
    /// Required by TextEdit for editor actions (insert, delete, cursor movement)
    /// which need font_system for text shaping.
    pub font_system: &'a mut glyphon::FontSystem,

    /// Clipboard access for copy/paste/cut operations.
    ///
    /// Shared via `Arc` so that the same backend (arboard on desktop, stub on iOS)
    /// can be cheaply cloned into every `EventContext` constructed during event
    /// dispatch without taking ownership of the underlying platform handle.
    pub clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,

    /// Build owner for marking elements dirty from event handlers.
    ///
    /// Uses a shared reference (`&BuildOwner`) because `mark_needs_build()`
    /// takes `&self` via RefCell interior mutability.
    ///
    /// This is `Some` when the pipeline provides BuildOwner access
    /// (which is the normal case), and `None` in test contexts.
    pub build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from Signal callbacks.
    ///
    /// When a `Signal::set()` fires its dirty callback from within
    /// an event handler, it sends the element ID through this channel.
    pub dirty_sender: Option<&'a std::sync::mpsc::Sender<ElementKey>>,

    /// Render object registry for element-to-render-object communication.
    ///
    /// Available when the event handler provides render object access.
    /// Used by scroll-aware elements to query render object state.
    render_objects: Option<&'a RenderObjectRegistry>,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementKey>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,

    /// Pointer position in the deepest hit target's local coordinate space.
    /// Equivalent to Flutter's `localPosition` — computed as
    /// `pointer_position - inner_bounds.origin`.
    local_position: Point<Logical>,

    /// Shared scale factor source.
    scale_source: ScaleSource,
}
```

Replace with (3 fields removed: `pointer_position`, `focused_element`, `scale_source`; 6 fields made private: `bounds`, `modifiers`, `font_system`, `clipboard`, `build_owner`, `dirty_sender`):

```rust
pub struct EventContext<'a> {
    /// The element receiving this event.
    element_id: ElementKey,

    /// Bounds of the element receiving the event.
    bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    modifiers: Modifiers,

    /// Font system for text editing operations.
    ///
    /// Required by TextEdit for editor actions (insert, delete, cursor movement)
    /// which need font_system for text shaping.
    font_system: &'a mut glyphon::FontSystem,

    /// Clipboard access for copy/paste/cut operations.
    ///
    /// Shared via `Arc` so that the same backend (arboard on desktop, stub on iOS)
    /// can be cheaply cloned into every `EventContext` constructed during event
    /// dispatch without taking ownership of the underlying platform handle.
    clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,

    /// Build owner for marking elements dirty from event handlers.
    ///
    /// Uses a shared reference (`&BuildOwner`) because `mark_needs_build()`
    /// takes `&self` via RefCell interior mutability.
    ///
    /// This is `Some` when the pipeline provides BuildOwner access
    /// (which is the normal case), and `None` in test contexts.
    build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from Signal callbacks.
    ///
    /// When a `Signal::set()` fires its dirty callback from within
    /// an event handler, it sends the element ID through this channel.
    dirty_sender: Option<&'a std::sync::mpsc::Sender<ElementKey>>,

    /// Render object registry for element-to-render-object communication.
    ///
    /// Available when the event handler provides render object access.
    /// Used by scroll-aware elements to query render object state.
    render_objects: Option<&'a RenderObjectRegistry>,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementKey>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,

    /// Pointer position in the deepest hit target's local coordinate space.
    /// Equivalent to Flutter's `localPosition` — computed as
    /// `pointer_position - inner_bounds.origin`.
    local_position: Point<Logical>,
}
```

- [ ] **Step 3: Narrow the `new()` constructor**

Find the `new` function (currently lines 90-120):

```rust
    /// Create a new event context.
    pub fn new(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale_source: ScaleSource,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale_source,
            font_system,
            clipboard,
            build_owner: None,
            dirty_sender: None,
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }
```

Replace with (3 args dropped: `pointer_position`, `focused_element`, `scale_source`):

```rust
    /// Create a new event context.
    pub fn new(
        element_id: ElementKey,
        local_position: Point<Logical>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            local_position,
            bounds,
            modifiers,
            font_system,
            clipboard,
            build_owner: None,
            dirty_sender: None,
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }
```

- [ ] **Step 4: Narrow the `with_build_owner()` constructor**

Find the `with_build_owner` function (currently lines 122-153):

```rust
    /// Create a new event context with BuildOwner access.
    pub fn with_build_owner(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale_source: ScaleSource,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale_source,
            font_system,
            clipboard,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }
```

Replace with (3 args dropped: `pointer_position`, `focused_element`, `scale_source`):

```rust
    /// Create a new event context with BuildOwner access.
    pub fn with_build_owner(
        element_id: ElementKey,
        local_position: Point<Logical>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            local_position,
            bounds,
            modifiers,
            font_system,
            clipboard,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }
```

- [ ] **Step 5: Remove 9 dead methods, add 6 accessors**

Find the impl block methods section (currently lines 155-242). The methods to remove are `is_pointer_inside`, `is_focused_self`, `is_focused` (arg form), `has_focus`, `scale`, `is_control_pressed`, `is_shift_pressed`, `is_alt_pressed`, `mark_needs_build`. The methods to keep are `element_id`, `local_position`, `request_focus`, `clear_focus`, `focus_request`, `should_clear_focus`, `render_objects`.

Find this exact block (currently lines 160-242):

```rust
    /// Check if the pointer is inside the element bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Get the pointer position in the deepest hit target's local space.
    /// Equivalent to Flutter's `localPosition`.
    pub fn local_position(&self) -> Point<Logical> {
        self.local_position
    }

    /// Get the DPI scale factor.
    pub fn scale(&self) -> Scale {
        self.scale_source.get()
    }

    /// Check if this element is currently focused.
    /// Uses the element's own ID stored in this context.
    pub fn is_focused_self(&self) -> bool {
        self.focused_element == Some(self.element_id)
    }

    /// Check if a specific element is currently focused.
    pub fn is_focused(&self, element: ElementKey) -> bool {
        self.focused_element == Some(element)
    }

    /// Check if any element has focus.
    pub fn has_focus(&self) -> bool {
        self.focused_element.is_some()
    }

    /// Request focus for an element.
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementKey) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
    }

    /// Request to clear focus from the currently focused element.
    pub fn clear_focus(&mut self) {
        self.clear_focus_request = true;
        self.focus_request = None;
    }

    /// Get the focus request (if any).
    pub fn focus_request(&self) -> Option<ElementKey> {
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

    /// Mark an element as needing rebuild.
    pub fn mark_needs_build(&self, element_id: ElementKey) {
        if let Some(bo) = self.build_owner {
            bo.mark_needs_build(element_id);
        }
    }

    /// Get the render object registry, if available.
    pub fn render_objects(&self) -> Option<&RenderObjectRegistry> {
        self.render_objects
    }
}
```

Replace with (9 methods removed, 6 accessors added):

```rust
    /// Get the pointer position in the deepest hit target's local space.
    /// Equivalent to Flutter's `localPosition`.
    pub fn local_position(&self) -> Point<Logical> {
        self.local_position
    }

    /// Get the bounds of the element receiving the event.
    pub fn bounds(&self) -> Bounds<Logical> {
        self.bounds
    }

    /// Get the keyboard modifiers active during this event.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Get the font system for text editing operations.
    ///
    /// Returns `&mut` so handlers can pass it to controller methods
    /// (insert, delete, cursor movement) that need font_system for
    /// text shaping.
    pub fn font_system(&mut self) -> &mut glyphon::FontSystem {
        self.font_system
    }

    /// Get the clipboard for copy/paste/cut operations.
    pub fn clipboard(&self) -> &std::sync::Arc<dyn crate::platform::Clipboard> {
        &self.clipboard
    }

    /// Get the build owner, if available.
    ///
    /// Returns `None` in test contexts; `Some` in production.
    pub fn build_owner(&self) -> Option<&BuildOwner> {
        self.build_owner
    }

    /// Get the dirty sender, if available.
    ///
    /// Used by `Signal::set()` callbacks fired from event handlers
    /// to send the element ID through the channel for rebuild scheduling.
    pub fn dirty_sender(&self) -> Option<&std::sync::mpsc::Sender<ElementKey>> {
        self.dirty_sender
    }

    /// Request focus for an element.
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementKey) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
    }

    /// Request to clear focus from the currently focused element.
    pub fn clear_focus(&mut self) {
        self.clear_focus_request = true;
        self.focus_request = None;
    }

    /// Get the focus request (if any).
    pub fn focus_request(&self) -> Option<ElementKey> {
        self.focus_request
    }

    /// Check if the element requested to clear focus.
    pub fn should_clear_focus(&self) -> bool {
        self.clear_focus_request
    }

    /// Get the render object registry, if available.
    pub fn render_objects(&self) -> Option<&RenderObjectRegistry> {
        self.render_objects
    }
}
```

- [ ] **Step 6: Delete 3 tests for removed methods**

In the `#[cfg(test)] mod tests` block at the bottom of `vexo/src/event_context.rs`, delete these three test functions entirely:

1. `test_event_context_is_pointer_inside` (currently lines 291-323) — tests the removed `is_pointer_inside()`.

2. `test_event_context_is_focused_self` (currently lines 325-357) — tests the removed `is_focused_self()`.

3. `test_event_context_modifiers` (currently lines 404-424) — tests the removed `is_control_pressed()` / `is_shift_pressed()` / `is_alt_pressed()`.

Delete each function in full, including its `#[test]` attribute and the trailing blank line.

- [ ] **Step 7: Update the 3 kept tests to use the narrowed constructor**

After deleting the 3 tests above, 3 tests remain that construct `EventContext::new(...)`. Each currently passes 9 args; after narrowing, passes 6 args (drops `pointer_position`, `focused_element`, `scale_source`).

Find `test_event_context_element_id` (currently around lines 272-289):

```rust
    #[test]
    fn test_event_context_element_id() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );
        assert_eq!(ctx.element_id(), element);
    }
```

Replace with:

```rust
    #[test]
    fn test_event_context_element_id() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );
        assert_eq!(ctx.element_id(), element);
    }
```

Find `test_event_context_focus_request` (currently around lines 359-380):

```rust
    #[test]
    fn test_event_context_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );

        let target = make_key();
        ctx.request_focus(target);
        assert_eq!(ctx.focus_request(), Some(target));
        assert!(!ctx.should_clear_focus());
    }
```

Replace with:

```rust
    #[test]
    fn test_event_context_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );

        let target = make_key();
        ctx.request_focus(target);
        assert_eq!(ctx.focus_request(), Some(target));
        assert!(!ctx.should_clear_focus());
    }
```

Find `test_event_context_clear_focus_request` (currently around lines 382-402):

```rust
    #[test]
    fn test_event_context_clear_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );

        ctx.clear_focus();
        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }
```

Replace with:

```rust
    #[test]
    fn test_event_context_clear_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
        test_clipboard(),
        );

        ctx.clear_focus();
        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }
```

- [ ] **Step 8: Clean up unused imports in event_context.rs**

After the edits, check whether `ScaleSource` and `Scale` are still used in `event_context.rs`. Run:

```bash
rg "ScaleSource|Scale" vexo/src/event_context.rs
```

If `ScaleSource` only appears in the now-deleted `scale_source` field/arg/test code, remove it from the import on line 9:

```rust
use crate::core::{Bounds, Logical, Point, Scale, ScaleSource};
```

Replace with:

```rust
use crate::core::{Bounds, Logical, Point};
```

If `ScaleSource` or `Scale` still appears anywhere else in the file, leave the import alone. The grep output is the source of truth — do not guess.

- [ ] **Step 9: Do NOT build yet**

The crate will not compile until Tasks 2-5 update the caller files. Proceed to Task 2.

---

### Task 2: Migrate `text_edit.rs` field accesses to accessors

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs:390` (`ctx.bounds.height()` → `ctx.bounds().height()`)
- Modify: `vexo/src/widgets/text_edit.rs:409` (`ctx.modifiers` → `ctx.modifiers()`)
- Modify: `vexo/src/widgets/text_edit.rs:396, 418, 425, 432, 439, 446, 453, 457, 460, 463, 474, 482, 488, 510, 514` (`ctx.font_system` → `ctx.font_system()`)
- Modify: `vexo/src/widgets/text_edit.rs:478, 483, 487` (`ctx.clipboard` → `ctx.clipboard()`)

**Interfaces:**
- Consumes: Task 1's new accessors `bounds()`, `modifiers()`, `font_system()`, `clipboard()`.
- Produces: `text_edit.rs` no longer touches `EventContext` fields directly.

- [ ] **Step 1: Migrate `ctx.bounds.height()`**

Find this line (line 390):

```rust
                let vertical_offset = ((ctx.bounds.height() - text_height) / 2.0).max(0.0);
```

Replace with:

```rust
                let vertical_offset = ((ctx.bounds().height() - text_height) / 2.0).max(0.0);
```

- [ ] **Step 2: Migrate `ctx.modifiers`**

Find this line (line 409):

```rust
                let modifiers = ctx.modifiers;
```

Replace with:

```rust
                let modifiers = ctx.modifiers();
```

- [ ] **Step 3: Migrate all `ctx.font_system` to `ctx.font_system()`**

There are ~15 sites where `ctx.font_system` is passed as an argument to controller methods. Each is a standalone expression. Use `replaceAll: true` semantics: every occurrence of the exact substring `ctx.font_system` (without trailing `()`) must become `ctx.font_system()`.

The sites are at lines: 396, 418, 425, 432, 439, 446, 453, 457, 460, 463, 474, 482, 488, 510, 514.

Apply this transformation to each. For example, line 396:

Before:
```rust
                    .click_at(buffer_x, buffer_y, ctx.font_system);
```

After:
```rust
                    .click_at(buffer_x, buffer_y, ctx.font_system());
```

And line 418:

Before:
```rust
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Left,
                            shift,
                            ctx.font_system,
                        );
```

After:
```rust
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Left,
                            shift,
                            ctx.font_system(),
                        );
```

Repeat for every `ctx.font_system` occurrence in the file. The pattern is identical: add `()` after `ctx.font_system`.

**Caution:** Do NOT use `replaceAll: true` on the literal string `"ctx.font_system"` because that would also match the already-correct `"ctx.font_system()"` if any exist. Instead, match the longer context string for each call. If using a tool that supports regex, the pattern `ctx\.font_system\b(?!\()` matches only bare occurrences. If unsure, edit each of the ~15 sites individually using the surrounding code as the `oldString` anchor.

- [ ] **Step 4: Migrate `ctx.clipboard` to `ctx.clipboard()`**

Three sites. Find line 478:

```rust
                                        ctx.clipboard.set_text(&s);
```

This occurs twice (lines 478 and 483). For each, replace with:

```rust
                                        ctx.clipboard().set_text(&s);
```

Find line 487:

```rust
                                    if let Some(s) = ctx.clipboard.get_text() {
```

Replace with:

```rust
                                    if let Some(s) = ctx.clipboard().get_text() {
```

- [ ] **Step 5: Do NOT build yet**

Proceed to Task 3.

---

### Task 3: Migrate `scroll_view.rs` field accesses to accessors

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:129` (`ctx.build_owner` → `ctx.build_owner()`)
- Modify: `vexo/src/elements/scroll_view.rs:387` (`ctx.dirty_sender.cloned()` → `ctx.dirty_sender().cloned()`)

**Interfaces:**
- Consumes: Task 1's new accessors `build_owner()`, `dirty_sender()`.
- Produces: `scroll_view.rs` no longer touches `EventContext` fields directly.

- [ ] **Step 1: Migrate `ctx.build_owner`**

Find this block (line 129):

```rust
        if let Some(bo) = ctx.build_owner {
            bo.mark_needs_build(ctx.element_id());
        }
```

Replace with:

```rust
        if let Some(bo) = ctx.build_owner() {
            bo.mark_needs_build(ctx.element_id());
        }
```

- [ ] **Step 2: Migrate `ctx.dirty_sender.cloned()`**

Find this line (line 387):

```rust
                let Some(tx) = ctx.dirty_sender.cloned() else {
```

Replace with:

```rust
                let Some(tx) = ctx.dirty_sender().cloned() else {
```

- [ ] **Step 3: Do NOT build yet**

Proceed to Task 4.

---

### Task 4: Update `event_handler.rs` constructor callsites

**Files:**
- Modify: `vexo/src/event_handler.rs:176, 205, 294, 353, 394, 453, 528` (7 `EventContext::with_build_owner(...)` callsites)

**Interfaces:**
- Consumes: Task 1's narrowed `with_build_owner(element_id, local_position, bounds, modifiers, font_system, build_owner, dirty_sender, render_objects, clipboard)` signature.
- Produces: All 7 `event_handler.rs` callsites match the narrowed signature.

- [ ] **Step 1: Update callsite at line 176**

Find this block (lines 176-189):

```rust
                                let mut ctx = EventContext::with_build_owner(
                                    winner_id,
                                    position,
                                    position,
                                    focus_manager.primary_focus_element(),
                                    bounds,
                                    modifiers,
                                    scale_source.clone(),
                                    font_system,
                                    build_owner,
                                    dirty_sender,
                                    Some(render_objects),
                                    clipboard.clone(),
                                );
```

Replace with (drop `pointer_position`, `focused_element`, `scale_source` args):

```rust
                                let mut ctx = EventContext::with_build_owner(
                                    winner_id,
                                    position,
                                    bounds,
                                    modifiers,
                                    font_system,
                                    build_owner,
                                    dirty_sender,
                                    Some(render_objects),
                                    clipboard.clone(),
                                );
```

- [ ] **Step 2: Update callsite at line 205**

Find the next `EventContext::with_build_owner(` call. It has the same structure as the one in Step 1 — same 12 args in the same order. Replace with the same 9-arg form (drop `position` (the second one, which was `pointer_position`), `focus_manager.primary_focus_element()`, and `scale_source.clone()`).

Before (12 args):

```rust
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
                                position,
                                position,
                                focus_manager.primary_focus_element(),
                                bounds,
                                modifiers,
                                scale_source.clone(),
                                font_system,
                                build_owner,
                                dirty_sender,
                                Some(render_objects),
                                clipboard.clone(),
                            );
```

After (9 args):

```rust
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
                                position,
                                bounds,
                                modifiers,
                                font_system,
                                build_owner,
                                dirty_sender,
                                Some(render_objects),
                                clipboard.clone(),
                            );
```

- [ ] **Step 3: Update callsite at line 294**

Same pattern as Steps 1-2. Find the third `EventContext::with_build_owner(` call and apply the same transformation: drop the 2nd `position` arg, the `focus_manager.primary_focus_element()` arg, and the `scale_source.clone()` arg.

Before (12 args):

```rust
                                let mut ctx = EventContext::with_build_owner(
                                    winner_id,
                                    position,
                                    position,
                                    focus_manager.primary_focus_element(),
                                    bounds,
                                    modifiers,
                                    scale_source.clone(),
                                    font_system,
                                    build_owner,
                                    dirty_sender,
                                    Some(render_objects),
                                    clipboard.clone(),
                                );
```

After (9 args):

```rust
                                let mut ctx = EventContext::with_build_owner(
                                    winner_id,
                                    position,
                                    bounds,
                                    modifiers,
                                    font_system,
                                    build_owner,
                                    dirty_sender,
                                    Some(render_objects),
                                    clipboard.clone(),
                                );
```

- [ ] **Step 4: Update callsite at line 353**

Same pattern. Apply the same transformation to the fourth `EventContext::with_build_owner(` call.

Before (12 args):

```rust
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
                                position,
                                position,
                                focus_manager.primary_focus_element(),
                                bounds,
                                modifiers,
                                scale_source.clone(),
                                font_system,
                                build_owner,
                                dirty_sender,
                                Some(render_objects),
                                clipboard.clone(),
                            );
```

After (9 args):

```rust
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
                                position,
                                bounds,
                                modifiers,
                                font_system,
                                build_owner,
                                dirty_sender,
                                Some(render_objects),
                                clipboard.clone(),
                            );
```

- [ ] **Step 5: Update callsite at line 394**

Same pattern. Apply the same transformation to the fifth `EventContext::with_build_owner(` call.

Before (12 args):

```rust
                let mut ctx = EventContext::with_build_owner(
                    winner_id,
                    position,
                    position,
                    focus_manager.primary_focus_element(),
                    bounds,
                    modifiers,
                    scale_source.clone(),
                    font_system,
                    build_owner,
                    dirty_sender,
                    Some(render_objects),
                    clipboard.clone(),
                );
```

After (9 args):

```rust
                let mut ctx = EventContext::with_build_owner(
                    winner_id,
                    position,
                    bounds,
                    modifiers,
                    font_system,
                    build_owner,
                    dirty_sender,
                    Some(render_objects),
                    clipboard.clone(),
                );
```

- [ ] **Step 6: Update callsite at line 453**

This callsite is in the keyboard-event handler. Its current form (lines 453-466):

```rust
        let mut ctx = EventContext::with_build_owner(
            focused,
            Point::zero(),
            Point::zero(), // no pointer position for keyboard events
            focus_manager.primary_focus_element(),
            bounds,
            modifiers,
            scale_source.clone(),
            font_system,
            build_owner,
            dirty_sender,
            Some(render_objects),
            clipboard.clone(),
        );
```

Replace with (drop `pointer_position` (the second `Point::zero()`), `focus_manager.primary_focus_element()`, and `scale_source.clone()`; keep the first `Point::zero()` which is `local_position`):

```rust
        let mut ctx = EventContext::with_build_owner(
            focused,
            Point::zero(),
            bounds,
            modifiers,
            font_system,
            build_owner,
            dirty_sender,
            Some(render_objects),
            clipboard.clone(),
        );
```

- [ ] **Step 7: Update callsite at line 528**

Same pattern as Steps 1-5. Apply the same transformation to the seventh `EventContext::with_build_owner(` call.

Before (12 args):

```rust
                        let mut ctx = EventContext::with_build_owner(
                            hit_element,
                            position,
                            local_position,
                            focus_manager.primary_focus_element(),
                            bounds,
                            modifiers,
                            scale_source.clone(),
                            font_system,
                            build_owner,
                            dirty_sender,
                            Some(render_objects),
                            clipboard.clone(),
                        );
```

After (9 args — note this is the one callsite where `position` and `local_position` differ; keep `local_position` as the 2nd arg):

```rust
                        let mut ctx = EventContext::with_build_owner(
                            hit_element,
                            local_position,
                            bounds,
                            modifiers,
                            font_system,
                            build_owner,
                            dirty_sender,
                            Some(render_objects),
                            clipboard.clone(),
                        );
```

- [ ] **Step 8: Do NOT build yet**

Proceed to Task 5.

---

### Task 5: Update `gesture_detector.rs` test constructor callsites

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs:629, 668, 704` (3 `EventContext::new(...)` test callsites)

**Interfaces:**
- Consumes: Task 1's narrowed `new(element_id, local_position, bounds, modifiers, font_system, render_objects, clipboard)` signature.
- Produces: All 3 `gesture_detector.rs` test callsites match the narrowed signature.

- [ ] **Step 1: Update callsite at line 629**

Find this block (lines 629-640):

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            Point::new(50.0, 25.0),
            None,
            bounds,
            crate::input::Modifiers::default(),
            crate::core::ScaleSource::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

Replace with (drop the 2nd `Point::new(50.0, 25.0)` (was `pointer_position`), the `None` (was `focused_element`), and `ScaleSource::default()`):

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

- [ ] **Step 2: Update callsite at line 668**

Find this block (lines 668-679):

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            Point::new(50.0, 25.0),
            None,
            bounds,
            crate::input::Modifiers::default(),
            crate::core::ScaleSource::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

Replace with:

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

- [ ] **Step 3: Update callsite at line 704**

Find this block (lines 704-715):

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(200.0, 200.0), // Outside bounds
            Point::new(200.0, 200.0),
            None,
            bounds,
            crate::input::Modifiers::default(),
            crate::core::ScaleSource::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

Replace with (keep the comment on the first `Point::new`):

```rust
        let mut ctx = EventContext::new(
            element_id,
            Point::new(200.0, 200.0), // Outside bounds
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
```

- [ ] **Step 4: Do NOT build yet — Task 6 builds**

Proceed to Task 6.

---

### Task 6: Final build, test, and commit

**Files:**
- None (verification + commit only).

**Interfaces:**
- Consumes: Tasks 1-5.
- Produces: A single commit that narrows `EventContext` and migrates all callers.

- [ ] **Step 1: Build all three crates**

Run: `cargo build -p vexo && cargo build -p vexo_uikit && cargo build -p shared_app`
Expected: All three build cleanly.

If any build fails:
- "expected X arguments, found Y" on `EventContext::new` or `with_build_owner` → a callsite was missed. Find it with `rg "EventContext::(new|with_build_owner)" vexo/src/` and update it.
- "no field `pointer_position`/`focused_element`/`scale_source` on type `EventContext`" → a caller still accesses a removed field. Find it with `rg "ctx\.(pointer_position|focused_element|scale_source)" vexo/src/` and migrate it to the accessor (or delete the call if the method was removed).
- "no method named `is_pointer_inside`/`is_focused_self`/`has_focus`/`scale`/`is_control_pressed`/`is_shift_pressed`/`is_alt_pressed`/`mark_needs_build`/`is_focused`" → a caller still invokes a removed method. Find it with `rg "ctx\.(is_pointer_inside|is_focused_self|has_focus|scale|is_control_pressed|is_shift_pressed|is_alt_pressed|mark_needs_build|is_focused)\b" vexo/src/` and either delete the call (if dead) or migrate it (e.g., `ctx.is_control_pressed()` → `ctx.modifiers().control`).
- "cannot borrow `ctx` as mutable" on `ctx.font_system()` → the call site holds another borrow of `ctx`. Refactor to release the other borrow first, or bind `let fs = ctx.font_system();` once at the top of the arm.

- [ ] **Step 2: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass. The count should drop by 3 (the 3 deleted tests in `event_context.rs`).

If any test fails, investigate. Do NOT silently re-add removed methods or fields — the spec's audit found zero production callers, so a failure indicates either a missed callsite or a test that was secretly covering a removed method.

- [ ] **Step 3: Verify no stray references to removed surface remain**

Run: `rg "ctx\.(pointer_position|focused_element|scale_source|is_pointer_inside|is_focused_self|is_focused|has_focus|scale\b|is_control_pressed|is_shift_pressed|is_alt_pressed|mark_needs_build)\b" vexo/src/ --type rust`
Expected: No matches. (The `\b` after `scale` and the removed method names prevents matching `scale_source` etc. — but `scale_source` is in the field list separately.)

Any match is a missed callsite — fix it before committing.

Also run: `rg "ctx\.(bounds|modifiers|font_system|clipboard|build_owner|dirty_sender)\b(?!\()" vexo/src/ --type rust`
Expected: No matches. Every field access should now go through the accessor (with `()`).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/event_context.rs vexo/src/widgets/text_edit.rs vexo/src/elements/scroll_view.rs vexo/src/event_handler.rs vexo/src/widgets/gesture_detector.rs
git commit -m "refactor(event-context): narrow EventContext to used surface only

Remove 9 dead methods (is_pointer_inside, is_focused arg form,
is_focused_self, has_focus, scale, is_*_pressed, mark_needs_build) and
the 3 fields that only fed them (pointer_position, focused_element,
scale_source). Make the 6 externally-read pub fields private and add
accessors. Migrate text_edit.rs and scroll_view.rs to the accessors;
update 7 event_handler.rs and 3 gesture_detector.rs constructor
callsites to the narrowed signatures.

No behavior change — the removed methods had zero production callers.
Mirrors the RenderContext and LifecycleContext narrowings."
```

- [ ] **Step 5: Sanity-check the commit**

Run: `git show --stat HEAD`
Expected: exactly 5 files modified (`vexo/src/event_context.rs`, `vexo/src/widgets/text_edit.rs`, `vexo/src/elements/scroll_view.rs`, `vexo/src/event_handler.rs`, `vexo/src/widgets/gesture_detector.rs`), no others.

---

## Self-Review

**1. Spec coverage:**
- Spec §"Narrowed `EventContext` struct": 3 fields removed + 6 fields made private → Task 1 Steps 2. ✓
- Spec §"Narrowed methods": 9 removed + 6 accessors added → Task 1 Step 5. ✓
- Spec §"Constructor signature narrowing": both constructors drop 3 args → Task 1 Steps 3-4. ✓
- Spec §"Docstring update": struct docstring reword → Task 1 Step 1. ✓
- Spec §"Caller migration" text_edit.rs: bounds/modifiers/font_system/clipboard → Task 2. ✓
- Spec §"Caller migration" scroll_view.rs: build_owner/dirty_sender → Task 3. ✓
- Spec §"Caller migration" event_handler.rs: 7 callsites → Task 4. ✓
- Spec §"Caller migration" gesture_detector.rs: 3 test callsites → Task 5. ✓
- Spec §"Test impact": 3 tests deleted + 3 kept tests updated → Task 1 Steps 6-7. ✓
- Spec §"Migration Plan": single-phase, single commit, `cargo build` all 3 + `cargo test --workspace` → Task 6. ✓
- Spec §"Out of Scope" / "Non-Goals": no task touches `RenderContext`, `LifecycleContext`, `ElementContext`, `LayoutContext`, `PaintContext`, `BuildOwner`, or `on_event` signatures. ✓

**2. Placeholder scan:** No TBD/TODO. Every step shows the exact code or command. The `font_system` migration in Task 2 Step 3 explicitly addresses the `replaceAll` footgun and gives the regex pattern. ✓

**3. Type consistency:**
- `EventContext::new` signature: 7 args (Task 1 Step 3 defines, Tasks 5 + Task 1 Step 7 call). ✓
- `EventContext::with_build_owner` signature: 9 args (Task 1 Step 4 defines, Task 4 calls). ✓
- Accessor names: `bounds()`, `modifiers()`, `font_system()`, `clipboard()`, `build_owner()`, `dirty_sender()` (Task 1 Step 5 defines, Tasks 2-3 call). ✓
- Accessor return types: `bounds() -> Bounds<Logical>`, `modifiers() -> Modifiers`, `font_system() -> &mut glyphon::FontSystem`, `clipboard() -> &Arc<dyn Clipboard>`, `build_owner() -> Option<&BuildOwner>`, `dirty_sender() -> Option<&Sender<ElementKey>>`. Caller usage matches: `ctx.bounds().height()` (Bounds has `.height()`), `ctx.modifiers()` (assigned to `let modifiers = ...`), `ctx.font_system()` (passed as `&mut` arg), `ctx.clipboard().set_text(&s)` / `.get_text()` (Arc derefs to Clipboard trait), `ctx.build_owner()` (pattern-matched `if let Some(bo) = ...`), `ctx.dirty_sender().cloned()` (Option's `.cloned()`). ✓

No issues found.
