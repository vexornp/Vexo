# SaveLayer for Opacity (Offscreen Render-Target Grouping) Design

**Date:** 2026-08-12
**Status:** Approved (pending user spec review)
**Scope:** `vexo/` framework rendering layer

## Motivation

`Opacity` on a subtree containing both an opaque background quad and light
text renders the text as a white rectangle during iOS push/pop navigation
animations.

### Root cause

`Opacity` is implemented by CPU alpha-multiplication: the command processor
multiplies the opacity value into each child command's color alpha
(`vexo/src/render/command_processor.rs:54,88`). When the subtree contains an
opaque background quad (alpha 1.0), the multiplier drops its fill alpha below
1.0. `compute_op_locations` (`vexo/src/frame_builder.rs:443`) then reclassifies
that quad as a transparent quad → Phase 3 (rendered *after* text). Light text
(Phase 2) renders directly on the window's hardcoded white clear color
(`vexo/src/render/wgpu_backend.rs:698`) before the dark background is
composited — visible as a white rectangle per text line.

This is a fundamental limitation of the "flatten opacity into per-command
alphas" approach. The correct rendering model (Flutter/Skia's `SaveLayer`)
renders the opacity subtree to an offscreen buffer as a unit, then composites
the buffer at the given alpha — preserving internal layering. Vexo has no
offscreen render-target infrastructure today; the v1 Opacity design
(`docs/superpowers/specs/2026-06-25-opacity-modifier-design.md:13`) explicitly
chose CPU alpha-multiplication over GPU render-to-texture citing "massive
complexity for v1."

This design adds the offscreen grouping that was deferred.

### The glyphon constraint

Vexo uses a single `glyphon::TextRenderer` per frame. `prepare()` replaces its
entire vertex buffer each call, so text can be rendered in only one render pass
per frame under the current design. Any SaveLayer group containing text hits
this wall — and our bug is specifically about text under opacity, so we cannot
dodge it.

The prior art doc (`2026-07-15-flat-ordered-draw-list-design.md:60-67`)
explicitly defers text interleaving, calling it "a much larger rework (multiple
prepare/render cycles, or replacing glyphon with per-glyph quad rendering)."

**Resolution (decided in brainstorming):** Per-group `TextRenderer` instances.
Each SaveLayer group owns its own TextRenderer; the main pass keeps its own for
non-grouped text. Each prepares only its own text's vertices. This is the
cleanest model — groups are self-contained, and glyphon supports multiple
TextRenderers sharing one `TextAtlas` and `FontSystem`.

## Design decisions (from brainstorming)

1. **Per-group TextRenderer.** Each SaveLayer group gets its own
   `glyphon::TextRenderer` sharing the backend's existing `TextAtlas` and
   `FontSystem`. Text requests are partitioned by group.
2. **Always SaveLayer (when alpha < 1.0).** Every `Opacity` widget with
   `alpha < 1.0` allocates an offscreen target. `Opacity(1.0)` is a no-op
   skip. Ship the principled fix first; add heuristics later if profiling
   shows a need.
3. **Marker ops in flat list (Approach A).** `DrawOp::BeginSaveLayer` /
   `DrawOp::EndSaveLayer` markers live in the existing flat `ops` Vec. The
   backend scans with a stack to find matching End markers. Preserves the
   flat-list invariant from `2026-07-15-flat-ordered-draw-list-design.md`.

## Architecture

### Section 1: Command flow & data model

**New RenderCommands** (in `vexo/src/render/command.rs`):

```rust
PushSaveLayer { bounds: Bounds<Logical>, opacity: f32 }
PopSaveLayer
```

The painter (`vexo/src/painter.rs:247-251`) changes: when `obj.opacity()`
returns `Some(o)`:
- If `o >= 1.0`: emit nothing (no-op skip — small improvement over today,
  which emits a no-op Push/Pop).
- If `o < 1.0`: emit `PushSaveLayer { bounds: obj.computed_bounds(), opacity: o }`
  before children, `PopSaveLayer` after. The bounds come from the Opacity
  render object's `computed_bounds` (already read by the painter for clip
  emission).

The existing `PushOpacity`/`PopOpacity` commands remain in the enum as the
documented fallback path (see Rollback). The command processor's alpha-multiply
path for them is kept but goes unused once SaveLayer is active.

**New DrawOp markers** (in `vexo/src/frame_builder.rs`):

```rust
DrawOp::BeginSaveLayer { bounds: Bounds<Logical>, opacity: f32 }
DrawOp::EndSaveLayer
```

These are sentinel entries in the existing flat
`ops: Vec<(DrawOp, Option<Bounds>, Vec<RClipEntry>)>`. Their per-op tuple
fields are `None`/empty (the group's own bounds live inside the `BeginSaveLayer`
variant). `compute_op_locations` classifies them as a new
`OpLocation::SaveLayerMarker` kind so the backend's phase loops skip them (they
are not drawn directly).

**command_processor change** (`vexo/src/render/command_processor.rs`):

On `PushSaveLayer`: push a "save-layer-active" flag onto the opacity stack
and emit `DrawOp::BeginSaveLayer` into the FrameBuilder. Do NOT update
`current_opacity` — ops inside the group are rendered at their original alpha;
the group opacity is applied at composite time, not baked in.

On `PopSaveLayer`: emit `DrawOp::EndSaveLayer`. Pop the save-layer flag.

On leaf commands (Rect, Text, Caret, Image) while inside a SaveLayer group:
emit them with their **original** alpha (no multiplication). The
`current_opacity` stays at 1.0 for the duration of the group.

`PushOpacity`/`PopOpacity` (the old CPU-multiply path) remain handled for the
fallback case, but are not emitted by the painter once SaveLayer is active.

**Nested groups** work naturally: the flat list may look like
`[op1, BeginSaveLayer(A), op2, BeginSaveLayer(B), op3, EndSaveLayer(B), op4, EndSaveLayer(A), op5]`.
The backend handles nesting with a stack (push on Begin, pop on End).

**paint_index / z-depth**: continues monotonically across the flat list,
unchanged. A group's Begin marker, contained ops, and End marker get contiguous
z-depths. The group's composite quad (inserted by the backend at End) sits at
the group's Begin-marker paint-order position.

### Section 2: Offscreen texture infrastructure

Vexo has no offscreen render-target infrastructure today. This design adds it.

**Texture creation** — each SaveLayer group needs a color texture
(`RENDER_ATTACHMENT | TEXTURE_BINDING`) sized to the group's bounds (physical
pixels, rounded up). Format matches the surface format (`self.config.format`)
to avoid format-conversion blends at composite time.

**Allocation strategy — per-frame for v1.** Allocate a new GPU texture per
group per frame; drop at end of frame. Simple. Cost is negligible at our
current scale (1-2 groups per frame: nav dim, modal fade). Pooling with
keyed reuse is a deferred optimization (TODO) until profiling shows a need.

**Depth attachment — own per offscreen pass.** Each offscreen pass gets its
own depth texture sized to the group bounds, so the three-phase algorithm
(Phase 1 depth-write, Phase 2/3 depth-test) runs identically inside the
offscreen pass. This is the faithful SaveLayer model: the group renders as if
it were its own framebuffer.

**Clear color — transparent.** The offscreen pass clears to transparent
(`LoadOp::Clear(transparent)`), not to the theme background. Essential: the
group composites as a textured quad with alpha = group opacity, so anything
not drawn inside the group must be transparent (alpha 0), not white. The
group's own background quad fills its region opaquely; the surrounding
transparent area does not bleed white.

**Sizing** — the texture is sized to the group's `bounds` (physical pixels,
rounded up). Ops inside the group are painted in window-absolute coordinates
by the command processor today; for the offscreen pass, they are translated
into group-local coordinates (subtract `bounds.left/top`). Applies to quads,
images, text positions, scissor rects, and SDF rclip bounds.

### Section 3: Per-group TextRenderer

Each SaveLayer group owns a `glyphon::TextRenderer` to render text into its
offscreen target. The main pass keeps its own TextRenderer for non-grouped
text. All TextRenderers share the backend's existing `TextAtlas` and
`FontSystem`.

**Text request routing** — `FrameBuilder` tracks a stack of active group text
lists. When `BeginSaveLayer` is emitted, push a new `Vec<TextRequest>`; text
ops inside the group append to it. On `EndSaveLayer`, the group's text list is
handed to the backend alongside the group's bounds+opacity. The main pass's
text list is the bottom of the stack (text outside any group).

The main-pass text list is just the group-stack's bottom entry.

**TextRenderer lifecycle — pooled.** Keep a `Vec<TextRenderer>` in `WgpuBackend`
that grows to the max concurrent groups seen. Reuse across frames (prepare
overwrites the vertex buffer anyway). Construction involves GPU buffer
allocation and pipeline binding — measurably more expensive than a texture
allocation, so pooling is warranted here (unlike the texture pooling which is
deferred). Pool size is bounded by max concurrent groups (currently 1-2).

**Shared atlas & font system** — glyphon supports multiple TextRenderers
against one `TextAtlas`/`FontSystem`. No duplication of glyph atlas data. (API
verification: confirm `TextRenderer::new` accepts shared references during
implementation.)

**Prepare cycle** — `prepare` is called once per active TextRenderer (main +
each group). Each prepare call uploads only its own text's vertices. This is
the per-group-prepare cost accepted in the brainstorming decision.

**Render cycle** — inside each offscreen pass (one per group), call
`group_text_renderer.render(&atlas, &viewport, &mut offscreen_render_pass)`.
In the main pass, call `main_text_renderer.render(...)` with only the
non-grouped text. The viewport for an offscreen pass is sized to the group
bounds, not the surface.

**Coordinate space for text** — text positions in `TextRequest` are currently
in window-absolute coordinates. For a group's offscreen pass, they are
translated to group-local coordinates (subtract `bounds.left/top`), same as
quads. Handled at FrameBuilder level when the group is active: text requests
pushed while a group is active get their positions adjusted.

### Section 4: Backend render algorithm

Replaces the body of `execute_render_pass` (`vexo/src/render/wgpu_backend.rs:1114-1282`).

**Algorithm:**

```
fn execute_render_pass(&mut self, ops, text_lists):
    begin main render pass (surface target, surface-sized depth,
                            clear to theme bg)

    # Recursive helper: render a range of ops into `render_pass`,
    # using `text_renderer` for text and `target_origin` as the
    # coordinate-space origin (window-absolute for main pass,
    # group-local for offscreen passes).
    fn render_range(pass, ops, start, end, text_renderer, target_origin,
                    depth_texture):
        # Three-phase iteration within [start, end)
        # Phase 1: opaque quads/images (depth-write ON)
        # Phase 2: text — text_renderer.render(...)
        # Phase 3: transparent quads (depth-write OFF)
        # On BeginSaveLayer marker:
        #   - scan forward to find matching EndSaveLayer (stack-based)
        #   - allocate offscreen color texture + depth texture sized to
        #     group.bounds (physical pixels)
        #   - begin offscreen render pass (transparent clear)
        #   - recurse: render_range(offscreen_pass, ops, i+1, end_inner,
        #                            group_text_renderer, group.bounds.origin,
        #                            group_depth_texture)
        #   - end offscreen pass
        #   - insert composite quad into current pass's Phase 3:
        #       textured quad sampling offscreen texture,
        #       alpha = group.opacity, at group.bounds position,
        #       z-depth = group's paint_index
        #   - advance index past EndSaveLayer marker
        # On EndSaveLayer marker: unreachable (handled by the scan above)

    render_range(main_pass, ops, 0, ops.len(), main_text_renderer,
                 window_origin, surface_depth_texture)
    end main render pass
```

**Group scanning** — when the inner loop hits `BeginSaveLayer`, it scans
forward to find the matching `EndSaveLayer`, tracking nesting depth (counter
incremented on Begin, decremented on End; matching End when counter returns to
0). O(n) per group, total O(n) per frame since each op is scanned once. The
scan is the only new CPU cost.

**Composite quad insertion** — the group's offscreen result is sampled as a
textured quad in the *parent* pass's Phase 3 (transparent phase), because the
group has `opacity < 1.0` so the composite is a translucent quad — it must
render after opaque content in the parent. Its z-depth is the group's
`BeginSaveLayer` paint_index, preserving paint-order correctness relative to
siblings. The composite quad uses the existing image pipeline (samples a
texture, applies alpha) — no new shader needed.

**Nested groups** — recursion handles this naturally. An inner group renders
into its own offscreen target, composites into the outer group's offscreen
pass's Phase 3, and the outer group then composites into the main pass's
Phase 3. Two levels of offscreen rendering for nested `Opacity(Opacity(...))`.
Correct, if expensive — accepted by the "always SaveLayer" decision.

**Depth attachment per pass** — each offscreen pass gets its own depth texture
sized to the group bounds. The main pass uses the surface-sized depth texture.
Depth values are per-pass (not shared), which is correct — each pass is an
independent framebuffer.

**Coordinate translation** — ops inside a group are stored in window-absolute
coords by the command processor. When rendering into the offscreen pass, the
backend subtracts `group.bounds.origin` from each op's position to get
group-local coords. Applies to quads, images, text positions, scissor rects,
and SDF rclip bounds. The existing `command_processor` clip-transform logic
(which transforms clips through the transform stack) is the precedent — we add
a "subtract group origin" step when rendering into an offscreen target.

**Scissor/rclip inside groups** — clips active inside a group are snapshot at
op-add time in window-absolute space. For the offscreen pass, they are
translated to group-local space, same as op positions.

**The composite quad's own clip** — naturally enforced because the offscreen
texture is exactly the group bounds — sampling outside the texture's UV range
returns transparent (with `AddressMode::Clamp`). No explicit clip needed on
the composite quad.

**Paint index / z-depth** — the z-depth formula (`1.0 - paint_index / 65536.0`)
continues to produce correct ordering. The composite quad is a Phase 3
insertion at the group's Begin-marker z-depth; Phase 3 already renders after
Phase 1, and within Phase 3 the z-depth ordering holds.

## Scope

### In scope

- `PushSaveLayer`/`PopSaveLayer` RenderCommand variants.
- `DrawOp::BeginSaveLayer`/`DrawOp::EndSaveLayer` markers in FrameBuilder.
- command_processor: emit markers, do NOT alpha-multiply inside groups.
- Painter: emit `PushSaveLayer`/`PopSaveLayer` when `opacity() < 1.0`, nothing
  when `opacity() >= 1.0`.
- WgpuBackend: offscreen texture + depth allocation per group, recursive
  `render_range` with three-phase per pass, composite quad insertion.
- Per-group TextRenderer pool in WgpuBackend, sharing atlas + font system.
- Text request routing: per-group text lists on FrameBuilder.
- Coordinate translation to group-local space for offscreen passes.

### Out of scope

- **Hardcoded white clear color** (`wgpu_backend.rs:698`). SaveLayer makes it
  less dangerous (text no longer renders directly on the clear color), but it
  is still wrong in dark mode. Separate follow-up: wire `set_clear_color` to
  the theme background.
- **BackdropFilter** (ROADMAP.md). Also needs offscreen infrastructure, but a
  different feature: samples already-rendered content behind the group, not
  the group's own content. SaveLayer gives us the texture-pooling foundation;
  BackdropFilter layers on later.
- **Performance optimization** (heuristic skip for single-primitive subtrees,
  texture pooling). Deferred until profiling shows a need.
- **Revert of the navigation.rs mobile-dim workaround** (commit `80ae65b`).
  Once SaveLayer ships, the nav dim should revert to `Opacity(0.85)` like
  desktop — the principled fix handles it. This is a follow-up cleanup.

## Migration & rollback

**Migration path.** SaveLayer is a behind-the-scenes change to how `Opacity`
renders. No widget API changes — `Opacity::new(child, 0.85)` keeps the same
signature. The change is invisible to widget authors. The only observable
difference is correct rendering.

**Rollback.** If SaveLayer surfaces a critical regression, revert the painter
to emit `PushOpacity`/`PopOpacity` (CPU alpha-multiply) instead of
`PushSaveLayer`/`PopSaveLayer`. This is a one-line painter change — the
`PushOpacity`/`PopOpacity` command variants and the command_processor's
alpha-multiply path are kept in the codebase as the documented fallback. The
offscreen infrastructure simply goes unused.

Keep `PushOpacity`/`PopOpacity` in the enum for one release as the documented
fallback, removing them only after SaveLayer is proven in production.

## Testing strategy

- **Unit tests on FrameBuilder:** assert that `BeginSaveLayer`/`EndSaveLayer`
  markers appear at the right positions in the flat ops list, with correct
  bounds/opacity, for nested and non-nested cases.
- **Unit tests on command_processor:** assert that ops inside a SaveLayer group
  are NOT alpha-multiplied (their alpha stays at the original value), while ops
  outside any group are unaffected.
- **Integration test on WgpuBackend** using a new `OffscreenCaptureBackend`
  (or extending `MockBackend` to record texture contents): render an
  `Opacity(0.85, DecoratedBox(bg=black, Text("hello")))` subtree and assert
  the text renders after the background (no white-rectangle artifact). This is
  the end-to-end regression guard.
- **Manual verification on iOS:** push/pop a conversation in dark mode, confirm
  no white rectangles, confirm the nav dim looks correct.

## Open questions for implementation

These are flagged for verification during the writing-plans / implementation
phase, not blockers for the design:

1. **glyphon `TextRenderer::new` signature.** Confirm it accepts shared
   `&TextAtlas`/`&mut FontSystem` references, or whether ownership is
   required. If ownership is required, the sharing model needs rework.
2. **rclip uniform update path.** The rclip uniform currently uploads
   window-absolute bounds. The offscreen pass needs group-local bounds.
   Verify the uniform update can accept translated bounds without a shader
   change.
3. **Composite quad pipeline.** Verify the existing image pipeline can sample
   an arbitrary render-target texture (not just the image atlas). If the image
   pipeline is hardcoded to the atlas, a thin "textured quad" pipeline may be
   needed — should be a small shader reusing image-pipeline structure.
4. **Texture format for offscreen target.** Surface format is
   `self.config.format` (typically `Bgra8UnormSrgb` or similar). Confirm this
   format supports `RENDER_ATTACHMENT | TEXTURE_BINDING` usage on the target
   platforms (iOS Metal, desktop wgpu backends).
