# Flat Ordered Draw List (Z-Order Fix) Design

**Date:** 2026-07-15
**Status:** Approved (pending user spec review)
**Scope:** `vexo/` framework rendering layer

## Motivation

`Stack` (and any overlapping subtree) renders with the wrong z-order: a
`Stack.push(Image).push(Decoration)` paints the decoration *under* the image,
even though the painter emits commands in the correct order (image first,
decoration last → decoration should be on top).

### Root cause

The bug is **not** a missing depth buffer. It is a draw-order layering defect
in the backend:

1. `Painter::paint_recursive` (`vexo/src/painter.rs:71`) traverses the render
   object tree depth-first and emits `RenderCommand`s in correct paint order.
   For the example above it emits `[Image, Rect]` — decoration last, on top.
   Correct.
2. `FrameBuilder` (`vexo/src/frame_builder.rs:31`) buckets commands by primitive
   type into separate per-`ClipGroup` vectors: `quads`, `image_requests`,
   `text_requests`. Relative order across types is **discarded** here.
3. `WgpuBackend::execute_render_pass` (`vexo/src/render/wgpu_backend.rs:755`)
   draws in fixed pipeline order: ALL quads first → ALL images → ALL text.

Result: the decoration `Rect` (a quad) draws in pass 1, the `Image` draws in
pass 2 *over* it → decoration under image. Exactly the reported symptom.

### How Flutter handles z-order

Flutter does **not** use a depth buffer for 2D. It uses painter's algorithm:
a single ordered display list where every `drawRect` / `drawImage` /
`drawText` is recorded and replayed in emit order. `RenderStack.paint()`
paints children in list order (last on top). Opacity/transform/clip use a
`saveLayer`/`save`/`restore` stack — directly analogous to Vexo's
`Push*`/`Pop*` commands. Vexo's `Painter` already follows this model; only
the backend bucketing breaks the contract.

A depth buffer would be *wrong* here: Vexo uses
`BlendState::ALPHA_BLENDING` on both pipelines, and depth-write +
alpha-blending causes translucent objects to wrongly occlude things behind
them. The paint-order approach is correct.

## Scope

### In scope

- Replace `ClipGroup` with a single flat ordered list of geometry ops, each
  carrying its own clip bounds.
- Interleave quads and images in draw order (fixes the reported bug and all
  quad/image z-order cases, including across clip boundaries).
- Preserve text clip correctness by moving clip bounds onto each
  `TextRequest`.
- Text remains a final full-viewport pass (see Out of Scope).

### Out of scope

- **Interleaving text into the ordered draw list.** glyphon's `prepare()`
  replaces its whole vertex buffer each call, so per-clip-group (or per-op)
  prepare+render is not possible within a single render pass. Interleaving
  text correctly is a much larger rework (multiple prepare/render cycles, or
  replacing glyphon with per-glyph quad rendering). Deferred to a later
  phase. Text therefore remains "always on top of geometry" — a documented
  limitation.
- Performance benchmarking. No regression is expected (state changes are
  change-triggered; draw-call count increases modestly but remains well
  within budget for typical UIs).
- Draw-call batching of consecutive same-type+same-clip runs. The new
  iteration loop draws one instance per op. A future local optimization can
  coalesce runs with no API impact.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Approach | B — fully flat ordered list, per-op clip | User-selected. Fixes z-order both within and across clip regions. Closest to Flutter's flat display list. |
| Text placement | Final full-viewport pass (unchanged) | glyphon's single-prepare-per-pass API blocks text interleaving without a much larger rework. User-selected scope. |
| Clip storage | On each op / text request | `ClipGroup` is removed, so clip context must travel with each primitive. |
| Pipeline/clip state | Change-triggered only | Scissor and pipeline bindings set only when the value differs from the previous op. Minimizes GPU state changes. |
| Draw-call granularity | One draw per op | Simplest correct iteration. Run-coalescing is a backward-compatible local optimization left for the future. |
| Migration | Dual-write then cut-over, stepwise green | Each step compiles and passes `cargo test -p vexo` before the next. De-risks the refactor. |

## Architecture

### Data structures (`vexo/src/frame_builder.rs`)

New `DrawOp` enum — one variant per interleavable geometry primitive:

```rust
/// A single drawable primitive in paint order.
enum DrawOp {
    Quad(QuadInstance),
    Image(ImageRequest),
}
```

New `FrameBuilder` shape — flat ordered list replaces `ClipGroup`:

```rust
pub struct FrameBuilder {
    ops: Vec<(DrawOp, Option<Bounds>)>,   // (primitive, clip_bounds) in paint order
    text_requests: Vec<TextRequest>,       // each TextRequest carries its own clip_bounds
    // State stacks — unchanged
    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
    transform_stack: Vec<AffineTransform>,
    current_transform: AffineTransform,
}
```

### Removed

- `ClipGroup`, `clip_groups`, `current_group_index`, `current_group()`
- `flatten_quads()`, `flatten_image_requests()`, `clip_groups()`
- `DrawRange`
- `FlattenedQuads`

### Added / changed

- `add_rect()` pushes `(DrawOp::Quad(instance), self.current_clip())`.
- `add_image()` pushes `(DrawOp::Image(request), self.current_clip())`.
- `add_text()` pushes a `TextRequest` (which now carries `clip_bounds` set
  from `self.current_clip()`).
- New accessors:
  - `ops() -> &[(DrawOp, Option<Bounds>)]`
  - `quad_instances() -> Vec<QuadInstance>` — filtered from `ops`, order preserved
  - `image_requests() -> Vec<ImageRequest>` — filtered from `ops`, order preserved
  - `text_requests() -> &[TextRequest]`

### Text clip propagation (`vexo/src/text_processor.rs`)

`TextRequest` gains a `clip_bounds` field:

```rust
#[derive(Clone)]
pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    pub font_family: Option<String>,
    pub max_width: Option<f32>,
    pub clip_bounds: Option<Bounds>,   // NEW
}
```

`text_processor.rs` reads `req.clip_bounds` instead of `group.clip_bounds`.
The existing fallback-to-viewport logic (`text_processor.rs:113-122`) moves
verbatim — same behavior, just sourced from the request.

`process_text_requests` signature changes from iterating `clip_groups` to
accepting `&[TextRequest]` directly.

## Data flow

```
RenderObject tree
       │  Painter::paint_recursive (depth-first, paint order)
       ▼
Vec<RenderCommand>                          [Image, Rect, PushClip, Text, PopClip, ...]
       │  command_processor::process_commands (unchanged)
       ▼
FrameBuilder                                ops: [(Quad, clip), (Image, clip), ...]
                                            text_requests: [TextReq{clip_bounds}, ...]
       │  WgpuBackend::upload_geometry
       ▼
typed instance buffers (quads, images)     op_locations: [Quad(0), Image(0), Quad(1), ...]
       │  WgpuBackend::execute_render_pass
       ▼
GPU: linear draw in paint order            [draw quad 0, draw image 0, draw quad 1, ...]
       │  then text pass (full-viewport scissor)
       ▼
frame
```

## Geometry upload (`vexo/src/render/wgpu_backend.rs`)

Two typed instance buffers remain (quads and images have different vertex
layouts). Upload in paint order within each type; record each op's typed-buffer
index so draw iteration can reference it.

```rust
pub fn upload_geometry(&mut self, frame_builder: &FrameBuilder) {
    let mut quad_instances: Vec<QuadInstance> = Vec::new();
    let mut image_instances: Vec<ImageInstance> = Vec::new();
    let mut op_locations: Vec<OpLocation> = Vec::with_capacity(frame_builder.ops().len());

    for (op, _clip) in frame_builder.ops() {
        match op {
            DrawOp::Quad(q) => {
                op_locations.push(OpLocation::Quad { index: quad_instances.len() as u32 });
                quad_instances.push(*q);
            }
            DrawOp::Image(req) => {
                let region = self.image_allocator.get_region(req.image_key)
                    .expect("Image key not in atlas");
                let instance = ImageInstance::from_logical(/* ...as today... */);
                op_locations.push(OpLocation::Image { index: image_instances.len() as u32 });
                image_instances.push(instance);
            }
        }
    }

    self.ensure_instance_capacity(quad_instances.len());
    self.queue.write_buffer(&self.instance_buffer, 0,
        bytemuck::cast_slice(&quad_instances));
    self.ensure_image_instance_capacity(image_instances.len());
    self.queue.write_buffer(&self.image_instance_buffer, 0,
        bytemuck::cast_slice(&image_instances));

    self.current_op_locations = op_locations;
    self.current_op_clips = frame_builder.ops().iter()
        .map(|(_, c)| *c).collect();
}

enum OpLocation { Quad { index: u32 }, Image { index: u32 } }
```

`OpLocation` index construction is extracted to a free function
`compute_op_locations(ops) -> Vec<OpLocation>` so it can be unit-tested
without a GPU surface.

## Draw iteration (`vexo/src/render/wgpu_backend.rs`)

Rewrites `execute_render_pass`. Single linear pass over `op_locations`.
State changes are **change-triggered only** — scissor and pipeline are set
solely when the value differs from the previous op.

```text
prev_pipeline = None
prev_clip = sentinel

for (loc, clip) in op_locations.iter().zip(op_clips) {
    // 1. Scissor: only set when clip changes
    if clip != prev_clip {
        set_scissor_from(clip);      // None => full viewport
        prev_clip = clip;
    }

    // 2. Pipeline: only switch when op type changes
    let kind = loc.kind();            // Quad or Image
    if Some(kind) != prev_pipeline {
        match kind {
            Quad => { set_pipeline(quad_pipeline); bind vertex buffers (Vertex, QuadInstance) }
            Image => { set_pipeline(image_pipeline); bind vertex buffers (ImageVertex, ImageInstance) + image atlas bind group }
        }
        prev_pipeline = Some(kind);
    }

    // 3. Draw one instance
    draw_indexed(0..6, 0, loc.index..loc.index+1);
}

// Text pass — unchanged
set_scissor_rect(0, 0, w, h);
text_renderer.render(...);
```

### Edge cases

- **Empty frame:** `op_locations` empty → geometry pass is a no-op; only
  clear + text pass run.
- **All one type:** zero pipeline switches; behaves like the old
  single-pass path.
- **`clip == None`:** maps to full-viewport scissor
  (`0, 0, width, height`), same as today's "no clip" branch.

### Performance characteristics

- **Pipeline switches:** O(type transitions) — typically 1–3 per frame, same
  as before. No per-op cost.
- **Scissor changes:** O(clip-boundary crossings) — fewer than the old
  per-clip-group code when multiple ops share a clip (old code set scissor
  once per group; new code sets it once per *change*, which is ≤).
- **Draw calls:** O(ops) — up from O(clip groups). The one real cost of
  Approach B. For a typical UI (~50–300 quads/images) modern GPUs handle
  hundreds of draw calls per frame comfortably; Vexo is not draw-call-bound
  today. A future local optimization can coalesce consecutive
  same-type+same-clip runs into batched draws with no API impact.

## Files touched

| File | Change |
|---|---|
| `vexo/src/frame_builder.rs` | Replace `ClipGroup` with flat `ops` list; add `DrawOp`, `OpLocation`; update `add_*` methods; new accessors; remove `flatten_*`, `clip_groups`, `DrawRange`, `FlattenedQuads`. |
| `vexo/src/render/wgpu_backend.rs` | Rewrite `upload_geometry` to build `op_locations`; rewrite `execute_render_pass` for linear paint-order draw; add `current_op_locations`, `current_op_clips` fields; extract `compute_op_locations`. |
| `vexo/src/text_processor.rs` | Read `clip_bounds` from each `TextRequest` instead of `ClipGroup`; `process_text_requests` takes `&[TextRequest]`. |
| `vexo/src/text_pipeline.rs` | Pass `frame_builder.text_requests()` to `text_processor`; remove `clip_groups` reference. |
| `vexo/src/render/mock_backend.rs` | Update assertions to new accessors; replace `flatten_image_requests()` with `image_requests()`. |

### Untouched

- `vexo/src/painter.rs` — already emits in correct paint order.
- `vexo/src/render/command_processor.rs` — already calls `add_*` in paint
  order; signatures unchanged. The fix is structurally transparent to it.
- All widget / element / render-object code — none touch `FrameBuilder`
  internals directly.

## Testing

### Unit tests (`frame_builder.rs`)

1. `test_ops_preserve_paint_order` — push `[rect, image, rect]`, assert
   `ops()` returns `[Quad, Image, Quad]` in that exact order. Core regression
   test for the reported bug.
2. `test_op_carries_clip_bounds` — `push_clip(bounds)` → `add_rect` →
   `add_image` → `pop_clip` → `add_rect`; assert op 0 and op 1 carry the
   clip, op 2 carries `None`.
3. `test_text_request_carries_clip_bounds` — `push_clip` → `add_text` →
   `pop_clip` → `add_text`; assert `clip_bounds` set/unset correctly.
4. `test_quad_instances_flatten_preserves_order` — mixed ops, assert
   `quad_instances()` returns only quads in insertion order.
5. `test_image_requests_preserve_order` — mixed ops, assert
   `image_requests()` returns only images in insertion order.

### Unit tests (`wgpu_backend.rs`)

6. `test_op_location_indices` — construct `FrameBuilder` with
   `[quad, image, quad, quad, image]`, assert `compute_op_locations` returns
   `[Quad(0), Image(0), Quad(1), Quad(2), Image(1)]`. Pure logic test, no
   GPU surface needed.

### Unit tests (`command_processor.rs`)

7. `test_process_rect_then_image_preserves_order` — feed `[Rect, Image]`
   commands, assert `frame_builder.ops()` is `[Quad, Image]` (not
   `[Quad]` + separate image bucket). Proves the fix end-to-end at the
   command-processing layer without a GPU.
8. `test_process_image_then_rect_preserves_order` — reverse order variant.
9. All 16 existing `command_processor` tests pass unchanged (they use `add_*`
   APIs whose signatures are preserved; assertions on
   `quad_instances()`/`text_requests()` still work via the new accessors).

### Integration verification

10. **Manual GUI check** — user runs `cargo run -p desktop_demo` with a test
    case: `Stack::new().push(Image).push(Decoration)`. Expected: decoration
    renders *on top of* image. Final acceptance test (per CLAUDE.md's
    "never run the demo yourself" rule).

## Migration plan

Stepwise, each step compiles + `cargo test -p vexo` green before the next:

1. Add `DrawOp`, `OpLocation`, `TextRequest.clip_bounds` — keep `ClipGroup`
   temporarily (dual-write). `FrameBuilder::add_*` methods populate both old
   and new storage.
2. Switch `text_processor` to read clip bounds from each `TextRequest`
   instead of `ClipGroup` — old accessors still used by tests.
3. Rewrite `execute_render_pass` to iterate `ops` — remove `ClipGroup`.
4. Remove `ClipGroup`, `flatten_quads`, `flatten_image_requests`, old
   accessors.
5. Update `mock_backend` assertions to new accessors.

## Not tested / deferred

- Cross-clip text z-order (text still always on top — documented
  limitation).
- Performance benchmarks — no regression expected.
- Draw-call run coalescing — backward-compatible future optimization.
