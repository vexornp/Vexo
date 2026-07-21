# ClipRRect Widget — Design

**Date:** 2026-07-21
**Status:** Approved (section-by-section)
**Scope:** `vexo` crate, `shared_app` callers

## Motivation

The `Image` widget today carries a `corner_radius: f32` field that is passed
all the way to the image fragment shader, where it is applied as a per-image
SDF mask (`vexo/src/render_objects/image.rs:22-57`,
`vexo/src/render/wgpu_backend.rs:723`). This works for images, but it leaves
Vexo without any general mechanism for clipping an arbitrary subtree to a
rounded rectangle — the operation Flutter calls `ClipRRect`.

The existing clip mechanism is rectangular-only:

- `RenderObject::clip_bounds() -> Option<Bounds<Logical>>` is consumed by the
  painter (`vexo/src/painter.rs:217-229`), which emits `RenderCommand::PushClip
  { bounds }`, processed by `CommandProcessor` into a `FrameBuilder` clip
  stack, and ultimately enforced by the wgpu backend as a GPU scissor rect
  (`vexo/src/render/wgpu_backend.rs:830-856`). Scissor rects cannot round.
- `RenderCommand::PushCornerRadius` exists but only affects `paint_style()`'s
  fill/border quads (`vexo/src/painter.rs:76-98`); it does not propagate to
  children or to `RenderCommand::Image`. Image clipping is therefore a
  separate, image-only path.

Consequence: producing a rounded avatar today requires
`Image::from_bytes(..).with_corner_radius(diameter / 2.0)` inside
`DecoratedBox::with_style(.., Style::default().clip())`
(`shared_app/src/widgets/avatar.rs:6-13`). The `clip()` is a no-op rectangle
on a square image; the circle comes entirely from the Image shader. There is
no way to round-clip a column, a stack of overlapping children, or any future
widget that lacks its own `corner_radius` field.

## Goals

- Add a `ClipRRect` widget that clips its single child subtree to a rounded
  rectangle, matching Flutter's `ClipRRect` mental model.
- Mechanism is general — works for any child, not just `Image`.
- Existing rectangular clip path (scissor) is untouched; common case stays
  branchless in the shader.
- Migrate `Image::with_corner_radius` callers to `ClipRRect`, then remove
  `Image.corner_radius`, `ImageRenderObject.corner_radius`,
  `RenderCommand::Image.corner_radius`, and the image shader's SDF branch.

## Non-Goals

- **Hit-test clipping.** Vexo's hit-test path does not consult
  `clip_bounds()` today — `DecoratedBox + Style::clip()` does not clip
  gestures to its rectangle. `ClipRRect` v1 matches this existing behavior:
  hit tests use the unrounded `computed_bounds`. Rounded hit-test clipping
  is a separate spec.
- **Approach C: shader consolidation.** During brainstorming, Approach C
  proposed lifting Image's existing SDF routine into a shared clip-mask
  pipeline applied to every draw op (quad, text, caret, image), and
  using that shared mask for Image instead of its own
  `corner_radius` field. That consolidation is **subsumed by this
  spec's migration plan**: after migration, `Image.corner_radius` is
  removed and the image draw op picks up the shared rclip mask like
  every other draw op (the rclip uniform is applied in the fragment
  shader regardless of draw-op kind). No separate Approach C work
  remains.
- **Other clip shapes.** `ClipOval`, `ClipPath` are out of scope. Only
  rounded rectangles ship in v1.
- **No layout changes.** `ClipRRect` is a true pass-through proxy — no
  Taffy node, like `Transform` and `DecoratedBox`.

## Architecture

Four new types, one new `RenderObject` hook, two new `RenderCommand`
variants. Each new type mirrors an existing one in the codebase.

| Layer | New type | Mirrors |
|---|---|---|
| Widget | `ClipRRect` | `Transform` |
| Element | `ClipRRectElement` | `DecoratedBoxElement` |
| Render object | `ClipRRectRenderObject` | `TransformRenderObject` |
| Render command | `PushClipRRect` / `PopClipRRect` | `PushClip` / `PopClip` |

### New `RenderObject` hook

```rust
// vexo/src/render_object.rs — added to the RenderObject trait
fn clip_corner_radius(&self) -> Option<f32> {
    None
}
```

Default `None` so existing render objects are unaffected. A separate hook
(rather than bundling radius into `clip_bounds()`) keeps the rectangular
clip path zero-cost: every RO implements `clip_bounds()`, and bundling
radius would force every implementation to surface it.

### Painter decision matrix

The painter reads both hooks once per RO per paint pass and chooses the
command kind. The radius decision is made at push time; the matching Pop
kind is recorded, so a mid-frame `set_radius(0.0)` cannot cause a
push/pop mismatch.

| `clip_bounds()` | `clip_corner_radius()` | Emitted |
|---|---|---|
| `None` | _ | nothing |
| `Some(b)` | `None` or `0.0` | `PushClip{b}` / `PopClip` (unchanged) |
| `Some(b)` | `Some(r)` (r > 0) | `PushClipRRect{b, r}` / `PopClipRRect` (new) |

`ClipRRectRenderObject` returns `Some(bounds)` from `clip_bounds()` and
`Some(radius)` (when radius > 0) from `clip_corner_radius()`. No
special-case "is this a ClipRRect?" check anywhere — the painter does
the rest generically. `DecoratedBox` continues to return `Some(bounds)` +
`None` radius → plain rectangular clip, zero behavior change.

### Layout: pass-through

`ClipRRectRenderObject` is `is_pass_through() == true`. Its `layout()`
borrows the child's Taffy node, identical to
`TransformRenderObject::layout` (`vexo/src/widgets/transform.rs:73-83`).
No Taffy node of its own, no layout effect on the child. After Taffy
computes, `apply_layout` reads the child node's computed bounds and
stores them in `self.computed_bounds` — this is what `clip_bounds()`
returns, so `ClipRRect` clips to the child's own painted rectangle.

### Backend: parallel rclip stack

`FrameBuilder` gains a parallel stack alongside the existing
`clip_stack`:

```rust
rclip_stack: Vec<(Bounds, f32)>,   // parallel to clip_stack
```

Each `DrawOp` already pairs with `Option<Bounds>` (the rectangular
clip). Add a parallel rclip snapshot recorded at op-add time. When
`rclip_stack` is empty, the snapshot is empty and the shader
fast-paths.

The wgpu backend uploads the per-op rclip snapshot as a small uniform
array (capped at depth 8). Fragment shader multiplies the final alpha
by the product of SDF masks. When `rclip_count == 0`, the loop is
skipped — the existing path runs unchanged.

## Component Contracts

### `ClipRRect` widget

New file: `vexo/src/widgets/clip_rrect.rs`

```rust
pub struct ClipRRect {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    radius: f32,
}

impl ClipRRect {
    pub fn new(radius: f32, child: impl Widget + 'static) -> Self;
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self;
    pub fn child(&self) -> &dyn Widget;
    pub fn radius(&self) -> f32;
}
```

**Invariants:**

- `radius >= 0.0`. Negative radius is a programmer error —
  `debug_assert!(radius >= 0.0)` in `new()`, plus clamp to 0.0 at the RO
  boundary. No panic in release.
- `radius == 0.0` is valid and means "rectangular clip". The RO returns
  `None` from `clip_corner_radius()` in this case, so the painter takes
  the existing `PushClip` path. Avoids creating a no-op rounded-clip
  stack entry.
- Pass-through proxy: `child()` returns `Some`, no `Layout` field, no
  Taffy node. Same structural shape as `DecoratedBox`.

### `ClipRRectElement`

Co-located in `vexo/src/widgets/clip_rrect.rs`. Structurally identical
to `DecoratedBoxElement` (`vexo/src/widgets/decorated_box.rs:44-242`).
Manages: focus attachment, single child via `child_ops`, render object
lifecycle. `rebuild()` calls `update_render_object` with `PAINT`-only
result (radius change never requires relayout — pass-through RO has no
layout).

### `ClipRRectRenderObject`

New file: `vexo/src/render_objects/clip_rrect.rs`

```rust
pub struct ClipRRectRenderObject {
    radius: f32,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl RenderObject for ClipRRectRenderObject {
    fn is_pass_through(&self) -> bool { true }
    fn clip_bounds(&self) -> Option<Bounds<Logical>> { self.computed_bounds }
    fn clip_corner_radius(&self) -> Option<f32> {
        if self.radius > 0.0 { Some(self.radius) } else { None }
    }
    // layout/apply_layout: borrow child's Taffy node, store
    //                       computed_bounds — identical to
    //                       TransformRenderObject
    // paint: returns vec![] (clip is applied by painter around children)
    // hit_test: returns computed_bounds.contains(position) — unrounded,
    //           matches existing DecoratedBox+clip() behavior
}

impl ClipRRectRenderObject {
    pub fn set_radius(&mut self, radius: f32) -> bool;
}
```

`set_radius` returns `true` on change (drives `UpdateResult::PAINT`).

### New render commands

Added to `vexo/src/render/command.rs`:

```rust
RenderCommand::PushClipRRect {
    bounds: Bounds<Logical>,
    radius: f32,
},
RenderCommand::PopClipRRect,
```

No change to `PushClip` / `PopClip`. The two paths coexist; the painter
picks one based on the RO's hooks.

### Painter change

`vexo/src/painter.rs:217-229` (the existing PushClip block) and
`vexo/src/painter.rs:280-282` (the existing PopClip block) become:

```rust
let clip = obj.clip_bounds();
let clip_radius = obj.clip_corner_radius();
let use_rclip = clip_radius.map(|r| r > 0.0).unwrap_or(false);
if let Some(local_clip) = &clip {
    let absolute_clip = /* existing offset logic */;
    if use_rclip {
        ctx.push_command(RenderCommand::PushClipRRect {
            bounds: absolute_clip,
            radius: clip_radius.unwrap(),
        });
    } else {
        ctx.push_command(RenderCommand::PushClip { bounds: absolute_clip });
    }
}
// ... paint children ...
if clip.is_some() {
    ctx.push_command(if use_rclip {
        RenderCommand::PopClipRRect
    } else {
        RenderCommand::PopClip
    });
}
```

The `use_rclip` boolean is computed once before the push; the matching
Pop kind uses the same boolean. No risk of push/pop mismatch.

### `FrameBuilder` change

`vexo/src/frame_builder.rs` gains a parallel stack and APIs mirroring
the existing rectangular ones:

```rust
rclip_stack: Vec<(Bounds, f32)>,
MAX_RCLIP_DEPTH: usize = 8,

pub fn push_rclip(&mut self, bounds: Bounds, radius: f32);
pub fn pop_rclip(&mut self);
pub fn current_rclip(&self) -> &[(Bounds, f32)];
```

Each `DrawOp` tuple extends from `(DrawOp, Option<Bounds>)` to
`(DrawOp, Option<Bounds>, SmallVec<[RClipEntry; 4]>)` (or equivalent
packed representation). When `rclip_stack` is empty, the snapshot is
empty and the shader fast-paths.

**Nesting cap:** `push_rclip` enforces `MAX_RCLIP_DEPTH = 8`. Push
beyond that logs `log::warn!("[ClipRRect] max depth 8 exceeded, dropping")`
and silently drops the entry. The matching `pop_rclip` will pop the
previous entry — slight stack imbalance is fine because the dropped
push never recorded anything. Documented behavior, not a panic.

### `CommandProcessor` change

`vexo/src/render/command_processor.rs` handles the two new variants,
identical in structure to the existing `PushClip` handling
(`vexo/src/render/command_processor.rs:166-185`), including the
transform-aware AABB expansion:

```rust
RenderCommand::PushClipRRect { bounds, radius } => {
    let adjusted = bounds.offset_by(current_offset);
    let effective = if current_transform.is_identity() {
        adjusted
    } else {
        current_transform.transform_bounds(&adjusted)
    };
    frame_builder.push_rclip(effective, *radius);
}
RenderCommand::PopClipRRect => {
    frame_builder.pop_rclip();
}
```

When `DrawOp`s are added (`add_rect`, `add_image`, `add_text`,
`add_shadow_rect`), each snapshots `current_rclip()` alongside the
existing `current_clip()`.

### Wgpu backend

`vexo/src/render/wgpu_backend.rs`:

- `current_op_rclips: Vec<SmallVec<[RClipEntry; 4]>>` populated in
  `upload_geometry`, parallel to `current_op_clips`.
- Per-op uniform upload: the rclip snapshot as a uniform array, e.g.
  `uniform_array<vec4<f32>, 8>` for bounds + `uniform_array<f32, 8>`
  for radii, plus a count. (Packing tighter — `vec4<f32>` =
  `(left, top, right, bottom)` and a separate `vec4<f32>` =
  `(r0, r1, r2, r3)` to fit 4 entries per vec4 — is an implementation
  detail left to the implementer.)
- Fragment shader: for each active rclip, compute the SDF distance to
  the rounded-rect boundary; if outside (`distance > 0`), alpha = 0;
  if inside but within 1px of the edge (`-1 < distance <= 0`), apply
  smoothstep antialiasing. Multiply the resulting mask into the
  fragment's final alpha.
- Fast path: if `rclip_count == 0`, the loop is skipped (early-out or
  compile-time specialization).

**SDF formula** (standard, same one Image uses today):

```glsl
// Given fragment position p and rounded rect (b, r):
vec2 q = abs(p - center(b)) - half_size(b) + r;
float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
// dist <= 0 inside, dist > 0 outside, |dist| < 1 = AA band
```

### Public API surface

`vexo/src/widgets/mod.rs` exports `ClipRRect`.
`vexo/src/lib.rs` re-exports it. No other public API changes; the
`RenderObject::clip_corner_radius()` hook is added with a default
`None` impl so existing ROs are unaffected.

## Data Flow & Lifecycle

### Mount

1. App's `view()` returns `ClipRRect::new(8.0, Image::new(...))`.
2. Element inflation: `ClipRRect::create_element()` → `ClipRRectElement`.
3. `ClipRRectElement::mount()` (mirrors
   `DecoratedBoxElement::mount` at `decorated_box.rs:117-139`):
   - Create focus attachment for this element (child needs a focus
     parent).
   - `mount_render_object(context)` → registers
     `ClipRRectRenderObject` with the registry, gets back
     `RenderObjectKey`.
   - Inflate the single child via
     `context.inflate_child(None, child_widget.clone_boxed())`.
4. Pipeline mounts the child element, which mounts the child's RO.
5. Pipeline links the child RO as the parent's child:
   `parent.set_child_id(child_ro_key)` + `parent.add_child(child_ro_key)`
   via `RenderObjectRegistry::set_child`.

### Layout (pass-through)

Bottom-up, same as `TransformRenderObject::layout`
(`vexo/src/widgets/transform.rs:73-83`):

1. Child RO's `layout()` runs first, creating the child's Taffy node,
   returns `LayoutResult { node, size }`.
2. `ClipRRectRenderObject::layout()` receives `child_nodes =
   [child_node]`, stores `child_layout_node = Some(child_node)`,
   returns `LayoutResult { node: child_node, size: zero }`.
3. Layouter sees pass-through: grandparent links directly to the
   child's Taffy node (no extra node for ClipRRect). ClipRRect has
   zero layout footprint.
4. After Taffy computes, `apply_layout` runs:
   `ClipRRectRenderObject::apply_layout` reads the child node's
   computed bounds from the engine and stores them in
   `self.computed_bounds`. This is what `clip_bounds()` returns —
   ClipRRect clips to the child's own painted rectangle.

### Paint

`Painter::paint_recursive` walks the RO tree
(`vexo/src/painter.rs:161-288`). When it reaches
`ClipRRectRenderObject`:

1. `obj.paint(ctx)` returns `vec![]` — ClipRRect itself paints
   nothing. (The clip is enforced around the children, not as a
   self-painted decoration.)
2. Read `clip = obj.clip_bounds()` → `Some(child_bounds)`. Read
   `radius = obj.clip_corner_radius()` → `Some(r)` (assuming r > 0).
3. Compute `absolute_clip` from `local_clip + absolute_position`
   (existing logic at `painter.rs:220-225`).
4. Emit `RenderCommand::PushClipRRect { bounds: absolute_clip, radius:
   r }`.
5. (Optional paint-transform / PushOffset / PushOpacity blocks are
   skipped — ClipRRect has none.)
6. Recurse into the single child. Child's commands (e.g.
   `RenderCommand::Image`) get recorded with the current rclip
   snapshot attached at command-processor time.
7. Emit `RenderCommand::PopClipRRect`.

### Command processing → FrameBuilder

`process_commands` (`command_processor.rs`) walks the flat command
list, maintaining `current_offset` and `current_transform` (existing).
The new branch is described in **CommandProcessor change** above.
When `DrawOp`s are added, each snapshots `current_rclip()` alongside
the existing `current_clip()`.

### GPU upload & render

`upload_geometry` (`wgpu_backend.rs`) populates `current_op_locations`
and `current_op_clips` (existing). Add a parallel
`current_op_rclips: Vec<SmallVec<[RClipEntry; 4]>>`.

Render loop (`wgpu_backend.rs:822-870`) — for each op:

1. Existing: set scissor from `current_op_clips[i]` (rectangular fast
   cull).
2. New: upload the op's rclip snapshot to a uniform buffer (or update
   a dynamic offset into a uniform array).
3. Issue draw call.

The fragment shader reads `rclip_count` + the arrays. If
`rclip_count == 0`, the existing path runs unchanged (no SDF math).
If `rclip_count > 0`, multiply the final fragment alpha by the
product of SDF masks:

```glsl
float mask = 1.0;
for (int i = 0; i < rclip_count; i++) {
    mask *= sdf_rounded_rect_alpha(p, rclip_bounds[i], rclip_radius[i]);
}
out_color.a *= mask;
```

### Rebuild

When the widget rebuilds with a new `radius` but same child type:

1. `ClipRRectElement::rebuild(new_widget)` downcasts to
   `Box<dyn Widget>`.
2. `update_render_object(ro)` calls
   `ClipRRectRenderObject::set_radius(self.radius)` — returns `true`
   on change → `UpdateResult::PAINT`.
3. No `mark_needs_layout` (pass-through RO has no layout).
4. `mark_needs_paint(ro_id)`.
5. Reconcile child via `update_child` — if child widget `can_update`,
   update in place; otherwise replace.

### Unmount

1. `ClipRRectElement::unmount`:
   - `unmount_render_object(context)` — removes the RO from the
     registry. Pass-through flag means the child's Taffy node is NOT
     orphaned (child owns it).
   - Detach focus attachment.
   - Recursively unmount child element.

## Edge Cases & Failure Modes

- **Nested ClipRRect** (`ClipRRect(20, ClipRRect(10, child))`): both
  masks multiply in the shader. Visually equivalent to intersection.
  Naturally correct.
- **ClipRRect wrapping a larger child**: child's natural size
  determines `computed_bounds`, so the clip rectangle equals the
  child's painted rect. If the child overflows its parent (Taffy
  overflow), the clip still applies to the child's own bounds — same
  behavior as `DecoratedBox + clip()`.
- **ClipRRect inside a Transform**: command processor expands the
  clip's bounds to the transformed AABB (existing logic at
  `command_processor.rs:176-180`). This means a rotated ClipRRect
  clips to its rotated bounding box, not the rotated rounded rect
  itself. This matches the existing `PushClip` behavior for
  `DecoratedBox+clip` under transforms — we're not making it worse,
  and fixing it is a separate spec.
- **`radius == 0.0` rebuild**: RO returns `None` from
  `clip_corner_radius()`, painter emits `PushClip` instead. The
  `set_radius(0.0)` path triggers PAINT (style changed), no relayout.
- **Depth > 8**: `push_rclip` logs `log::warn!` and ignores the push
  (the stack stays at 8). The matching `pop_rclip` then pops the
  previous (8th) entry — the dropped 9th push simply never took
  effect, so the stack correctly returns to 7. Documented behavior,
  not a panic.
- **Pre-layout state**: `clip_bounds()` returns `None` until
  `apply_layout` runs (e.g. on the very first frame before Taffy has
  computed). Painter skips the clip block entirely. No crash, no
  spurious clip. `ClipRRect` always has a child by construction
  (`child: Box<dyn Widget>`), so "empty child" is not a reachable
  state.

## Migration: `Image.corner_radius` removal

After `ClipRRect` lands and is validated:

1. Update `shared_app/src/widgets/avatar.rs` (the sole current caller
   of `Image::with_corner_radius`) to:
   ```rust
   ClipRRect::new(
       diameter / 2.0,
       WithLayout::new(
           Image::from_bytes(bytes).expect("avatar bytes are valid PNG"),
           Layout::default().width(diameter).height(diameter),
       ),
   ).boxed()
   ```
   The `DecoratedBox + Style::clip()` wrapper can be removed — it was
   a no-op rectangle on a square image.
2. Audit `shared_app` and `desktop_demo` for any other
   `with_corner_radius` call sites; migrate each.
3. Remove, in a single follow-up commit:
   - `Image::with_corner_radius`, `Image::corner_radius`, and the
     `corner_radius` field from `Image`
     (`vexo/src/widgets/image.rs:13, 35-46`).
   - `ImageRenderObject::corner_radius`, `set_corner_radius`, and
     constructor parameter (`vexo/src/render_objects/image.rs:22, 28,
     55-57`).
   - `RenderCommand::Image.corner_radius` field
     (`vexo/src/render/command.rs:75`).
   - The image shader's SDF branch in
     `vexo/src/render/wgpu_backend.rs:723` and the `corner_radius`
     plumbing through `CommandProcessor` and `FrameBuilder::ImageRequest`.
4. Update any tests that assert on `RenderCommand::Image.corner_radius`
   (e.g. `vexo/src/render_objects/image.rs:273-298`).

## Testing

### Unit tests

- `ClipRRect` widget: construction, `with_key`, `child()`, `radius()`,
  clone preservation. Mirror the `Image` widget tests at
  `vexo/src/widgets/image.rs:114-167`.
- `ClipRRectRenderObject`:
  - `is_pass_through() == true`.
  - `clip_corner_radius()` returns `Some(r)` when r > 0, `None` when
    r == 0.
  - `set_radius` change detection (returns `true` on change, `false`
    on no-op).
  - `clip_bounds()` returns `None` before `apply_layout`, `Some`
    after.
- `FrameBuilder`:
  - `push_rclip` / `pop_rclip` maintain the stack correctly.
  - `current_rclip()` returns the active slice.
  - Depth cap: pushing 9 entries logs and drops the 9th; the stack
    stays at 8.
  - DrawOp snapshot: an op added after `push_rclip` carries the
    snapshot; an op added after `pop_rclip` does not.

### Integration tests

- `painter` emits `PushClipRRect` / `PopClipRRect` for a
  `ClipRRectRenderObject` with r > 0, and `PushClip` / `PopClip` for
  r == 0 or for a `DecoratedBox` with `Style::clip()`.
- `command_processor` correctly routes `PushClipRRect` to
  `frame_builder.push_rclip` with offset/transform-adjusted bounds.
- E2E (extend `vexo/src/e2e_test.rs`): build a `ClipRRect` wrapping a
  colored `DecoratedBox`, assert the render command stream contains
  `PushClipRRect { bounds, radius }` ... `PopClipRRect` in the right
  order, with child commands between them.

### Visual / manual verification

- Avatar in `shared_app` chat screens renders as a circle, identical
  to before migration.
- A `ClipRRect` wrapping a `Column` of overlapping children clips the
  children to the rounded rect (no child bleeds past the corner
  curve).
- Nested `ClipRRect`s produce the visually correct intersection.
- `ClipRRect(radius=0.0, child)` is visually identical to no clip
  (fast path).

## File-Level Summary

| File | Change |
|---|---|
| `vexo/src/render_object.rs` | Add `clip_corner_radius()` hook (default `None`). |
| `vexo/src/render/command.rs` | Add `PushClipRRect` / `PopClipRRect` variants. |
| `vexo/src/painter.rs` | Read `clip_corner_radius()`, choose command kind. |
| `vexo/src/widgets/clip_rrect.rs` | **New.** `ClipRRect` widget + `ClipRRectElement`. |
| `vexo/src/render_objects/clip_rrect.rs` | **New.** `ClipRRectRenderObject`. |
| `vexo/src/widgets/mod.rs` | Export `ClipRRect`. |
| `vexo/src/lib.rs` | Re-export `ClipRRect`. |
| `vexo/src/frame_builder.rs` | Add `rclip_stack`, `push_rclip`/`pop_rclip`/`current_rclip`, per-op rclip snapshot. |
| `vexo/src/render/command_processor.rs` | Handle `PushClipRRect` / `PopClipRRect`. |
| `vexo/src/render/wgpu_backend.rs` | Upload rclip uniform, SDF mask in fragment shader. |
| `shared_app/src/widgets/avatar.rs` | Migrate to `ClipRRect`. |

## Follow-Ups

- **Rounded hit-test clipping.** Today, `clip_bounds()` does not
  affect hit testing — `DecoratedBox + Style::clip()` does not clip
  gestures. `ClipRRect` v1 inherits this. A follow-up spec could add
  rounded hit-test clipping by having the hit-test path consult
  `clip_corner_radius()` and apply the SDF test.
- **Transform-aware rclip.** Today, `PushClipRRect` under a
  `Transform` expands to the transformed AABB (matching `PushClip`'s
  existing behavior). A follow-up could clip to the actual transformed
  rounded rect for tighter visual correctness.
