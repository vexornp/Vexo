# ClipRRect Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `ClipRRect` widget that clips any child subtree to a rounded rectangle, then migrate `Image.corner_radius` callers to it and remove the field.

**Architecture:** Approach B from the design spec — a parallel `PushClipRRect`/`PopClipRRect` command path and a separate `rclip_stack` in `FrameBuilder`, leaving the existing rectangular scissor clip path untouched. The wgpu backend uploads per-op rclip snapshots to a uniform buffer with dynamic offsets; the fragment shader multiplies in an SDF mask per active entry. `ClipRRectRenderObject` is a pass-through proxy (no Taffy node) that returns `Some(bounds)` from `clip_bounds()` and `Some(radius)` from a new `clip_corner_radius()` hook.

**Tech Stack:** Rust, wgpu 27.0.1, WGSL, Taffy 0.9.1, existing three-tree architecture.

## Global Constraints

- Vexo workspace at `/Users/peiyan_wang/Workspace/ui_platform`, three crates: `vexo/`, `shared_app/`, `desktop_demo/`.
- Build: `cargo build` after every Rust edit. Test: `cargo test` after every feature.
- **Never run `cargo run -p desktop_demo`** — ask the user to run it for visual verification.
- Follow existing code patterns: pass-through proxy ROs mirror `TransformRenderObject`; single-child elements mirror `DecoratedBoxElement`.
- `MAX_RCLIP_DEPTH = 8` — enforced at `FrameBuilder::push_rclip`, log warn + drop on overflow.
- Negative `ClipRRect` radius: `debug_assert!` in `new()`, clamp to 0.0 at RO boundary.
- Design spec: `docs/superpowers/specs/2026-07-21-clip-rrect-widget-design.md`.

---

## Phase 1: ClipRRect Implementation

### Task 1: Add `clip_corner_radius()` hook to `RenderObject` trait

**Files:**
- Modify: `vexo/src/render_object.rs:437-444` (after `clip_bounds`)
- Test: `vexo/src/render_object.rs` (test module)

**Interfaces:**
- Produces: `RenderObject::clip_corner_radius(&self) -> Option<f32>` with default `None`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `vexo/src/render_object.rs` (after the existing `test_render_object_opacity_default` test at line ~962):

```rust
#[test]
fn test_render_object_clip_corner_radius_default_none() {
    struct TestRO;
    impl RenderObject for TestRO {
        fn layout(
            &mut self,
            _ctx: &mut LayoutContext,
            _child_nodes: &[LayoutNodeKey],
        ) -> LayoutResult {
            unimplemented!()
        }
        fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
        fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
            vec![]
        }
        fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
            true
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }
    let ro = TestRO;
    assert!(
        ro.clip_corner_radius().is_none(),
        "clip_corner_radius() must default to None"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_render_object_clip_corner_radius_default_none`
Expected: FAIL — method `clip_corner_radius` not found on `TestRO`.

- [ ] **Step 3: Add the trait method with default impl**

In `vexo/src/render_object.rs`, add after the existing `clip_bounds()` method (line ~444, after its closing brace):

```rust
    /// Get the corner radius for this render object's clip, if any.
    ///
    /// When present (and > 0.0), the painter emits `PushClipRRect`/
    /// `PopClipRRect` around this object's children instead of the
    /// plain `PushClip`/`PopClip`. The radius is applied as an SDF
    /// mask in the fragment shader on top of the rectangular scissor
    /// clip from `clip_bounds()`.
    ///
    /// Return `None` (the default) for plain rectangular clipping.
    /// Return `Some(r)` only when `r > 0.0`.
    fn clip_corner_radius(&self) -> Option<f32> {
        None
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_render_object_clip_corner_radius_default_none`
Expected: PASS.

- [ ] **Step 5: Run full test suite to verify no regressions**

Run: `cargo test -p vexo`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render_object.rs
git commit -m "feat(render): add clip_corner_radius() hook to RenderObject trait

Default returns None so existing render objects are unaffected. When
overridden to return Some(r > 0), the painter will emit PushClipRRect
instead of PushClip around the RO's children."
```

---

### Task 2: Add `PushClipRRect` / `PopClipRRect` render commands

**Files:**
- Modify: `vexo/src/render/command.rs:26-132` (enum) and test module
- Test: `vexo/src/render/command.rs`

**Interfaces:**
- Produces: `RenderCommand::PushClipRRect { bounds: Bounds<Logical>, radius: f32 }` and `RenderCommand::PopClipRRect`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `vexo/src/render/command.rs`:

```rust
#[test]
fn test_push_clip_rrect_command() {
    let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
    let cmd = RenderCommand::PushClipRRect {
        bounds,
        radius: 8.0,
    };
    match cmd {
        RenderCommand::PushClipRRect { bounds: b, radius } => {
            assert_eq!(b.left, 10.0);
            assert_eq!(b.width(), 100.0);
            assert_eq!(radius, 8.0);
        }
        _ => panic!("Expected PushClipRRect"),
    }
}

#[test]
fn test_pop_clip_rrect_command() {
    let cmd = RenderCommand::PopClipRRect;
    match cmd {
        RenderCommand::PopClipRRect => {}
        _ => panic!("Expected PopClipRRect"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_push_clip_rrect_command`
Expected: FAIL — `PushClipRRect` variant not found.

- [ ] **Step 3: Add the new variants**

In `vexo/src/render/command.rs`, add after `PopClip` (line ~86) in the `RenderCommand` enum:

```rust
    /// Push a rounded-rect clipping region onto the stack.
    /// All subsequent commands are clipped to this rounded rectangle.
    /// The radius is applied as an SDF mask in the fragment shader;
    /// the rectangular bounds are also applied as a scissor rect for
    /// fast coarse culling.
    PushClipRRect {
        /// The clipping bounds in logical coordinates.
        bounds: Bounds<Logical>,
        /// The corner radius for the rounded-rect clip.
        radius: f32,
    },

    /// Pop the most recent rounded-rect clipping region from the stack.
    PopClipRRect,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_push_clip_rrect_command test_pop_clip_rrect_command`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/command.rs
git commit -m "feat(render): add PushClipRRect/PopClipRRect render commands

Parallel to PushClip/PopClip but carries a corner radius. The painter
will emit these when a render object's clip_corner_radius() returns
Some(r > 0)."
```

---

### Task 3: Extend `FrameBuilder` with `rclip_stack`

This is the largest task. It adds the parallel rclip stack, the per-op rclip snapshot, and the depth cap.

**Files:**
- Modify: `vexo/src/frame_builder.rs`
- Test: `vexo/src/frame_builder.rs` (test module)

**Interfaces:**
- Produces: `RClipEntry` struct, `FrameBuilder::push_rclip`, `pop_rclip`, `current_rclip`, `MAX_RCLIP_DEPTH`, and per-op rclip snapshot in `ops` and `text_requests`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `vexo/src/frame_builder.rs`:

```rust
#[test]
fn test_rclip_stack_push_pop() {
    let mut fb = FrameBuilder::new();
    assert!(fb.current_rclip().is_empty());

    let b1 = Bounds::new(0.0, 0.0, 100.0, 100.0);
    fb.push_rclip(b1, 8.0);
    assert_eq!(fb.current_rclip().len(), 1);
    assert_eq!(fb.current_rclip()[0], (b1, 8.0));

    fb.pop_rclip();
    assert!(fb.current_rclip().is_empty());
}

#[test]
fn test_rclip_stack_depth_cap() {
    let mut fb = FrameBuilder::new();
    let b = Bounds::new(0.0, 0.0, 10.0, 10.0);
    for _ in 0..MAX_RCLIP_DEPTH {
        fb.push_rclip(b, 4.0);
    }
    assert_eq!(fb.current_rclip().len(), MAX_RCLIP_DEPTH);

    // Pushing beyond the cap should log and drop, not panic.
    fb.push_rclip(b, 4.0);
    assert_eq!(
        fb.current_rclip().len(),
        MAX_RCLIP_DEPTH,
        "depth cap must silently drop excess pushes"
    );

    // Pop returns to MAX-1.
    fb.pop_rclip();
    assert_eq!(fb.current_rclip().len(), MAX_RCLIP_DEPTH - 1);
}

#[test]
fn test_rclip_snapshot_on_add_rect() {
    let mut fb = FrameBuilder::new();
    let b = Bounds::new(0.0, 0.0, 50.0, 50.0);

    // Op before rclip: empty snapshot.
    fb.add_rect(b, Color::RED, None, 0.0);
    assert!(fb.ops()[0].2.is_empty());

    // Op inside rclip: snapshot has one entry.
    fb.push_rclip(b, 8.0);
    fb.add_rect(b, Color::RED, None, 0.0);
    assert_eq!(fb.ops()[1].2.len(), 1);
    fb.pop_rclip();

    // Op after rclip: empty snapshot again.
    fb.add_rect(b, Color::RED, None, 0.0);
    assert!(fb.ops()[2].2.is_empty());
}

#[test]
fn test_rclip_snapshot_on_add_image() {
    let mut fb = FrameBuilder::new();
    let b = Bounds::new(0.0, 0.0, 50.0, 50.0);

    fb.push_rclip(b, 12.0);
    fb.add_image(ImageRequest {
        position: [0.0, 0.0],
        size: [50.0, 50.0],
        image_key: 1,
        corner_radius: 0.0,
        transform: AffineTransform::identity().to_array(),
        opacity: 1.0,
    });
    fb.pop_rclip();

    assert_eq!(fb.ops()[0].2.len(), 1);
    assert_eq!(fb.ops()[0].2[0], (b, 12.0));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo test_rclip`
Expected: FAIL — `push_rclip`, `current_rclip`, `MAX_RCLIP_DEPTH` not found; `ops` tuple has wrong arity.

- [ ] **Step 3: Add `RClipEntry` type alias and `MAX_RCLIP_DEPTH` constant**

At the top of `vexo/src/frame_builder.rs`, after the `Bounds` type alias (line ~60):

```rust
/// Maximum nesting depth for rounded-rect clips. Pushes beyond this
/// are logged and dropped. The shader uniform array is sized to this.
pub const MAX_RCLIP_DEPTH: usize = 8;

/// A single rounded-rect clip entry: (bounds, radius).
pub type RClipEntry = (Bounds, f32);
```

- [ ] **Step 4: Add `rclip_stack` field and per-op rclip snapshot**

Change the `ops` field type (line ~67) from:

```rust
    ops: Vec<(DrawOp, Option<Bounds>)>,
```

to:

```rust
    ops: Vec<(DrawOp, Option<Bounds>, Vec<RClipEntry>)>,
```

Add the `rclip_stack` field to the `FrameBuilder` struct (after `clip_stack` at line ~73):

```rust
    rclip_stack: Vec<RClipEntry>,
```

- [ ] **Step 5: Initialize `rclip_stack` in `new()` and clear in `clear()`**

In `FrameBuilder::new()` (line ~85), add:

```rust
            rclip_stack: Vec::new(),
```

In `FrameBuilder::clear()` (line ~96), add:

```rust
        self.rclip_stack.clear();
```

- [ ] **Step 6: Add `push_rclip`, `pop_rclip`, `current_rclip` methods**

Add these methods to `impl FrameBuilder` (after the existing `pop_clip` / `current_clip` methods, around line ~163):

```rust
    /// Push a rounded-rect clip onto the rclip stack.
    /// All subsequent DrawOps snapshot this stack until `pop_rclip`.
    /// Silently drops the push (with a warning log) if `MAX_RCLIP_DEPTH`
    /// is exceeded — the shader uniform array cannot hold more.
    pub fn push_rclip(&mut self, bounds: Bounds, radius: f32) {
        if self.rclip_stack.len() >= MAX_RCLIP_DEPTH {
            log::warn!(
                "[ClipRRect] max depth {} exceeded, dropping rclip push",
                MAX_RCLIP_DEPTH
            );
            return;
        }
        self.rclip_stack.push((bounds, radius));
    }

    /// Pop the most recent rounded-rect clip from the stack.
    pub fn pop_rclip(&mut self) {
        self.rclip_stack.pop();
    }

    /// Get the current active rclip stack as a slice.
    /// Empty slice means no rounded-rect clip is active.
    pub fn current_rclip(&self) -> &[RClipEntry] {
        &self.rclip_stack
    }

    /// Snapshot the current rclip stack for attaching to a DrawOp.
    fn snapshot_rclip(&self) -> Vec<RClipEntry> {
        if self.rclip_stack.is_empty() {
            Vec::new()
        } else {
            self.rclip_stack.clone()
        }
    }
```

- [ ] **Step 7: Update all `add_*` methods to snapshot rclip**

Update `add_rect` (line ~212) — change the `self.ops.push(...)` call from:

```rust
        self.ops.push((DrawOp::Quad(instance), clip));
```

to:

```rust
        self.ops
            .push((DrawOp::Quad(instance), clip, self.snapshot_rclip()));
```

Do the same for `add_shadow_rect` (line ~284):

```rust
        self.ops
            .push((DrawOp::Quad(instance), clip, self.snapshot_rclip()));
```

And `add_image` (line ~308):

```rust
        self.ops
            .push((DrawOp::Image(request), clip, self.snapshot_rclip()));
```

- [ ] **Step 8: Add `rclip_bounds` field to `TextRequest`**

In the `TextRequest` struct (line ~34), add after `clip_bounds`:

```rust
    /// Effective rounded-rect clip stack when this text was added.
    /// Empty slice means no rounded clip active. Applied as SDF masks
    /// in the text fragment shader (future; currently snapshotted but
    /// not yet enforced for text — matches existing text clip behavior).
    pub rclip_snapshot: Vec<RClipEntry>,
```

Update `add_text` (line ~287) to snapshot:

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
            rclip_snapshot: self.snapshot_rclip(),
        });
    }
```

- [ ] **Step 9: Update all readers of `ops` to use the new tuple arity**

Update `ops()` (line ~321):

```rust
    pub fn ops(&self) -> &[(DrawOp, Option<Bounds>, Vec<RClipEntry>)] {
        &self.ops
    }
```

Update `image_requests()` (line ~326) — change `|(op, _)|` to `|(op, _, _)|`:

```rust
    pub fn image_requests(&self) -> Vec<ImageRequest> {
        self.ops
            .iter()
            .filter_map(|(op, _, _)| match op {
                DrawOp::Image(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }
```

Update `compute_op_locations()` (line ~338) — change `|(op, _)|` to `|(op, _, _)|`.

Update `quad_count()` (line ~105), `has_quads()` (line ~112), and `image_count()` (line ~313) — change `|(op, _)|` to `|(op, _, _)|`.

Update `quad_instances()` (line ~121) — change `|(op, _)|` to `|(op, _, _)|`.

- [ ] **Step 10: Update existing tests that construct `ops` or reference the tuple**

In the `tests` module, find any test that accesses `fb.ops()` or constructs ops tuples. Update them to use the 3-element tuple. Specifically, search for `.ops()` calls in tests and update assertions. For example, `test_flatten_image_requests` and similar tests that iterate ops need the extra `_, _`.

Run `cargo test -p vexo -- --nocapture 2>&1 | grep "error\["` to find compilation errors and fix each by adding the extra `_, _` or `.2` field access.

- [ ] **Step 11: Run tests to verify they pass**

Run: `cargo test -p vexo`
Expected: all tests pass, including the four new rclip tests.

- [ ] **Step 12: Commit**

```bash
git add vexo/src/frame_builder.rs
git commit -m "feat(frame-builder): add rclip_stack and per-op rclip snapshots

Parallel to the existing rectangular clip_stack. Each DrawOp and
TextRequest now carries a snapshot of the active rclip stack at
add-time. Depth capped at MAX_RCLIP_DEPTH=8 with warn-and-drop on
overflow. The wgpu backend will upload these snapshots as per-op
uniform data for the fragment shader SDF mask."
```

---

### Task 4: Handle `PushClipRRect` / `PopClipRRect` in `CommandProcessor`

**Files:**
- Modify: `vexo/src/render/command_processor.rs:166-185`
- Test: `vexo/src/render/command_processor.rs`

**Interfaces:**
- Consumes: `FrameBuilder::push_rclip`, `pop_rclip` (from Task 3).
- Produces: `CommandProcessor` correctly routes `PushClipRRect`/`PopClipRRect` to the frame builder.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `vexo/src/render/command_processor.rs`:

```rust
#[test]
fn test_process_push_clip_rrect() {
    let mut frame_builder = FrameBuilder::new();
    let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
    let commands = vec![RenderCommand::PushClipRRect {
        bounds,
        radius: 8.0,
    }];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert_eq!(frame_builder.current_rclip().len(), 1);
    assert_eq!(frame_builder.current_rclip()[0], (bounds, 8.0));
}

#[test]
fn test_process_pop_clip_rrect() {
    let mut frame_builder = FrameBuilder::new();
    let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
    let commands = vec![
        RenderCommand::PushClipRRect {
            bounds,
            radius: 8.0,
        },
        RenderCommand::PopClipRRect,
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert!(frame_builder.current_rclip().is_empty());
}

#[test]
fn test_process_push_clip_rrect_with_offset() {
    let mut frame_builder = FrameBuilder::new();
    let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
    let commands = vec![RenderCommand::PushClipRRect {
        bounds,
        radius: 8.0,
    }];

    process_commands(&commands, &mut frame_builder, Point::new(5.0, 7.0));

    let entry = &frame_builder.current_rclip()[0];
    assert_eq!(entry.0.left, 15.0); // 10 + 5
    assert_eq!(entry.0.top, 27.0); // 20 + 7
    assert_eq!(entry.1, 8.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo test_process_push_clip_rrect test_process_pop_clip_rrect`
Expected: FAIL — non-exhaustive match on `RenderCommand` (the new variants aren't handled).

- [ ] **Step 3: Add the match arms**

In `vexo/src/render/command_processor.rs`, add after the existing `PopClip` arm (line ~183):

```rust
            RenderCommand::PushClipRRect { bounds, radius } => {
                let adjusted_bounds = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                let effective_bounds = if current_transform.is_identity() {
                    adjusted_bounds
                } else {
                    current_transform.transform_bounds(&adjusted_bounds)
                };
                frame_builder.push_rclip(effective_bounds, *radius);
            }
            RenderCommand::PopClipRRect => {
                frame_builder.pop_rclip();
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo test_process_push_clip_rrect test_process_pop_clip_rrect test_process_push_clip_rrect_with_offset`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/command_processor.rs
git commit -m "feat(command-processor): handle PushClipRRect/PopClipRRect

Routes rounded-rect clip commands to FrameBuilder's rclip_stack,
applying offset and transform-aware AABB expansion (same logic as
the existing PushClip handler)."
```

---

### Task 5: Update painter to emit `PushClipRRect` based on RO hook

**Files:**
- Modify: `vexo/src/painter.rs:217-229` (PushClip block) and `vexo/src/painter.rs:280-282` (PopClip block)
- Test: `vexo/src/painter.rs` or a new integration test

**Interfaces:**
- Consumes: `RenderObject::clip_corner_radius()` (from Task 1), `RenderCommand::PushClipRRect` (from Task 2).
- Produces: Painter emits the correct clip command based on the RO's hooks.

- [ ] **Step 1: Write the failing test**

This is an integration test. Add to `vexo/src/painter.rs` test module (or create one if it doesn't exist). We need a mock RO that returns `Some(radius)` and verify the painter emits `PushClipRRect`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, Logical, Point};
    use crate::render::RenderCommand;
    use crate::render_object::{
        HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject,
    };
    use crate::layout::LayoutNodeKey;

    /// A mock RO that clips its children to a rounded rect.
    struct MockClipRRectRO {
        bounds: Option<Bounds<Logical>>,
        radius: f32,
        child: Option<RenderObjectKey>,
    }

    impl RenderObject for MockClipRRectRO {
        fn layout(
            &mut self,
            _ctx: &mut LayoutContext,
            child_nodes: &[LayoutNodeKey],
        ) -> LayoutResult {
            LayoutResult {
                node: child_nodes[0],
                size: crate::core::Size::zero(),
            }
        }
        fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
        fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
            vec![]
        }
        fn hit_test(&self, _p: Point<Logical>, _ctx: &HitTestContext) -> bool {
            true
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn clip_bounds(&self) -> Option<Bounds<Logical>> {
            self.bounds
        }
        fn clip_corner_radius(&self) -> Option<f32> {
            if self.radius > 0.0 {
                Some(self.radius)
            } else {
                None
            }
        }
        fn children(&self) -> &[RenderObjectKey] {
            match &self.child {
                Some(c) => std::slice::from_ref(c),
                None => &[],
            }
        }
    }

    // The painter test is structural — we verify the decision logic by
    // checking that a RO with clip_corner_radius() == Some(r > 0) causes
    // the painter to emit PushClipRRect. Since paint_recursive requires a
    // full registry, this test is kept minimal: it verifies the hook is
    // consulted. Full e2e coverage is in Task 13.
    #[test]
    fn test_mock_clip_rrect_ro_returns_radius() {
        let ro = MockClipRRectRO {
            bounds: Some(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0)),
            radius: 8.0,
            child: None,
        };
        assert_eq!(ro.clip_corner_radius(), Some(8.0));
        assert!(ro.clip_bounds().is_some());
    }

    #[test]
    fn test_mock_clip_rrect_ro_radius_zero_returns_none() {
        let ro = MockClipRRectRO {
            bounds: Some(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0)),
            radius: 0.0,
            child: None,
        };
        assert_eq!(ro.clip_corner_radius(), None);
    }
}
```

- [ ] **Step 2: Run test to verify it passes (hook is already there)**

Run: `cargo test -p vexo test_mock_clip_rrect_ro`
Expected: PASS (the hook exists from Task 1; this test validates the contract the painter will rely on).

- [ ] **Step 3: Update the painter's PushClip block**

In `vexo/src/painter.rs`, find the block at lines 217-229 that reads `obj.clip_bounds()` and emits `PushClip`. Replace it with:

```rust
        // If this object clips its children, push clip before painting children.
        let clip = obj.clip_bounds();
        let clip_radius = obj.clip_corner_radius();
        let use_rclip = clip_radius.map(|r| r > 0.0).unwrap_or(false);
        if let Some(local_clip) = &clip {
            let absolute_clip = crate::core::Bounds::new(
                absolute_position.x,
                absolute_position.y,
                absolute_position.x + local_clip.width(),
                absolute_position.y + local_clip.height(),
            );
            if use_rclip {
                ctx.push_command(RenderCommand::PushClipRRect {
                    bounds: absolute_clip,
                    radius: clip_radius.unwrap(),
                });
            } else {
                ctx.push_command(RenderCommand::PushClip {
                    bounds: absolute_clip,
                });
            }
        }
```

- [ ] **Step 4: Update the painter's PopClip block**

Find the block at lines 280-282 that emits `PopClip`. Replace it with:

```rust
        // Pop clip after children
        if clip.is_some() {
            if use_rclip {
                ctx.push_command(RenderCommand::PopClipRRect);
            } else {
                ctx.push_command(RenderCommand::PopClip);
            }
        }
```

- [ ] **Step 5: Build and run all tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: build succeeds, all tests pass. The existing e2e tests that assert `PushClip`/`PopClip` for `DecoratedBox + clip()` should still pass (DecoratedBox returns `None` from `clip_corner_radius()`).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/painter.rs
git commit -m "feat(painter): emit PushClipRRect when RO returns clip_corner_radius

The painter reads both clip_bounds() and clip_corner_radius() once per
RO per paint pass. When radius > 0, it emits PushClipRRect/PopClipRRect
instead of PushClip/PopClip. DecoratedBox+clip() continues to use the
rectangular path (returns None from clip_corner_radius)."
```

---

### Task 6: Create `ClipRRectRenderObject`

**Files:**
- Create: `vexo/src/render_objects/clip_rrect.rs`
- Modify: `vexo/src/render_objects/mod.rs` (add module declaration)
- Test: `vexo/src/render_objects/clip_rrect.rs`

**Interfaces:**
- Consumes: `RenderObject` trait (with the new `clip_corner_radius` hook from Task 1).
- Produces: `ClipRRectRenderObject` with `new(radius)`, `set_radius(radius) -> bool`, pass-through layout, `clip_bounds()` + `clip_corner_radius()`.

- [ ] **Step 1: Check `render_objects/mod.rs` for module declaration pattern**

Read `vexo/src/render_objects/mod.rs` to see how existing ROs are declared.

- [ ] **Step 2: Write the failing tests**

Create `vexo/src/render_objects/clip_rrect.rs` with tests first:

```rust
use crate::core::{Bounds, Logical, Point};
use crate::layout::LayoutNodeKey;
use crate::render::RenderCommand;
use crate::render_object::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject,
};

/// Pass-through render object that clips its child to a rounded rectangle.
///
/// Layout is pass-through (borrows child's Taffy node). The clip is
/// applied by the painter via `clip_bounds()` + `clip_corner_radius()`,
/// which emits `PushClipRRect`/`PopClipRRect` around the child's paint
/// commands. The fragment shader multiplies in an SDF mask.
pub struct ClipRRectRenderObject {
    radius: f32,
    child: Option<crate::id::RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl ClipRRectRenderObject {
    pub fn new(radius: f32) -> Self {
        Self {
            radius: radius.max(0.0),
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the corner radius. Returns true if it changed.
    pub fn set_radius(&mut self, radius: f32) -> bool {
        let clamped = radius.max(0.0);
        if self.radius != clamped {
            self.radius = clamped;
            true
        } else {
            false
        }
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl RenderObject for ClipRRectRenderObject {
    fn layout(
        &mut self,
        _ctx: &mut LayoutContext,
        child_nodes: &[LayoutNodeKey],
    ) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             ClipRRect always has a child per its constructor",
        );
        self.child_layout_node = Some(child_node);
        LayoutResult {
            node: child_node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(child_node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[crate::id::RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn set_child_id(&mut self, child: crate::id::RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(
        &mut self,
        old: crate::id::RenderObjectKey,
        new: crate::id::RenderObjectKey,
    ) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_corner_radius(&self) -> Option<f32> {
        if self.radius > 0.0 {
            Some(self.radius)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_rrect_ro_is_pass_through() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert!(
            ro.is_pass_through(),
            "ClipRRectRenderObject must be pass-through"
        );
    }

    #[test]
    fn test_clip_rrect_ro_clip_corner_radius_some_when_positive() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert_eq!(ro.clip_corner_radius(), Some(8.0));
    }

    #[test]
    fn test_clip_rrect_ro_clip_corner_radius_none_when_zero() {
        let ro = ClipRRectRenderObject::new(0.0);
        assert_eq!(ro.clip_corner_radius(), None);
    }

    #[test]
    fn test_clip_rrect_ro_set_radius_change_detection() {
        let mut ro = ClipRRectRenderObject::new(8.0);
        assert!(ro.set_radius(12.0));
        assert!(!ro.set_radius(12.0));
        assert!(ro.set_radius(0.0));
        assert!(!ro.set_radius(0.0));
    }

    #[test]
    fn test_clip_rrect_ro_negative_radius_clamped() {
        let ro = ClipRRectRenderObject::new(-5.0);
        assert_eq!(ro.radius(), 0.0);
        assert_eq!(ro.clip_corner_radius(), None);
    }

    #[test]
    fn test_clip_rrect_ro_clip_bounds_none_before_layout() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert!(ro.clip_bounds().is_none());
    }
}
```

- [ ] **Step 3: Add module declaration**

In `vexo/src/render_objects/mod.rs`, add:

```rust
pub mod clip_rrect;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo test_clip_rrect_ro`
Expected: PASS (all 6 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render_objects/clip_rrect.rs vexo/src/render_objects/mod.rs
git commit -m "feat(render-objects): add ClipRRectRenderObject

Pass-through proxy that clips its child to a rounded rectangle via
clip_bounds() + clip_corner_radius(). Layout borrows the child's Taffy
node (identical to TransformRenderObject). Paint returns empty — the
clip is applied by the painter around the child's commands."
```

---

### Task 7: Create `ClipRRect` widget + `ClipRRectElement`

**Files:**
- Create: `vexo/src/widgets/clip_rrect.rs`
- Modify: `vexo/src/widgets/mod.rs` (add module + export)
- Modify: `vexo/src/lib.rs` (re-export if needed)
- Test: `vexo/src/widgets/clip_rrect.rs`

**Interfaces:**
- Consumes: `ClipRRectRenderObject` (from Task 6).
- Produces: `ClipRRect` widget with `new(radius, child)`, `with_key`, `child()`, `radius()`. `ClipRRectElement` mirrors `DecoratedBoxElement`.

- [ ] **Step 1: Read `vexo/src/widgets/decorated_box.rs:44-242` for the element pattern**

The element is structurally identical to `DecoratedBoxElement`. Use it as the template.

- [ ] **Step 2: Write the widget + element + tests**

Create `vexo/src/widgets/clip_rrect.rs`:

```rust
//! ClipRRect widget — clips its child to a rounded rectangle.
//!
//! This widget clips its single child subtree to a rounded rectangle.
//! The clip is applied at paint time via `PushClipRRect`/`PopClipRRect`
//! render commands, which the GPU backend enforces as an SDF mask in
//! the fragment shader.
//!
//! This is the Vexo equivalent of Flutter's `ClipRRect`.
//!
//! # Example
//!
//! ```ignore
//! ClipRRect::new(8.0, DecoratedBox::with_style(
//!     Text::new("Clipped!"),
//!     Style::default().background(Color::RED),
//! ))
//! ```

use std::any::Any;

use crate::core::Logical;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::key::WidgetKey;
use crate::render_objects::ClipRRectRenderObject;
use crate::{
    Element, ElementContext, ElementKey, EventContext, RenderObject, RenderObjectKey, UpdateResult,
    Widget,
};

// ============================================================================
// ClipRRectElement
// ============================================================================

pub struct ClipRRectElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl ClipRRectElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for ClipRRectElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for ClipRRectElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.widget = Some(widget);
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}

impl Element for ClipRRectElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        self.mount_render_object(context);

        if let Some(widget) = &self.widget {
            if let Some(child_widget) = widget.child() {
                context.inflate_child(None, child_widget.clone_boxed());
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        self.widget
            .as_ref()
            .map(|old| old.as_any().type_id() == widget.type_id())
            .unwrap_or(false)
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            let old_child = context.children().first().copied();
            if let Some(child_widget) = self.get_child_widget() {
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = old_child {
                context.unmount_child(old_child_key);
            }
        }

        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        child_ro: Option<RenderObjectKey>,
        context: &mut ElementContext,
    ) {
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}

// ============================================================================
// ClipRRect Widget
// ============================================================================

/// A widget that clips its child to a rounded rectangle.
///
/// The clip is applied at paint time via the GPU fragment shader (SDF
/// mask). Layout is pass-through — the child sizes itself naturally.
///
/// # Example
///
/// ```ignore
/// ClipRRect::new(8.0, DecoratedBox::with_style(
///     Text::new("Clipped!"),
///     Style::default().background(Color::RED),
/// ))
/// ```
pub struct ClipRRect {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    radius: f32,
}

impl ClipRRect {
    /// Create a new ClipRRect with the given corner radius and child.
    ///
    /// A radius of 0.0 means "rectangular clip" (degenerates to the
    /// existing PushClip path). Negative radius is clamped to 0.0.
    pub fn new(radius: f32, child: impl Widget + 'static) -> Self {
        debug_assert!(
            radius >= 0.0,
            "ClipRRect radius must be non-negative, got {}",
            radius
        );
        Self {
            key: None,
            child: Box::new(child),
            radius: radius.max(0.0),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the corner radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl Clone for ClipRRect {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            radius: self.radius,
        }
    }
}

impl Widget for ClipRRect {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ClipRRectElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ClipRRectRenderObject::new(self.radius))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<ClipRRectRenderObject>()
        {
            if ro.set_radius(self.radius) {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_clip_rrect_creation() {
        let w = ClipRRect::new(8.0, Text::new("Hi"));
        assert!(w.key().is_none());
        assert_eq!(w.radius(), 8.0);
    }

    #[test]
    fn test_clip_rrect_with_key_local() {
        let w = ClipRRect::new(8.0, Text::new("Hi")).with_key("my-clip");
        assert_eq!(w.key(), Some(WidgetKey::Local(Key::new("my-clip"))));
    }

    #[test]
    fn test_clip_rrect_with_key_global() {
        let gk = GlobalKey::new();
        let w = ClipRRect::new(8.0, Text::new("Hi")).with_key(gk.clone());
        assert_eq!(w.key(), Some(WidgetKey::Global(gk)));
    }

    #[test]
    fn test_clip_rrect_negative_radius_clamped() {
        let w = ClipRRect::new(-5.0, Text::new("Hi"));
        assert_eq!(w.radius(), 0.0);
    }

    #[test]
    fn test_clip_rrect_clone_preserves_fields() {
        let w = ClipRRect::new(12.0, Text::new("Hi")).with_key("clipped");
        let cloned = w.clone();
        assert_eq!(cloned.key(), w.key());
        assert_eq!(cloned.radius(), w.radius());
    }

    #[test]
    fn test_clip_rrect_render_object_is_pass_through() {
        let w = ClipRRect::new(8.0, Text::new("Hi"));
        let ro = w.create_render_object();
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_clip_rrect_update_render_object_paint_only() {
        let w1 = ClipRRect::new(8.0, Text::new("Hi"));
        let mut ro = w1.create_render_object();
        assert_eq!(w1.update_render_object(ro.as_mut()), UpdateResult::NONE);

        let w2 = ClipRRect::new(12.0, Text::new("Hi"));
        let result = w2.update_render_object(ro.as_mut());
        assert!(result.contains(UpdateResult::PAINT));
        assert!(!result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_clip_rrect_can_update_same_type() {
        let w1 = ClipRRect::new(8.0, Text::new("Hi"));
        let w2 = ClipRRect::new(12.0, Text::new("Hi"));
        let mut elem = ClipRRectElement::new();
        elem.set_widget(&w1);
        assert!(elem.can_update(w2.as_any()));
    }
}
```

- [ ] **Step 3: Add module declaration and export**

In `vexo/src/widgets/mod.rs`, add `mod clip_rrect;` to the module list (after `mod decorated_box;`) and add `pub use clip_rrect::ClipRRect;` to the exports (after `pub use decorated_box::DecoratedBox;`).

- [ ] **Step 4: Check if `lib.rs` re-exports widgets**

Read `vexo/src/lib.rs` and check if `ClipRRect` needs an explicit re-export. If `DecoratedBox` is re-exported there, add `ClipRRect` alongside it.

- [ ] **Step 5: Run tests**

Run: `cargo test -p vexo test_clip_rrect`
Expected: all 8 tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/clip_rrect.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat(widgets): add ClipRRect widget + element

ClipRRect clips its child subtree to a rounded rectangle. Widget and
element mirror DecoratedBox (single-child pass-through proxy). The
render object returns Some(radius) from clip_corner_radius() when
radius > 0, causing the painter to emit PushClipRRect/PopClipRRect."
```

---

### Task 8: Add rclip uniform buffer and bind group to wgpu backend

This task adds the GPU resources for per-op rclip data. The actual upload and per-op binding are in Tasks 9 and 10.

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs`

**Interfaces:**
- Produces: `RClipUniform` struct, `rclip_uniform_buffer`, `rclip_bind_group`, `rclip_bind_group_layout`. Pipeline layouts updated to include the rclip bind group.

- [ ] **Step 1: Define the `RClipUniform` struct**

In `vexo/src/render/wgpu_backend.rs`, add after the `GlobalUniforms` struct (line ~64):

```rust
/// Per-op rounded-rect clip data, uploaded to the GPU as a uniform.
///
/// Layout matches the WGSL `RClipUniform` struct in shader.wgsl and
/// image_shader.wgsl. Sized for `MAX_RCLIP_DEPTH` entries. Each op gets
/// its own slot in the uniform buffer at a dynamic offset.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RClipUniform {
    /// Number of active rclip entries (0..=8). Padded to vec4 alignment.
    count: [f32; 4],
    /// Bounds for each entry: (left, top, right, bottom) in logical pixels.
    bounds: [[f32; 4]; 8],
    /// Radii for each entry, packed two-per-vec4 for alignment.
    radii: [[f32; 4]; 2],
}

impl RClipUniform {
    /// All zeros — count=0 means "no rclip active" (shader fast path).
    const ZERO: Self = Self {
        count: [0.0; 4],
        bounds: [[0.0; 4]; 8],
        radii: [[0.0; 4]; 2],
    };

    fn from_entries(entries: &[(crate::frame_builder::Bounds, f32)]) -> Self {
        let mut u = Self::ZERO;
        let n = entries.len().min(8);
        u.count[0] = n as f32;
        for i in 0..n {
            let (b, r) = &entries[i];
            u.bounds[i] = [b.left, b.top, b.right, b.bottom];
            // Pack radii: indices 0-3 in radii[0], 4-7 in radii[1].
            u.radii[i / 4][i % 4] = *r;
        }
        u
    }
}

/// Dynamic offset alignment for the rclip uniform buffer.
/// wgpu requires uniform buffer offsets to be aligned to
/// `min_uniform_buffer_offset_alignment` (typically 256 bytes).
const RCLIP_UNIFORM_ALIGN: wgpu::BufferAddress = 256;
```

- [ ] **Step 2: Add rclip fields to `WgpuBackend` struct**

Add to the `WgpuBackend` struct (after `current_op_clips` at line ~108):

```rust
    // Per-op rounded-rect clip data
    rclip_uniform_buffer: wgpu::Buffer,
    rclip_bind_group_layout: wgpu::BindGroupLayout,
    rclip_bind_group: wgpu::BindGroup,
    /// Per-op dynamic offsets into rclip_uniform_buffer. Index aligns
    /// with current_op_locations. Offset 0 is always the ZERO slot.
    current_op_rclip_offsets: Vec<u32>,
```

- [ ] **Step 3: Create the bind group layout, buffer, and bind group in `init()`**

In the `init()` method, after the `global_bind_group` creation (around line ~287), add:

```rust
        let rclip_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("RClip Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(
                            (std::mem::size_of::<RClipUniform>() as u64).try_into().unwrap(),
                        ),
                    },
                    count: None,
                }],
            });
```

After the global uniform buffer creation, add the rclip buffer. Size it for up to 1000 ops × 256-byte aligned slots:

```rust
        const INITIAL_RCLIP_CAPACITY: usize = 1_000;
        let rclip_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RClip Uniform Buffer"),
            size: RCLIP_UNIFORM_ALIGN * INITIAL_RCLIP_CAPACITY as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let rclip_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RClip Bind Group"),
            layout: &rclip_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: rclip_uniform_buffer.as_entire_binding(),
            }],
        });
```

- [ ] **Step 4: Update pipeline layouts to include rclip bind group**

Find the `render_pipeline_layout` creation (line ~201) and change it from:

```rust
                bind_group_layouts: &[Some(&global_bind_group_layout)],
```

to:

```rust
                bind_group_layouts: &[Some(&global_bind_group_layout), Some(&rclip_bind_group_layout)],
```

Find the image pipeline layout creation (around line ~373) and change it from:

```rust
                bind_group_layouts: &[Some(&global_bind_group_layout), Some(&image_atlas_bind_group_layout)],
```

to:

```rust
                bind_group_layouts: &[Some(&global_bind_group_layout), Some(&image_atlas_bind_group_layout), Some(&rclip_bind_group_layout)],
```

- [ ] **Step 5: Store the new fields in the returned `WgpuBackend`**

At the end of `init()`, add the new fields to the struct literal:

```rust
            rclip_uniform_buffer,
            rclip_bind_group_layout,
            rclip_bind_group,
            current_op_rclip_offsets: Vec::new(),
```

- [ ] **Step 6: Initialize `current_op_rclip_offsets` in `upload_geometry`**

In `upload_geometry()` (line ~693), after `self.current_op_clips = op_clips;` (line ~750), add:

```rust
        // Compute per-op rclip offsets. Each op gets a slot in the
        // rclip uniform buffer. Ops with no rclip point to offset 0
        // (the ZERO slot). Ops with rclip data point to their slot.
        let mut rclip_offsets: Vec<u32> = Vec::with_capacity(op_locations.len());
        let mut next_slot: u32 = 1; // slot 0 is ZERO
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if rclip_snapshot.is_empty() {
                rclip_offsets.push(0);
            } else {
                rclip_offsets.push(next_slot * RCLIP_UNIFORM_ALIGN as u32);
                next_slot += 1;
            }
        }

        // Write the ZERO slot.
        queue.write_buffer(
            &self.rclip_uniform_buffer,
            0,
            bytemuck::bytes_of(&RClipUniform::ZERO),
        );

        // Write each non-zero op's rclip data.
        let mut slot: u32 = 1;
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if !rclip_snapshot.is_empty() {
                let uniform = RClipUniform::from_entries(rclip_snapshot);
                queue.write_buffer(
                    &self.rclip_uniform_buffer,
                    slot * RCLIP_UNIFORM_ALIGN as wgpu::BufferAddress,
                    bytemuck::bytes_of(&uniform),
                );
                slot += 1;
            }
        }

        self.current_op_rclip_offsets = rclip_offsets;
```

Note: `upload_geometry` already uses `self.queue.write_buffer(...)` for the existing instance buffers (lines 734-746), so `self.queue` is accessible.

- [ ] **Step 7: Build and verify compilation**

Run: `cargo build -p vexo`
Expected: compiles without errors. If `queue` is not accessible in `upload_geometry`, refactor the rclip writes into `execute_render_pass` before `encoder.begin_render_pass`.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs
git commit -m "feat(wgpu-backend): add rclip uniform buffer and bind group

Per-op rounded-rect clip data is uploaded to a uniform buffer with
dynamic offsets. The quad pipeline binds it at group 1; the image
pipeline at group 2 (after the atlas). Fragment shaders will read
the RClipUniform struct to apply SDF masks. The actual per-op
set_bind_group calls are added in the next task."
```

---

### Task 9: Per-op `set_bind_group` with dynamic offset in render pass

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs:822-900` (render loop)

**Interfaces:**
- Consumes: `current_op_rclip_offsets` (from Task 8).
- Produces: Each draw call has the correct rclip uniform bound.

- [ ] **Step 1: Read the render loop**

Read `vexo/src/render/wgpu_backend.rs:822-900` to understand the current pipeline-switching and draw logic.

- [ ] **Step 2: Add per-op rclip bind group setting**

Change the render loop from a zip-iterator to an enumerated zip so we have an index into `current_op_rclip_offsets`. Find the loop at line ~830:

```rust
            for (loc, clip) in self.current_op_locations.iter().zip(self.current_op_clips.iter()) {
```

Change to:

```rust
            for (i, (loc, clip)) in self.current_op_locations
                .iter()
                .zip(self.current_op_clips.iter())
                .enumerate()
            {
```

After the pipeline switch block (after line ~878) and before the draw call (line ~899), add:

```rust
                // 3. RClip bind group: per-op dynamic offset.
                //    Quad pipeline: group 1. Image pipeline: group 2
                //    (group 1 is the image atlas).
                let rclip_group = match kind {
                    OpKind::Quad => 1,
                    OpKind::Image => 2,
                };
                render_pass.set_bind_group(
                    rclip_group,
                    &self.rclip_bind_group,
                    &[self.current_op_rclip_offsets[i]],
                );
```

- [ ] **Step 3: Build**

Run: `cargo build -p vexo`
Expected: compiles.

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p vexo`
Expected: all pass (the GPU code isn't exercised in unit tests, but compilation + non-GPU tests should pass).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs
git commit -m "feat(wgpu-backend): set rclip bind group per-op with dynamic offset

Each draw call now binds the rclip uniform at the op's dynamic offset.
Quad pipeline uses bind group 1; image pipeline uses bind group 2
(group 1 is the atlas). The fragment shader will read this uniform to
apply the SDF mask."
```

---

### Task 10: Update `shader.wgsl` with rclip uniform + SDF mask

**Files:**
- Modify: `vexo/src/shader.wgsl`

**Interfaces:**
- Consumes: `RClipUniform` struct (from Task 8, must match layout).
- Produces: Fragment shader applies SDF mask when `rclip_count > 0`.

- [ ] **Step 1: Add the RClipUniform struct and binding**

At the top of `vexo/src/shader.wgsl`, after `GlobalUniforms`, add:

```wgsl
struct RClipUniform {
    count: vec4<f32>,              // .x = number of active entries (0..8)
    bounds: array<vec4<f32>, 8>,   // (left, top, right, bottom) per entry
    radii: array<vec4<f32>, 2>,    // 8 radii packed 4-per-vec4
};

@group(1) @binding(0) var<uniform> rclip: RClipUniform;
```

- [ ] **Step 2: Add the SDF helper function**

Before `fs_main`, add:

```wgsl
/// SDF distance to a rounded rectangle.
/// `p` is the fragment position in physical pixels.
/// `b` is the rect bounds (left, top, right, bottom) in physical pixels.
/// `r` is the corner radius in physical pixels.
/// Returns <= 0 inside, > 0 outside, |value| < 1 = 1px AA band.
fn sdf_rounded_rect(p: vec2<f32>, b: vec4<f32>, r: f32) -> f32 {
    let center = (b.xy + b.zw) * 0.5;
    let half_size = (b.zw - b.xy) * 0.5;
    let radius = min(r, min(half_size.x, half_size.y));
    let q = abs(p - center) - (half_size - radius);
    let outside = length(max(q, vec2<f32>(0.0)));
    let inside = min(max(q.x, q.y), 0.0);
    return outside + inside - radius;
}

/// Alpha multiplier for the active rclip stack. Returns 1.0 if no
/// rclip is active; otherwise the product of per-entry SDF masks.
/// `p` is the fragment position in physical pixels.
/// rclip.bounds and rclip.radii are in logical pixels — multiplied by
/// scale_factor here to match the physical-pixel SDF space.
fn rclip_alpha(p: vec2<f32>) -> f32 {
    let n = i32(rclip.count.x);
    if (n == 0) {
        return 1.0;
    }
    let sf = globals.scale_factor;
    var mask = 1.0;
    for (var i = 0; i < n; i = i + 1) {
        let b = rclip.bounds[i] * sf;
        let r = rclip.radii[i / 4][i % 4] * sf;
        let dist = sdf_rounded_rect(p, b, r);
        // Outside: dist > 0 → alpha 0. AA band: -1 < dist <= 0 (1px).
        let entry_alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
        mask = mask * entry_alpha;
    }
    return mask;
}
```

- [ ] **Step 3: Apply the mask in `fs_main`**

The fragment position in physical pixels is `in.uv * in.size` (same expression the existing SDF code uses at line 101). At the end of `fs_main`, before each `return` statement that outputs a color, multiply the alpha by `rclip_alpha(in.uv * in.size)`.

For the **shadow path** return (line ~80), change:

```wgsl
        return vec4<f32>(in.shadow_color.rgb, falloff * in.shadow_color.a);
```

to:

```wgsl
        return vec4<f32>(in.shadow_color.rgb, falloff * in.shadow_color.a * rclip_alpha(in.uv * in.size));
```

For the **fill/border path** returns:
- The early `return in.color;` (line ~88, radius < 0.5, no border) becomes:

```wgsl
            return vec4<f32>(in.color.rgb, in.color.a * rclip_alpha(in.uv * in.size));
```

- The `return mix(in.color, in.border_color, is_border);` (line ~98) becomes:

```wgsl
            let result = mix(in.color, in.border_color, is_border);
            return vec4<f32>(result.rgb, result.a * rclip_alpha(in.uv * in.size));
```

- The final `return vec4<f32>(final_color.rgb, final_color.a * fill_alpha);` (line ~120) becomes:

```wgsl
        return vec4<f32>(final_color.rgb, final_color.a * fill_alpha * rclip_alpha(in.uv * in.size));
```

- [ ] **Step 4: Build**

Run: `cargo build -p vexo`
Expected: compiles. Shader compilation happens at runtime; if there's a WGSL syntax error, it'll surface when the pipeline is created (in `WgpuBackend::init`). The build itself won't catch shader errors.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/shader.wgsl
git commit -m "feat(shader): add rclip SDF mask to quad fragment shader

The fragment shader reads the RClipUniform (bound at group 1) and
multiplies the output alpha by the product of SDF masks for each
active rounded-rect clip entry. When count == 0, rclip_alpha() returns
1.0 (fast path — no SDF math)."
```

---

### Task 11: Update `image_shader.wgsl` with rclip uniform + SDF mask

**Files:**
- Modify: `vexo/src/image_shader.wgsl`

Same as Task 10 but for the image pipeline. The rclip uniform is bound at group 2 (group 1 is the atlas).

- [ ] **Step 1: Read the current `image_shader.wgsl`**

Read `vexo/src/image_shader.wgsl` to understand its structure.

- [ ] **Step 2: Add the RClipUniform struct and binding**

At the top of `vexo/src/image_shader.wgsl`, add (same struct as Task 10):

```wgsl
struct RClipUniform {
    count: vec4<f32>,
    bounds: array<vec4<f32>, 8>,
    radii: array<vec4<f32>, 2>,
};

@group(2) @binding(0) var<uniform> rclip: RClipUniform;

fn sdf_rounded_rect(p: vec2<f32>, b: vec4<f32>, r: f32) -> f32 {
    let center = (b.xy + b.zw) * 0.5;
    let half_size = (b.zw - b.xy) * 0.5;
    let radius = min(r, min(half_size.x, half_size.y));
    let q = abs(p - center) - (half_size - radius);
    let outside = length(max(q, vec2<f32>(0.0)));
    let inside = min(max(q.x, q.y), 0.0);
    return outside + inside - radius;
}

fn rclip_alpha(p: vec2<f32>) -> f32 {
    let n = i32(rclip.count.x);
    if (n == 0) {
        return 1.0;
    }
    var mask = 1.0;
    for (var i = 0; i < n; i = i + 1) {
        let b = rclip.bounds[i];
        let r = rclip.radii[i / 4][i % 4];
        let dist = sdf_rounded_rect(p, b, r);
        let entry_alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
        mask = mask * entry_alpha;
    }
    return mask;
}
```

- [ ] **Step 3: Apply the mask in the image fragment shader**

The image shader's `fs_main` (line 59-81) has `in.uv * in.size` as the fragment position in physical pixels (line 68, same as the quad shader). The `in.size` is `inst_size * globals.scale_factor` (physical pixels, set at line 52).

The image shader has two return paths:
1. `radius < 0.5` (no per-image corner radius) → line 65: `return vec4<f32>(tex_color.rgb, tex_color.a * in.opacity);`
2. Per-image SDF path → line 80: `return vec4<f32>(tex_color.rgb, tex_color.a * fill_alpha * in.opacity);`

Both need the rclip mask multiplied into the alpha:

For path 1 (line 65), change to:
```wgsl
        return vec4<f32>(tex_color.rgb, tex_color.a * in.opacity * rclip_alpha(in.uv * in.size));
```

For path 2 (line 80), change to:
```wgsl
        return vec4<f32>(tex_color.rgb, tex_color.a * fill_alpha * in.opacity * rclip_alpha(in.uv * in.size));
```

The `rclip_alpha` function (from Step 2) takes the physical-pixel fragment position and applies the SDF mask from the `RClipUniform` (logical bounds × `scale_factor`).

- [ ] **Step 4: Build**

Run: `cargo build -p vexo`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/image_shader.wgsl
git commit -m "feat(shader): add rclip SDF mask to image fragment shader

Same RClipUniform struct and SDF mask as the quad shader, but bound
at group 2 (group 1 is the image atlas). The mask applies on top of
the image's own corner_radius SDF, enabling ClipRRect to clip images
that don't have their own corner_radius set."
```

---

### Task 12: E2E test for ClipRRect command stream

**Files:**
- Modify: `vexo/src/e2e_test.rs`

**Interfaces:**
- Consumes: `ClipRRect` widget (from Task 7), full pipeline.

- [ ] **Step 1: Read existing e2e test patterns**

Read `vexo/src/e2e_test.rs` to find how existing tests build a widget tree and assert on the render command stream. Look for tests that use `PushClip` as a reference (around line 463, 509).

- [ ] **Step 2: Write the test**

Add a test that builds a `ClipRRect` wrapping a colored `DecoratedBox` and asserts the command stream contains `PushClipRRect { bounds, radius }` ... `PopClipRRect` with child commands between them.

The exact test structure depends on the e2e test harness. Follow the pattern of existing tests that assert on `PushClip`. The test should:

1. Build a widget tree: `ClipRRect::new(8.0, DecoratedBox::with_style(MultiChild::column(..), Style::default().background(RED)))` or similar.
2. Run the pipeline for one frame.
3. Assert the render commands contain `PushClipRRect { radius: 8.0, .. }`.
4. Assert a `PopClipRRect` follows.
5. Assert child commands (e.g. `Rect`) appear between them.

- [ ] **Step 3: Run the test**

Run: `cargo test -p vexo --test e2e_test` (or `cargo test -p vexo test_clip_rrect_e2e` if it's a module)
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/e2e_test.rs
git commit -m "test(e2e): verify ClipRRect emits PushClipRRect/PopClipRRect

Builds a ClipRRect wrapping a DecoratedBox and asserts the render
command stream contains the expected PushClipRRect ... PopClipRRect
sequence with child commands between them."
```

---

### Task 13: Visual verification checkpoint

**This task requires the user to run the desktop demo.** Do not run `cargo run -p desktop_demo` yourself.

- [ ] **Step 1: Add a temporary ClipRRect demo to `shared_app/src/app.rs`**

In `shared_app/src/app.rs`, temporarily modify the `view()` function to wrap a colored box in `ClipRRect` so the user can visually verify the rounded clipping. Add this as the first child of the top-level container (before the `TabBarView`):

```rust
use vexo::{ClipRRect, DecoratedBox, Style};

// Inside view(), temporarily add:
let _clip_test = ClipRRect::new(20.0, DecoratedBox::with_style(
    vexo::WithLayout::new(
        vexo::Text::new("Clipped!"),
        Layout::default().width(200.0).height(100.0),
    ),
    Style::default().background(Color::BLUE),
));
```

Then include `_clip_test` as a child in the top-level `MultiChild` so it renders. The exact placement doesn't matter — it just needs to be visible on screen. This is temporary and will be removed in Step 4.

- [ ] **Step 2: Ask the user to run the demo**

Tell the user:

> Phase 1 implementation is complete. Please run `cargo run -p desktop_demo` and verify:
> 1. The ClipRRect demo widget shows a blue box with rounded corners (radius 20).
> 2. The text "Clipped!" inside it is also clipped to the rounded shape.
> 3. No rendering artifacts or shader errors in the console.
> 4. Existing UI (avatars, decorated boxes, etc.) still renders correctly.

- [ ] **Step 3: Wait for user confirmation**

Do not proceed to Phase 2 until the user confirms visual verification passes.

- [ ] **Step 4: Remove the temporary demo (if added)**

Once verified, remove the temporary demo widget added in Step 1.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove temporary ClipRRect visual test widget"
```

---

## Phase 2: Migration & Cleanup

### Task 14: Migrate `avatar.rs` to `ClipRRect`

**Files:**
- Modify: `shared_app/src/widgets/avatar.rs`

- [ ] **Step 1: Read current `avatar.rs`**

Read `shared_app/src/widgets/avatar.rs` (16 lines). The current implementation uses `Image::with_corner_radius(diameter / 2.0)` inside `DecoratedBox::with_style(.., Style::default().clip())`.

- [ ] **Step 2: Rewrite to use `ClipRRect`**

Replace the entire file content:

```rust
use std::rc::Rc;

use vexo::{ClipRRect, Image, Layout, Style, Widget, WithLayout};

pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    ClipRRect::new(
        diameter / 2.0,
        WithLayout::new(
            Image::from_bytes(bytes)
                .expect("avatar bytes are valid PNG"),
            Layout::default().width(diameter).height(diameter),
        ),
    )
    .boxed()
}
```

Note: the `DecoratedBox` + `Style::clip()` wrapper is removed — it was a no-op rectangle on a square image. The `ClipRRect` with `radius = diameter / 2.0` produces the circle.

- [ ] **Step 3: Build**

Run: `cargo build -p shared_app`
Expected: compiles. The `vexo::DecoratedBox` import is removed since it's no longer needed. If other files in `shared_app` still use `DecoratedBox`, the import stays in those files.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shared_app`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/widgets/avatar.rs
git commit -m "refactor(avatar): migrate to ClipRRect

Replace Image::with_corner_radius + DecoratedBox::clip() with
ClipRRect::new(diameter/2, ...). The DecoratedBox+clip() was a no-op
rectangle on a square image; ClipRRect provides the actual rounded
clip via the GPU fragment shader."
```

---

### Task 15: Remove `Image.corner_radius` field and all plumbing

This is a cleanup task that removes the now-redundant image-specific corner radius path. After the avatar migration, no callers use `Image::with_corner_radius`.

**Files:**
- Modify: `vexo/src/widgets/image.rs` — remove `corner_radius` field, `with_corner_radius`, `corner_radius()`, update tests
- Modify: `vexo/src/render_objects/image.rs` — remove `corner_radius` field, `set_corner_radius`, update `new()` signature, update tests
- Modify: `vexo/src/render/command.rs` — remove `corner_radius` from `RenderCommand::Image`
- Modify: `vexo/src/render/command_processor.rs` — remove `corner_radius` from image processing
- Modify: `vexo/src/frame_builder.rs` — remove `corner_radius` from `ImageRequest`, update tests
- Modify: `vexo/src/render/wgpu_backend.rs` — remove `corner_radius` from `ImageInstance` construction, remove the image shader's own SDF branch (or leave it — the shader's `inst_corner_radius` will always be 0.0, so the branch is dead code; removing it is cleaner)
- Modify: `vexo/src/image_shader.wgsl` — remove `corner_radius` from vertex output and the SDF branch in `fs_main` (since `inst_corner_radius` will always be 0.0, the SDF code is dead)

- [ ] **Step 1: Search for all references to `Image`'s `corner_radius`**

Run: `rg "with_corner_radius|set_corner_radius|corner_radius" --type rust vexo/src/widgets/image.rs vexo/src/render_objects/image.rs vexo/src/render/command.rs vexo/src/render/command_processor.rs vexo/src/frame_builder.rs vexo/src/render/wgpu_backend.rs vexo/src/image_shader.wgsl`

This gives the full list of sites to update.

- [ ] **Step 2: Remove `corner_radius` from `Image` widget**

In `vexo/src/widgets/image.rs`:
- Remove the `corner_radius: f32` field from the struct (line 13).
- Remove `corner_radius: 0.0` from `new()` (line 21).
- Remove `corner_radius` from `from_bytes` (it calls `new()`).
- Remove the `with_corner_radius` method (lines 35-38).
- Remove the `corner_radius()` accessor (lines 44-46).
- Remove `corner_radius` from `Clone::clone` (line 54).
- Remove `self.corner_radius` from `create_render_object` (line 71) — change to `ImageRenderObject::new(&self.image_data)`.
- Remove `self.corner_radius` from `update_render_object` (lines 87-89) — remove the `set_corner_radius` block.
- Remove tests: `test_image_widget_with_corner_radius`, `test_image_widget_corner_radius_default_zero`, `test_image_widget_clone_preserves_corner_radius` (lines 147-167).
- Update `test_image_widget_clone` to not assert on `corner_radius`.

- [ ] **Step 3: Remove `corner_radius` from `ImageRenderObject`**

In `vexo/src/render_objects/image.rs`:
- Remove the `corner_radius: f32` field (line 22).
- Change `new()` signature from `new(image_data: &ImageData, corner_radius: f32)` to `new(image_data: &ImageData)` (line 28). Remove `corner_radius` from the struct literal.
- Remove the `set_corner_radius` method (lines 55-62).
- Remove `self.corner_radius` from the `RenderCommand::Image` construction (line 123) — change to `corner_radius: 0.0` (or better, remove the field entirely in Step 4).
- Remove tests: `test_image_render_object_paint_emits_corner_radius`, `test_image_render_object_set_corner_radius_change_detection` (lines 273-298).

- [ ] **Step 4: Remove `corner_radius` from `RenderCommand::Image`**

In `vexo/src/render/command.rs`:
- Remove the `corner_radius: f32` field from the `Image` variant (line 75).
- Update the doc comment if needed.

- [ ] **Step 5: Update `CommandProcessor`**

In `vexo/src/render/command_processor.rs`, find the `RenderCommand::Image` match arm (around line 150). Remove `corner_radius: *corner_radius` from the `ImageRequest` construction. The `ImageRequest` struct will no longer have this field.

- [ ] **Step 6: Remove `corner_radius` from `ImageRequest`**

In `vexo/src/frame_builder.rs`:
- Remove `pub corner_radius: f32` from `ImageRequest` (line 55).
- Update all test constructions of `ImageRequest` to remove the field (search for `corner_radius:` in the test module).

- [ ] **Step 7: Update `WgpuBackend`**

In `vexo/src/render/wgpu_backend.rs`:
- Find `ImageInstance::from_logical` call (around line 718-726). Remove `req.corner_radius` from the arguments.
- Check `ImageInstance::from_logical` signature (search for it) and remove the `corner_radius` parameter. Update the struct construction inside `from_logical` to not set `inst_corner_radius` (or set it to 0.0 if the field remains in the struct for now — but ideally remove it from `ImageInstance` too).

- [ ] **Step 8: Update `image_shader.wgsl`**

In `vexo/src/image_shader.wgsl`:
- Remove `corner_radius` from `VertexOutput` (line 8).
- Remove `inst_corner_radius` from the vertex shader inputs (line 28).
- Remove `out.corner_radius = inst_corner_radius * globals.scale_factor;` (line 53).
- Remove the `corner_radius`-based SDF branch in `fs_main` (the `let radius = min(in.corner_radius, ...)` block). Since images no longer have their own corner radius, the image fragment shader just samples the texture and applies opacity + rclip mask.

The simplified `fs_main` for images should:
1. Sample the texture at `in.uv`.
2. Multiply by `in.color` (opacity/tint).
3. Multiply alpha by `rclip_alpha(...)` (from Task 11).
4. Return the result.

- [ ] **Step 9: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: all tests pass. Fix any compilation errors from missed references.

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: all pass.

- [ ] **Step 10: Search for any remaining references**

Run: `rg "with_corner_radius|set_corner_radius|inst_corner_radius" --type rust --type wgsl`
Expected: no matches (or only matches in unrelated contexts like the quad shader's `inst_corner_radius` for `DecoratedBox`).

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor(image): remove corner_radius field and plumbing

Image.corner_radius is no longer needed — callers use ClipRRect for
rounded image clipping. Removes:
- Image::with_corner_radius, Image::corner_radius field
- ImageRenderObject::corner_radius, set_corner_radius
- RenderCommand::Image.corner_radius
- ImageRequest.corner_radius
- ImageInstance corner_radius parameter
- image_shader.wgsl corner_radius SDF branch

The rclip uniform (from ClipRRect) now handles all rounded-rect
clipping for images, quads, and any other draw op uniformly."
```

---

## Summary

| Task | Phase | What |
|------|-------|------|
| 1 | 1 | `clip_corner_radius()` hook on `RenderObject` |
| 2 | 1 | `PushClipRRect`/`PopClipRRect` commands |
| 3 | 1 | `FrameBuilder` rclip_stack + per-op snapshots |
| 4 | 1 | `CommandProcessor` handles new commands |
| 5 | 1 | Painter emits `PushClipRRect` based on RO hook |
| 6 | 1 | `ClipRRectRenderObject` |
| 7 | 1 | `ClipRRect` widget + element |
| 8 | 1 | Wgpu backend: rclip uniform buffer + bind group |
| 9 | 1 | Wgpu backend: per-op `set_bind_group` |
| 10 | 1 | `shader.wgsl` rclip SDF mask |
| 11 | 1 | `image_shader.wgsl` rclip SDF mask |
| 12 | 1 | E2E test |
| 13 | 1 | Visual verification (user runs demo) |
| 14 | 2 | Migrate `avatar.rs` |
| 15 | 2 | Remove `Image.corner_radius` |
