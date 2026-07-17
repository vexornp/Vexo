# Gesture Arena: Tap vs. Scroll-Drag Disambiguation

**Date:** 2026-07-17
**Status:** Approved (design review)
**Scope:** `vexo` crate (new `gestures/` module, `event_handler.rs`, `pipeline.rs`, `element.rs`), `shared_app` (two call-site migrations)

## Problem

A drag on a tappable conversation row inside a `ScrollView` navigates (fires the row's `on_press`) instead of scrolling the list.

### Root Cause

Events dispatch deepest-to-shallowest through the hit-test path; the first element to return `Some(())` on press stops propagation (`event_handler.rs:167-208`). The conversation row wraps its content in `GestureDetector` via `.on_press()` (`conversation_list.rs:75`). On press, `GestureDetectorElement::on_event` fires `on_press` **immediately on pointer-down** and returns `Some(())` (`gesture_detector.rs:321-340`), stopping the event before it reaches the ancestor `ScrollViewElement`. The scroll view never sees the press, so `drag_active` is never set, and drags navigate instead of scroll.

This is exactly the problem Flutter's GestureArena solves: multiple recognizers competing for one pointer, resolved by a disambiguation rule rather than by event-bubble ordering.

## Design Decisions (from brainstorming)

1. **Scope:** Arena infrastructure (register/accept/reject) with two recognizers only — `TapRecognizer` and `VerticalDragRecognizer`. Other recognizers (long-press, horizontal-drag) plug in later without re-architecting.
2. **Disambiguation model:** Slop-threshold. On press, both recognizers become candidates. On move past slop → drag wins, tap rejected. On release before slop → tap wins, drag rejected. Arena infrastructure (register/accept/reject) is still built so other recognizers plug in later.
3. **Tap firing semantics:** Add `on_tap` (arena-mediated, fires on release-after-win) as the action callback. Keep `on_press` firing immediately on pointer-down as press-down feedback (Flutter's `onTapDown` analog). Migrate conversation list + send button call sites from `.on_press(action)` to `.on_tap(action)`.
4. **Architecture:** Pipeline-owned arena, per pointer. On press, `EventHandler` creates a fresh `GestureArena`, walks the hit-test path offering each element a chance to register a recognizer. Subsequent move/up events route into the arena, which resolves a winner and notifies via the owning element.

## Architecture Overview

**New module: `vexo/src/gestures/`.** Arena and recognizers as plain, decoupled structs — no element dependencies. Recognizers are self-contained state machines that expose `resolution()`; they never call arena methods and never hold user callbacks. Callbacks live on the elements that register them.

```
gestures/
├── mod.rs              # public API re-exports, TAP_SLOP / VERTICAL_DRAG_SLOP constants
├── arena.rs            # GestureArena (per-pointer resolver)
├── recognizer.rs       # GestureRecognizer trait, RecognizerResolution
├── arena_event.rs      # ArenaEvent { Down, Move, Up, Cancel }
├── tap.rs              # TapRecognizer
└── vertical_drag.rs    # VerticalDragRecognizer
```

**Resolution flow:**
1. On pointer press, `EventHandler` builds a fresh `GestureArena` and walks the hit-test path offering every element a chance to `register_gestures(&mut arena)`. `GestureDetectorElement` adds a `TapRecognizer`; `ScrollViewElement` adds a `VerticalDragRecognizer`.
2. Subsequent `PointerMoved` and `PointerButton::Released` events route **into the arena**, not through the `on_event` bubble. The arena feeds each recognizer, resolves a winner (slop exceeded → drag accepts → tap rejected; release before slop → tap wins via sweep), and the winning element fires its callback.
3. The arena is dropped on release.

**What stays outside the arena.** Mouse-wheel `Scroll` events and `Keyboard` events keep their current dispatch paths. Only pointer press/move/up participate in the arena. `on_press` (immediate press-down feedback) still fires through the existing `on_event` bubble on press — it is *not* gated by the arena.

**Single-pointer scope.** `InputEvent` currently carries no pointer id, so the framework is single-pointer. This design uses one arena at a time, created on press, dropped on release. Multi-touch (concurrent arenas keyed by pointer id) is a documented follow-up that requires adding pointer ids to `InputEvent` first.

## Arena and Recognizer Core

### `GestureRecognizer` trait (`recognizer.rs`)

```rust
pub trait GestureRecognizer: Any {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &mut ArenaContext);
    fn resolution(&self) -> RecognizerResolution;   // Pending | Accepted | Rejected
    fn accepted(&self) -> bool { matches!(self.resolution(), RecognizerResolution::Accepted) }
    fn rejected(&self) -> bool { matches!(self.resolution(), RecognizerResolution::Rejected) }
}
```

- `handle_event` advances the recognizer's internal state given `ArenaEvent::{Down, Move, Up, Cancel}`.
- `resolution()` exposes whether it has accepted/rejected/still pending. The arena reads this — recognizers never call arena methods directly (keeps them testable in isolation).
- `ArenaContext` carries the `down_position` and `current_position` — the shared facts every recognizer needs, computed once by the arena. Recognizers track their own accumulated state (e.g. `total_delta_y`) internally. No callbacks here; the arena decides what to fire by reading `resolution()` + the recognizer's `as_any()`.

### `ArenaEvent` (`arena_event.rs`)

```rust
pub enum ArenaEvent {
    Down { position: Point<Logical> },
    Move { position: Point<Logical> },
    Up   { position: Point<Logical> },
    Cancel,
}
```

### `RecognizerResolution`

```rust
pub enum RecognizerResolution { Pending, Accepted, Rejected }
```

### `GestureArena` (`arena.rs`)

One per active pointer press:

```rust
pub struct GestureArena {
    recognizers: Vec<(Box<dyn GestureRecognizer>, ElementKey)>,  // recognizer + owning element
    down_position: Point<Logical>,
    winner: Option<usize>,   // index into recognizers
    closed: bool,            // true once a winner is decided
}
impl GestureArena {
    pub fn new(down_position: Point<Logical>) -> Self;
    pub fn add(&mut self, recognizer: Box<dyn GestureRecognizer>, owner: ElementKey);
    pub fn handle_event(&mut self, event: ArenaEvent) -> ArenaOutcome;  // drives resolution
    pub fn winner(&self) -> Option<(usize, ElementKey)>;
    pub fn winner_recognizer(&self) -> Option<&dyn GestureRecognizer>;
}
```

**Resolution algorithm (slop model):**
1. `handle_event(Down)` — feed `Down` to every recognizer (record start position). No winner yet.
2. `handle_event(Move)` — feed `Move` to every recognizer. Then sweep: if any recognizer's `resolution()` is `Accepted` (e.g. `VerticalDragRecognizer` exceeded slop) → declare it the winner, mark `closed`, reject all others. If a recognizer is `Rejected` → drop it from future consideration (it stays in the list but is ignored). Last one standing with all others rejected could also be declared winner.
3. `handle_event(Up)` — feed `Up` to every recognizer. If arena still open: any recognizer that accepts on up (e.g. `TapRecognizer`) wins; otherwise sweep to the first non-rejected recognizer (Flutter's default sweep). Mark closed.
4. `handle_event(Cancel)` — feed `Cancel` to all, mark closed, no winner fires.

### `ArenaOutcome`

```rust
pub enum ArenaOutcome {
    Resolved { winner_index: usize },   // a recognizer accepted
    ClosedNoWinner,                      // cancel
    Open,                                // still competing
}
```

The arena itself does not fire user callbacks — it only resolves. `EventHandler` reads the winner and calls back into the owning element (via the `ElementKey` stored alongside the recognizer) to fire `on_tap` / apply scroll. Keeping the arena pure makes it unit-testable without any element/pipeline machinery.

**Single-winner invariant.** Once `closed == true`, no further recognizer can accept. `add()` is a no-op on a closed arena. This is the Flutter invariant that guarantees tap and drag can't both fire from one touch.

**No callbacks in recognizers.** Callbacks live on elements, not recognizers. The recognizer tracks state only; the element that registered it holds the callback. When `EventHandler` resolves a winner, it calls back into the owning element to fire `on_tap` / apply scroll. This is what makes the recognizer unit-testable in isolation and avoids `Rc<RefCell<dyn FnMut>>` inside the arena's hot path.

## The Two Recognizers

### `TapRecognizer` (`tap.rs`)

Recognizes a tap: down + up without movement past slop.

```rust
pub struct TapRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
}
```

State transitions on `ArenaEvent`:
- `Down` → store `down_position`, stay `Pending`.
- `Move` → if `|current.y - down.y|` or `|current.x - down.x|` exceeds **`TAP_SLOP` (18px, Flutter's `kTouchSlop`)** → `Rejected` (a drag is forming). Else stay `Pending`.
- `Up` → if still `Pending` (no slop breach) → `Accepted`. This is the tap win.
- `Cancel` → `Rejected`.

Note: `TapRecognizer` checks **both axes** against slop, not just vertical. A horizontal swipe (future swipe-to-delete) would also reject the tap. This matches Flutter — tap is rejected by *any* drag direction.

### `VerticalDragRecognizer` (`vertical_drag.rs`)

Recognizes a vertical drag, used by `ScrollViewElement`.

```rust
pub struct VerticalDragRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    last_position: Point<Logical>,
    total_delta_y: f32,
}
```

State transitions:
- `Down` → store `down_position` = `last_position`, `total_delta_y = 0`, stay `Pending`.
- `Move` → update `last_position`; accumulate `total_delta_y += delta.y`. If `|total_delta_y|` exceeds **`VERTICAL_DRAG_SLOP` (18px, Flutter's `kPanSlop`/`kTouchSlop` for vertical)** → `Accepted`. Once accepted, stays accepted (the drag "owns" this gesture).
- `Up` → if still `Pending` (never hit slop) → `Rejected` (it was a tap, not a drag). If already `Accepted` → stays `Accepted` (drag completed).
- `Cancel` → `Rejected`.

**Why `total_delta_y` not `|current - down|`.** Accumulating total movement (sum of per-move deltas) means a back-and-forth jitter that nets to ~0 still counts as drag intent. This matches Flutter's `VerticalDragGestureRecognizer` which uses cumulative movement. The alternative (net displacement) would let a user wiggle in place forever without resolving — bad UX.

### Constants

```rust
pub const TAP_SLOP: f32 = 18.0;
pub const VERTICAL_DRAG_SLOP: f32 = 18.0;
```

Both in `vexo/src/gestures/mod.rs` as `pub(crate)` — adjustable, and the two are equal because Flutter uses the same `kTouchSlop` for both. Keeping them separate constants lets a future horizontal-drag recognizer use `HORIZONTAL_DRAG_SLOP` independently.

### Resolution interaction (the bug fix)

On a touch in a scrollable list row:
- Press: both recognizers `Pending`.
- Small move (< 18px): both `Pending`, arena `Open`, no callback fires.
- Move past 18px vertical: `VerticalDragRecognizer` → `Accepted`. Arena declares it winner, rejects `TapRecognizer`. ScrollView's drag callback fires → scrolls. Tap never fires. Bug fixed.
- Release before 18px: `TapRecognizer` → `Accepted` on `Up`. Arena declares it winner, rejects `VerticalDragRecognizer`. Row's `on_tap` fires → navigates. No scroll. Normal tap works.

**What the drag recognizer does NOT do.** It does not call into `ScrollViewElement` to apply scroll deltas. It only resolves *that* the drag won. The element reads the recognizer's accumulated `last_position`/`total_delta_y` (via downcast) to drive `apply_scroll_offset`. This keeps the recognizer pure-state and the side-effectful scroll logic in the element where it already lives.

## Element Registration & Pipeline Wiring

### Element trait extensions (`element.rs`)

Two new default methods:

```rust
fn register_gestures(&mut self, _arena: &mut GestureArena, _self_id: ElementKey) {}
fn on_arena_winner_update(
    &mut self,
    _recognizer: &dyn GestureRecognizer,
    _event: &ArenaEvent,
    _ctx: &mut EventContext,
) {}
```

- `register_gestures` is called once on press for every element in the hit path. Default no-op; `GestureDetectorElement` overrides to add a `TapRecognizer`; `ScrollViewElement` overrides to add a `VerticalDragRecognizer`.
- `on_arena_winner_update` is called on each subsequent move/up **only for the winning element**. The element downcasts the recognizer to read its state and apply effects. Default no-op.

### Pipeline state (`pipeline.rs`)

`ThreeTreePipeline` gains one field:

```rust
current_arena: Option<GestureArena>,
```

Single-pointer (documented). `EventHandler` methods grow a `&mut Option<GestureArena>` parameter. Created on press, dropped on release.

### New event flow in `EventHandler::handle_pointer_event`

1. **Press.** Hit-test → element_path. (a) Bubble the press through `on_event` as today — `on_press` (immediate feedback) fires unchanged. (b) Create `GestureArena(down_position)`, store in pipeline. (c) Walk element_path (deepest→shallowest), call `element.register_gestures(&mut arena, element_id)` on each. Feed `ArenaEvent::Down` to the arena.

2. **Move.** (a) Feed `ArenaEvent::Move` to the arena. (b) If arena resolved a drag winner → call winning element's `on_arena_winner_update` (ScrollViewElement applies scroll delta). **Do not bubble** — the drag owns this pointer. (c) If arena still open (no winner) → bubble the move through `on_event` as today, so `MouseRegion` hover (`on_enter`/`on_exit`) keeps working. Recognizers still got fed via step (a).

3. **Release.** (a) Feed `ArenaEvent::Up` to the arena; it resolves (tap wins on up, or drag already won). (b) If a winner exists → call winning element's `on_arena_winner_update` (tap → `GestureDetectorElement` fires `on_tap`; drag → `ScrollViewElement` ends drag). (c) If tap won (or no arena/no winner) → bubble the release through `on_event` so `on_release` fires (release feedback, Flutter's `onTapUp` analog). (d) If drag won → do **not** bubble release (drag consumed the gesture). (e) Drop the arena.

### What stays unchanged

- `InputEvent::Scroll` (mouse wheel) → existing `handle_scroll_event` path to nearest scrollable ancestor. Wheel doesn't enter the arena.
- `InputEvent::Keyboard` → existing focused-element dispatch.
- `on_press` → fires immediately on press-down via the normal bubble, regardless of whether a drag later wins (matches Flutter's `onTapDown`). This is press *feedback*, not the *action*.

### Migration of existing element `on_event` handlers

**`ScrollViewElement::on_event` (`elements/scroll_view.rs:225-292`):**
- **Remove** the `PointerButton::Pressed` branch that sets `drag_active` (arena now decides).
- **Remove** the `PointerMoved` drag-scroll branch (arena winner handles it).
- **Remove** the `PointerButton::Released` drag-clear branch.
- **Keep** `InputEvent::Scroll` (wheel) and `InputEvent::Keyboard` branches unchanged.
- `drag_active`/`drag_last_y` fields move into the `VerticalDragRecognizer`'s state; the element reads them via downcast in `on_arena_winner_update`.

**`GestureDetectorElement::on_event` (`gesture_detector.rs:315-340`):**
- **Keep** the `Pressed` branch firing `on_press` (immediate feedback).
- **Keep** the `Released` branch firing `on_release` — but it now only runs when the release bubbles (i.e. tap won or no arena). When a drag won, the release doesn't bubble, so `on_release` won't fire. Correct.
- `on_tap` callback is **not** fired here — it fires from `on_arena_winner_update` when the tap recognizer wins.

### `GestureDetector` widget API (`gesture_detector.rs`)

- Add `on_tap: Option<Rc<RefCell<dyn FnMut()>>>` field + `.on_tap(callback)` builder.
- `on_press` and `on_release` remain as-is (immediate feedback hooks).

### `WidgetExt` trait (`widgets/mod.rs:298`)

- Add `.on_tap(callback)` extension that wraps in `GestureDetector::new(self).on_tap(callback)`.
- Keep `.on_press` (now = press-down feedback) and `.on_release`.

### Call-site migration

- `conversation_list.rs:75`: `.on_press(move || nav.push(...))` → `.on_tap(move || nav.push(...))`.
- `chat_screen.rs:176` (send button): `.on_press(on_send)` → `.on_tap(on_send)`.

These are the action callbacks (navigation, send) — they must be arena-mediated so a drag on the list doesn't navigate.

## Edge Cases & Behavior

1. **Tap outside any ScrollView (e.g. send button, tab bar).** Hit path contains a `GestureDetectorElement` but no `ScrollViewElement`. Arena has only a `TapRecognizer`. Press → `on_press` fires (bubble). Move < slop → still pending. Release → `TapRecognizer` accepts → `on_tap` fires. Identical timing to today for non-scroll taps. No regression.

2. **Tap inside ScrollView on a non-tappable row.** Hit path has `ScrollViewElement` but no `GestureDetectorElement`. Arena has only `VerticalDragRecognizer`. Press → `on_press` no-op (no detector). Release before slop → recognizer rejects on up, arena sweeps to no winner, no callback. A pure tap on non-tappable scroll content does nothing — correct.

3. **Press, no move, no release (pointer captured / window blur).** Window loses focus mid-press, or pointer leaves window. `EventHandler` must synthesize `ArenaEvent::Cancel`: feed Cancel to arena (all recognizers reject, `closed = true`, no winner), drop arena. Without this the arena would leak as `Some(...)` forever, blocking the next press. Implementation: on `WindowEvent::Focused(false)` or pointer-leave-with-button-held in `window.rs`, call a new `pipeline.cancel_current_gesture()`.

4. **Press inside ScrollView, release outside.** Press at (200,100) in scroll view → arena created with both recognizers. Drag < slop, move outside scroll view bounds, release outside. `ArenaEvent::Up` still feeds the arena (the arena tracks the pointer, not hit-test). `TapRecognizer` sees up with no slop breach → accepts → `on_tap` fires. This matches Flutter: once a recognizer is in the arena, it tracks the pointer to up regardless of position. The row navigates. Acceptable — a sub-slop press-and-drag-off still counts as a tap.

5. **Drag that exceeds slop, then released.** Press → both pending. Move past 18px → drag accepts, tap rejected, arena closed. Further moves → only drag winner's `on_arena_winner_update` called (scrolls). Release → drag winner's `on_arena_winner_update` called with `Up` (drag ends, no scroll applied on up). Release does not bubble → `on_release` doesn't fire on the row. Correct: the row was dragged, not tapped.

6. **`on_press` semantics when drag wins.** `on_press` already fired on press-down (immediate, via bubble). When drag later wins, `on_press` has *already* fired — it cannot be un-fired. This is exactly Flutter's `onTapDown`: it fires before the arena resolves, and stays fired even if a drag wins. If a consumer wants press-down *visual* feedback that reverts on drag-win, that's a future `on_tap_cancel` callback — **out of scope** for this design. `on_press` remains pure press-down notification.

7. **Nested ScrollViews (vertical inside vertical).** Both `ScrollViewElement`s register `VerticalDragRecognizer`s. On slop breach, the arena's "first to accept wins" rule applies — but both would accept at the same move event. Resolution: **deepest wins on ties**. The arena walks recognizers in registration order (deepest-first), so the deepest recognizer is at index 0 and wins the sweep. The outer ScrollView never scrolls. Matches Flutter (inner scroll view claims the gesture). No extra work needed — registration order handles it.

8. **Pointer id / multi-touch.** `InputEvent` has no pointer id today. This design uses one arena at a time. A second finger pressing while the first arena is open is **undefined behavior** (the press would either be ignored or replace the arena). Documented limitation. Multi-touch requires adding pointer ids to `InputEvent` first — future work, explicitly out of scope.

9. **Arena state across rebuilds.** The arena holds `Box<dyn GestureRecognizer>` owned by the pipeline, not the elements. If a rebuild swaps the `GestureDetectorElement`'s widget (e.g. row recycled), the recognizer in the arena is stale — but the arena is per-press and dropped on release, so the stale recognizer only matters between press and release. A rebuild mid-press is rare (the gesture itself doesn't trigger rebuilds; scroll offset changes do, but those rebuild the scroll view's child, not the detector). The `on_tap` callback is captured in the recognizer at registration time; if the widget rebuilds with a new callback, the stale recognizer fires the old one. Acceptable for v1 (Flutter has the same edge case and resolves it via `GestureDetector` identity; we can add GlobalKey-based invalidation later if it bites).

10. **Hit-test misses (press in empty space).** No arena created (hit result `!is_hit()`). Existing focus-clear on press unchanged.

## Testing Strategy

Three test layers, matching the codebase's existing patterns (unit tests in `#[cfg(test)] mod tests`, pipeline integration tests like `scroll_view.rs:461`).

### Layer 1 — Recognizer unit tests (`gestures/tap.rs`, `gestures/vertical_drag.rs`)

Pure state-machine tests, no pipeline, no elements. These are the highest-value tests because the recognizers are the disambiguation logic.

`TapRecognizer` tests:
- `tap_accepts_on_up_after_down_no_move` — Down then Up → `Accepted`.
- `tap_rejects_on_move_past_slop_vertical` — Down, Move y=20 → `Rejected`.
- `tap_rejects_on_move_past_slop_horizontal` — Down, Move x=20 → `Rejected` (both axes).
- `tap_stays_pending_on_move_within_slop` — Down, Move y=10 → `Pending`.
- `tap_rejects_on_cancel` — Down, Cancel → `Rejected`.
- `tap_rejects_on_up_after_slop_breach` — Down, Move y=20 (rejected), Up → still `Rejected`.

`VerticalDragRecognizer` tests:
- `drag_accepts_on_cumulative_move_past_slop` — Down, Move y=10, Move y=10 (total 20) → `Accepted`.
- `drag_stays_pending_on_single_small_move` — Down, Move y=10 → `Pending`.
- `drag_rejects_on_up_without_slop` — Down, Up (no slop breach) → `Rejected`.
- `drag_stays_accepted_after_slop` — Down, Move y=20 (accepted), Move y=5 → still `Accepted`.
- `drag_rejects_on_cancel` — Down, Cancel → `Rejected`.
- `drag_cumulative_back_and_forth_still_breaches` — Down, Move y=15, Move y=-15, Move y=20 (cumulative 50) → `Accepted` (net ~5 but cumulative 50). *Documents the cumulative-delta design decision.*

### Layer 2 — Arena unit tests (`gestures/arena.rs`)

Arena resolver with the real Tap+Drag pair.

- `arena_resolves_drag_winner_on_slop_breach` — add Tap + Drag, feed Down+Move(20y) → `Resolved{winner=drag}`, Tap `Rejected`.
- `arena_resolves_tap_winner_on_release_before_slop` — add Tap + Drag, feed Down+Up → `Resolved{winner=tap}`, Drag `Rejected`.
- `arena_open_during_small_move` — add Tap + Drag, feed Down+Move(10y) → `Open`, both `Pending`.
- `arena_closed_no_winner_on_cancel` — add Tap + Drag, feed Down+Cancel → `ClosedNoWinner`, both `Rejected`.
- `arena_single_recipient_sweeps_on_up` — add only Tap, feed Down+Up → `Resolved{winner=tap}` (sweep-to-first rule).
- `arena_deepest_wins_on_tie` — add Drag (deepest) then Drag (outer), feed Down+Move(20y) → winner is index 0 (deepest). *Validates edge case 7.*
- `arena_add_noop_after_closed` — close arena, `add()` third recognizer → arena ignores it, count unchanged.
- `arena_no_second_winner_after_closed` — close on drag, feed Up → winner stays drag (tap doesn't retro-accept).

### Layer 3 — Pipeline integration tests (`elements/scroll_view.rs` tests block)

These are the bug-fix validation tests — they exercise the full event flow. They mirror the existing `test_touch_drag_scrolls_via_pipeline` at line 461.

- `test_drag_in_tappable_row_scrolls_not_navigates` — **the bug repro.** ScrollView wrapping rows of `GestureDetector.on_tap`. Press at (200,100), Move y=-50 (past slop), Release. Assert: `ScrollController::current_offset() > 0` (scrolled) AND tap callback counter == 0 (did not navigate). This test *fails today* and *passes after the fix*.
- `test_tap_in_tappable_row_navigates_not_scrolls` — same setup, Press + Release (no move past slop). Assert: tap counter == 1 AND `current_offset() == 0` (no scroll).
- `test_drag_clamps_at_top_with_arena` — Press, drag down 1000px from offset 0 → `current_offset() == 0` (clamp preserved through arena path). *Replaces the current `test_touch_drag_clamps_at_top` which uses the old direct-drag path.*
- `test_mouse_wheel_unaffected_by_arena` — wheel event, no pointer press → `current_offset` changes via old `handle_scroll_event`. *Regression guard that wheel path is untouched.*
- `test_on_press_fires_on_down_regardless_of_drag_win` — Press (assert `on_press` counter == 1), then drag past slop, release. Assert: `on_press` counter still 1 (fired once on down), `on_tap` counter 0 (drag won). *Validates edge case 6 — `on_press` is immediate feedback, not gated by arena.*
- `test_cancel_on_blur_drops_arena` — Press, synthesize cancel (call `pipeline.cancel_current_gesture()`), press again at different location. Assert: second press creates fresh arena, no panic, no stale winner. *Validates edge case 3.*
- `test_tap_outside_scroll_view_unchanged` — bare `GestureDetector.on_tap`, no ScrollView. Press + Release → tap fires. *Regression guard for non-scroll taps (edge case 1).*

### Testing conventions

- Pipeline tests use `ThreeTreePipeline::new(Arc::new(AnimationTicker::new()))`, `crate::resource::new_font_system()`, `TaffyLayoutEngine::new()`, `test_clipboard()` helper — all already established in `scroll_view.rs` tests.
- No GPU/window needed — `MockBackend` not required since these tests exercise event routing, not rendering.
- Recognizer/arena tests use plain `assert_eq!(recognizer.resolution(), RecognizerResolution::Accepted)` — no async, no channels.

### Out of scope (not tested)

- Multi-touch (no pointer ids in `InputEvent`).
- Horizontal drag recognizer (not implemented).
- Long-press recognizer (not implemented).
- Visual press-feedback revert on drag-win (no `on_tap_cancel` callback).

## File Inventory

New files:
- `vexo/src/gestures/mod.rs`
- `vexo/src/gestures/arena.rs`
- `vexo/src/gestures/recognizer.rs`
- `vexo/src/gestures/arena_event.rs`
- `vexo/src/gestures/tap.rs`
- `vexo/src/gestures/vertical_drag.rs`

Modified files:
- `vexo/src/lib.rs` — declare `pub mod gestures;`
- `vexo/src/element.rs` — add `register_gestures` and `on_arena_winner_update` default methods
- `vexo/src/event_handler.rs` — arena creation on press, move/up routing through arena, `cancel_current_gesture`
- `vexo/src/pipeline.rs` — `current_arena: Option<GestureArena>` field, thread through `handle_event`
- `vexo/src/widgets/gesture_detector.rs` — add `on_tap` field + builder; `register_gestures` adds `TapRecognizer`; `on_arena_winner_update` fires `on_tap`
- `vexo/src/widgets/mod.rs` — add `.on_tap()` to `WidgetExt`
- `vexo/src/elements/scroll_view.rs` — remove drag branches from `on_event`; `register_gestures` adds `VerticalDragRecognizer`; `on_arena_winner_update` applies scroll delta; remove `drag_active`/`drag_last_y` fields
- `vexo/src/window.rs` — call `pipeline.cancel_current_gesture()` on `WindowEvent::Focused(false)` / pointer-leave-with-button-held
- `shared_app/src/chats/conversation_list.rs:75` — `.on_press(action)` → `.on_tap(action)`
- `shared_app/src/chats/chat_screen.rs:176` — `.on_press(on_send)` → `.on_tap(on_send)`
