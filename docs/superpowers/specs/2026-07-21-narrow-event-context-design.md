# Narrow EventContext — Design

**Date:** 2026-07-21
**Status:** Proposed
**Scope:** `vexo` crate (`vexo/src/event_context.rs` primary; caller migrations in
`vexo/src/widgets/text_edit.rs`, `vexo/src/elements/scroll_view.rs`,
`vexo/src/event_handler.rs`, `vexo/src/widgets/gesture_detector.rs`)

## Motivation

`EventContext` (`vexo/src/event_context.rs:24-88`) is passed to every
`Element::on_event()` and `ComponentState::on_event()` call. It currently
exposes:

- 8 `pub` fields: `pointer_position`, `focused_element`, `bounds`, `modifiers`,
  `font_system`, `clipboard`, `build_owner`, `dirty_sender`.
- 5 private fields: `element_id`, `render_objects`, `focus_request`,
  `clear_focus_request`, `local_position`, `scale_source`.
- 16 methods: `element_id()`, `is_pointer_inside()`, `local_position()`,
  `scale()`, `is_focused_self()`, `is_focused(element)`, `has_focus()`,
  `request_focus()`, `clear_focus()`, `focus_request()`,
  `should_clear_focus()`, `is_control_pressed()`, `is_shift_pressed()`,
  `is_alt_pressed()`, `mark_needs_build()`, `render_objects()`.

A grep across every `on_event` handler in the workspace (`vexo`, `vexo_uikit`,
`shared_app`) shows that user code touches only a subset of this surface.

### Dead methods (zero production callers)

| Method | Caller audit |
|---|---|
| `scale()` | 0 callers. Only consumer of `scale_source` field. |
| `has_focus()` | 0 callers. Only consumer of `focused_element` field. |
| `mark_needs_build(element)` | 0 callers. `scroll_view.rs:130` calls `ctx.build_owner.mark_needs_build(ctx.element_id())` directly. |
| `is_focused(element)` (arg form) | 0 production callers. Only consumer of `focused_element` field. |
| `is_pointer_inside()` | Only in `event_context.rs` tests. Only consumer of `pointer_position` field. |
| `is_focused_self()` | Only in `event_context.rs` tests. Only consumer of `focused_element` field. |
| `is_control_pressed()` | Only in `event_context.rs` tests. Reads `modifiers` field (which has accessor). |
| `is_shift_pressed()` | Only in `event_context.rs` tests. Reads `modifiers` field. |
| `is_alt_pressed()` | Only in `event_context.rs` tests. Reads `modifiers` field. |

### Dead fields (only feed dead methods)

| Field | Reason |
|---|---|
| `pointer_position` | Only feeds `is_pointer_inside()`. No external reader. |
| `focused_element` | Only feeds `is_focused*()` / `has_focus()`. No external reader. |
| `scale_source` | Only feeds `scale()`. No external reader. |

### `pub` fields with external readers (need encapsulation)

| Field | External readers |
|---|---|
| `bounds` | `text_edit.rs:390` (`ctx.bounds.height()`) |
| `modifiers` | `text_edit.rs:409` (`ctx.modifiers`) |
| `font_system` | `text_edit.rs` (~15 sites, every keyboard action) |
| `clipboard` | `text_edit.rs:478, 483, 487` (`.set_text` / `.get_text`) |
| `build_owner` | `scroll_view.rs:129` (`if let Some(bo) = ctx.build_owner`) |
| `dirty_sender` | `scroll_view.rs:387` (`ctx.dirty_sender.cloned()`) |

This mirrors the situations that motivated the `RenderContext` and
`LifecycleContext` narrowings (`docs/superpowers/specs/2026-07-21-narrow-render-context-design.md`
and `docs/superpowers/specs/2026-07-21-narrow-lifecycle-context-design.md`):
dead mutators and unused reads frozen into a public context object, and `pub`
fields leaking framework internals into the public API surface. Same first
principles apply: declarative over imperative, scope over global,
encapsulation.

### Event routing is unaffected

`event_handler.rs` computes `focused_element` per-call from
`focus_manager.primary_focus_element()` (lines 180, 209, 298, 357, 398, 448,
457, 532) and passes it as a constructor arg. The constructor currently
stores it in the `focused_element` field; after narrowing, the constructor
drops that arg. Routing logic uses the local variable (`focus_manager`'s
return), not the field — no behavior change.

Similarly, `pointer_position` (the global position) is passed as both the
`pointer_position` and `local_position` args by `event_handler.rs` for arena
winners that have no bounds yet. After narrowing, only `local_position` is
passed. `event_handler.rs` continues to compute the global `position` from
the input event for its own gesture-arena use; nothing in user `on_event`
handlers reads the global position.

## Goals

- Remove the 9 dead methods listed above.
- Remove the 3 dead fields (`pointer_position`, `focused_element`,
  `scale_source`).
- Drop the 3 corresponding args from `EventContext::new()` and
  `EventContext::with_build_owner()`.
- Make the 6 externally-read `pub` fields private, add accessors for them.
- Delete the 3 tests that test only removed methods.
- Update the 3 kept tests + 3 gesture_detector tests + 7 event_handler
  callsites to the narrowed constructor signature.
- Migrate the 2 caller files (`text_edit.rs`, `scroll_view.rs`) to use
  the new accessors.
- Preserve all current behavior. No handler changes behavior; the removed
  methods were dead, and the fields are accessed through accessors that
  return the same value.

## Non-Goals

- No changes to `RenderContext` or `LifecycleContext` (already narrowed).
- No changes to `ElementContext`, `LayoutContext`, or `PaintContext`.
  Different context types. (Note: `render_objects/text.rs:208` and
  `text_edit.rs:192` call `ctx.font_system()` on `LayoutContext`/`PaintContext`,
  not `EventContext` — out of scope.)
- No splitting of `EventContext` into public + internal types.
- No removal of `Option<>` wrapping on `build_owner`/`dirty_sender` (always
  `Some` in production, `None` in tests — separate concern).
- No migrating `event_handler.rs`'s 7 constructor callsites to a helper
  function (DRY opportunity, but separate refactor).
- No changes to `Element::on_event` or `ComponentState::on_event` signatures.
- No new abstraction types. Private fields + accessors enforce encapsulation.

## Design

### Narrowed `EventContext` struct

```rust
pub struct EventContext<'a> {
    element_id: ElementKey,                          // private (unchanged)
    // REMOVED: pointer_position (pub, dead externally + only fed is_pointer_inside)
    // REMOVED: focused_element (pub, dead externally + only fed is_focused*/has_focus)
    bounds: Bounds<Logical>,                         // private (was pub)
    modifiers: Modifiers,                            // private (was pub)
    font_system: &'a mut glyphon::FontSystem,        // private (was pub)
    clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,  // private (was pub)
    build_owner: Option<&'a BuildOwner>,             // private (was pub)
    dirty_sender: Option<&'a std::sync::mpsc::Sender<ElementKey>>,  // private (was pub)
    render_objects: Option<&'a RenderObjectRegistry>,  // private (unchanged)
    focus_request: Option<ElementKey>,               // private (unchanged)
    clear_focus_request: bool,                       // private (unchanged)
    local_position: Point<Logical>,                  // private (unchanged)
    // REMOVED: scale_source (only fed scale())
}
```

**Fields removed (3):** `pointer_position`, `focused_element`, `scale_source`.

**Fields made private (6):** `bounds`, `modifiers`, `font_system`,
`clipboard`, `build_owner`, `dirty_sender`.

**Fields unchanged:** `element_id`, `render_objects`, `focus_request`,
`clear_focus_request`, `local_position`.

### Narrowed methods

**Removed (9):** `is_pointer_inside`, `is_focused` (arg form),
`is_focused_self`, `has_focus`, `scale`, `is_control_pressed`,
`is_shift_pressed`, `is_alt_pressed`, `mark_needs_build`.

**Kept (existing, unchanged):** `element_id()`, `local_position()`,
`request_focus(element)`, `clear_focus()`, `focus_request()`,
`should_clear_focus()`, `render_objects()`.

**Added (6 accessors):**

```rust
pub fn bounds(&self) -> Bounds<Logical>
pub fn modifiers(&self) -> Modifiers
pub fn font_system(&mut self) -> &mut glyphon::FontSystem
pub fn clipboard(&self) -> &std::sync::Arc<dyn crate::platform::Clipboard>
pub fn build_owner(&self) -> Option<&BuildOwner>
pub fn dirty_sender(&self) -> Option<&std::sync::mpsc::Sender<ElementKey>>
```

The `font_system` accessor takes `&mut self` because the field is
`&'a mut glyphon::FontSystem` and callers (text_edit.rs) need a `&mut`
borrow to pass to controller methods like
`controller.insert_char(c, ctx.font_system())`. The accessor returns
`&mut glyphon::FontSystem`, forwarding the borrow.

All other accessors are `&self` and return by value (for `Copy` types
`Bounds`, `Modifiers`) or by reference (for `Arc`, `BuildOwner`,
`Sender`).

### Constructor signature narrowing

Both constructors drop 3 args (`pointer_position`, `focused_element`,
`scale_source`):

```rust
impl<'a> EventContext<'a> {
    pub fn new(
        element_id: ElementKey,
        local_position: Point<Logical>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self { ... }

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
    ) -> Self { ... }
}
```

Both constructors stay `pub fn` — parallel to `RenderContext::new` which is
also `pub`. `EventContext` is re-exported from `lib.rs:153` as public API,
so pub constructors are consistent with that surface.

### Docstring update

The struct docstring (currently lines 16-23) lists "Pointer position for hit
testing" and "Focus state for keyboard event routing" — both stale after
narrowing. Reword to:

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

### Caller migration

Three files consume `EventContext` fields directly — all migrate to
accessors:

**`vexo/src/widgets/text_edit.rs` (`on_event` handler):**

| Old | New |
|---|---|
| `ctx.bounds.height()` | `ctx.bounds().height()` |
| `ctx.modifiers` | `ctx.modifiers()` |
| `ctx.font_system` (passed as `&mut` to controller methods) | `ctx.font_system()` (returns `&mut`) |
| `ctx.clipboard.set_text(&s)` / `.get_text()` | `ctx.clipboard().set_text(&s)` / `.get_text()` |

`ctx.font_system` is used ~15 times in text_edit.rs (every keyboard action
calls `controller.<action>(ctx.font_system)`). Each becomes
`ctx.font_system()`. The `&mut` borrow works because the accessor returns
`&mut glyphon::FontSystem` and each call is a standalone expression — no
nested borrows across statements.

**`vexo/src/elements/scroll_view.rs` (`on_event` handler):**

| Old | New |
|---|---|
| `if let Some(bo) = ctx.build_owner` | `if let Some(bo) = ctx.build_owner()` |
| `ctx.dirty_sender.cloned()` | `ctx.dirty_sender().cloned()` |
| `ctx.render_objects()` | unchanged |
| `ctx.element_id()` | unchanged |

**`vexo/src/event_handler.rs` (framework, 7 callsites):**

Each `EventContext::with_build_owner(...)` call drops 3 args:
`pointer_position`, `focused_element`, `scale_source`. The 7 callsites are
at lines 176, 205, 294, 353, 394, 453, 528.

**`vexo/src/widgets/gesture_detector.rs` (3 test callsites):**

Each `EventContext::new(...)` call (lines 629, 668, 704) drops 3 args:
`pointer_position`, `focused_element`, `scale_source`. Each test currently
passes `Point::new(...)` for both `pointer_position` and `local_position`
(they're equal in tests) and `None` / `ScaleSource::default()` for the
others. After narrowing, only the `local_position` arg remains.

### Test impact

**`vexo/src/event_context.rs` (3 deleted tests):**

- `test_event_context_is_pointer_inside` (line 292) — tests removed
  `is_pointer_inside()`.
- `test_event_context_is_focused_self` (line 326) — tests removed
  `is_focused_self()`.
- `test_event_context_modifiers` (line 405) — tests removed
  `is_control_pressed()` / `is_shift_pressed()` / `is_alt_pressed()`.

**`vexo/src/event_context.rs` (3 kept tests, signature update):**

- `test_event_context_element_id` (line 273)
- `test_event_context_focus_request` (line 360)
- `test_event_context_clear_focus_request` (line 383)

Each currently passes 9 args to `EventContext::new(...)`; after narrowing,
passes 6 args (drops `pointer_position`, `focused_element`, `scale_source`).

**`vexo/src/widgets/gesture_detector.rs` (3 test callsites):**

Lines 629, 668, 704 — each drops 3 args from `EventContext::new(...)`.

**`vexo/src/event_handler.rs` (7 callsites):**

Lines 176, 205, 294, 353, 394, 453, 528 — each drops 3 args from
`EventContext::with_build_owner(...)`.

**External tests:**

None construct `EventContext` directly in `vexo_uikit` or `shared_app`
(grep confirms zero callsites). Zero changes.

**New tests:**

None. This is pure surface narrowing with no new behavior. The 3 kept
tests continue to verify the same behavior (element_id, focus request,
clear focus) through the narrowed constructor.

## Migration Plan

Single-phase — one commit, no behavior change. Parallel to the
`LifecycleContext` narrowing: pure mechanical refactor, no live callsite
needs behavior migration.

**Steps:**

1. In `vexo/src/event_context.rs`:
   - Remove 3 fields (`pointer_position`, `focused_element`, `scale_source`).
   - Make 6 fields private (`bounds`, `modifiers`, `font_system`,
     `clipboard`, `build_owner`, `dirty_sender`).
   - Drop 3 args from `new()` and `with_build_owner()` signatures and
     bodies.
   - Remove 9 methods (`is_pointer_inside`, `is_focused` arg form,
     `is_focused_self`, `has_focus`, `scale`, `is_control_pressed`,
     `is_shift_pressed`, `is_alt_pressed`, `mark_needs_build`).
   - Add 6 accessors (`bounds()`, `modifiers()`, `font_system()`,
     `clipboard()`, `build_owner()`, `dirty_sender()`).
   - Delete 3 tests for removed methods.
   - Update 3 kept tests to use the narrowed constructor signature.
   - Reword struct docstring.
2. In `vexo/src/widgets/text_edit.rs`:
   - `ctx.bounds` → `ctx.bounds()` (1 site, line 390)
   - `ctx.modifiers` → `ctx.modifiers()` (1 site, line 409)
   - `ctx.font_system` → `ctx.font_system()` (~15 sites, lines 396-514)
   - `ctx.clipboard` → `ctx.clipboard()` (3 sites, lines 478, 483, 487)
3. In `vexo/src/elements/scroll_view.rs`:
   - `ctx.build_owner` → `ctx.build_owner()` (1 site, line 129)
   - `ctx.dirty_sender.cloned()` → `ctx.dirty_sender().cloned()` (1 site,
     line 387)
4. In `vexo/src/event_handler.rs`:
   - Update 7 `EventContext::with_build_owner(...)` callsites (lines 176,
     205, 294, 353, 394, 453, 528) to drop 3 args each.
5. In `vexo/src/widgets/gesture_detector.rs`:
   - Update 3 `EventContext::new(...)` test callsites (lines 629, 668, 704)
     to drop 3 args each.
6. Build: `cargo build -p vexo`, `cargo build -p vexo_uikit`,
   `cargo build -p shared_app`.
7. Test: `cargo test --workspace`.

No manual GUI run needed — pure mechanical refactor with zero behavior
change. No callsite changes behavior; the removed methods were dead, and
the fields are accessed through accessors that return the same value.

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Missed a caller of a removed method/field | Low | Compile error | `cargo build --workspace` catches it. Grep audit covered `vexo`, `vexo_uikit`, `shared_app`. |
| `ctx.font_system()` borrow conflict in text_edit.rs (multiple uses in one match arm) | Low | Compile error | Each use is a standalone expression (`controller.action(ctx.font_system())`); `&mut self` accessor returns the borrow for the call duration. No nested borrows. If a conflict arises, bind `let fs = ctx.font_system();` once per arm. |
| Removed test was secretly covering a kept method | Low | Coverage gap | The 3 deleted tests test only removed methods (`is_pointer_inside`, `is_focused_self`, `is_*_pressed`). The 3 kept tests cover kept methods (`element_id`, `request_focus`/`focus_request`, `clear_focus`/`should_clear_focus`). No coverage overlap. |
| Hidden external caller of `EventContext::new`/`with_build_owner` | Very low | Compile error | Constructors are `pub` but `vexo_uikit` and `shared_app` greps show zero callsites. |
| `focused_element` removal breaks event routing | Very low | Behavior change | `event_handler.rs` computes `focused_element` per-call from `focus_manager.primary_focus_element()` (lines 180, 209, 298, 357, 398, 448, 457, 532) and passes it to the constructor. The constructor no longer stores it; routing logic uses the local variable, not the field. No behavior change. |
| `scale_source` removal breaks scale-dependent code path | Very low | Behavior change | `scale_source` was only consumed by `scale()` which has zero callers. No code path reads scale through `EventContext`. |

## Out of Scope

- `RenderContext` and `LifecycleContext` (already narrowed).
- `ElementContext`, `LayoutContext`, `PaintContext` changes.
- Splitting `EventContext` into public + internal types.
- Removing `Option<>` wrapping on `build_owner`/`dirty_sender`.
- Migrating `event_handler.rs`'s 7 constructor callsites to a helper
  function (DRY opportunity, separate refactor).
- `Element::on_event` or `ComponentState::on_event` signature changes.
- New abstraction types (no `EventCtxInner`).

These are legitimate future refactors but unrelated to the current goal:
removing dead methods/fields and encapsulating the remaining `pub` fields
behind accessors.
