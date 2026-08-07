# Telegram-Style Context Menu — Design

**Date:** 2026-08-07
**Scope:** Convert the chat message context menu from the iMessage style
(dim barrier + bright bubble copy + symmetric open/close spring) to a
Telegram-desktop style: a click-point-anchored popover cluster (reactions
pill + actions card) that scales in on open and dismisses instantly on
outside tap. No dim scrim, no bubble lift, no close animation.

Replaces the host-side rendering from `2026-08-06-imessage-context-menu-design.md`.
The `MenuBuilder` / `MenuContent` / `MenuMetrics` content contract and the
`message_menu.rs` builder are unchanged — the cards themselves stay the same.
All changes are concentrated in the `ContextMenu` host (`context_menu.rs`)
and its controller API (breaking).

## Goal

1. **Click-point anchor** — the menu cluster's top-left sits at the right-click
   cursor position (not the bubble's bounds/center).
2. **No dim barrier** — remove the full-screen scrim; the chat stays fully
   visible behind the menu.
3. **No bubble copy/lift** — do not clone or re-render the tapped bubble; it
   stays in place, undimmed.
4. **Two stacked cards as one cluster** — reactions pill on top, actions card
   directly below (gap between), left-aligned to the click x.
5. **Scale-in open animation** — cluster scales `0.92 → 1.0` about the click
   point (both cards share the origin so the cluster stays cohesive).
6. **Instant dismiss** — outside tap closes the menu immediately; no reverse
   spring, no close animation.
7. **Edge-aware fit** — vertical flip-up when no room below; horizontal
   left-clamp when the cluster would overflow the right edge.

## Non-goals (explicitly out of scope)

- **No long-press trigger.** Right-click only (unchanged from prior spec).
- **No real emoji.** FA icons stand in (unchanged).
- **No `Escape` dismiss.** Unchanged from today.
- **No close animation.** Dismiss is instant by design (matches native OS
  popups; the close is unobtrusive, the open draws the attention).
- **No horizontal mirror-flip.** Right-edge overflow shifts the cluster left
  (right edge stays in view) rather than flipping to extend left from the
  click point.
- **No cutout / bubble-lift fallback.** The bright bubble copy is removed
  outright; if a cutout is ever wanted again, it is a trivial re-add (carry
  `bubble_widget` back into `show()`), not a planned path.
- **No `should_rebuild()` overrides.** The menu is a short-lived overlay.
- **No keyboard shortcuts.** Unchanged.
- **No change to menu content.** `message_menu.rs` `builder()` produces the
  same reactions pill + actions card with the same `MenuMetrics`.

## Chosen approach: click-point API + host-render-only (Approach B)

Change `controller.show()` to take the click position instead of bubble
bounds + widget. The host's `render` builds a 3-layer Stack (content,
transparent dismiss barrier, menu cluster) anchored at the click point.
The cluster scales in about the click point on open; `close()` instantly
clears state and unmounts.

The public `context_menu_trigger(child, controller, builder)` sugar keeps
its signature — `chat_screen.rs` (the only external caller) is unaffected.
Internally the trigger forwards the click `pos` (already received from
`on_secondary_press`, previously discarded as `_pos`) instead of `bounds`.

### Rejected alternatives

- **Approach A — host-render-only, preserve `show` API.** Keep
  `show(bounds, bubble_widget, builder)` and anchor cards to
  `bubble_bounds.left/top`. *Rejected because:* the user explicitly wants
  click-point anchoring, and the API passes element bounds, not the click
  point. Cannot satisfy the requirement without an API change.
- **Approach C — keep `bubble_widget` as `Option` (future-proof).**
  `show(click_pos, bubble_widget: Option<Box<dyn Widget>>, builder)`.
  *Rejected because:* YAGNI. No current consumer renders the bubble. If a
  cutout is ever wanted again, it is a trivial add; carrying an `Option`
  now is speculative complexity.

## Architecture

### Phase machine simplifies to 3 states

The `Closing` phase existed solely to animate the reverse spring on close.
With instant dismiss it is dead code and is removed.

```
Closed ──show()──▶ Opening ──spring 0→1 settles──▶ Open ──close()──▶ Closed (instant)
```

- `show(click_pos, builder)` — starts the forward spring (0→1), phase=`Opening`,
  stores `click_pos` + `builder` in `OpenState`.
- `close()` — sets phase=`Closed`, clears `OpenState` immediately. No reverse
  spring, no retarget. The host rebuilds with `phase == Closed` and the
  overlay layers unmount.
- `advance(now)` — samples the spring; transitions `Opening → Open` on settle
  (snaps value to 1.0). No `Closing → Closed` transition (gone with the phase).

The `AnimationController`, spring params (`SpringDescription::ios(340.0, 1.0)`),
ticker wiring, and dirty callback stay as in the iMessage spec.
**Retarget-from-live-value applies only to re-show:** `show()` reads the
controller's current value and starts the forward spring from there → 1.0, so
close-then-re-show produces no jump. `close()` does NOT touch the spring — it
just flips phase + clears `OpenState`. A forward spring still running at the
moment of instant close continues ticking in the background but is a harmless
no-op: `advance()` guards on `phase != Closed` and the unmounted overlay means
nothing reads the value. The spring settles to 1.0 on its own (~0.6s) and stops
firing dirty callbacks. The only spring-driven value is `v`; `v` maps to
`scale = 0.92 + v * 0.08` for both cards (and nothing else — no dim alpha, no
bubble lift, no opacity).

### `ContextMenuController` API change (breaking)

```rust
// Before
pub fn show(&self, bubble_bounds: Bounds<Logical>, bubble_widget: Box<dyn Widget>,
             builder: MenuBuilder)
pub fn close(&self)  // starts reverse spring; open-state cleared after settle
pub fn phase(&self) -> Phase   // Closed | Opening | Open | Closing

// After
pub fn show(&self, click_pos: Point<Logical>, builder: MenuBuilder)
pub fn close(&self)  // instant: clears open-state, phase=Closed
pub fn phase(&self) -> Phase   // Closed | Opening | Open
```

`OpenState` stores `click_pos: Point<Logical>` + `builder` (no `bubble_widget`,
no `bubble_bounds`). The bubble widget clone is no longer carried since no
layer renders it.

`Phase` enum:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Closed, Opening, Open }
```

### `context_menu_trigger` — public signature unchanged

```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget>
```

Internally, the `on_secondary_press` callback now forwards `pos` (the click
point, already received but currently discarded as `_pos`):

```rust
child.on_secondary_press(move |pos, _bounds| {
    ctrl.show(pos, builder.clone());
})
```

The `child.clone_boxed()` capture is removed entirely — no bubble copy to
render. `chat_screen.rs:125` (the only external caller) is unaffected.

### Unchanged types

- `MenuContent { reactions, actions, metrics }` — same shape, same semantics.
- `MenuMetrics { reactions_size, actions_size, gap }` — same constants
  (pill 222×44, card 200×134, gap 8) from `message_menu.rs`.
- `MenuBuilder` — same `Rc<dyn Fn(&ContextMenuController, &ThemeData) -> MenuContent>`.
- `message_menu.rs` `builder()` — produces the same two cards.

## Host render: 3-layer Stack

`ContextMenu::render` (in `vexo_uikit/src/context_menu.rs`) builds a
`Stack`. Layer [1] (content) is always mounted; layers [2] and [3] are
mounted only when `phase != Closed`:

| Layer | Widget | Purpose |
|---|---|---|
| [1] Content | `self.child.clone_boxed()` | The chat screen, always rendered |
| [2] Transparent dismiss barrier | full-screen `GestureDetector.on_press(→close)` | Tappable, transparent. Dismisses on outside tap |
| [3] Menu cluster | `Positioned` pill + `Positioned` card | The two cards, anchored at the click point (Section below) |

### Layer [2]: transparent dismiss barrier

Same structure as the current dim barrier (lines 545-582 of the iMessage
implementation) **minus** the `Opacity` + `DecoratedBox(BLACK)`. Structure:

```
Positioned(0,0,0,0) → GestureDetector.on_press(→close) →
  WithLayout(width_percent=1, height_percent=1) → Text("")
```

Key property preserved: the barrier is **always hit-testable** regardless of
visual state, so dismiss works mid-open-animation. This is why it is a
`GestureDetector` over a full-screen `WithLayout` rather than a conditional —
the hit region exists even when nothing is painted. `Opacity` is paint-only
(layout + hit-test pass-through); since we paint nothing, we omit it.

**Tapping inside a card** does not dismiss — the card's own
`GestureDetector::on_tap` handles the action and calls `controller.close()`;
the gesture arena resolves the tap to the card, not the barrier underneath.

**Tapping the original bubble** (visible, not dimmed) dismisses — the press
passes through to the transparent barrier since the bubble is below layer [2].
This is correct Telegram behavior: tapping anywhere outside the menu cards
closes it, including tapping the message itself.

### Layer [3]: click-point-anchored cluster

**Cluster geometry.** Pill on top, actions card directly below, with
`metrics.gap` (8px) between them. Total cluster height =
`pill_h + gap + card_h`; cluster width = `max(pill_w, card_w)`.

**Default placement** — cluster's top-left at the click point:

```text
cluster_x = click_pos.x
cluster_y = click_pos.y
pill:  left = cluster_x, top = cluster_y
card:  left = cluster_x, top = cluster_y + pill_h + gap
```

Both cards share `cluster_x` as their `.left()` (left-aligned, not centered —
matches "top-left = click point"). If `pill_w != card_w`, the narrower one is
left-aligned to the cluster's left edge. (Currently `pill_w=222, card_w=200`,
so the card's right edge is 22px left of the pill's right edge — visually
fine, both anchored at the click x.)

**Vertical flip.** Compute `cluster_h = pill_h + gap + card_h`. If
`click_pos.y + cluster_h > window_h - 8`, the cluster flips **above** the
click point:

```text
cluster_y = click_pos.y - cluster_h   // cluster bottom = click_pos.y
```

Internal stack order unchanged (pill still on top, card below it) — only the
cluster's absolute y shifts up. The flip threshold is
`click_pos.y + cluster_h > window_h - 8` (would overflow the 8px bottom
margin). If it does not fit above either (rare: very tall menu, click near
vertical center), pick whichever side has more room.

**Horizontal left-clamp.** Compute `cluster_w = max(pill_w, card_w)`.

```text
cluster_x = click_pos.x, clamped to [8, window_w - 8 - cluster_w]
```

Both cards use `cluster_x` as their `.left()`. No horizontal mirror-flip —
right-edge overflow shifts the cluster left so its right edge stays at
`window_w - 8`.

**When `window_w` or `window_h` is 0** (no `MediaQuery` ancestor — defensive,
should not happen in production hosts): skip the clamp/flip, place at
`click_pos` directly. Same fallback posture as the current code.

### Scale animation: `scale_about_point`

Replaces the `scale_about_center` helper (lines 659-687 of the iMessage
implementation). Both cards scale about the **same origin = click point** so
the cluster stays cohesive as it grows — if each card scaled about its own
center, they would separate during the animation.

```rust
fn scale_about_point(
    child: Box<dyn Widget>,
    s: f32,
    origin: Point<Logical>,
) -> Box<dyn Widget> {
    // M = translate(ox, oy) ∘ scale(s, s) ∘ translate(-ox, -oy)
    let transform = vexo::AffineTransform::translation(origin.x, origin.y)
        .mul(&vexo::AffineTransform::scale(s, s))
        .mul(&vexo::AffineTransform::translation(-origin.x, -origin.y));
    vexo::Transform::new(child, transform).boxed()
}
```

Composed into a single `AffineTransform` (same rationale as `scale_about_center`:
one matrix → one inverse in the hit-tester, avoiding the per-level `is_inside`
failure). `TransformRenderObject` is a layout pass-through — wrapping a widget
in `scale_about_point` does not change its `computed_bounds`, only its painted
appearance and hit region.

**Application:**

```text
pill_scale = card_scale = 0.92 + v * 0.08   // v: 0→1 on open
pill: scale_about_point(content.reactions, pill_scale, click_pos)
card: scale_about_point(content.actions, card_scale, click_pos)
```

**No opacity fade.** Same as commit `8be03c0`: the cards stay opaque
throughout so they always write depth and occlude background text — avoids
the show-through-then-disappear artifact that `Opacity(v)` caused. The scale
animation alone provides the visual transition.

**Snap to target on settle** (already in place via `8be03c0`): when `is_done()`
fires, `v` snaps to 1.0, scale snaps to exactly 1.0. No float drift.

### Tunable constants

- `0.92` — open scale floor. If the 8% grow feels too subtle in practice, bump
  it (one-line change). Spring params (`ios(340.0, 1.0)`) likewise.
- `8.0` — window-edge margin for vertical flip + horizontal clamp.

## What is removed

From `vexo_uikit/src/context_menu.rs`:

- Dim barrier layer (current lines 545-582) — replaced by transparent barrier.
- Bright bubble copy layer (current lines 584-608) — removed entirely.
- `scale_about_center` helper (current lines 659-687) — replaced by
  `scale_about_point`.
- Bubble-bounds-relative edge-aware positioning (current lines 472-543) —
  replaced by click-point anchoring.
- `Phase::Closing` variant and its `Closing → Closed` transition in `advance()`.
- `bubble_widget` + `bubble_bounds` fields from `OpenState`.
- `child.clone_boxed()` capture in `context_menu_trigger`.

## What stays unchanged

- `MenuContent`, `MenuMetrics`, `MenuBuilder` types and semantics.
- `ContextMenuController` phase machine (minus `Closing`), spring params,
  retarget-from-live-value, `advance()`, dirty callbacks, ticker wiring.
- `message_menu.rs` `builder()` content (reactions pill + actions card).
- The dismiss-on-outside-tap behavior (now on a transparent barrier).
- `MediaQuery::of(ctx)` and `Theme::of(ctx)` dependencies in the host (still
  rebuilds on resize; builder still reads live theme).

## Migration: test call sites

~20 `controller.show(bounds, bubble_widget, builder)` call sites in
`context_menu.rs` tests + 1 in `message_menu.rs:308` become
`controller.show(pos, builder)`. `pos` derived from the old `bounds` center
or a representative click point per test. This is mechanical.

Tests referencing `Phase::Closing` (a handful) are updated to assert
`Phase::Closed` immediately after `close()` — there is no `Closing` state to
pass through anymore.

## Testing

- **Unit:** existing context-menu tests updated to the new `show(pos, builder)`
  signature. The metrics verification test in `message_menu.rs`
  (`test_metrics_match_real_sizes`) is updated mechanically.
- **New test — click-point anchor:** open the menu at a known `click_pos`,
  read back the pill/card `Positioned` offsets from the render tree, assert
  they equal `click_pos` (default placement, no overflow).
- **New test — vertical flip:** open with `click_pos.y` near the bottom edge,
  assert the cluster's top is above the click point.
- **New test — horizontal clamp:** open with `click_pos.x` near the right edge,
  assert `cluster_x = window_w - 8 - cluster_w`.
- **New test — instant dismiss:** call `close()`, assert `phase() == Closed`
  immediately (no `Closing` state), assert overlay layers unmount on next
  rebuild.
- **Existing test — dismiss barrier tappable mid-open:** the transparent
  barrier must remain hit-testable during `Opening` (same property as the dim
  barrier; test #5 from the iMessage suite, updated to assert no dim is
  painted).
- **Manual:** `cargo run -p desktop_demo`, right-click a message near each
  edge of the window (top, bottom, left, right, center) and confirm the
  cluster positions correctly, scales in, and dismisses instantly on outside
  tap.
