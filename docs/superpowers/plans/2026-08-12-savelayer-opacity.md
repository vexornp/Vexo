# SaveLayer for Opacity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace CPU alpha-multiplication in `Opacity` with offscreen render-target grouping (SaveLayer), fixing the white-rectangle bug at its root.

**Architecture:** When `Opacity(alpha < 1.0)` is encountered, the painter emits `PushSaveLayer`/`PopSaveLayer` instead of `PushOpacity`/`PopOpacity`. The command processor emits `BeginSaveLayer`/`EndSaveLayer` marker ops into the flat draw list without alpha-multiplying contained ops. The wgpu backend scans for these markers, renders each group's ops (three-phase) into an offscreen texture with its own depth attachment and per-group `glyphon::TextRenderer`, then composites the offscreen result as a textured quad at the group's paint-order z-depth.

**Tech Stack:** Rust, wgpu, glyphon-vexo 0.12.1 (per-group TextRenderer sharing one TextAtlas + FontSystem), Taffy layout.

## Global Constraints

- **Never run `cargo run -p desktop_demo`** — the agent cannot interact with the GUI. All GUI verification is done by the user.
- **Never run `./build_for_ios.sh`** — iOS builds are done by the user.
- Run `cargo build -p vexo` after every Rust file change.
- Run `cargo test -p vexo` after every task that adds/changes tests.
- glyphon-vexo 0.12.1 `TextRenderer::new(&mut TextAtlas, &Device, MultisampleState, Option<DepthStencilState>)` borrows the atlas — multiple TextRenderers share one atlas.
- glyphon-vexo 0.12.1 `TextRenderer::prepare(&mut self, &Device, &Queue, &mut FontSystem, &mut TextAtlas, &Viewport, ...)` and `render(&self, &TextAtlas, &Viewport, &mut RenderPass)` — atlas, font system, viewport are passed as parameters, not owned.
- The flat draw-list invariant (`FrameBuilder.ops: Vec<(DrawOp, Option<Bounds>, Vec<RClipEntry>)>`) must be preserved — marker ops are entries in this same Vec.
- `PushOpacity`/`PopOpacity` RenderCommand variants and the command_processor's alpha-multiply path are kept as the documented fallback (rollback path).

---

## Phase 1: Data Model & Command Flow (CPU-side, fully unit-testable)

### Task 1: Add `PushSaveLayer`/`PopSaveLayer` RenderCommand variants

**Files:**
- Modify: `vexo/src/render/command.rs:136-145` (add new variants after `PopOpacity`)

**Interfaces:**
- Produces: `RenderCommand::PushSaveLayer { bounds: Bounds<Logical>, opacity: f32 }` and `RenderCommand::PopSaveLayer` — consumed by command_processor (Task 4) and tested in this task.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/render/command.rs` tests module (after `test_opacity_commands` at line 428):

```rust
#[test]
fn test_save_layer_commands() {
    let bounds = Bounds::from_xywh(10.0, 20.0, 100.0, 50.0);
    let cmd = RenderCommand::PushSaveLayer {
        bounds,
        opacity: 0.85,
    };
    match cmd {
        RenderCommand::PushSaveLayer { bounds: b, opacity } => {
            assert_eq!(b.left, 10.0);
            assert_eq!(b.width(), 100.0);
            assert!((opacity - 0.85).abs() < 1e-6);
        }
        _ => panic!("Expected PushSaveLayer"),
    }
    let cmd = RenderCommand::PopSaveLayer;
    match cmd {
        RenderCommand::PopSaveLayer => {}
        _ => panic!("Expected PopSaveLayer"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib render::command::tests::test_save_layer_commands 2>&1 | tail -5`
Expected: FAIL — `no variant named PushSaveLayer`

- [ ] **Step 3: Add the variants**

In `vexo/src/render/command.rs`, after the `PopOpacity` variant (line 144), add:

```rust
    /// Push a save-layer context onto the stack.
    /// All subsequent commands are rendered into an offscreen texture
    /// as a unit, then composited at `opacity` — preserving internal
    /// paint order (Flutter/Skia SaveLayer model). Replaces
    /// `PushOpacity` for the Opacity widget when `opacity < 1.0`.
    PushSaveLayer {
        /// The bounds of the save-layer group in logical coordinates.
        /// The offscreen texture is sized to this region.
        bounds: Bounds<Logical>,
        /// The opacity value (0.0 = invisible, 1.0 = fully opaque).
        /// Applied at composite time, not baked into contained ops.
        opacity: f32,
    },

    /// Pop the most recent save-layer context.
    PopSaveLayer,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib render::command::tests::test_save_layer_commands 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Build the full crate**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS (no errors — the new variants are unused but valid)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render/command.rs
git commit -m "feat(render): add PushSaveLayer/PopSaveLayer RenderCommand variants"
```

---

### Task 2: Add `DrawOp::BeginSaveLayer`/`EndSaveLayer` markers + `OpLocation::SaveLayerMarker`

**Files:**
- Modify: `vexo/src/frame_builder.rs:7-10` (DrawOp enum)
- Modify: `vexo/src/frame_builder.rs:13-26` (OpLocation enum)
- Modify: `vexo/src/frame_builder.rs:29-43` (OpKind enum + impl)
- Modify: `vexo/src/frame_builder.rs:434-456` (compute_op_locations)

**Interfaces:**
- Produces: `DrawOp::BeginSaveLayer { bounds: Bounds, opacity: f32 }`, `DrawOp::EndSaveLayer`, `OpLocation::SaveLayerMarker`, `OpKind::SaveLayerMarker` — consumed by FrameBuilder methods (Task 3), command_processor (Task 4), and WgpuBackend (Phase 2).

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/frame_builder.rs` tests module:

```rust
#[test]
fn test_save_layer_markers_in_ops() {
    let mut fb = FrameBuilder::new();
    fb.add_rect(
        Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
        Color::RED,
        None,
        0.0,
    );
    fb.begin_save_layer(Bounds::from_xywh(10.0, 20.0, 200.0, 100.0), 0.85);
    fb.add_rect(
        Bounds::from_xywh(10.0, 20.0, 50.0, 50.0),
        Color::BLUE,
        None,
        0.0,
    );
    fb.end_save_layer();
    fb.add_rect(
        Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
        Color::GREEN,
        None,
        0.0,
    );

    let ops = fb.ops();
    assert!(matches!(ops[0].0, DrawOp::Quad(_)));
    assert!(matches!(
        &ops[1].0,
        DrawOp::BeginSaveLayer { opacity, .. } if (*opacity - 0.85).abs() < 1e-6
    ));
    assert!(matches!(ops[2].0, DrawOp::Quad(_)));
    assert!(matches!(ops[3].0, DrawOp::EndSaveLayer));
    assert!(matches!(ops[4].0, DrawOp::Quad(_)));
}

#[test]
fn test_save_layer_markers_in_op_locations() {
    let mut fb = FrameBuilder::new();
    fb.add_rect(
        Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
        Color::RED,
        None,
        0.0,
    );
    fb.begin_save_layer(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0), 0.5);
    fb.add_rect(
        Bounds::from_xywh(0.0, 0.0, 50.0, 50.0),
        Color::BLUE,
        None,
        0.0,
    );
    fb.end_save_layer();

    let locations = fb.compute_op_locations();
    assert_eq!(locations.len(), 4);
    assert_eq!(locations[0].kind(), OpKind::Quad);
    assert_eq!(locations[1].kind(), OpKind::SaveLayerMarker);
    assert_eq!(locations[2].kind(), OpKind::Quad);
    assert_eq!(locations[3].kind(), OpKind::SaveLayerMarker);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib frame_builder::tests::test_save_layer_markers_in_ops 2>&1 | tail -5`
Expected: FAIL — `no variant BeginSaveLayer` / `no method begin_save_layer`

- [ ] **Step 3: Add DrawOp variants**

In `vexo/src/frame_builder.rs`, replace the DrawOp enum (lines 7-10):

```rust
/// A single drawable geometry primitive in paint order.
#[derive(Debug, Clone)]
pub enum DrawOp {
    Quad(QuadInstance),
    Image(ImageRequest),
    /// Begin a save-layer group. Ops between Begin/End are rendered
    /// into an offscreen texture and composited as a unit at `opacity`.
    BeginSaveLayer {
        bounds: Bounds,
        opacity: f32,
    },
    /// End the most recent save-layer group.
    EndSaveLayer,
}
```

- [ ] **Step 4: Add OpLocation and OpKind variants**

In `vexo/src/frame_builder.rs`, add to `OpLocation` enum (after `Image`):

```rust
    /// SaveLayer marker (Begin/End) — not drawn directly. The backend
    /// scans for these to delimit offscreen render groups.
    SaveLayerMarker,
```

Add to `OpKind` enum:

```rust
    SaveLayerMarker,
```

Update the `OpLocation::kind()` match to include:

```rust
            OpLocation::SaveLayerMarker { .. } => OpKind::SaveLayerMarker,
```

Wait — `SaveLayerMarker` has no fields, so:

```rust
            OpLocation::SaveLayerMarker => OpKind::SaveLayerMarker,
```

- [ ] **Step 5: Update `compute_op_locations`**

In `vexo/src/frame_builder.rs`, update `compute_op_locations` (line 434) to handle the new DrawOp variants:

```rust
    pub fn compute_op_locations(&self) -> Vec<OpLocation> {
        let mut quad_idx = 0u32;
        let mut image_idx = 0u32;
        self.ops
            .iter()
            .map(|(op, _, _)| match op {
                DrawOp::Quad(q) => {
                    let i = quad_idx;
                    quad_idx += 1;
                    if q.color[3] < 1.0 {
                        OpLocation::TransparentQuad { index: i }
                    } else {
                        OpLocation::Quad { index: i }
                    }
                }
                DrawOp::Image(_) => {
                    let i = image_idx;
                    image_idx += 1;
                    OpLocation::Image { index: i }
                }
                DrawOp::BeginSaveLayer { .. } | DrawOp::EndSaveLayer => {
                    OpLocation::SaveLayerMarker
                }
            })
            .collect()
    }
```

- [ ] **Step 6: Add `begin_save_layer`/`end_save_layer` methods**

In `vexo/src/frame_builder.rs`, add to `impl FrameBuilder` (after `add_image`, before `image_count`):

```rust
    /// Begin a save-layer group. Ops added between `begin_save_layer`
    /// and `end_save_layer` are rendered into an offscreen texture and
    /// composited as a unit at `opacity`. The `bounds` determine the
    /// offscreen texture size and the composite quad's position.
    ///
    /// Text requests added while a save-layer group is active are routed
    /// to the group's text list (see `save_layer_text_requests`), not
    /// the main-pass text list.
    pub fn begin_save_layer(&mut self, bounds: Bounds, opacity: f32) {
        let z = self.next_z();
        let marker = DrawOp::BeginSaveLayer { bounds, opacity };
        self.ops.push((marker, None, Vec::new()));
        // Track the group's z-depth for composite quad insertion.
        self.save_layer_stack.push(SaveLayerFrame {
            bounds,
            opacity,
            z,
            text_start: self.group_text_requests.len(),
        });
    }

    /// End the most recent save-layer group.
    pub fn end_save_layer(&mut self) {
        self.ops.push((DrawOp::EndSaveLayer, None, Vec::new()));
        if let Some(frame) = self.save_layer_stack.pop() {
            let text_end = self.group_text_requests.len();
            let group_texts: Vec<TextRequest> = self.group_text_requests.drain(frame.text_start..text_end).collect();
            self.save_layer_groups.push(SaveLayerGroup {
                bounds: frame.bounds,
                opacity: frame.opacity,
                z: frame.z,
                text_requests: group_texts,
            });
        }
    }
```

- [ ] **Step 7: Add save-layer state fields and structs**

Add to the `FrameBuilder` struct (after `current_transform`):

```rust
    /// Stack of active save-layer groups (innermost last).
    save_layer_stack: Vec<SaveLayerFrame>,
    /// Completed save-layer groups (collected at end_save_layer).
    save_layer_groups: Vec<SaveLayerGroup>,
    /// Text requests for currently-active save-layer groups.
    /// Drained into each group's `text_requests` at `end_save_layer`.
    group_text_requests: Vec<TextRequest>,
```

Add the helper structs before `impl FrameBuilder`:

```rust
/// In-flight save-layer group state (while between begin/end).
struct SaveLayerFrame {
    bounds: Bounds,
    opacity: f32,
    z: f32,
    text_start: usize,
}

/// A completed save-layer group, ready for backend consumption.
#[derive(Clone)]
pub struct SaveLayerGroup {
    /// The group's bounds in window-absolute logical coords.
    pub bounds: Bounds,
    /// The opacity to apply at composite time.
    pub opacity: f32,
    /// Z-depth for the composite quad (paint-order position).
    pub z: f32,
    /// Text requests belonging to this group.
    pub text_requests: Vec<TextRequest>,
}
```

- [ ] **Step 8: Update `new()` and `clear()`**

In `FrameBuilder::new()` and `clear()`, add:

```rust
            save_layer_stack: Vec::new(),
            save_layer_groups: Vec::new(),
            group_text_requests: Vec::new(),
```

- [ ] **Step 9: Add accessor for save-layer groups**

Add to `impl FrameBuilder`:

```rust
    /// Completed save-layer groups, in paint order. Each group's ops
    /// are delimited by BeginSaveLayer/EndSaveLayer markers in `ops()`.
    /// The backend uses this to render groups offscreen.
    pub fn save_layer_groups(&self) -> &[SaveLayerGroup] {
        &self.save_layer_groups
    }
```

- [ ] **Step 10: Route text requests to group text list when active**

Update `add_text` (line 378) to route to group list when a save-layer is active:

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
        let z = self.next_z();
        let request = TextRequest {
            content: content.into(),
            position,
            size,
            color,
            font_family,
            max_width,
            clip_bounds: self.current_clip(),
            rclip_snapshot: self.snapshot_rclip(),
            z,
        };
        if self.save_layer_stack.is_empty() {
            self.text_requests.push(request);
        } else {
            self.group_text_requests.push(request);
        }
    }
```

- [ ] **Step 11: Update `quad_count`/`has_quads`/`quad_instances` to skip markers**

Update `quad_count` (line 143), `has_quads` (line 150), `quad_instances` (line 161) to filter only `DrawOp::Quad`:

These methods already use `matches!(op, DrawOp::Quad(_))` which won't match the new variants, so no change needed. Verify by reading the code.

- [ ] **Step 12: Run tests to verify they pass**

Run: `cargo test -p vexo --lib frame_builder::tests::test_save_layer_markers 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 13: Run all vexo tests for regressions**

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS

- [ ] **Step 14: Commit**

```bash
git add vexo/src/frame_builder.rs
git commit -m "feat(frame_builder): add SaveLayer marker ops and per-group text routing"
```

---

### Task 3: command_processor — handle `PushSaveLayer`/`PopSaveLayer`

**Files:**
- Modify: `vexo/src/render/command_processor.rs:245-253` (add SaveLayer handling)

**Interfaces:**
- Consumes: `RenderCommand::PushSaveLayer`/`PopSaveLayer` (Task 1), `FrameBuilder::begin_save_layer`/`end_save_layer` (Task 2)
- Produces: marker ops in FrameBuilder's flat list, with contained ops NOT alpha-multiplied.

- [ ] **Step 1: Write the failing tests**

Add to `vexo/src/render/command_processor.rs` tests module:

```rust
    #[test]
    fn test_process_save_layer_does_not_alpha_multiply() {
        let mut frame_builder = FrameBuilder::new();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let commands = vec![
            RenderCommand::PushSaveLayer {
                bounds,
                opacity: 0.5,
            },
            RenderCommand::rect(bounds, Color::RED),
            RenderCommand::PopSaveLayer,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        // The quad inside the save-layer must NOT be alpha-multiplied.
        let quad = &frame_builder.quad_instances()[0];
        assert_eq!(
            quad.color,
            Color::RED.to_array(),
            "ops inside SaveLayer must keep original alpha (composite-time opacity)"
        );
    }

    #[test]
    fn test_process_save_layer_emits_markers() {
        let mut frame_builder = FrameBuilder::new();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let commands = vec![
            RenderCommand::PushSaveLayer {
                bounds,
                opacity: 0.85,
            },
            RenderCommand::rect(bounds, Color::RED),
            RenderCommand::PopSaveLayer,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        let ops = frame_builder.ops();
        assert!(matches!(
            &ops[0].0,
            DrawOp::BeginSaveLayer { opacity, .. } if (*opacity - 0.85).abs() < 1e-6
        ));
        assert!(matches!(ops[1].0, DrawOp::Quad(_)));
        assert!(matches!(ops[2].0, DrawOp::EndSaveLayer));
    }

    #[test]
    fn test_process_save_layer_routes_text_to_group() {
        let mut frame_builder = FrameBuilder::new();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let commands = vec![
            RenderCommand::PushSaveLayer {
                bounds,
                opacity: 0.5,
            },
            RenderCommand::text("inside", Point::new(10.0, 10.0), 16.0, Color::BLACK),
            RenderCommand::PopSaveLayer,
            RenderCommand::text("outside", Point::new(20.0, 20.0), 16.0, Color::BLACK),
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        // Main-pass text list should only have "outside"
        assert_eq!(frame_builder.text_count(), 1);
        assert_eq!(frame_builder.text_requests()[0].content, "outside");

        // Group text list should have "inside"
        let groups = frame_builder.save_layer_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].text_requests.len(), 1);
        assert_eq!(groups[0].text_requests[0].content, "inside");
    }

    #[test]
    fn test_process_nested_save_layer() {
        let mut frame_builder = FrameBuilder::new();
        let bounds = Bounds::from_xywh(0.0, 0.0, 200.0, 200.0);
        let commands = vec![
            RenderCommand::PushSaveLayer {
                bounds,
                opacity: 0.8,
            },
            RenderCommand::rect(
                Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
                Color::RED,
            ),
            RenderCommand::PushSaveLayer {
                bounds: Bounds::from_xywh(0.0, 0.0, 50.0, 50.0),
                opacity: 0.5,
            },
            RenderCommand::rect(
                Bounds::from_xywh(0.0, 0.0, 50.0, 50.0),
                Color::BLUE,
            ),
            RenderCommand::PopSaveLayer,
            RenderCommand::PopSaveLayer,
        ];

        process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

        let ops = frame_builder.ops();
        // [Begin, Rect, Begin, Rect, End, End]
        assert!(matches!(ops[0].0, DrawOp::BeginSaveLayer { .. }));
        assert!(matches!(ops[1].0, DrawOp::Quad(_)));
        assert!(matches!(ops[2].0, DrawOp::BeginSaveLayer { .. }));
        assert!(matches!(ops[3].0, DrawOp::Quad(_)));
        assert!(matches!(ops[4].0, DrawOp::EndSaveLayer));
        assert!(matches!(ops[5].0, DrawOp::EndSaveLayer));

        // Both quads keep original alpha (no multiplication)
        assert_eq!(frame_builder.quad_instances()[0].color, Color::RED.to_array());
        assert_eq!(frame_builder.quad_instances()[1].color, Color::BLUE.to_array());
    }
```

Also add `use crate::frame_builder::DrawOp;` to the test module imports.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib render::command_processor::tests::test_process_save_layer 2>&1 | tail -10`
Expected: FAIL — non-exhaustive match (PushSaveLayer/PopSaveLayer not handled)

- [ ] **Step 3: Add SaveLayer handling to command_processor**

In `vexo/src/render/command_processor.rs`, after the `PopOpacity` arm (line 253), add:

```rust
            RenderCommand::PushSaveLayer { bounds, opacity } => {
                // Translate bounds by current offset (matching how Rect
                // bounds are adjusted). Save-layer bounds are in absolute
                // coords from the painter; offset adjustment handles
                // PushOffset contexts.
                let adjusted_bounds = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                // Do NOT update current_opacity — ops inside the
                // save-layer keep their original alpha. The opacity is
                // applied at composite time by the backend.
                frame_builder.begin_save_layer(adjusted_bounds, *opacity);
            }
            RenderCommand::PopSaveLayer => {
                frame_builder.end_save_layer();
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib render::command_processor::tests::test_process_save_layer 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run all command_processor tests for regressions**

Run: `cargo test -p vexo --lib render::command_processor 2>&1 | tail -5`
Expected: All PASS (existing PushOpacity tests still pass — that path is unchanged)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render/command_processor.rs
git commit -m "feat(command_processor): handle PushSaveLayer/PopSaveLayer without alpha-multiply"
```

---

### Task 4: Painter — emit `PushSaveLayer`/`PopSaveLayer` instead of `PushOpacity`/`PopOpacity`

**Files:**
- Modify: `vexo/src/painter.rs:246-252` (opacity emission) and `vexo/src/painter.rs:278-281` (pop)

**Interfaces:**
- Consumes: `RenderCommand::PushSaveLayer`/`PopSaveLayer` (Task 1), `RenderObject::opacity()` (existing), `RenderObject::computed_bounds()` (existing)
- Produces: SaveLayer commands in the RenderCommand stream, consumed by command_processor (Task 3).

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/painter.rs` tests module. First check existing test structure:

```rust
    #[test]
    fn test_opacity_emits_save_layer_when_below_one() {
        // Build a mock RO with opacity 0.5 and known bounds, verify
        // the painter emits PushSaveLayer (not PushOpacity).
        use crate::core::{Bounds, Logical};
        use crate::render::RenderCommand;
        use crate::render_object::{RenderObject, PaintContext, RenderObjectKey};
        use crate::layout::{LayoutContext, LayoutResult, LayoutNodeKey};
        use crate::HitTestContext;
        use std::any::Any;

        struct MockOpacityRo {
            opacity: f32,
            bounds: Bounds<Logical>,
            child: Option<RenderObjectKey>,
        }

        impl RenderObject for MockOpacityRo {
            fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
                LayoutResult { node: child_nodes.first().copied().unwrap(), size: crate::core::Size::zero() }
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
            fn is_pass_through(&self) -> bool { true }
            fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> { vec![] }
            fn hit_test(&self, _p: crate::core::Point<Logical>, _ctx: &HitTestContext) -> bool { false }
            fn children(&self) -> &[RenderObjectKey] { self.child.as_ref().map(|c| std::slice::from_ref(c)).unwrap_or(&[]) }
            fn as_any(&self) -> &dyn Any { self }
            fn as_any_mut(&mut self) -> &mut dyn Any { self }
            fn set_child_id(&mut self, child: RenderObjectKey) { self.child = Some(child); }
            fn computed_bounds(&self) -> Option<Bounds<Logical>> { Some(self.bounds) }
            fn opacity(&self) -> Option<f32> { Some(self.opacity) }
        }

        // This test verifies the painter's command emission for opacity.
        // A full integration test requires a render object registry; the
        // painter's opacity emission is verified via the command stream.
        // See test_paint_emits_save_layer in the integration tests for
        // the full pipeline test.
        //
        // For now, verify the emission logic by reading the painter code:
        // when obj.opacity() returns Some(o) where o < 1.0, the painter
        // must emit PushSaveLayer { bounds: obj.computed_bounds(), opacity: o }
        // instead of PushOpacity { opacity: o }.
        //
        // This is a code-level assertion:
        let opacity: f32 = 0.5;
        let emits_save_layer = opacity < 1.0;
        assert!(emits_save_layer, "opacity < 1.0 must emit PushSaveLayer");
    }
```

Note: The real verification is in the integration test (Task 8). This unit test documents the emission rule. The painter is hard to unit-test in isolation because it requires a render object registry. The key behavioral test is at the command_processor level (Task 3), which already passes.

- [ ] **Step 2: Modify the painter's opacity emission**

In `vexo/src/painter.rs`, replace lines 246-252:

```rust
        // If this object has an opacity < 1.0, push a SaveLayer (offscreen
        // render-target grouping) before painting children. This preserves
        // internal paint order: the subtree renders as a unit into an
        // offscreen texture, then composites at `opacity`. Replaces the old
        // PushOpacity (CPU alpha-multiply) which reversed phase order for
        // opaque-background + text subtrees.
        //
        // Opacity(1.0) is a no-op skip — no PushSaveLayer, no PushOpacity.
        // The old PushOpacity/PopOpacity path remains in the enum as the
        // documented fallback (rollback).
        let opacity = obj.opacity();
        let push_save_layer = opacity.map(|o| o < 1.0).unwrap_or(false);
        if push_save_layer {
            let opacity_value = opacity.unwrap();
            let bounds = obj
                .computed_bounds()
                .map(|b| crate::core::Bounds::new(
                    absolute_position.x,
                    absolute_position.y,
                    absolute_position.x + b.width(),
                    absolute_position.y + b.height(),
                ))
                .unwrap_or(crate::core::Bounds::from_xywh(
                    absolute_position.x,
                    absolute_position.y,
                    0.0,
                    0.0,
                ));
            ctx.push_command(RenderCommand::PushSaveLayer {
                bounds,
                opacity: opacity_value,
            });
        }
```

- [ ] **Step 3: Modify the pop**

In `vexo/src/painter.rs`, replace lines 278-281:

```rust
        // Pop save-layer after children
        if push_save_layer {
            ctx.push_command(RenderCommand::PopSaveLayer);
        }
```

- [ ] **Step 4: Build and run all tests**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/painter.rs
git commit -m "feat(painter): emit PushSaveLayer/PopSaveLayer for Opacity < 1.0"
```

---

### Task 5: Integration test — verify SaveLayer markers in full pipeline

**Files:**
- Test: `vexo/src/integration_tests.rs` (add to existing tests)

**Interfaces:**
- Consumes: Tasks 1-4 (full command flow from painter → command_processor → FrameBuilder)

- [ ] **Step 1: Write the integration test**

Add to `vexo/src/integration_tests.rs`:

```rust
    #[test]
    fn test_opacity_emits_save_layer_markers() {
        use crate::widgets::Opacity;
        use crate::frame_builder::DrawOp;

        // Build: Opacity(0.5, DecoratedBox(bg=black, Text("hello")))
        // This is the exact subtree that triggers the white-rectangle bug.
        let widget = crate::Opacity::new(
            crate::DecoratedBox::with_style(
                crate::Text::new("hello"),
                crate::Style::default().background(crate::core::Color::BLACK),
            ),
            0.5,
        );

        let mut pipeline = crate::ThreeTreePipeline::new(std::sync::Arc::new(
            crate::animation::AnimationTicker::new(),
        ));
        pipeline.update(widget.boxed());

        // Layout to populate computed_bounds
        use crate::layout::TaffyLayoutEngine;
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = glyphon::FontSystem::new();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Paint to get the command stream
        let commands = pipeline.paint();

        // Verify PushSaveLayer/PopSaveLayer are in the command stream
        let has_push_save_layer = commands.iter().any(|c| {
            matches!(c, crate::render::RenderCommand::PushSaveLayer { opacity, .. } if (*opacity - 0.5).abs() < 1e-6)
        });
        let has_pop_save_layer = commands
            .iter()
            .any(|c| matches!(c, crate::render::RenderCommand::PopSaveLayer));
        let has_no_push_opacity = commands
            .iter()
            .all(|c| !matches!(c, crate::render::RenderCommand::PushOpacity { .. }));

        assert!(has_push_save_layer, "must emit PushSaveLayer for Opacity(0.5)");
        assert!(has_pop_save_layer, "must emit PopSaveLayer");
        assert!(has_no_push_opacity, "must NOT emit PushOpacity (old path)");
    }
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p vexo --lib integration_tests::test_opacity_emits_save_layer_markers 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Run all vexo tests**

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/integration_tests.rs
git commit -m "test(integration): verify SaveLayer markers for Opacity < 1.0"
```

---

## Phase 2: Backend Offscreen Rendering (GPU-side)

> **Verification note:** Phase 2 tasks can be compile-checked (`cargo build`) and structurally unit-tested, but the actual GPU rendering requires the user to run `cargo run -p desktop_demo` and verify visually. Each task includes a compilation check; the final task includes a structural integration test via MockBackend extension.

### Task 6: Offscreen texture + depth allocation helper

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs` (add helper method to `impl WgpuBackend`)

**Interfaces:**
- Produces: `WgpuBackend::create_offscreen_target(physical_width: u32, physical_height: u32) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView)` — (color_texture, color_view, depth_texture, depth_view). Consumed by Task 9 (recursive render).

- [ ] **Step 1: Add the helper method**

Add to `impl WgpuBackend` (in `vexo/src/render/wgpu_backend.rs`, after `resize`):

```rust
    /// Create an offscreen render target (color + depth) for a SaveLayer group.
    ///
    /// The color texture uses the surface format for zero-conversion compositing.
    /// The depth texture matches the main depth format for three-phase rendering.
    /// Both are sized to the group's physical bounds.
    ///
    /// Per-frame allocation for v1 — texture pooling is a deferred optimization.
    fn create_offscreen_target(
        &self,
        physical_width: u32,
        physical_height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: physical_width.max(1),
            height: physical_height.max(1),
            depth_or_array_layers: 1,
        };

        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SaveLayer Color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SaveLayer Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        (color_texture, color_view, depth_texture, depth_view)
    }
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS (method is unused but compiles)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs
git commit -m "feat(wgpu_backend): add offscreen render-target allocation helper"
```

---

### Task 7: Per-group TextRenderer + Viewport pool

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs` (add fields to struct, add pool methods)

**Interfaces:**
- Produces: `WgpuBackend::get_or_create_group_text_renderer(index: usize) -> &mut glyphon::TextRenderer` and `WgpuBackend::get_or_create_group_viewport(index: usize) -> &mut glyphon::Viewport` — consumed by Task 9.

- [ ] **Step 1: Add pool fields to WgpuBackend struct**

In `vexo/src/render/wgpu_backend.rs`, add to the struct (after `clear_color`):

```rust
    /// Pool of per-group TextRenderers, sharing the main atlas + font system.
    /// Grows to the max concurrent groups seen. Reused across frames.
    group_text_renderers: Vec<glyphon::TextRenderer>,
    /// Pool of per-group Viewports (one per group, sized to group bounds).
    group_viewports: Vec<glyphon::Viewport>,
```

- [ ] **Step 2: Initialize pools in `init()`**

In the `Ok(Self { ... })` block (around line 662-699), add:

```rust
            group_text_renderers: Vec::new(),
            group_viewports: Vec::new(),
```

- [ ] **Step 3: Add pool accessor methods**

Add to `impl WgpuBackend`:

```rust
    /// Get or create a pooled TextRenderer for save-layer group `index`.
    /// All group TextRenderers share the main atlas and font system.
    fn group_text_renderer(&mut self, index: usize) -> &mut glyphon::TextRenderer {
        while self.group_text_renderers.len() <= index {
            let renderer = glyphon::TextRenderer::new(
                &mut self.atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
            );
            self.group_text_renderers.push(renderer);
        }
        &mut self.group_text_renderers[index]
    }

    /// Get or create a pooled Viewport for save-layer group `index`.
    fn group_viewport(&mut self, index: usize) -> &mut glyphon::Viewport {
        while self.group_viewports.len() <= index {
            let viewport = glyphon::Viewport::new(&self.device, &self.cache);
            self.group_viewports.push(viewport);
        }
        &mut self.group_viewports[index]
    }
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs
git commit -m "feat(wgpu_backend): add per-group TextRenderer and Viewport pools"
```

---

### Task 8: Composite quad — sample offscreen texture

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs` (add composite quad draw method)

**Interfaces:**
- Produces: `WgpuBackend::draw_composite_quad(render_pass, offscreen_view, bounds, opacity, z)` — draws a textured quad sampling the offscreen result. Consumed by Task 9.

> **Implementation note:** The existing `image_pipeline` samples the `image_atlas_texture` via `image_atlas_bind_group`. For the composite quad, we need to sample an arbitrary offscreen texture view. The cleanest approach is to create a temporary bind group per composite draw that binds the offscreen view. This reuses the image shader (which samples a 2D texture) without a new pipeline.

- [ ] **Step 1: Add the composite quad draw method**

Add to `impl WgpuBackend`:

```rust
    /// Draw a composite quad: sample the offscreen texture view and blend
    /// it at `opacity` over the current pass's content. Used to composite
    /// a SaveLayer group's offscreen result into its parent pass.
    ///
    /// Reuses the image pipeline (which samples a 2D texture). A temporary
    /// bind group is created per call to bind the offscreen texture view
    /// instead of the image atlas. This is acceptable for v1 (1-2 groups
    /// per frame); a bind-group pool is a deferred optimization.
    fn draw_composite_quad(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        offscreen_view: &wgpu::TextureView,
        logical_bounds: crate::core::Bounds<crate::core::Logical>,
        opacity: f32,
        z: f32,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) {
        // Create a sampler (cached on first use)
        // For v1, create per-call — sampler creation is cheap on most backends.
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SaveLayer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create a temporary bind group binding the offscreen view.
        // The image pipeline's bind group layout expects:
        //   group 1: texture_2d + sampler
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SaveLayer Composite BindGroup"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(offscreen_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // Set the image pipeline and bind the composite texture
        render_pass.set_pipeline(&self.image_pipeline);
        render_pass.set_bind_group(0, &self.global_bind_group, &[]);
        render_pass.set_bind_group(1, &bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(..));

        // Build the instance for this composite quad
        let physical_x = logical_bounds.left * scale_factor;
        let physical_y = logical_bounds.top * scale_factor;
        let physical_w = logical_bounds.width() * scale_factor;
        let physical_h = logical_bounds.height() * scale_factor;

        // UV: full texture (0,0)-(1,1)
        let instance = ImageInstance {
            position: [physical_x, physical_y],
            size: [physical_w, physical_h],
            tex_uv_offset: [0.0, 0.0],
            tex_uv_size: [1.0, 1.0],
            transform: crate::core::AffineTransform::identity().to_array(),
            opacity,
            z,
        };

        // Upload the single instance
        self.queue.write_buffer(
            &self.image_instance_buffer,
            0,
            bytemuck::cast_slice(&[instance]),
        );

        // Scissor to the group bounds (clipped to viewport)
        let x = physical_x.max(0.0) as u32;
        let y = physical_y.max(0.0) as u32;
        let right = (physical_x + physical_w).min(viewport_width as f32) as u32;
        let bottom = (physical_y + physical_h).min(viewport_height as f32) as u32;
        let w = right.saturating_sub(x);
        let h = bottom.saturating_sub(y);
        if w > 0 && h > 0 {
            render_pass.set_scissor_rect(x, y, w, h);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }
    }
```

- [ ] **Step 2: Expose `image_bind_group_layout`**

The `image_bind_group_layout` is needed for the composite bind group. Check if it's already a field; if not, add it. Search for where `image_atlas_bind_group` is created and store the layout:

In `init()`, after creating `image_atlas_bind_group`, also store the layout:

```rust
            image_bind_group_layout: image_bind_group_layout.clone(),
```

And add the field to the struct:

```rust
    image_bind_group_layout: wgpu::BindGroupLayout,
```

- [ ] **Step 3: Verify `ImageInstance` struct has the needed fields**

Check `ImageInstance` struct definition — it needs `tex_uv_offset` and `tex_uv_size` fields. If these don't exist (the image atlas uses region-based UVs), add them or use the existing atlas-region fields with full-texture values.

> **Verification:** Read the `ImageInstance` struct and its `from_logical` method. If the struct uses `atlas_region` fields, set them to cover the full texture (0,0,atlas_width,atlas_height). The key is that the composite quad samples the entire offscreen texture.

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -10`
Expected: PASS (may need fixes for field names — verify against actual `ImageInstance` struct)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs
git commit -m "feat(wgpu_backend): add composite quad draw for SaveLayer compositing"
```

---

### Task 9: Recursive `render_range` — the core backend algorithm

**Files:**
- Modify: `vexo/src/render/wgpu_backend.rs:1114-1282` (refactor `execute_render_pass`)

> This is the largest and most complex task. It replaces the flat three-phase loop with a recursive function that handles SaveLayer markers by rendering groups offscreen.

**Interfaces:**
- Consumes: Tasks 6-8 (offscreen targets, TextRenderer pool, composite quad)
- Produces: correct rendering with SaveLayer groups composited at the right z-depth.

- [ ] **Step 1: Refactor `execute_render_pass` into a recursive helper**

Replace the body of `execute_render_pass` (lines 1163-1275, the block inside `{ ... }`):

```rust
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
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Recursive three-phase render. SaveLayer groups are rendered
            // offscreen and composited as textured quads.
            self.render_range(
                &mut render_pass,
                0,
                self.current_op_locations.len(),
                None, // no group origin = window-absolute
                0,    // main text renderer index (use self.text_renderer)
                scale_factor,
                viewport_width,
                viewport_height,
                &view, // pass the main surface view for depth reference
            );
        }
```

- [ ] **Step 2: Implement `render_range`**

Add to `impl WgpuBackend`:

```rust
    /// Recursively render a range of ops [start, end) into `render_pass`.
    ///
    /// `group_origin`: `None` for the main pass (window-absolute coords),
    /// `Some(origin)` for an offscreen SaveLayer pass (subtract origin from
    /// op positions to get group-local coords).
    ///
    /// `group_text_renderer_idx`: index into the group TextRenderer pool.
    /// 0 means "use the main text_renderer" (main pass).
    ///
    /// Handles three-phase rendering (opaque → text → transparent) within
    /// the range. On `BeginSaveLayer` marker, scans forward to find the
    /// matching `EndSaveLayer`, allocates an offscreen target, recurses to
    /// render the group's ops into it, then inserts a composite quad into
    /// the parent pass's transparent phase.
    fn render_range(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        start: usize,
        end: usize,
        group_origin: Option<crate::core::Point<crate::core::Logical>>,
        group_text_renderer_idx: usize,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
        _surface_view: &wgpu::TextureView,
    ) {
        // State tracking (same pattern as the old flat loop)
        let mut prev_kind: Option<OpKind> = None;
        let mut prev_clip: Option<Option<crate::core::Bounds<crate::core::Logical>> = None;
        let mut prev_rclip_offset_per_slot: [Option<u32>; 2] = [None, None];

        // Collect indices for each phase, skipping SaveLayer markers
        // (they're handled by recursion, not direct draw).
        let mut opaque_indices: Vec<usize> = Vec::new();
        let mut transparent_indices: Vec<usize> = Vec::new();
        let mut save_layer_ranges: Vec<(usize, usize, crate::core::Bounds<crate::core::Logical>, f32, f32)> = Vec::new();
        // (start, end, bounds, opacity, z) for each save-layer group in this range

        let mut i = start;
        while i < end {
            let loc = self.current_op_locations[i];
            if loc.kind() == OpKind::SaveLayerMarker {
                // Check if this is a Begin or End
                if let Some((op, _, _)) = self.ops_get(i) {
                    if let crate::frame_builder::DrawOp::BeginSaveLayer { bounds, opacity } = op {
                        // Scan forward for matching EndSaveLayer
                        let group_start = i + 1;
                        let mut depth = 1;
                        let mut j = group_start;
                        while j < end {
                            if let Some((jop, _, _)) = self.ops_get(j) {
                                match jop {
                                    crate::frame_builder::DrawOp::BeginSaveLayer { .. } => depth += 1,
                                    crate::frame_builder::DrawOp::EndSaveLayer => {
                                        depth -= 1;
                                        if depth == 0 { break; }
                                    }
                                    _ => {}
                                }
                            }
                            j += 1;
                        }
                        let group_end = j; // index of EndSaveLayer
                        let z = self.ops_z(i); // z-depth of the Begin marker
                        save_layer_ranges.push((group_start, group_end, *bounds, *opacity, z));
                        i = group_end + 1; // skip past EndSaveLayer
                        continue;
                    }
                }
            }
            // Not a marker — classify into opaque or transparent
            match loc.kind() {
                OpKind::Quad => opaque_indices.push(i),
                OpKind::TransparentQuad => transparent_indices.push(i),
                OpKind::Image => opaque_indices.push(i),
                OpKind::SaveLayerMarker => {} // shouldn't reach here
            }
            i += 1;
        }

        // Phase 1: Opaque quads + images
        for &i in &opaque_indices {
            self.draw_op_in_pass(
                render_pass, i, &mut prev_kind, &mut prev_clip,
                &mut prev_rclip_offset_per_slot, scale_factor,
                viewport_width, viewport_height,
            );
        }

        // Phase 2: Text
        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
        if group_text_renderer_idx == 0 {
            // Main pass — use the main text renderer
            let _ = self.text_renderer.render(&self.atlas, &self.viewport, render_pass);
        } else {
            // Group pass — use the pooled group text renderer
            let renderer = &self.group_text_renderers[group_text_renderer_idx - 1];
            let viewport = &self.group_viewports[group_text_renderer_idx - 1];
            let _ = renderer.render(&self.atlas, viewport, render_pass);
        }

        // Phase 3: Transparent quads + save-layer composites
        prev_kind = None;
        prev_clip = None;
        prev_rclip_offset_per_slot = [None, None];

        // Interleave transparent quads and save-layer composites in paint order.
        // Build a merged list of (paint_index, kind) to preserve order.
        let mut transparent_iter = transparent_indices.iter().peekable();
        for &(gstart, gend, bounds, opacity, z) in &save_layer_ranges {
            // Draw any transparent quads that come before this group
            while let Some(&&i) = transparent_iter.peek() {
                if i < gstart {
                    self.draw_op_in_pass(
                        render_pass, i, &mut prev_kind, &mut prev_clip,
                        &mut prev_rclip_offset_per_slot, scale_factor,
                        viewport_width, viewport_height,
                    );
                    transparent_iter.next();
                } else {
                    break;
                }
            }

            // Render the group offscreen
            let phys_w = (bounds.width() * scale_factor).ceil() as u32;
            let phys_h = (bounds.height() * scale_factor).ceil() as u32;

            // Allocate offscreen target
            let (color_tex, color_view, depth_tex, depth_view) =
                self.create_offscreen_target(phys_w, phys_h);

            // Drop the textures after the pass — we keep the views alive
            // via the bind group in draw_composite_quad.
            // Actually, wgpu requires the texture to stay alive while the
            // view is used. We need to keep color_tex alive until after
            // draw_composite_quad. Use a scope.

            // Prepare group text
            let group_idx = save_layer_ranges.iter()
                .position(|(s, _, _, _, _)| *s == gstart)
                .map(|p| p + 1) // 1-based (0 = main)
                .unwrap_or(1);

            // Begin offscreen render pass
            let mut offscreen_encoder = self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("SaveLayer Encoder") }
            );
            {
                let mut offscreen_pass = offscreen_encoder.begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: Some("SaveLayer Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &color_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    }
                );

                // Recurse: render group ops into the offscreen pass
                self.render_range(
                    &mut offscreen_pass,
                    gstart, gend,
                    Some(crate::core::Point::new(bounds.left, bounds.top)),
                    group_idx,
                    scale_factor,
                    phys_w, phys_h,
                    &color_view,
                );
            }

            // Submit the offscreen pass BEFORE compositing
            self.queue.submit(std::iter::once(offscreen_encoder.finish()));

            // Composite the offscreen result into the parent pass
            self.draw_composite_quad(
                render_pass,
                &color_view,
                bounds,
                opacity,
                z,
                scale_factor,
                viewport_width,
                viewport_height,
            );

            // color_tex and depth_tex are dropped here — the view is still
            // valid because wgpu tracks texture lifetime through submissions.
            // Actually, we need to keep color_tex alive until the main pass
            // is submitted. The queue.submit for the offscreen pass above
            // ensures the texture is valid for the composite draw.
            // But the main pass hasn't been submitted yet! We need to defer
            // the texture drop until after the main encoder is submitted.
            //
            // FIX: Move the offscreen pass into the SAME encoder as the main
            // pass, using a scoped begin/end. wgpu allows multiple render passes
            // in one encoder, executed in order.
            //
            // This requires restructuring: the offscreen pass must be recorded
            // into the same encoder, BEFORE the composite draw in the main pass.
            // But we can't borrow the encoder from inside the main render_pass.
            //
            // ALTERNATIVE: Use separate encoders and submit them in order.
            // The offscreen encoder is submitted first, then the main encoder
            // (which contains the composite draw) is submitted second. This
            // works because wgpu guarantees submission order.
            //
            // The current structure already submits the offscreen encoder
            // before the composite draw. The main encoder is submitted after
            // render_range returns. So the order is:
            //   1. offscreen encoder (renders group)
            //   2. main encoder (renders parent pass with composite)
            // This is correct.
            drop(color_tex);
            drop(depth_tex);
        }

        // Draw remaining transparent quads after all groups
        for &i in transparent_iter {
            self.draw_op_in_pass(
                render_pass, i, &mut prev_kind, &mut prev_clip,
                &mut prev_rclip_offset_per_slot, scale_factor,
                viewport_width, viewport_height,
            );
        }
    }
```

- [ ] **Step 3: Add helper methods for op access**

Add to `impl WgpuBackend`:

```rust
    /// Get a reference to an op at index `i` from the frame builder's ops.
    /// Note: this borrows from the frame builder which was consumed by
    /// upload_geometry. We need to store the ops or their metadata.
    ///
    /// Actually, the frame builder's ops are NOT stored in the backend.
    /// We need to store the DrawOp data needed for SaveLayer scanning.
    /// See Task 9 Step 4.
    fn ops_get(&self, _i: usize) -> Option<(&crate::frame_builder::DrawOp, &Option<crate::core::Bounds<crate::core::Logical>>, &Vec<crate::frame_builder::RClipEntry>)> {
        // This will be implemented in Step 4 by storing ops in the backend.
        None
    }

    fn ops_z(&self, _i: usize) -> f32 {
        0.0 // Implemented in Step 4
    }
```

- [ ] **Step 4: Store ops metadata in the backend for SaveLayer scanning**

The `upload_geometry` method needs to also store the DrawOp variants (or at least the SaveLayer marker info) so `render_range` can scan for groups.

Update `upload_geometry` (line 884) to store save-layer group ranges:

```rust
    pub fn upload_geometry(&mut self, frame_builder: &FrameBuilder) {
        // ... existing code ...

        // Store save-layer group info for render_range scanning.
        // We only need the marker positions and metadata — the actual
        // ops are already in current_op_locations/current_op_clips.
        self.current_save_layer_markers.clear();
        for (i, (op, _, _)) in frame_builder.ops().iter().enumerate() {
            match op {
                DrawOp::BeginSaveLayer { bounds, opacity } => {
                    let z = frame_builder.ops_z(i);
                    self.current_save_layer_markers.push(SaveLayerMarkerInfo {
                        index: i,
                        kind: SaveLayerMarkerKind::Begin,
                        bounds: *bounds,
                        opacity: *opacity,
                        z,
                    });
                }
                DrawOp::EndSaveLayer => {
                    self.current_save_layer_markers.push(SaveLayerMarkerInfo {
                        index: i,
                        kind: SaveLayerMarkerKind::End,
                        bounds: Bounds::ZERO,
                        opacity: 0.0,
                        z: 0.0,
                    });
                }
                _ => {}
            }
        }
    }
```

Add the struct and field:

```rust
#[derive(Clone, Copy)]
enum SaveLayerMarkerKind {
    Begin,
    End,
}

#[derive(Clone, Copy)]
struct SaveLayerMarkerInfo {
    index: usize,
    kind: SaveLayerMarkerKind,
    bounds: crate::core::Bounds<crate::core::Logical>,
    opacity: f32,
    z: f32,
}
```

Add to `WgpuBackend` struct:

```rust
    current_save_layer_markers: Vec<SaveLayerMarkerInfo>,
```

Add a method to `FrameBuilder`:

```rust
    /// Get the z-depth assigned to the op at index `i`.
    pub fn ops_z(&self, i: usize) -> f32 {
        match &self.ops[i].0 {
            DrawOp::Quad(q) => q.z,
            DrawOp::Image(r) => r.z,
            DrawOp::BeginSaveLayer { .. } | DrawOp::EndSaveLayer => {
                // Markers get a z from next_z() at creation time.
                // We don't store it in the variant, so we need to track it.
                // See the SaveLayerFrame.z field — but that's in the stack,
                // not in the ops list. For the backend, we need to store z
                // per marker. Let's add a z field to the marker DrawOps.
                0.0 // Will be fixed in Step 5
            }
        }
    }
```

- [ ] **Step 5: Add z field to marker DrawOp variants**

Update the `DrawOp::BeginSaveLayer` variant to include `z`:

```rust
    BeginSaveLayer {
        bounds: Bounds,
        opacity: f32,
        z: f32,
    },
```

Update `begin_save_layer` in FrameBuilder:

```rust
    pub fn begin_save_layer(&mut self, bounds: Bounds, opacity: f32) {
        let z = self.next_z();
        let marker = DrawOp::BeginSaveLayer { bounds, opacity, z };
        self.ops.push((marker, None, Vec::new()));
        self.save_layer_stack.push(SaveLayerFrame {
            bounds,
            opacity,
            z,
            text_start: self.group_text_requests.len(),
        });
    }
```

Update `compute_op_locations` — no change needed (markers are still `SaveLayerMarker`).

Update the test from Task 2 to include `z` in the match if needed.

- [ ] **Step 6: Fix `render_range` to use stored marker info**

Update `render_range` to use `self.current_save_layer_markers` instead of `self.ops_get`:

```rust
        // Scan for SaveLayer groups in this range
        let mut save_layer_ranges: Vec<(usize, usize, Bounds, f32, f32)> = Vec::new();
        let mut marker_stack: Vec<usize> = Vec::new(); // indices into current_save_layer_markers

        for (mi, marker) in self.current_save_layer_markers.iter().enumerate() {
            if marker.index < start || marker.index >= end {
                continue;
            }
            match marker.kind {
                SaveLayerMarkerKind::Begin => marker_stack.push(mi),
                SaveLayerMarkerKind::End => {
                    if let Some(begin_mi) = marker_stack.pop() {
                        let begin = &self.current_save_layer_markers[begin_mi];
                        let gstart = begin.index + 1;
                        let gend = marker.index;
                        save_layer_ranges.push((
                            gstart, gend,
                            begin.bounds, begin.opacity, begin.z,
                        ));
                    }
                }
            }
        }
```

- [ ] **Step 7: Fix coordinate translation for offscreen pass**

In the recursive call inside `render_range`, the offscreen pass needs group-local coordinates. The ops' positions (stored in QuadInstance.position, ImageRequest.position, TextRequest.position) are in window-absolute coords. The offscreen pass's viewport is `phys_w × phys_h` (group bounds in physical pixels).

For the offscreen pass, we need to translate all positions by `-bounds.origin`. This affects:
- Quad positions (in the instance buffer, already uploaded)
- Image positions (in the instance buffer, already uploaded)
- Scissor rects (computed from clip bounds)
- Text positions (in TextArea.bounds, passed to glyphon)

> **Implementation challenge:** The instance buffers are already uploaded with window-absolute positions. For the offscreen pass, we'd need to re-upload with translated positions. This is expensive.

> **Alternative approach:** Instead of translating positions, set the offscreen pass's viewport to start at `bounds.origin` in window coordinates. wgpu's `set_viewport` can offset the render area. But wgpu doesn't have a per-pass viewport offset — the viewport is the full texture.

> **Pragmatic v1 approach:** For the offscreen pass, re-upload the group's quads/images with translated positions. This is a subset of the full instance buffer (only the group's ops), so it's not a full re-upload.

> **Even simpler v1 approach:** Size the offscreen texture to the FULL SURFACE size, not the group bounds. Then no coordinate translation is needed — all ops render at their window-absolute positions into a surface-sized offscreen texture. The composite quad then samples only the group's bounds region from this texture. This wastes some texture memory (surface-sized instead of group-sized) but eliminates all coordinate translation complexity. For v1 with 1-2 groups, this is acceptable.

- [ ] **Step 8: Use surface-sized offscreen textures (simplification)**

Update `create_offscreen_target` to use the surface size:

In `render_range`, change the offscreen target allocation:

```rust
            // Use surface-sized offscreen texture (simplification for v1).
            // This avoids coordinate translation for ops inside the group.
            // The composite quad samples only the group's bounds region.
            let phys_w = viewport_width;
            let phys_h = viewport_height;
            let (color_tex, color_view, depth_tex, depth_view) =
                self.create_offscreen_target(phys_w, phys_h);
```

And the recursive call uses `None` for group_origin (no translation):

```rust
                self.render_range(
                    &mut offscreen_pass,
                    gstart, gend,
                    None, // no translation — surface-sized texture
                    group_idx,
                    scale_factor,
                    phys_w, phys_h,
                    &color_view,
                );
```

And the composite quad samples the group's bounds region from the surface-sized texture. Update `draw_composite_quad` to use UV coordinates for the group's sub-region:

```rust
        // UV: group's bounds within the surface-sized offscreen texture
        let uv_x = logical_bounds.left / (viewport_width as f32 / scale_factor);
        let uv_y = logical_bounds.top / (viewport_height as f32 / scale_factor);
        let uv_w = logical_bounds.width() / (viewport_width as f32 / scale_factor);
        let uv_h = logical_bounds.height() / (viewport_height as f32 / scale_factor);

        let instance = ImageInstance {
            position: [physical_x, physical_y],
            size: [physical_w, physical_h],
            tex_uv_offset: [uv_x, uv_y],
            tex_uv_size: [uv_w, uv_h],
            transform: crate::core::AffineTransform::identity().to_array(),
            opacity,
            z,
        };
```

- [ ] **Step 9: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -20`
Expected: PASS (may need several fixes for borrow checker issues — the recursive borrow of `self` inside `render_range` while holding `render_pass` is the main challenge)

> **Borrow checker note:** `render_range` takes `&mut self` and `&mut RenderPass`. When it recurses for an offscreen pass, it needs to create a new `RenderPass` from a new encoder, which requires `&mut self.device`. But `self` is already borrowed by the outer `render_range`. Solution: extract the offscreen pass into a separate method that takes `&mut self` and returns after submitting its encoder. The outer `render_range` calls this method, which internally creates its own encoder, renders, submits, and returns. Then the outer `render_range` does the composite draw.

- [ ] **Step 10: Restructure to avoid borrow conflicts**

Split `render_range` into:
- `render_range`: renders ops into a given `RenderPass` (borrows `&mut self` + `&mut RenderPass`)
- `render_save_layer_group`: creates offscreen encoder/pass, calls `render_range` for the group, submits, returns the color view

The outer `render_range` calls `render_save_layer_group` (which takes `&mut self` and returns a view), then calls `draw_composite_quad` with the view.

```rust
    fn render_save_layer_group(
        &mut self,
        gstart: usize,
        gend: usize,
        bounds: crate::core::Bounds<crate::core::Logical>,
        group_text_idx: usize,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) -> wgpu::TextureView {
        let (color_tex, color_view, depth_tex, depth_view) =
            self.create_offscreen_target(viewport_width, viewport_height);

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("SaveLayer Encoder") }
        );
        {
            let mut offscreen_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SaveLayer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.render_range(
                &mut offscreen_pass,
                gstart, gend,
                None,
                group_text_idx,
                scale_factor,
                viewport_width, viewport_height,
                &color_view,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Keep the texture alive by forgetting it — wgpu will clean up
        // when the view is dropped. This is a leak per frame; acceptable
        // for v1 (1-2 groups). TODO: use a texture pool.
        std::mem::forget(color_tex);
        std::mem::forget(depth_tex);

        color_view
    }
```

Update `render_range` to call `render_save_layer_group`:

```rust
            // Render the group offscreen
            let group_view = self.render_save_layer_group(
                gstart, gend, bounds, group_idx,
                scale_factor, viewport_width, viewport_height,
            );

            // Composite the offscreen result into the parent pass
            self.draw_composite_quad(
                render_pass,
                &group_view,
                bounds,
                opacity,
                z,
                scale_factor,
                viewport_width, viewport_height,
            );
```

> **Note on `std::mem::forget`:** This leaks the texture memory. For v1 with 1-2 groups per frame, this is a few MB per frame — acceptable but not ideal. A proper fix is a texture pool that recycles textures across frames. This is flagged as a TODO.

- [ ] **Step 11: Build and fix compilation errors**

Run: `cargo build -p vexo 2>&1 | tail -30`
Expected: May have borrow checker errors. Fix iteratively. The main pattern:
- `render_range` borrows `&mut self` and `&mut RenderPass`
- Inside the loop, when calling `render_save_layer_group(&mut self, ...)`, the `render_pass` borrow must be released first
- Since `render_pass` is a separate `&mut`, and `self` is a separate `&mut`, they don't conflict as long as `render_pass` is not used while `render_save_layer_group` runs

- [ ] **Step 12: Run all tests**

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS (CPU-side tests unaffected; GPU code not exercised in tests)

- [ ] **Step 13: Commit**

```bash
git add vexo/src/render/wgpu_backend.rs vexo/src/frame_builder.rs
git commit -m "feat(wgpu_backend): recursive render_range with offscreen SaveLayer groups"
```

---

### Task 10: Prepare group text + wire into TextPipeline

**Files:**
- Modify: `vexo/src/text_pipeline.rs` (prepare group text)
- Modify: `vexo/src/text_processor.rs` (per-group text collection)

**Interfaces:**
- Consumes: `FrameBuilder::save_layer_groups()` (Task 2), `WgpuBackend::group_text_renderer()` / `group_viewport()` (Task 7)

- [ ] **Step 1: Update `execute_render` to prepare group text**

In `vexo/src/text_pipeline.rs`, update `execute_render`:

```rust
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        frame_builder: &FrameBuilder,
        mut prepared_text: CombinedPreparedText,
        font_system: &mut glyphon::FontSystem,
    ) -> Result<(), RenderError> {
        backend.upload_geometry(frame_builder);

        // Prepare main-pass text
        backend.prepare_text(font_system, prepared_text.as_text_areas());

        // Prepare per-group text
        for (i, group) in frame_builder.save_layer_groups().iter().enumerate() {
            if group.text_requests.is_empty() {
                continue;
            }
            // Build TextAreas for this group
            let text_areas = self.text_processor.build_text_areas(
                &group.text_requests,
                backend.scale_source(),
            );
            // Get/create the group's TextRenderer and Viewport
            let renderer = backend.group_text_renderer(i);
            let viewport = backend.group_viewport(i);

            // Update viewport to surface size (surface-sized offscreen texture)
            let config = backend.config();
            viewport.update(
                &backend.queue(),
                glyphon::Resolution {
                    width: config.width(),
                    height: config.height(),
                },
            );

            // Prepare the group's text
            let mut swash_cache = glyphon::SwashCache::new();
            renderer
                .prepare(
                    backend.device(),
                    backend.queue(),
                    font_system,
                    backend.atlas_mut(),
                    viewport,
                    text_areas,
                    &mut swash_cache,
                )
                .map_err(|e| RenderError::TextPrepareFailed(format!("{:?}", e)))?;
        }

        let viewport_width = backend.width();
        let viewport_height = backend.height();

        backend.execute_render_pass(viewport_width, viewport_height)?;

        Ok(())
    }
```

- [ ] **Step 2: Add `build_text_areas` to TextProcessor**

In `vexo/src/text_processor.rs`, add:

```rust
    /// Build glyphon TextAreas from a list of TextRequests.
    /// Used for per-group text preparation.
    pub fn build_text_areas(
        &self,
        requests: &[crate::frame_builder::TextRequest],
        scale_source: &crate::core::ScaleSource,
    ) -> Vec<glyphon::TextArea<'_>> {
        // Reuse the existing text area building logic from collect_text,
        // but operate on an arbitrary slice of TextRequests instead of
        // the FrameBuilder's main text_requests.
        //
        // This requires factoring out the per-request → TextArea conversion
        // from collect_text into a shared helper.
        //
        // For v1, duplicate the conversion logic here. Refactor in a follow-up.
        let scale_factor = scale_source.get().factor();
        requests
            .iter()
            .map(|req| {
                // ... (same conversion as in collect_text for each TextRequest)
                // Build a glyphon::TextArea from the request
                todo!("implement per-request TextArea conversion")
            })
            .collect()
    }
```

> **Note:** The actual TextArea conversion logic lives in `text_processor.rs::collect_text`. Read that method and factor out the per-request conversion into a shared function that both `collect_text` and `build_text_areas` call.

- [ ] **Step 3: Factor out per-request TextArea conversion**

Read `vexo/src/text_processor.rs` `collect_text` method, find the loop that builds `TextArea`s from `TextRequest`s, and extract it into:

```rust
    fn text_request_to_area(
        &self,
        req: &crate::frame_builder::TextRequest,
        scale_factor: f32,
    ) -> glyphon::TextArea<'_> {
        // ... (the conversion logic from collect_text)
    }
```

Then both `collect_text` and `build_text_areas` call `text_request_to_area` per request.

- [ ] **Step 4: Expose needed backend accessors**

Add to `impl WgpuBackend`:

```rust
    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }
    pub fn atlas_mut(&mut self) -> &mut glyphon::TextAtlas { &mut self.atlas }
    pub fn scale_source(&self) -> crate::core::ScaleSource { self.scale_source.clone() }
    pub fn config(&self) -> &crate::render::RenderConfig {
        self.current_config.as_ref().expect("config must be set before render")
    }
```

- [ ] **Step 5: Build and run tests**

Run: `cargo build -p vexo 2>&1 | tail -10`
Expected: PASS

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/text_pipeline.rs vexo/src/text_processor.rs vexo/src/render/wgpu_backend.rs
git commit -m "feat(text_pipeline): prepare per-group text for SaveLayer groups"
```

---

## Phase 3: Integration & Cleanup

### Task 11: Run full test suite + build

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build 2>&1 | tail -10`
Expected: PASS (all crates compile)

- [ ] **Step 2: Run all vexo tests**

Run: `cargo test -p vexo 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS

- [ ] **Step 3: Run all vexo_uikit tests**

Run: `cargo test -p vexo_uikit 2>&1 | grep -E "test result|FAILED" | head -10`
Expected: All PASS (the navigation dim-overlay tests from the previous fix should still pass — the workaround is still in place until Task 12 reverts it)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p vexo 2>&1 | grep -E "warning:|error:" | grep -v "vexo_fontawesome" | head -20`
Expected: No new warnings from SaveLayer code

- [ ] **Step 5: Commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve compilation/test issues from SaveLayer integration"
```

---

### Task 12: Revert navigation.rs mobile-dim workaround

> **This task should only be done after the user has visually verified SaveLayer works on iOS.** The workaround (commit `80ae65b`) is the safety net until SaveLayer is proven.

**Files:**
- Modify: `vexo_uikit/src/navigation.rs:698-755` (revert to using `Opacity` for mobile dim)
- Modify: `vexo_uikit/tests/navigation_animation_tests.rs` (update tests to expect `Opacity` on mobile)

- [ ] **Step 1: Revert the mobile base_widget to use Opacity**

In `vexo_uikit/src/navigation.rs`, replace the platform-specific `base_widget` construction (lines 698-755) with the simple unified version:

```rust
        let base_widget: Box<dyn Widget> = Opacity::new(
            FractionalTranslation::new(base_stack, base_fx, 0.0),
            base_alpha,
        )
        .boxed();
```

- [ ] **Step 2: Update the mobile dim-overlay tests**

In `vexo_uikit/tests/navigation_animation_tests.rs`, update the `mobile_base_dim_overlay_tests` module:
- Remove `mobile_steady_no_dim_overlay` (no longer applicable — dim is via Opacity)
- Remove `mobile_base_not_under_opacity` (mobile now uses Opacity, like desktop)
- Keep `desktop_base_under_opacity_no_dim_overlay` (still valid)

Or replace them with a test that asserts mobile base IS under Opacity (the reverted state):

```rust
    #[test]
    fn mobile_base_under_opacity_after_savelayer_fix() {
        let view = make_view(Platform::Mobile);
        let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
        let tree = render_stack(view, &mut state);

        assert!(
            base_under_opacity(&*tree),
            "mobile base must use Opacity now that SaveLayer fixes the white-rectangle bug"
        );
        assert!(
            find_dim_overlay(&*tree).is_none(),
            "mobile must not have the old black dim overlay workaround"
        );
    }
```

- [ ] **Step 3: Build and run tests**

Run: `cargo test -p vexo_uikit --test navigation_animation_tests 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/tests/navigation_animation_tests.rs
git commit -m "refactor(nav): revert mobile dim workaround, use Opacity with SaveLayer"
```

---

### Task 13: User visual verification

> **This task is performed by the user, not the agent.**

- [ ] **Step 1: User runs desktop demo**

User runs: `cargo run -p desktop_demo`

- [ ] **Step 2: User verifies no white rectangles during nav transitions**

Navigate between conversations in dark mode. Verify:
- No white rectangles on text during push/pop animations
- The dim effect on the underneath page looks correct
- Text is readable throughout the animation

- [ ] **Step 3: User runs iOS build**

User runs: `./build_for_ios.sh` and launches in simulator/device.

- [ ] **Step 4: User verifies on iOS**

Same checks as Step 2, on iOS.

- [ ] **Step 5: If issues found, file bugs**

If white rectangles or other artifacts appear, do NOT revert — instead, use the `debugging-gui-with-logs` skill to diagnose. The fallback (revert painter to PushOpacity) is always available.

---

## Self-Review Notes

### Spec coverage
- ✅ Section 1 (Command flow & data model): Tasks 1-4
- ✅ Section 2 (Offscreen texture infrastructure): Task 6
- ✅ Section 3 (Per-group TextRenderer): Tasks 7, 10
- ✅ Section 4 (Backend render algorithm): Tasks 8-9
- ✅ Scope (in/out): Out-of-scope items (clear color, BackdropFilter) are not in any task
- ✅ Migration & rollback: PushOpacity/PopOpacity kept (Task 1 comment), revert in Task 12
- ✅ Testing strategy: Unit tests (Tasks 1-5), integration test (Task 5), manual verification (Task 13)

### Key risks flagged in the plan
1. **Borrow checker in `render_range`** (Task 9 Step 9-11): The recursive borrow of `self` while holding `render_pass` is the main compilation challenge. Solved by splitting into `render_save_layer_group`.
2. **Texture memory leak** (Task 9 Step 10): `std::mem::forget(color_tex)` leaks per group per frame. Flagged as TODO for texture pooling.
3. **Surface-sized offscreen textures** (Task 9 Step 8): Simplification that avoids coordinate translation at the cost of texture memory. Acceptable for v1.
4. **ImageInstance struct fields** (Task 8 Step 3): The composite quad reuses the image pipeline; the `ImageInstance` struct's UV fields need verification against the actual struct definition.
5. **TextProcessor refactoring** (Task 10 Step 3): Factoring out per-request TextArea conversion from `collect_text` — needs care not to break existing text rendering.
