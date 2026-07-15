# Flat Ordered Draw List (Z-Order Fix) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `ClipGroup`-based primitive bucketing in `FrameBuilder`/`WgpuBackend` with a single flat ordered draw list, so quads and images interleave correctly in paint order and `Stack`-style z-order bugs are eliminated.

**Architecture:** `FrameBuilder` stores `ops: Vec<(DrawOp, Option<Bounds>)>` in paint order plus `text_requests: Vec<TextRequest>` (each carrying its own `clip_bounds`). `WgpuBackend::upload_geometry` walks `ops` once to fill two typed instance buffers (quads, images) and records `op_locations: Vec<OpLocation>` mapping each op to its typed-buffer index. `execute_render_pass` iterates `op_locations` linearly, setting scissor and pipeline only on change, drawing one instance per op. Text remains a final full-viewport pass.

**Tech Stack:** Rust, wgpu 27.0.1, glyphon (text), existing `QuadInstance`/`ImageInstance` GPU buffers.

## Global Constraints

- No depth buffer. Z-order is purely paint-order (Flutter model).
- No behavior change to `Painter` (`vexo/src/painter.rs`) or `command_processor` (`vexo/src/render/command_processor.rs`) — they already emit in correct paint order.
- `add_rect` / `add_image` / `add_text` public signatures on `FrameBuilder` are preserved (callers, including `command_processor` and `mock_backend`, must not need edits for signature reasons).
- Each task ends with `cargo test -p vexo` green before the next begins.
- Per `CLAUDE.md`: never run `cargo run -p desktop_demo` yourself; the user does the final GUI acceptance check.
- Text interleaving is out of scope — text stays a final full-viewport pass. Each `TextRequest` carries its own `clip_bounds` so glyphon's `TextArea.bounds` keeps working without `ClipGroup`.

**Reference spec:** `docs/superpowers/specs/2026-07-15-flat-ordered-draw-list-design.md`

---

### Task 1: Add `DrawOp`, `OpLocation`, `TextRequest.clip_bounds` (dual-write)

This task adds the new types and the new field alongside the existing `ClipGroup` storage. `FrameBuilder::add_*` methods populate BOTH old and new storage so everything compiles and all existing tests stay green. No production draw path reads the new storage yet.

**Files:**
- Modify: `vexo/src/frame_builder.rs` (entire file)
- Modify: `vexo/src/text_processor.rs:85` (signature of `process_text_requests` will accept `&[TextRequest]` in Task 3; for now leave `ClipGroup` path intact and add the field)

**Interfaces:**
- Produces: `DrawOp` enum, `OpLocation` enum, `FrameBuilder::ops()`, `FrameBuilder::op_locations()` (computed, no GPU), `TextRequest.clip_bounds` field, `FrameBuilder::text_requests()` returns `&[TextRequest]` (already does; just adds field).

- [ ] **Step 1: Write failing test for `DrawOp` ordering + clip propagation**

Append to `vexo/src/frame_builder.rs` tests module:

```rust
#[test]
fn test_ops_preserve_paint_order() {
    let mut fb = FrameBuilder::new();
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 10.0, 10.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0],
        size: [10.0, 10.0],
        image_key: 1,
        corner_radius: 0.0,
        transform: AffineTransform::identity().to_array(),
        opacity: 1.0,
    });
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 10.0, 10.0), Color::BLUE, None, 0.0);

    let ops = fb.ops();
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0].0, DrawOp::Quad(_)));
    assert!(matches!(ops[1].0, DrawOp::Image(_)));
    assert!(matches!(ops[2].0, DrawOp::Quad(_)));
}

#[test]
fn test_op_carries_clip_bounds() {
    let mut fb = FrameBuilder::new();
    let clip = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
    fb.push_clip(clip);
    fb.add_rect(Bounds::from_xywh(10.0, 10.0, 10.0, 10.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [10.0, 10.0],
        size: [10.0, 10.0],
        image_key: 1,
        corner_radius: 0.0,
        transform: AffineTransform::identity().to_array(),
        opacity: 1.0,
    });
    fb.pop_clip();
    fb.add_rect(Bounds::from_xywh(20.0, 20.0, 10.0, 10.0), Color::BLUE, None, 0.0);

    let ops = fb.ops();
    assert_eq!(ops[0].1, Some(clip));
    assert_eq!(ops[1].1, Some(clip));
    assert_eq!(ops[2].1, None);
}

#[test]
fn test_text_request_carries_clip_bounds() {
    let mut fb = FrameBuilder::new();
    let clip = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
    fb.push_clip(clip);
    fb.add_text("inside".to_string(), Point::new(0.0, 0.0), 16.0, Color::BLACK, None, None);
    fb.pop_clip();
    fb.add_text("outside".to_string(), Point::new(0.0, 0.0), 16.0, Color::BLACK, None, None);

    let reqs = fb.text_requests();
    assert_eq!(reqs[0].clip_bounds, Some(clip));
    assert_eq!(reqs[1].clip_bounds, None);
}

#[test]
fn test_quad_instances_flatten_preserves_order() {
    let mut fb = FrameBuilder::new();
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0], size: [1.0, 1.0], image_key: 1,
        corner_radius: 0.0, transform: AffineTransform::identity().to_array(), opacity: 1.0,
    });
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 2.0, 2.0), Color::BLUE, None, 0.0);

    let quads = fb.quad_instances();
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[0].size, [1.0, 1.0]);
    assert_eq!(quads[1].size, [2.0, 2.0]);
}

#[test]
fn test_image_requests_preserve_order() {
    let mut fb = FrameBuilder::new();
    fb.add_image(ImageRequest {
        position: [0.0, 0.0], size: [1.0, 1.0], image_key: 10,
        corner_radius: 0.0, transform: AffineTransform::identity().to_array(), opacity: 1.0,
    });
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0], size: [2.0, 2.0], image_key: 20,
        corner_radius: 0.0, transform: AffineTransform::identity().to_array(), opacity: 1.0,
    });

    let imgs = fb.image_requests();
    assert_eq!(imgs.len(), 2);
    assert_eq!(imgs[0].image_key, 10);
    assert_eq!(imgs[1].image_key, 20);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib frame_builder::tests`
Expected: compile errors — `DrawOp` undefined, `ops()` / `image_requests()` missing, `TextRequest.clip_bounds` missing.

- [ ] **Step 3: Add `DrawOp` enum, `OpLocation` enum, and `TextRequest.clip_bounds` field**

At the top of `vexo/src/frame_builder.rs` (after existing imports), add:

```rust
use crate::quad_instance::QuadInstance;

/// A single drawable geometry primitive in paint order.
#[derive(Debug, Clone)]
pub enum DrawOp {
    Quad(QuadInstance),
    Image(ImageRequest),
}

/// Where an op landed in the typed instance buffer, for draw iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpLocation {
    Quad { index: u32 },
    Image { index: u32 },
}

impl OpLocation {
    pub fn kind(&self) -> OpKind {
        match self {
            OpLocation::Quad { .. } => OpKind::Quad,
            OpLocation::Image { .. } => OpKind::Image,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Quad,
    Image,
}
```

Add `clip_bounds` to `TextRequest`:

```rust
#[derive(Clone)]
pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    pub font_family: Option<String>,
    pub max_width: Option<f32>,
    pub clip_bounds: Option<Bounds>,
}
```

- [ ] **Step 4: Add new `ops` storage to `FrameBuilder` and dual-write in `add_*`**

Add fields to `FrameBuilder` struct (keep existing `clip_groups` etc.):

```rust
pub struct FrameBuilder {
    clip_groups: Vec<ClipGroup>,           // kept temporarily (dual-write)
    current_group_index: Option<usize>,
    ops: Vec<(DrawOp, Option<Bounds>)>,    // NEW
    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
    transform_stack: Vec<AffineTransform>,
    current_transform: AffineTransform,
}
```

In `FrameBuilder::new()` and `clear()`, add `ops: Vec::new()` initialization / `self.ops.clear()`.

In `add_rect`, after the existing `self.current_group().quads.push(instance);`, add:

```rust
self.ops.push((DrawOp::Quad(instance), self.current_clip()));
```

In `add_image`, after the existing `self.current_group().image_requests.push(request);`, add:

```rust
self.ops.push((DrawOp::Image(request.clone()), self.current_clip()));
```

In `add_text`, the existing `TextRequest { ... }` construction gains a `clip_bounds` field. Set it to `self.current_clip()`:

```rust
self.current_group().text_requests.push(TextRequest {
    content: content.into(),
    position,
    size,
    color,
    font_family,
    max_width,
    clip_bounds: self.current_clip(),
});
```

- [ ] **Step 5: Add new accessors**

Add to `impl FrameBuilder`:

```rust
/// All geometry ops in paint order, each with its clip bounds.
pub fn ops(&self) -> &[(DrawOp, Option<Bounds>)] {
    &self.ops
}

/// Quad instances filtered from `ops`, in paint order.
pub fn image_requests(&self) -> Vec<ImageRequest> {
    self.ops.iter().filter_map(|(op, _)| match op {
        DrawOp::Image(r) => Some(r.clone()),
        _ => None,
    }).collect()
}

/// Compute typed-buffer locations for each op in paint order.
/// Pure function — no GPU access. Used by upload and unit-tested directly.
pub fn compute_op_locations(&self) -> Vec<OpLocation> {
    let mut quad_idx = 0u32;
    let mut image_idx = 0u32;
    self.ops.iter().map(|(op, _)| match op {
        DrawOp::Quad(_) => {
            let i = quad_idx; quad_idx += 1; OpLocation::Quad { index: i }
        }
        DrawOp::Image(_) => {
            let i = image_idx; image_idx += 1; OpLocation::Image { index: i }
        }
    }).collect()
}
```

The existing `quad_instances()`, `quad_count()`, `text_count()`, `image_count()`, `text_requests()` accessors stay as-is (they read the dual-written `clip_groups`/`text_requests`). `text_requests()` returns `&[TextRequest]` — the new field rides along transparently.

- [ ] **Step 6: Run new tests to verify they pass**

Run: `cargo test -p vexo --lib frame_builder::tests`
Expected: all new tests pass; existing tests still pass.

- [ ] **Step 7: Run full crate test suite**

Run: `cargo test -p vexo`
Expected: PASS (no behavior change — new storage is dual-written, old paths still drive rendering).

- [ ] **Step 8: Commit**

```bash
git add vexo/src/frame_builder.rs
git commit -m "refactor(frame_builder): add DrawOp/OpLocation + clip_bounds dual-write

Adds flat ordered ops list alongside existing ClipGroup storage.
add_rect/add_image/add_text now populate both; no render-path change yet.
Foundation for paint-order interleaving in later tasks."
```

---

### Task 2: Switch `text_processor` to read clip from `TextRequest`

Now that every `TextRequest` carries `clip_bounds`, `text_processor` can read from the request directly instead of from `ClipGroup`. This decouples text from `ClipGroup` so `ClipGroup` can be removed later.

**Files:**
- Modify: `vexo/src/text_processor.rs:85-166` (rewrite `process_text_requests` signature + `collect_text`)
- Modify: `vexo/src/text_pipeline.rs:28-37` (pass `frame_builder.text_requests()` instead of `clip_groups()`)

**Interfaces:**
- Consumes: `TextRequest.clip_bounds` (Task 1)
- Produces: `TextProcessor::process_text_requests` now takes `&[TextRequest]` instead of `&[ClipGroup]`. `TextProcessor::collect_text` reads `frame_builder.text_requests()`.

- [ ] **Step 1: Write failing test — text uses request clip, not group clip**

This is a behavioral-equivalence assertion. Add to `vexo/src/text_processor.rs` tests module (create one if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, Color, Logical, Point, Physical, ScaleSource, Size};
    use crate::frame_builder::{FrameBuilder, ImageRequest};
    use crate::core::AffineTransform;

    fn font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        glyphon::FontSystem::new_with_fonts([glyphon::fontdb::Source::Binary(
            std::sync::Arc::new(font_data),
        )])
    }

    #[test]
    fn test_collect_text_reads_clip_from_request() {
        let mut fb = FrameBuilder::new();
        let clip = Bounds::<Logical>::from_xywh(5.0, 5.0, 50.0, 50.0);
        fb.push_clip(clip);
        fb.add_text(
            "hi".to_string(),
            Point::new(10.0, 10.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );
        fb.pop_clip();
        // Outside clip — clip_bounds should be None
        fb.add_text(
            "lo".to_string(),
            Point::new(10.0, 10.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );

        let mut fs = font_system();
        let scale = ScaleSource::new(1.0);
        let mut proc = TextProcessor::new();
        let prepared = proc.collect_text(&mut fb, &mut fs, &scale, Size::<Physical>::new(800.0, 600.0));

        // No panic, and both text areas present
        let mut areas = prepared.as_text_areas();
        assert_eq!(areas.len(), 2);
        // First area's bounds should reflect the clip; second should be full viewport.
        // glyphon's TextArea.bounds is a glyphon::Bounds (i32). We don't assert exact
        // values (resolution-dependent); just that the call succeeds.
        let _ = areas.drain(..);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib text_processor::tests`
Expected: compile error — `collect_text` signature mismatch or `clip_groups()` reference fails to compile after step 3.

- [ ] **Step 3: Rewrite `process_text_requests` to take `&[TextRequest]`**

Replace the body of `process_text_requests` in `vexo/src/text_processor.rs` (currently at line 85) with:

```rust
fn process_text_requests(
    &mut self,
    font_system: &mut FontSystem,
    requests: &[TextRequest],
    scale_source: &ScaleSource,
    viewport_physical: Size<Physical>,
) -> PreparedText {
    let scale = scale_source.get();
    let mut buffers: Vec<Buffer> = Vec::new();
    let mut text_area_data: Vec<TextAreaData> = Vec::new();

    for req in requests {
        let buffer = self.cache.get_or_create(font_system, req);
        let physical_pos = req.position.to_physical(scale);

        let bounds = if let Some(clip) = req.clip_bounds {
            clip.to_physical(scale)
        } else {
            Bounds::<Physical>::from_xywh(
                0.0,
                0.0,
                viewport_physical.width,
                viewport_physical.height,
            )
        };

        let (buf, data) = Self::create_text_area(
            buffer,
            physical_pos,
            scale_source,
            bounds,
            req.color,
        );
        buffers.push(buf);
        text_area_data.push(data);
    }

    self.cache.evict_stale();

    PreparedText {
        buffers,
        text_area_data,
    }
}
```

Update the `use` import at the top of `text_processor.rs`:

```rust
use crate::core::{Bounds, Color, Physical, ScaleSource, Size};
use crate::frame_builder::TextRequest;
use crate::text_cache::TextCache;
```

Remove the `use crate::frame_builder::ClipGroup;` import.

- [ ] **Step 4: Update `collect_text` to pass `frame_builder.text_requests()`**

Replace the body of `collect_text` (currently at line 150) with:

```rust
pub fn collect_text(
    &mut self,
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    font_system: &mut FontSystem,
    scale_source: &ScaleSource,
    viewport_physical: Size<Physical>,
) -> CombinedPreparedText {
    let requests = frame_builder.text_requests();
    let regular = self.process_text_requests(
        font_system,
        requests,
        scale_source,
        viewport_physical,
    );
    CombinedPreparedText { regular }
}
```

- [ ] **Step 5: Run text_processor test to verify it passes**

Run: `cargo test -p vexo --lib text_processor::tests`
Expected: PASS.

- [ ] **Step 6: Run full crate test suite**

Run: `cargo test -p vexo`
Expected: PASS. (text_requests() still reads the dual-written list — same requests, just sourced differently for clip bounds.)

- [ ] **Step 7: Commit**

```bash
git add vexo/src/text_processor.rs vexo/src/text_pipeline.rs
git commit -m "refactor(text_processor): read clip_bounds from TextRequest

text_processor no longer depends on ClipGroup; each TextRequest carries
its own clip_bounds. Prepares for ClipGroup removal."
```

---

### Task 3: Rewrite `execute_render_pass` to iterate `ops`; remove `ClipGroup`

This task flips the production draw path from `clip_groups`-bucketed to flat-ordered-`ops`, then removes `ClipGroup` entirely.

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs:685-828` (rewrite `upload_image_geometry`→ merge into `upload_geometry`; rewrite `execute_render_pass`)
- Modify: `vexo/src/text_pipeline.rs:39-78` (drop `flatten_quads`/`flatten_image_requests`/`clip_groups` usage)
- Modify: `vexo/src/frame_builder.rs` (remove `ClipGroup`, `clip_groups`, `current_group`, `current_group_index`, `flatten_quads`, `flatten_image_requests`, `DrawRange`, `FlattenedQuads`; update `quad_instances()`/`quad_count()`/`image_count()`/`text_count()` to read from `ops`/`text_requests`)

**Interfaces:**
- Consumes: `FrameBuilder::ops()`, `FrameBuilder::compute_op_locations()`, `FrameBuilder::text_requests()` (Tasks 1-2)
- Produces: `WgpuBackend::upload_geometry` now does both quad+image upload and stores `current_op_locations` + `current_op_clips`. `WgpuBackend::execute_render_pass` signature simplifies — takes only `viewport_width`, `viewport_height`.

- [ ] **Step 1: Write failing test for `compute_op_locations`**

Append to `vexo/src/frame_builder.rs` tests module:

```rust
#[test]
fn test_compute_op_locations_indices() {
    let mut fb = FrameBuilder::new();
    // Sequence: quad, image, quad, quad, image
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0], size: [1.0, 1.0], image_key: 1,
        corner_radius: 0.0, transform: AffineTransform::identity().to_array(), opacity: 1.0,
    });
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
    fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0], size: [1.0, 1.0], image_key: 2,
        corner_radius: 0.0, transform: AffineTransform::identity().to_array(), opacity: 1.0,
    });

    let locs = fb.compute_op_locations();
    assert_eq!(locs.len(), 5);
    assert_eq!(locs[0], OpLocation::Quad { index: 0 });
    assert_eq!(locs[1], OpLocation::Image { index: 0 });
    assert_eq!(locs[2], OpLocation::Quad { index: 1 });
    assert_eq!(locs[3], OpLocation::Quad { index: 2 });
    assert_eq!(locs[4], OpLocation::Image { index: 1 });
}
```

- [ ] **Step 2: Run test to verify it passes** (it already should — Task 1 implemented `compute_op_locations`)

Run: `cargo test -p vexo --lib frame_builder::tests::test_compute_op_locations_indices`
Expected: PASS.

- [ ] **Step 3: Write failing test for `command_processor` end-to-end ordering**

Append to `vexo/src/render/command_processor.rs` tests module:

```rust
#[test]
fn test_process_rect_then_image_preserves_order() {
    use crate::image_atlas::ImageKey;
    use crate::render::RenderCommand;
    let mut frame_builder = crate::frame_builder::FrameBuilder::new();
    let commands = vec![
        RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 10.0, 10.0), Color::RED),
        RenderCommand::Image {
            bounds: Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            image_key: ImageKey::default(),
            corner_radius: 0.0,
        },
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    let ops = frame_builder.ops();
    assert_eq!(ops.len(), 2, "rect+image must produce 2 ops, not bucketed");
    assert!(matches!(ops[0].0, crate::frame_builder::DrawOp::Quad(_)));
    assert!(matches!(ops[1].0, crate::frame_builder::DrawOp::Image(_)));
}

#[test]
fn test_process_image_then_rect_preserves_order() {
    use crate::image_atlas::ImageKey;
    use crate::render::RenderCommand;
    let mut frame_builder = crate::frame_builder::FrameBuilder::new();
    let commands = vec![
        RenderCommand::Image {
            bounds: Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            image_key: ImageKey::default(),
            corner_radius: 0.0,
        },
        RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 10.0, 10.0), Color::RED),
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    let ops = frame_builder.ops();
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0].0, crate::frame_builder::DrawOp::Image(_)));
    assert!(matches!(ops[1].0, crate::frame_builder::DrawOp::Quad(_)));
}
```

If `ImageKey` is not `Default`, replace `ImageKey::default()` with a constructor that compiles — check `vexo/src/image_atlas.rs` for the actual type. Run `cargo build -p vexo` first to find out, then adjust the test to use whatever constructor is available.

- [ ] **Step 4: Run new command_processor tests to verify they pass**

Run: `cargo test -p vexo --lib command_processor::tests::test_process_rect_then_image_preserves_order command_processor::tests::test_process_image_then_rect_preserves_order`
Expected: PASS. (`command_processor` already calls `add_*` in paint order; `ops()` already preserves order from Task 1. This test locks in the property so the rewrite below can't regress it.)

- [ ] **Step 5: Add `current_op_locations` + `current_op_clips` fields to `WgpuBackend`**

In `vexo/src/render/wgpu_backend.rs`, add fields to `WgpuBackend` struct (near `current_config`):

```rust
// Current frame's op locations + clips, populated by upload_geometry.
current_op_locations: Vec<crate::frame_builder::OpLocation>,
current_op_clips: Vec<Option<crate::core::Bounds<crate::core::Logical>>>,
```

Initialize both to `Vec::new()` in the `Ok(Self { ... })` block of `init()`.

- [ ] **Step 6: Rewrite `upload_geometry` to build op_locations; merge in image upload**

Replace the existing `upload_geometry` method (currently at line 699) with:

```rust
/// Upload geometry (quads + images) from frame builder to GPU buffers.
/// Also records per-op typed-buffer locations for draw iteration.
pub fn upload_geometry(&mut self, frame_builder: &FrameBuilder) {
    let op_locations = frame_builder.compute_op_locations();
    let op_clips: Vec<Option<crate::core::Bounds<crate::core::Logical>>> = frame_builder
        .ops()
        .iter()
        .map(|(_, clip)| *clip)
        .collect();

    let mut quad_instances: Vec<QuadInstance> = Vec::new();
    let mut image_instances: Vec<ImageInstance> = Vec::new();
    let atlas_size = [
        self.image_allocator.atlas_width() as f32,
        self.image_allocator.atlas_height() as f32,
    ];

    for (op, _) in frame_builder.ops() {
        match op {
            crate::frame_builder::DrawOp::Quad(q) => {
                quad_instances.push(*q);
            }
            crate::frame_builder::DrawOp::Image(req) => {
                let region = self
                    .image_allocator
                    .get_region(req.image_key)
                    .expect("Image key not found in atlas");
                let instance = ImageInstance::from_logical(
                    req.position,
                    req.size,
                    region,
                    atlas_size,
                    req.corner_radius,
                    AffineTransform::from_array(req.transform),
                    req.opacity,
                );
                image_instances.push(instance);
            }
        }
    }

    if !quad_instances.is_empty() {
        self.ensure_instance_capacity(quad_instances.len());
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&quad_instances),
        );
    }
    if !image_instances.is_empty() {
        self.ensure_image_instance_capacity(image_instances.len());
        self.queue.write_buffer(
            &self.image_instance_buffer,
            0,
            bytemuck::cast_slice(&image_instances),
        );
    }

    self.current_op_locations = op_locations;
    self.current_op_clips = op_clips;
}
```

Delete the old `upload_image_geometry` method (line 686) — it's now merged into `upload_geometry`.

Add the necessary imports at the top of the file:

```rust
use crate::frame_builder::{FrameBuilder, OpKind};
use crate::quad_instance::QuadInstance;
```

(`FrameBuilder` import likely already exists; verify. `OpKind` is new.)

- [ ] **Step 7: Rewrite `execute_render_pass` to iterate ops linearly**

Replace the body of `execute_render_pass` (currently at line 712) with:

```rust
pub fn execute_render_pass(
    &mut self,
    viewport_width: u32,
    viewport_height: u32,
) -> Result<(), RenderError> {
    if !self.is_configured {
        return Err(RenderError::SurfaceNotConfigured);
    }

    let scale_factor = self.scale_source.get().factor();

    let output = match self.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => frame,
        other => return Err(RenderError::AcquireFailed(format!("{:?}", other))),
    };

    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Iterate ops in paint order. Set scissor + pipeline only on change.
        // prev_clip uses Option<Option<Bounds>> so the initial state (no
        // scissor ever set yet) is distinguishable from "scissor set to None
        // (full viewport)". This matters when the first op has clip == None:
        // we still need to set the scissor once.
        let mut prev_kind: Option<OpKind> = None;
        let mut prev_clip: Option<Option<crate::core::Bounds<crate::core::Logical>>> = None;

        for (loc, clip) in self.current_op_locations.iter().zip(self.current_op_clips.iter()) {
            // 1. Scissor: only set when clip changes.
            //    Compare Option<Bounds> by value via the Option<Option> sentinel.
            let clip_value = *clip;
            if prev_clip != Some(clip_value) {
                match clip {
                    Some(c) => {
                        let x = (c.left * scale_factor).max(0.0) as u32;
                        let y = (c.top * scale_factor).max(0.0) as u32;
                        let right = (c.right * scale_factor).min(viewport_width as f32) as u32;
                        let bottom = (c.bottom * scale_factor).min(viewport_height as f32) as u32;
                        let w = right.saturating_sub(x);
                        let h = bottom.saturating_sub(y);
                        if w == 0 || h == 0 {
                            // Fully clipped — skip this op. Still advance prev_clip
                            // so we don't repeatedly re-set scissor for adjacent
                            // ops with the same degenerate clip.
                            prev_clip = Some(clip_value);
                            continue;
                        }
                        render_pass.set_scissor_rect(x, y, w, h);
                    }
                    None => {
                        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                    }
                }
                prev_clip = Some(clip_value);
            }

            // 2. Pipeline: only switch when op kind changes.
            let kind = loc.kind();
            if Some(kind) != prev_kind {
                match kind {
                    OpKind::Quad => {
                        render_pass.set_pipeline(&self.render_pipeline);
                        render_pass.set_bind_group(0, &self.global_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    }
                    OpKind::Image => {
                        render_pass.set_pipeline(&self.image_pipeline);
                        render_pass.set_bind_group(0, &self.global_bind_group, &[]);
                        render_pass.set_bind_group(1, &self.image_atlas_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(..));
                    }
                }
                prev_kind = Some(kind);
            }

            // 3. Draw one instance. Index buffer is per-pipeline (same indices 0..6).
            match kind {
                OpKind::Quad => {
                    render_pass.set_index_buffer(
                        self.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                }
                OpKind::Image => {
                    render_pass.set_index_buffer(
                        self.image_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                }
            }
            let idx = match loc {
                crate::frame_builder::OpLocation::Quad { index } => *index,
                crate::frame_builder::OpLocation::Image { index } => *index,
            };
            render_pass.draw_indexed(0..6, 0, idx..idx + 1);
        }

        // Text pass — full-viewport scissor, unchanged.
        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
        self.text_renderer
            .render(&self.atlas, &self.viewport, &mut render_pass)
            .map_err(|e| RenderError::TextPrepareFailed(format!("{:?}", e)))?;
    }

    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    self.atlas.trim();

    Ok(())
}
```

- [ ] **Step 8: Update `TextPipeline::execute_render` to use the new signature**

Replace the body of `execute_render` in `vexo/src/text_pipeline.rs` (currently at line 40) with:

```rust
pub fn execute_render(
    &mut self,
    backend: &mut WgpuBackend,
    frame_builder: &FrameBuilder,
    mut prepared_text: CombinedPreparedText,
    font_system: &mut glyphon::FontSystem,
) -> Result<(), RenderError> {
    backend.upload_geometry(frame_builder);

    backend.prepare_text(font_system, prepared_text.as_text_areas());

    let viewport_width = backend.width();
    let viewport_height = backend.height();

    backend.execute_render_pass(viewport_width, viewport_height)?;

    Ok(())
}
```

- [ ] **Step 9: Remove `ClipGroup` and dead code from `frame_builder.rs`**

Delete from `vexo/src/frame_builder.rs`:
- `ClipGroup` struct
- `DrawRange` struct
- `FlattenedQuads` struct
- `clip_groups: Vec<ClipGroup>` and `current_group_index: Option<usize>` fields from `FrameBuilder`
- `current_group()` method
- `flatten_quads()` method
- `flatten_image_requests()` method
- `clip_groups()` method
- The dual-write lines in `add_rect` / `add_image` / `add_text` that referenced `self.current_group().*` (keep only the `self.ops.push(...)` line)

Update remaining accessors to read from `ops`:

```rust
pub fn quad_count(&self) -> usize {
    self.ops.iter().filter(|(op, _)| matches!(op, DrawOp::Quad(_))).count()
}

pub fn has_quads(&self) -> bool {
    self.ops.iter().any(|(op, _)| matches!(op, DrawOp::Quad(_)))
}

pub fn quad_instances(&self) -> Vec<QuadInstance> {
    self.ops.iter().filter_map(|(op, _)| match op {
        DrawOp::Quad(q) => Some(*q),
        _ => None,
    }).collect()
}

pub fn text_count(&self) -> usize {
    self.text_requests.len()
}

pub fn text_requests(&self) -> &[TextRequest] {
    &self.text_requests
}

pub fn image_count(&self) -> usize {
    self.ops.iter().filter(|(op, _)| matches!(op, DrawOp::Image(_))).count()
}
```

Remove the `text_requests: Vec<TextRequest>` field from `ClipGroup` (the field is gone with the struct). The `FrameBuilder` now stores `text_requests: Vec<TextRequest>` directly — add this field to `FrameBuilder` if not already present from Task 1, and ensure `add_text` pushes to `self.text_requests` directly (not via `current_group`).

Update `add_text` to push directly:

```rust
pub fn add_text(
    &mut self,
    content: impl Into<String>,
    position: Point<Logical>,
    size: f32,
    color: impl Into<Color>,
    font_family: Option<String>,
    max_width: Option<f32>,
) {
    let color: Color = color.into();
    self.text_requests.push(TextRequest {
        content: content.into(),
        position,
        size,
        color,
        font_family,
        max_width,
        clip_bounds: self.current_clip(),
    });
}
```

- [ ] **Step 10: Run full crate test suite**

Run: `cargo test -p vexo`
Expected: PASS. (Some `mock_backend` tests may break if they referenced `flatten_image_requests` — Task 4 fixes those. If they break here, that's expected; proceed to Task 4.)

- [ ] **Step 11: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs vexo/src/text_pipeline.rs vexo/src/frame_builder.rs
git commit -m "refactor(render): flat ordered draw list for z-order

Replaces ClipGroup bucketing with a single Vec<(DrawOp, Option<Bounds>)>
in paint order. execute_render_pass iterates ops linearly, setting scissor
+ pipeline only on change. Fixes Stack z-order bugs (image-over-decoration
etc.) by drawing quads and images in the order the Painter emitted them.

Text remains a final full-viewport pass (documented limitation).
ClipGroup/DrawRange/FlattenedQuads/flatten_* removed."
```

---

### Task 4: Update `mock_backend` to new accessors

**Files:**
- Modify: `vexo/src/render/mock_backend.rs` (only if any test referenced removed methods — `flatten_image_requests` etc.)

**Interfaces:**
- Consumes: `FrameBuilder::image_requests()`, `FrameBuilder::quad_instances()`, `FrameBuilder::text_requests()` (all from Task 1/3)

- [ ] **Step 1: Build to find any remaining breakage in mock_backend**

Run: `cargo build -p vexo --tests`
Expected: either PASS, or compile errors pointing at `flatten_image_requests` / `clip_groups` references.

- [ ] **Step 2: Fix any references to removed methods**

If `mock_backend.rs` references `flatten_image_requests()`, replace with `image_requests()`. The current `mock_backend.rs` (read in planning) only uses `quad_count`, `quad_instances`, `text_count`, `text_requests` — all preserved. So this step is likely a no-op; verify and move on.

- [ ] **Step 3: Run full crate test suite**

Run: `cargo test -p vexo`
Expected: PASS.

- [ ] **Step 4: Commit** (only if changes were made)

```bash
git add vexo/src/render/mock_backend.rs
git commit -m "test(mock_backend): use new flat accessors"
```

If no changes: skip commit, note in the task report that mock_backend was already compatible.

---

### Task 5: End-to-end build + manual GUI acceptance

This task is the final gate. The user runs the demo; the implementer does not (per CLAUDE.md).

**Files:**
- None modified.

- [ ] **Step 1: Clean build of the whole workspace**

Run: `cargo build`
Expected: PASS, no warnings related to the refactor.

- [ ] **Step 2: Full workspace test**

Run: `cargo test`
Expected: PASS across `vexo`, `shared_app`, `vexo_uikit`, `desktop_demo`.

- [ ] **Step 3: Final commit if anything was missed**

If steps 1-2 surfaced any issue, fix and commit with a descriptive message. Otherwise skip.

- [ ] **Step 4: Hand off to user for manual GUI acceptance**

Report to the user:

> Implementation complete. All workspace tests pass. To verify the z-order
> fix visually, please run:
>
> ```bash
> cargo run -p desktop_demo
> ```
>
> and exercise a `Stack` with overlapping image + decoration (e.g. the
> existing demo's image-with-overlay case). Expected: decoration renders
> on top of image. The previous bug had decoration rendering *under* the
> image.

- [ ] **Step 5: Mark plan complete once user confirms visual fix**

The plan is done when the user reports the decoration renders on top of the image in the running demo.
