# iMessage-Style Context Menu — Design

**Date:** 2026-08-06
**Scope:** Redesign the chat message context menu to match iMessage's layout
and animation: a dimmed "spotlight" backdrop with the tapped bubble lifted
bright on top, a reactions pill scaling in above the bubble, and an actions
card scaling in below it — all driven by a single critical spring with
symmetric open/close. Replaces the current single-card instant-appear menu in
`shared_app/src/chats/message_menu.rs`.

Builds on the `ContextMenu` host / `ContextMenuController` / `MenuBuilder`
trio shipped in `2026-08-05-custom-context-menu-view-design.md` and restyled
in `2026-08-05-styled-context-menu-design.md`. This spec changes the host
(adds animation + cutout), the controller API (breaking), the builder output
shape (one widget → two cards + metrics), and adds one small framework hook
(global bounds in the secondary-press callback).

## Goal

Replace the current static single-card menu (reactions row + divider + 3 item
rows, instant mount/unmount) with the iMessage effect:

1. **Spotlight dim** — screen darkens except the tapped bubble.
2. **Bubble lift** — the tapped bubble brightens and lifts ~4px / scales ~3%.
3. **Reactions pill** — a compact pill of 6 FA icons scales+fades in above
   the bubble.
4. **Actions card** — Copy/Reply/Delete scales+fades in below the bubble.
5. **Single spring** — all four overlays move in lockstep on open and close.
6. **Symmetric close** — dismiss starts a reverse spring; the menu stays
   mounted until it settles (no instant unmount).
7. **Edge-aware positioning** — cards flip above/below the bubble when near
   screen edges; horizontally clamped to stay on-screen.

## Non-goals (explicitly out of scope)

- **No long-press trigger.** Right-click only (decided in brainstorming).
  iOS users get no menu — this redesign is desktop-only for now.
- **No real emoji.** FA icons stand in (unchanged from prior spec — the text
  pipeline is monochrome-only, no emoji font loaded).
- **No `Escape` dismiss.** Unchanged from today.
- **No staggered row animation.** Whole-card scale+fade only (matches
  iMessage — its rows don't stagger either).
- **No animated hover tint.** Hover stays instant (driven by its own
  `Signal<bool>`, as today).
- **No cutout-frame fallback unless the spike test fails.** The bright
  bubble copy is the primary approach; the 4-rect cutout is a documented
  fallback, not a dual implementation.
- **No `should_rebuild()` overrides.** The menu is a short-lived overlay,
  not a hot path. Default `true` everywhere. (The host's per-tick rebuild
  during animation is necessary, not a hot-path optimization target.)
- **No new `vexo_uikit` module for menu tokens.** Colors derived inline
  from `ThemeData`, as today.
- **No keyboard shortcuts** (⌘C etc.) — unchanged from prior spec.

## Chosen approach: host owns the animation + cutout (Approach A)

The `ContextMenu` host `Component` grows a real `State` owning an
`AnimationController` + a 4-state phase machine. `controller.show()` is
extended to carry the tapped bubble's global bounds + a widget clone + the
builder; `close()` flips to `Closing` (keeps rendering) and only unmounts
when the spring settles. Each frame the host reads the controller value and
renders: animated dim barrier → bright bubble copy re-projected on top (the
cutout) → reactions pill and actions card wrapped in `Transform::scale` +
`Opacity`, anchored above/below the bubble center with edge-aware flip/clamp.
The builder stays pure content (the reactions pill + actions card).

To get the bubble's **global bounds**, add one small framework piece: extend
`GestureDetector` so the secondary-press callback receives the element's
global bounds (computed during event dispatch, where the render tree is
walkable). `context_menu_trigger` forwards `pos + bounds + bubble widget
clone` to `controller.show(...)`.

### Rejected alternatives

- **Approach B — builder owns the animation; host stays near-current.** Open
  animation is "free" by wrapping builder content in a mount-triggered
  scale+fade (like `FadeTransition`). For close, the barrier signals a
  `close_requested` signal that the builder's wrapper observes; the wrapper
  runs its exit spring, then calls the real `controller.close()` to unmount.
  *Rejected because:* the bubble cutout still forces the host to render the
  dim + bright bubble copy — so the host needs the same bounds plumbing and
  its own controller for the dim alpha anyway. You end up with **two**
  animation controllers (dim in host, menu in builder) that must stay in
  lockstep. Sync bugs, split logic.
- **Approach C — minimal: animate the menu only, drop the true cutout.**
  Keep the current host; wrap builder content in a mount-triggered
  scale+fade-in. Skip the dim + cutout entirely (Telegram-lite look).
  *Rejected because:* the user explicitly chose the iMessage bubble cutout,
  and this doesn't deliver it.

## Architecture

### Phase machine in the host

The `ContextMenu` host `Component` gains a real `State` owning an
`AnimationController` + a 4-state phase machine:

```
Closed ──show()──▶ Opening ──spring 0→1 settles──▶ Open ──close()──▶ Closing ──spring 1→0 settles──▶ Closed
```

- `show()` starts the forward spring (0→1) and stores the tapped bubble's
  bounds + widget + builders.
- `close()` starts the reverse spring (1→0); the menu **stays mounted**
  during `Closing` and only unmounts when the spring settles.
- `on_tick` advances the controller; transitions `Opening→Open` and
  `Closing→Closed` (the latter clears the open-state signal → unmount).

The `AnimationController` lives inside `ContextMenuController`'s shared
state (so `close()`, called from barrier/item/reaction closures with only
`&ContextMenuController`, can drive it). The host wires the ticker + dirty
callback in `on_mount` and reads `controller.animation_value()` each render.

### `ContextMenuController` API change (breaking)

```rust
// Before
pub fn show(&self, position: Point<Logical>, builder: MenuBuilder)
pub fn close(&self)  // clears position immediately

// After
pub fn show(&self, bubble_bounds: Bounds<Logical>, bubble_widget: Box<dyn Widget>,
             builder: MenuBuilder)
pub fn close(&self)  // starts reverse spring; open-state cleared only after settle
pub fn animation_value(&self) -> f64
pub fn phase(&self) -> Phase    // Closed | Opening | Open | Closing
pub fn set_animation_ticker(&self, t: Arc<AnimationTicker>)   // host calls in on_mount
pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>)

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase { Closed, Opening, Open, Closing }
```

`position` is dropped — the host now anchors everything to `bubble_bounds`
(the menu cards frame the bubble). `bubble_widget` is the bright, lifted
copy.

The controller's internal signal changes from `Signal<Option<Point>>` to
`Signal<Option<OpenState>>` (where `OpenState` carries `bubble_bounds` +
`bubble_widget` + `builder`). The host reads it via `signal_value`; `None`
means "closed → unmount overlays." This signal is the "open-state signal"
referenced elsewhere in this spec. It is cleared on `Closed` (after the
reverse spring settles), not on `close()`.

### `MenuBuilder` output split

One closure, two outputs (shares the ctrl+theme capture):

```rust
pub struct MenuContent {
    pub reactions: Box<dyn Widget>,  // the pill, rendered above the bubble
    pub actions:    Box<dyn Widget>,  // the card, rendered below the bubble
    pub metrics:    MenuMetrics,      // sizes for positioning + transform anchors
}

pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &ThemeData) -> MenuContent>);
```

### Framework change: global bounds in the secondary-press callback

`context_menu_trigger`'s `on_secondary_press` currently receives only
`Point`. Today render objects store bounds **relative to their parent**, and
`RenderContext` doesn't expose the registry — so the host can't resolve a
bubble's global bounds at render time. We extend `GestureDetector::
on_secondary_press` (and the trigger) to also deliver the element's
**global bounds**, computed during event dispatch where the render tree is
walkable. `context_menu_trigger` forwards `pos + bounds + bubble_widget.
clone_boxed()` to `controller.show(...)`.

### Host render output (Stack, push order → hit-test reverse)

1. **Content** (chat screen; tapped bubble sits here, will be dimmed)
2. **Dim-barrier** — full-screen, animated alpha (`0 → ~0.4`),
   `GestureDetector on_press → close`. Doubles as the dismiss barrier.
3. **Bright bubble copy** — `Positioned` at `bubble_bounds`, wrapped in
   `Transform` (scale ~1.03 + translate-up ~4px around the bubble center).
   Its own `GestureDetector on_press → close` (tapping the lifted bubble
   dismisses — matches iMessage; no reliance on event pass-through).
4. **Reactions pill** — `Positioned` above the bubble; scale 0.8→1.0 +
   opacity 0→1 with the spring; per-icon `GestureDetector`s.
5. **Actions card** — `Positioned` below the bubble; same scale+fade;
   per-row `GestureDetector`s.

Taps on cards/bubble hit their handlers; taps on the dim hit the barrier →
close. The dim covers the original bubble (it appears dimmed); the bright
copy on top is the lifted focal point.

## Visual layout & widget tree

### Positioning model

All anchoring is relative to `bubble_bounds` (a global `Bounds<Logical>`:
left, top, width, height). The host positions each overlay with
`Positioned::new(...).left(x).top(y)`.

**Default (room above + below):**
- Reactions pill: centered horizontally over the bubble, bottom edge 8px
  above the bubble top.
- Actions card: centered horizontally, top edge 8px below the bubble bottom.

**Edge-aware flipping (decided per-open, not animated mid-open):**
- If not enough room above for the reactions pill (< pill height + 8 + 8
  margin), put the pill **below the actions card** (iMessage does this on
  edge cases).
- If not enough room below for the actions card, flip the whole stack
  **above** the bubble (reactions on top, actions below it, both above the
  bubble).
- Horizontal: clamp so the card never leaves the window; if the bubble is
  near a side, shift the cards to keep them on-screen. Cards are centered
  on the bubble's horizontal center, then clamped to
  `[8, window_w - card_w - 8]`.

Window size comes from `MediaQuery::of(ctx)` (already a dependency path the
host can read).

### Menu card visual style (carried over from current `message_menu.rs`)

Only the *assembly* changes (two cards instead of one). Per-card style is
unchanged:
- Outer `DecoratedBox`: `theme.surface` bg, `theme.outline` 1px border,
  12px corner radius, shadow `BLACK@0.20` blur 12 offset `(0,4)`.
- `min_width: 200.0` on the actions card; the reactions pill is
  `width: auto` (sized to its content, ~pill-shaped via 18px corner radius
  + tighter padding).

### Reactions pill

```
DecoratedBox(surface, outline@1, radius=18, shadow)
└── WithLayout(padding_each(6,6,5,5))
    └── row!(gap=6.0).justify(Center)
        ├── Icon(ThumbsUp,    18.0, on_surface_variant) → GestureDetector
        ├── Icon(Heart,       18.0, ...)                 → GestureDetector
        ├── Icon(FaceLaugh,   18.0, ...)                 → GestureDetector
        ├── Icon(FaceSurprise,18.0, ...)                 → GestureDetector
        ├── Icon(FaceSadTear, 18.0, ...)                 → GestureDetector
        └── Icon(FaceAngry,   18.0, ...)                 → GestureDetector
```

Each reaction: `on_tap → log + ctrl.close()`. No hover bg (stateless, as
today). Icon size bumped 16→18 (iMessage's pill icons read slightly larger).

### Actions card

```
DecoratedBox(surface, outline@1, radius=12, shadow)
└── WithLayout(min_width=200)
    └── column!(gap=0)
        ├── MenuRow(Copy,   Icons::Copy,   destructive=false)
        ├── MenuRow(Reply,  Icons::Reply,  destructive=false)
        └── MenuRow(Delete, Icons::Trash,  destructive=true)
```

Same `MenuRow` component as today (leading FA icon + label + hover tint +
`on_tap → log + close`). **No divider** — the divider existed only to
separate reactions from actions in the single-card layout; with two cards
it's gone.

### Full host render tree (when open)

```
Stack
├── [1] content (chat screen — tapped bubble sits here, dimmed by overlay)
├── [2] Positioned(left=0,top=0,right=0,bottom=0)                    // dim-barrier
│       └── Opacity(alpha = spring * 0.4)
│           └── DecoratedBox(BLACK, full-size)
│               └── GestureDetector.on_press(→ close)
├── [3] Positioned(left=bubble.left, top=bubble.top)                 // bright bubble copy
│       └── Transform(scale = 1.0 + spring*0.03, translate_y = -spring*4.0, anchor=center)
│           └── Opacity(alpha = 1.0)                                  // full bright
│               └── bubble_widget.clone()
│                   └── GestureDetector.on_press(→ close)
├── [4] Positioned(left=reactions_x, top=reactions_y)               // reactions pill
│       └── Transform(scale = 0.8 + spring*0.2, anchor=center)
│           └── Opacity(alpha = spring)
│               └── <reactions pill from builder>
└── [5] Positioned(left=actions_x, top=actions_y)                    // actions card
        └── Transform(scale = 0.8 + spring*0.2, anchor=center)
            └── Opacity(alpha = spring)
                └── <actions card from builder>
```

### Transform anchor note

`Transform::scale` in vexo scales about the origin (top-left), not the
center. To scale a card about its center, the host wraps it in a transform
chain: `translate(-w/2, -h/2)` → `scale(s,s)` → `translate(w/2, h/2)`,
using the `metrics` sizes for `w`/`h`. The card's *actual* layout is
unaffected — it still lays out at its real content-driven size via the
normal layout pass; `metrics` is used **only** for this transform-anchor
math and for positioning. The host does NOT pin the card's width/height.
If a card's real laid-out size diverges from `metrics`, the transform
anchor is slightly off (scale pivots from a slightly wrong center) but
the card still paints at its real size — acceptable for v1, and the
constants will be tuned during implementation by reading back real laid-
out sizes.

### Constants (passed from builder to host)

```rust
pub struct MenuMetrics {
    pub reactions_size: Size<Logical>,  // ~ (168, 30) — 6 icons × (18+6gap) + padding
    pub actions_size:   Size<Logical>,  // ~ (200, 108) — 3 rows × (14+16pad)
    pub gap: f32,                       // 8.0 — gap between bubble and cards
}
```

`MenuBuilder` returns `MenuContent { reactions, actions, metrics }`. The
host uses `metrics` for positioning + transform anchors; the widgets are
still laid out normally for painting.

## Animation

### Driver

A single `AnimationController` per menu instance, driven by a **critical
spring** (`SpringDescription::ios(340.0, 1.0)` — same params the codebase
already uses for KeyboardAvoidance and scroll settle). The spring gives the
natural iMessage feel: fast attack, gentle settle, no overshoot (critical
damping).

- `show()` → `controller.animate_with(Box::new(SpringSimulation::new(
  SpringDescription::ios(340.0, 1.0), 0.0, 1.0, 0.0)))`
- `close()` → `controller.animate_with(Box::new(SpringSimulation::new(
  SpringDescription::ios(340.0, 1.0), 1.0, 0.0, 0.0)))`

Critical spring settles in ~0.6s; feels snappy yet soft. No duration tuning
needed.

### Value mapping (one spring, four consumers)

`v = controller.value()` runs 0→1 on open, 1→0 on close. Each overlay
reads `v`:

| Overlay | Transform | Opacity |
|---|---|---|
| Dim barrier | — | `v * 0.4` (fades in with the menu) |
| Bright bubble copy | scale `1.0 + v*0.03`, translate_y `-v*4.0` (lifts ~4px, grows 3%) | `1.0` (always full bright — it's the focal point) |
| Reactions pill | scale `0.8 + v*0.2` (grows from 80% to 100%) | `v` |
| Actions card | scale `0.8 + v*0.2` | `v` |

All four move in lockstep — one controller, no sync bugs.

### Scale anchor

Scale-about-center is achieved by wrapping each card:
`Transform::translate(-w/2, -h/2)` → `Transform::scale(s,s)` →
`Transform::translate(w/2, h/2)`, using the `metrics` sizes. The bubble
copy scales about its own center (using `bubble_bounds.w/h`).

### Phase transitions

```
Closed  ──show()──▶  Opening   (spring 0→1, value < 1.0)
Opening ──settle──▶  Open      (spring done, value == 1.0)
Open    ──close()──▶ Closing   (spring 1→0, value > 0.0)
Closing ──settle──▶  Closed    (spring done, value == 0.0 → clear position → unmount)
```

- During `Opening`/`Closing`, the host re-renders every tick (the
  controller's dirty callback fires).
- On settle, `on_tick` detects `!controller.is_animating()` and flips the
  phase. For `Closing→Closed`, it clears the open-state signal → next render
  omits the overlays.
- **Early close during open:** if `close()` is called mid-`Opening`, the
  controller cancels the forward spring and starts a reverse spring *from
  the current value* (`animate_with` stamps a new sim at `now` with
  `from = controller.value()`). Velocity is NOT carried over — the new
  spring starts with `v0 = 0`, so there's a velocity discontinuity but no
  position jump. For a critical spring settling in ~0.6s, this is
  imperceptible. `animate_with` already handles the cancel-and-replace
  (per `animate_with_cancels_prior_sim` test). So a user who right-clicks
  then immediately clicks away gets a smooth positional reversal, not a
  jump.
- **Re-show during close:** if `show()` is called mid-`Closing`, the
  forward spring starts from the current value (same `v0 = 0` caveat) —
  smooth retarget positionally. The new bubble bounds/widget replace the
  old ones.

### Dismiss paths (all funnel through `close()`)

| Trigger | Path |
|---|---|
| Click outside (dim barrier) | barrier `on_press` → `ctrl.close()` |
| Click the lifted bubble | bubble-copy `on_press` → `ctrl.close()` |
| Click a reaction | reaction `on_tap` → log + `ctrl.close()` |
| Click an action row | row `on_tap` → log + `ctrl.close()` |
| Right-click another bubble while open | dim barrier catches it → `ctrl.close()` (v1 limitation: close-then-right-click-again — unchanged from today) |
| Escape key | Not handled in v1 (unchanged) |

All of these start the reverse spring; the menu stays visible until it
settles.

### What does NOT animate

- **Hover tint on `MenuRow`** stays instant (driven by its own
  `Signal<bool>`, as today). Mixing hover into the spring would complicate
  the row for no perceptual gain.
- **Card content** doesn't animate independently — only the whole-card
  scale+fade. iMessage's rows don't stagger either.
- **Bubble text** doesn't animate — only the bubble's scale/position as a
  unit.

### Why not `FadeTransition`/`SlideTransition`?

Those own their controller and start on mount — fine for open, but they
have no close path (they can't run a reverse spring while staying mounted,
and they can't defer their own unmount). The host-owned controller is what
makes symmetric open/close possible.

## Testing

### Philosophy

Per CLAUDE.md: control-flow changes get integration tests that exercise
the path end-to-end. The open/close lifecycle is new multi-frame control
flow — existing tests only assert "menu appears after right-click" (single
frame). We add tests for the *new* behavior (animation lifecycle, cutout,
dismiss-during-animation), and keep the existing presence tests as
regression nets.

### New tests (in `vexo_uikit/src/context_menu.rs` test module)

These test the host/controller in isolation with a minimal builder (a
single `Text` per card), using the `ThreeTreePipeline` + `AnimationTicker`
+ `TaffyLayoutEngine` pattern already established in the file.

1. **`test_show_starts_open_spring`** — call `controller.show(bounds,
   widget, builder)`. Assert: `controller.animation_value()` is 0.0→
   advancing; `controller.phase()` is `Opening`; open-state signal is
   `Some`; after `pipeline.perform_rebuilds()` + advancing the ticker past
   settle, phase becomes `Open` and value == 1.0.
2. **`test_close_starts_reverse_spring_not_immediate_unmount`** — open,
   settle to `Open`, call `close()`. Assert: phase is `Closing`, open-state
   signal is *still* `Some` (menu stays mounted), value is decreasing.
   Advance ticker past settle → phase `Closed`, open-state signal `None`.
3. **`test_early_close_during_open_reverses_smoothly`** — open, advance a
   few frames (value ~0.5), call `close()`. Assert: value continues from
   ~0.5 *downward* (no jump to 1.0 then down), phase `Closing`. Settles to
   `Closed`. This is the retarget regression — the bug it catches is
   "early close snaps to 1.0 before reversing."
4. **`test_reshow_during_close_retargets_upward`** — open, settle, start
   close, advance a few frames (value ~0.5), call `show()` with new
   bounds. Assert: value continues from ~0.5 *upward*, phase `Opening`,
   new bounds are in effect.
5. **`test_dim_barrier_dismiss_during_animation`** — open, advance to
   mid-open, dispatch a click on the barrier. Assert: `close()` fired,
   phase `Closing`. (Existing `test_barrier_dismiss_on_outside_click`
   only tests the steady-state open case.)
6. **`test_bright_bubble_copy_rendered_on_top`** — open, settle. Walk the
   render tree from root; assert the bubble widget's content appears
   **twice** (once in-content, once as the bright copy). Confirms the
   dual-render path works — the key risk flagged in the Architecture
   section.
7. **`test_bubble_copy_size_matches_original`** — the spike test for the
   dual-render assumption. Open, settle, lay out. Find both bubble render
   objects in the tree; assert their `computed_bounds` sizes are equal.
   If this ever fails, fall back to the cutout-frame approach.
8. **`test_edge_flip_when_no_room_above`** — open with `bubble_bounds.top`
   near 0 (no room above for pill). Assert reactions pill is positioned
   *below* the actions card (or the whole stack flips below the bubble,
   per the edge logic). Verifies positioning logic doesn't clip the pill
   off-screen.
9. **`test_edge_flip_when_no_room_below`** — open with
   `bubble_bounds.bottom` near window height. Assert the whole stack
   flips above the bubble.

### Extended existing tests (in `shared_app/src/chats/chat_screen.rs`)

The current `test_right_click_bubble_opens_context_menu` and
`test_right_click_menu_contains_reactions_and_items` assert
"Copy/Reply/Delete" appear after right-click. These need updating because:
- The builder API changed (`MenuContent` with `reactions` + `actions`
  instead of one widget).
- "Copy"/"Reply"/"Delete" now live in the *actions* card; the assertions
  still hold but the tree walk may need to account for the split.

Update them to the new API; keep them as presence regression nets. The
animation lifecycle tests above live in `vexo_uikit` (framework-level),
not here.

### Tests deliberately NOT written

- **Hover tint** — unchanged from today; rely on manual visual verification
  (same call as the prior spec).
- **Theme reactivity** — `test_builder_reads_current_theme` still covers
  it; the builder signature change (returns `MenuContent`) doesn't affect
  the theme-read path.
- **Exact spring curve values** — assert phase transitions and monotonic
  direction, not specific values at specific times (fragile to spring
  param tweaks). The spring itself is already tested in `simulation.rs`.
- **Transform anchor correctness** — visually verified. Asserting "scale
  about center" via render-tree math is disproportionately heavy for a
  demo.

### Verification gates (per CLAUDE.md)

```bash
cargo build -p vexo          # GestureDetector global-bounds change
cargo test   -p vexo         # context_menu lifecycle tests (1-9 above)
cargo build -p vexo_uikit    # host State + controller API
cargo test   -p vexo_uikit   # same lifecycle tests if hosted there
cargo build -p shared_app    # builder split + chat_screen trigger update
cargo test   -p shared_app   # updated presence tests
cargo build -p desktop_demo  # demo compiles
# Then ask the user to run cargo run -p desktop_demo and right-click a bubble
```

### Manual visual checklist (handed to the user)

1. Right-click a bubble → screen dims, tapped bubble lifts slightly +
   brightens, reactions pill scales in above it, actions card scales in
   below it — all moving together.
2. Mid-open, click outside → menu reverses smoothly (no snap) and
   unmounts.
3. Mid-open, right-click another bubble → menu closes (reverses); need a
   second right-click to open the new one (v1 limitation, unchanged).
4. Reactions pill: 6 FA icons, centered above the bubble, pill-shaped
   (18px radius).
5. Actions card: Copy/Reply/Delete, hover tint works, Delete is red.
6. Click a reaction or action → log line + menu reverses + unmounts.
7. Right-click a bubble near the top of the screen → reactions pill flips
   below the actions card (both still below the bubble, reordered).
8. Right-click a bubble near the bottom → both cards flip above the
   bubble.
9. Right-click a bubble near the left/right edge → cards clamp on-screen,
   don't clip.
10. Toggle theme while menu open → cards + bubble copy re-render with new
    colors.

## Scope & migration

### Framework (`vexo/`)

- `GestureDetector` / `on_secondary_press`: extend the callback to receive
  global bounds (computed during event dispatch). One small, additive
  change to the gesture plumbing. Existing callers that ignore the new arg
  keep working (callback signature change is the breaking part).

### `vexo_uikit/src/context_menu.rs`

- `ContextMenuController`: breaking API change (`show` takes
  `bubble_bounds + bubble_widget + builder`; `close` starts reverse
  spring; new `animation_value`/`phase`/`set_animation_ticker`/
  `set_dirty_callback`). Open-state signal now cleared on settle, not on
  `close()`.
- `ContextMenu` host: gains a real `State` owning the phase machine +
  ticker/dirty wiring in `on_mount`. Render produces the 5-layer Stack
  (content, dim, bright bubble copy, reactions, actions).
- `MenuBuilder`: returns `MenuContent { reactions, actions, metrics }`
  instead of one widget.
- 9 new lifecycle tests + updated existing tests.

### `shared_app/src/chats/`

- `message_menu.rs`: `builder()` rewritten to return `MenuContent` (split
  reactions pill + actions card, drop divider). `MenuRow` reused as-is.
  Add `MenuMetrics` constants.
- `chat_screen.rs`: `context_menu_trigger` call site updated (now passes
  the bubble widget + receives global bounds from the gesture). Existing
  presence tests updated to the new builder API.

### `desktop_demo/`

No changes (consumes `ChatScreen` unchanged).

### Migration impact

The `ContextMenuController::show` signature change is breaking — but the
only caller is `context_menu_trigger` (in the same crate) and the chat
screen's test builders. Both are updated in this change. No external
consumers.

The `on_secondary_press` callback signature change is breaking for any
caller passing a closure — but again, the only caller is
`context_menu_trigger`. If there are other callers (grep will confirm
during implementation), they're updated in the same change. The
`Widget::on_secondary_press` trait method is updated identically.

## Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| **Dual-render of bubble widget produces different layout** (the spike-test concern) | Low — same widget + same data → deterministic layout. But avatars decode lazily; the copy might trigger a second decode. | Spike test #7 gates this. If it fails, fall back to cutout-frame (4 rects around the bubble, no copy, no lift). Avatar re-decode is cached in `ChatScreenState` already (lines 50-52), so the copy reuses the cached `ImageData`. |
| **Global-bounds computation in event dispatch is wrong** (off-by-parent, scroll offset not accounted for) | Medium — render objects store parent-relative bounds; walking to root and summing must handle `ScrollView`'s scroll offset. | Test #6 (bubble copy rendered on top) implicitly validates this: if bounds are wrong, the copy appears in the wrong place and the test's tree-walk still passes but manual visual check fails. Add an explicit bounds-equality assertion in the event-dispatch path during implementation. |
| **`on_tick` fires during `Closing` but controller has already settled** (race: settle happens between the last tick and the phase check) | Low — `animate_with` sets `Stopped` synchronously on settle; `on_tick` checks `is_animating()` and flips phase. | Test #2 covers the close→settle→unmount path. If the phase never flips to `Closed`, the menu stays mounted forever and the test hangs/fails. |
| **Spring feels wrong** (too slow/fast, wrong damping) | Low — using the exact params (`ios(340.0, 1.0)`) already validated for KeyboardAvoidance. | Tunable in one constant. Manual visual checklist item 1 confirms feel. |
| **Edge-flip logic clips cards** (positioning math off) | Medium — the flip + clamp logic has several branches. | Tests #8, #9 cover the two flip cases. Horizontal clamp covered by manual checklist item 9. |
| **Performance: re-rendering the bubble copy every frame during animation** | Low — the bubble is a small subtree (text + avatar). The dim barrier is a single full-screen rect. The cards are tiny. Per-frame cost is trivial vs. the scroll/keyboard hot paths. | No action needed for v1. If profiling shows a problem, wrap the bubble copy in `Memo` (level-2 rebuild skip). |

## Open questions for implementation (not blockers)

- **Exact `MenuMetrics` values** — the constants (~168×30 pill, ~200×108
  card) are estimates from the icon size + padding math. They'll be tuned
  during implementation by laying out the cards once and reading back the
  real sizes, then updating the constants. The host positions with these
  constants, so if they're slightly off, cards are slightly mis-centered
  but never broken.
- **Whether `GestureDetectorRenderObject` exposes enough during event
  dispatch to compute global bounds, or whether the event handler needs to
  walk the render tree.** Confirmed during implementation; if the render
  object's `computed_bounds` is parent-relative and the event handler has
  the parent chain, the walk is straightforward. This is the first thing
  to spike.

## Key risk to verify first (per CLAUDE.md test-first rule)

The bubble-copy approach assumes rendering the bubble widget twice (once
in-content under the dim, once as the bright copy) produces identical
layout. Since it's the same widget + same data, layout is deterministic —
but spike a test confirming the copy's laid-out size matches the original
before building the rest. If the dual-render proves problematic, the
fallback is a 4-rect cutout frame (original bubble shows through a hole;
no copy, no lift) — faithful cutout minus the lift.
