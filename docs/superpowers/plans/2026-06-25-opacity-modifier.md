# Opacity Modifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `.opacity()` modifier to Vexo's Widget trait that makes an entire subtree semi-transparent via CPU-side alpha multiplication.

**Architecture:** Dedicated `Opacity` widget wraps a child and stores an `f32` opacity value. A new `OpacityRenderObject` returns the opacity via a new `RenderObject::opacity()` method. The Painter emits `PushOpacity`/`PopOpacity` commands. The CommandProcessor maintains an opacity stack and multiplies alpha into Rect/Text/Caret colors; for Image, opacity passes through to the GPU shader via a new field on `ImageInstance`.

**Tech Stack:** Rust, wgpu, glyphon, Taffy

## Global Constraints

- Opacity values clamped to `[0.0, 1.0]` at widget construction time
- Zero opacity widgets are still laid out and hit-testable
- Opacity does NOT affect hit testing
- Nested opacity values multiply through the stack
- Alpha multiplication for Rect/Text/Caret happens in CommandProcessor (colors pre-multiplied before reaching FrameBuilder)
- Image opacity passes through to the GPU shader (no CPU color to multiply)
- Follow the existing Transform widget/element/render-object pattern exactly

**Dependency order:** Tasks 1 and 2 are independent. Task 4 must complete before Task 3 (CommandProcessor references `ImageRequest.opacity`). Task 5 depends on Task 4. Task 6 depends on Tasks 1 and 2. Tasks 7, 8, 9 are sequential (each depends on the prior). Task 10 depends on all. Recommended order: 1 → 2 → 4 → 5 → 3 → 6 → 7 → 8 → 9 → 10 → 11.

---

### Task 1: Add `opacity()` method to RenderObject trait

**Files:**
- Modify: `vexo/src/render_object.rs:329-337` (after `scroll_offset()`)

**Interfaces:**
- Produces: `RenderObject::opacity(&self) -> Option<f32>` — default returns `None`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/render_object.rs, inside mod tests
#[test]
fn test_render_object_opacity_default() {
    struct TestRO;
    impl RenderObject for TestRO {
        fn layout(&mut self, _ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
            unimplemented!()
        }
        fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
        fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> { vec![] }
        fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool { true }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }
    let ro = TestRO;
    assert!(ro.opacity().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_render_object_opacity_default`
Expected: FAIL with "no method named `opacity` found"

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/render_object.rs`, add after the `scroll_offset()` method (line ~337):

```rust
    /// Get the opacity for this render object, if any.
    ///
    /// When present, the painter emits `PushOpacity`/`PopOpacity` around
    /// this object's children. The opacity value (0.0..1.0) is multiplied
    /// into the alpha of all descendant colors.
    fn opacity(&self) -> Option<f32> {
        None
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_render_object_opacity_default`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render_object.rs
git commit -m "feat: add opacity() default method to RenderObject trait"
```

---

### Task 2: Add `PushOpacity`/`PopOpacity` to RenderCommand

**Files:**
- Modify: `vexo/src/render/command.rs:114-116` (after `PopTransform`)

**Interfaces:**
- Produces: `RenderCommand::PushOpacity { opacity: f32 }`, `RenderCommand::PopOpacity`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/render/command.rs, inside mod tests
#[test]
fn test_opacity_commands() {
    let cmd = RenderCommand::PushOpacity { opacity: 0.5 };
    match cmd {
        RenderCommand::PushOpacity { opacity } => assert_eq!(opacity, 0.5),
        _ => panic!("Expected PushOpacity"),
    }
    let cmd = RenderCommand::PopOpacity;
    match cmd {
        RenderCommand::PopOpacity => {},
        _ => panic!("Expected PopOpacity"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_opacity_commands`
Expected: FAIL with "no variant `PushOpacity`"

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/render/command.rs`, add after the `PopTransform` variant (line ~116):

```rust
    /// Push an opacity context onto the stack.
    /// All subsequent commands have their alpha multiplied by this value.
    PushOpacity {
        /// The opacity value (0.0 = invisible, 1.0 = fully opaque).
        opacity: f32,
    },

    /// Pop the most recent opacity context from the stack.
    PopOpacity,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_opacity_commands`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/command.rs
git commit -m "feat: add PushOpacity/PopOpacity render commands"
```

---

### Task 3: Add opacity handling to CommandProcessor

**Files:**
- Modify: `vexo/src/render/command_processor.rs`

**Interfaces:**
- Consumes: `RenderCommand::PushOpacity`, `RenderCommand::PopOpacity` from Task 2
- Produces: Opacity stack that multiplies alpha into Rect/Text/Caret/Image colors

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/render/command_processor.rs, inside mod tests
#[test]
fn test_process_rect_with_opacity() {
    let mut frame_builder = FrameBuilder::new();
    let commands = vec![
        RenderCommand::PushOpacity { opacity: 0.5 },
        RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0), Color::RED),
        RenderCommand::PopOpacity,
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert_eq!(frame_builder.quad_count(), 1);
    let quad = &frame_builder.quad_instances()[0];
    assert_eq!(quad.color, Color::RED.with_alpha(0.5).to_array());
}

#[test]
fn test_process_nested_opacity() {
    let mut frame_builder = FrameBuilder::new();
    let commands = vec![
        RenderCommand::PushOpacity { opacity: 0.5 },
        RenderCommand::PushOpacity { opacity: 0.5 },
        RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0), Color::RED),
        RenderCommand::PopOpacity,
        RenderCommand::PopOpacity,
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert_eq!(frame_builder.quad_count(), 1);
    let quad = &frame_builder.quad_instances()[0];
    // 0.5 * 0.5 = 0.25
    assert_eq!(quad.color, Color::RED.with_alpha(0.25).to_array());
}

#[test]
fn test_process_text_with_opacity() {
    let mut frame_builder = FrameBuilder::new();
    let commands = vec![
        RenderCommand::PushOpacity { opacity: 0.5 },
        RenderCommand::text("Hello", Point::new(10.0, 20.0), 16.0, Color::BLACK),
        RenderCommand::PopOpacity,
    ];

    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert_eq!(frame_builder.text_count(), 1);
    let text = &frame_builder.text_requests()[0];
    assert_eq!(text.color, Color::BLACK.with_alpha(0.5));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_process_rect_with_opacity`
Expected: FAIL with "no variant `PushOpacity`" or compilation error

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/render/command_processor.rs`, add after the `transform_stack` declaration (line ~36):

```rust
    let mut current_opacity: f32 = 1.0;
    let mut opacity_stack: Vec<f32> = Vec::new();
```

Then, in the `match cmd` block, add cases for the new commands and modify existing color handling:

For `PushOpacity`/`PopOpacity` (add before the closing `}` of the match):

```rust
            RenderCommand::PushOpacity { opacity } => {
                opacity_stack.push(current_opacity);
                current_opacity = current_opacity * opacity;
            }
            RenderCommand::PopOpacity => {
                if let Some(prev_opacity) = opacity_stack.pop() {
                    current_opacity = prev_opacity;
                }
            }
```

For `Rect` command, multiply fill and stroke color alpha by `current_opacity`:

```rust
            RenderCommand::Rect {
                bounds,
                fill,
                stroke,
                corner_radius,
            } => {
                let fill = fill.with_alpha(fill.a * current_opacity);
                let stroke = stroke.map(|s| Stroke::new(s.color.with_alpha(s.color.a * current_opacity), s.width));
                let adjusted_bounds = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                frame_builder.push_transform(current_transform);
                frame_builder.add_rect(adjusted_bounds, fill, stroke, *corner_radius);
                frame_builder.pop_transform();
            }
```

For `Text` command, multiply color alpha by `current_opacity`:

```rust
            RenderCommand::Text {
                content,
                position,
                font_size,
                color,
                max_width,
            } => {
                let color = color.with_alpha(color.a * current_opacity);
                let offset_pos = Point::new(
                    position.x + current_offset.x,
                    position.y + current_offset.y,
                );
                let final_pos = if current_transform.is_identity() {
                    offset_pos
                } else {
                    let relative = Point::new(offset_pos.x - current_origin.x, offset_pos.y - current_origin.y);
                    let transformed = current_transform.transform_point(relative);
                    Point::new(transformed.x + current_origin.x, transformed.y + current_origin.y)
                };
                frame_builder.add_text(content, final_pos, *font_size, color, *max_width);
            }
```

For `Caret` command, multiply color alpha by `current_opacity`:

```rust
            RenderCommand::Caret {
                position,
                height,
                color,
            } => {
                let color = color.with_alpha(color.a * current_opacity);
                let offset_pos: Point<Logical> = Point::new(
                    position.x + current_offset.x,
                    position.y + current_offset.y,
                );
                let final_pos = if current_transform.is_identity() {
                    offset_pos
                } else {
                    let relative = Point::new(offset_pos.x - current_origin.x, offset_pos.y - current_origin.y);
                    let transformed = current_transform.transform_point(relative);
                    Point::new(transformed.x + current_origin.x, transformed.y + current_origin.y)
                };
                let bounds = Bounds::from_xywh(final_pos.x, final_pos.y, 2.0, *height);
                frame_builder.push_transform(current_transform);
                frame_builder.add_rect(bounds, color, None, 0.0);
                frame_builder.pop_transform();
            }
```

For `Image` command, pass `current_opacity` into the image request:

```rust
            RenderCommand::Image { bounds, image_key, corner_radius } => {
                let offset_bounds: Bounds<Logical> = Bounds::new(
                    bounds.left + current_offset.x,
                    bounds.top + current_offset.y,
                    bounds.right + current_offset.x,
                    bounds.bottom + current_offset.y,
                );
                frame_builder.add_image(crate::frame_builder::ImageRequest {
                    position: [offset_bounds.left, offset_bounds.top],
                    size: [offset_bounds.width(), offset_bounds.height()],
                    image_key: *image_key,
                    corner_radius: *corner_radius,
                    transform: current_transform.to_array(),
                    opacity: current_opacity,
                });
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_process_rect_with_opacity test_process_nested_opacity test_process_text_with_opacity`
Expected: PASS (Note: ImageRequest.opacity field from Task 4 must exist first)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/command_processor.rs
git commit -m "feat: add opacity stack to CommandProcessor with alpha multiplication"
```

**Note:** This task depends on Task 2 (RenderCommand variants) and Task 4 (ImageRequest.opacity). If implementing sequentially, complete Tasks 2 and 4 first, then this task.

---

### Task 4: Add `opacity` field to ImageRequest and ImageInstance

**Files:**
- Modify: `vexo/src/frame_builder.rs:16-22` (ImageRequest struct)
- Modify: `vexo/src/image_instance.rs` (ImageInstance struct, desc, from_logical)
- Modify: `vexo/src/render/wgpu_backend.rs:680-683` (pass opacity to ImageInstance)

**Interfaces:**
- Produces: `ImageRequest { opacity: f32, ... }`, `ImageInstance { opacity: f32, ... }`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/frame_builder.rs, inside mod tests
#[test]
fn test_image_request_opacity() {
    let req = ImageRequest {
        position: [10.0, 20.0],
        size: [100.0, 50.0],
        image_key: 1,
        corner_radius: 8.0,
        transform: AffineTransform::identity().to_array(),
        opacity: 0.5,
    };
    assert_eq!(req.opacity, 0.5);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_image_request_opacity`
Expected: FAIL with "no field `opacity` on type `ImageRequest`"

- [ ] **Step 3: Write minimal implementation**

In `vexo/src/frame_builder.rs`, add `opacity` field to `ImageRequest`:

```rust
#[derive(Clone)]
pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub corner_radius: f32,
    pub transform: [f32; 6],
    pub opacity: f32,
}
```

Update all `add_image()` call sites to include `opacity: 1.0` (there is one in `command_processor.rs` for the non-opacity path — but we'll update that in Task 3). For now, update the existing test in `frame_builder.rs`:

In the existing `test_add_image_request` and `test_flatten_image_requests`, add `opacity: 1.0` to the ImageRequest constructors.

In `vexo/src/image_instance.rs`, replace one `_padding` slot with `opacity`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_origin: [f32; 2],
    pub uv_size: [f32; 2],
    pub corner_radius: f32,
    pub opacity: f32,
    pub transform: [f32; 6],
    pub _padding: [f32; 1],
}
```

Update `from_logical()` to accept and assign opacity:

```rust
    pub fn from_logical(
        pos: [f32; 2],
        size: [f32; 2],
        region: &AtlasRegion,
        atlas_size: [f32; 2],
        corner_radius: f32,
        transform: AffineTransform,
        opacity: f32,
    ) -> Self {
        Self {
            position: pos,
            size,
            uv_origin: [
                region.x as f32 / atlas_size[0],
                region.y as f32 / atlas_size[1],
            ],
            uv_size: [
                region.width as f32 / atlas_size[0],
                region.height as f32 / atlas_size[1],
            ],
            corner_radius,
            opacity,
            transform: transform.to_array(),
            _padding: [0.0],
        }
    }
```

Update `desc()` — the vertex attribute at offset 36 changes from `Float32x2` (padding) to `Float32` (opacity), and a new attribute at offset 40 for the transform:

```rust
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<ImageInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 24, shader_location: 4, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 36, shader_location: 9, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 40, shader_location: 6, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 48, shader_location: 7, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 56, shader_location: 8, format: wgpu::VertexFormat::Float32x2 },
            ],
        }
    }
```

In `vexo/src/render/wgpu_backend.rs`, update the `upload_image_geometry` call to pass opacity:

```rust
    pub fn upload_image_geometry(&mut self, frame_builder: &FrameBuilder) {
        let (requests, _ranges) = frame_builder.flatten_image_requests();
        if requests.is_empty() { return; }
        let atlas_size = [self.image_allocator.atlas_width() as f32, self.image_allocator.atlas_height() as f32];
        let instances: Vec<ImageInstance> = requests.iter().map(|req| {
            let region = self.image_allocator.get_region(req.image_key).expect("Image key not found in atlas");
            ImageInstance::from_logical(req.position, req.size, region, atlas_size, req.corner_radius, AffineTransform::from_array(req.transform), req.opacity)
        }).collect();
        self.ensure_image_instance_capacity(instances.len());
        self.queue.write_buffer(&self.image_instance_buffer, 0, bytemuck::cast_slice(&instances));
    }
```

Also update any existing `ImageRequest` construction sites that don't yet have the `opacity` field — search for `ImageRequest {` and add `opacity: 1.0` to each.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_image_request_opacity`
Expected: PASS

- [ ] **Step 5: Run full build to check no compilation errors**

Run: `cargo build -p vexo`
Expected: SUCCESS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/frame_builder.rs vexo/src/image_instance.rs vexo/src/render/wgpu_backend.rs
git commit -m "feat: add opacity field to ImageRequest and ImageInstance"
```

---

### Task 5: Update image shader to use opacity

**Files:**
- Modify: `vexo/src/image_shader.wgsl`

**Interfaces:**
- Consumes: `inst_opacity` attribute from `ImageInstance` (Task 4)

- [ ] **Step 1: Write the minimal implementation**

Update `vexo/src/image_shader.wgsl`. Add `opacity` field to `VertexOutput` and wire it through the vertex shader, then multiply in the fragment shader:

```wgsl

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_origin: vec2<f32>,
    @location(2) uv_size: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) corner_radius: f32,
    @location(5) opacity: f32,
};

struct GlobalUniforms {
    screen_size: vec2<f32>,
    scale_factor: f32,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;
@group(1) @binding(0) var image_atlas: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_uv_origin: vec2<f32>,
    @location(4) inst_uv_size: vec2<f32>,
    @location(5) inst_corner_radius: f32,
    @location(9) inst_opacity: f32,
    @location(6) inst_transform_ab: vec2<f32>,
    @location(7) inst_transform_cd: vec2<f32>,
    @location(8) inst_transform_ef: vec2<f32>,
) -> VertexOutput {
    let local_pos = model_pos * inst_size;
    let half_size = inst_size * 0.5;
    let centered_x = local_pos.x - half_size.x;
    let centered_y = local_pos.y - half_size.y;

    let tx = inst_transform_ab.x * centered_x + inst_transform_cd.x * centered_y + inst_transform_ef.x;
    let ty = inst_transform_ab.y * centered_x + inst_transform_cd.y * centered_y + inst_transform_ef.y;

    let logical_pos = vec2<f32>(tx + half_size.x + inst_pos.x, ty + half_size.y + inst_pos.y);
    let pixel_pos = logical_pos * globals.scale_factor;
    let nx = (pixel_pos.x / globals.screen_size.x) * 2.0 - 1.0;
    let ny = 1.0 - (pixel_pos.y / globals.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = model_pos;
    out.uv_origin = inst_uv_origin;
    out.uv_size = inst_uv_size;
    out.size = inst_size * globals.scale_factor;
    out.corner_radius = inst_corner_radius * globals.scale_factor;
    out.opacity = inst_opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);
    let atlas_uv = in.uv_origin + in.uv * in.uv_size;
    let tex_color = textureSample(image_atlas, image_sampler, atlas_uv);

    if (radius < 0.5) {
        return vec4<f32>(tex_color.rgb, tex_color.a * in.opacity);
    }

    let pixel_pos = in.uv * in.size;
    let half_size = in.size * 0.5;
    let center_pos = pixel_pos - half_size;
    let inner_dist = abs(center_pos) - (half_size - radius);
    let corner_dist = length(max(inner_dist, vec2<f32>(0.0))) - radius;
    let sdf = min(max(inner_dist.x, inner_dist.y), 0.0) + corner_dist;
    let fill_alpha = 1.0 - smoothstep(-1.0, 1.0, sdf);

    if (fill_alpha <= 0.0) {
        discard;
    }

    return vec4<f32>(tex_color.rgb, tex_color.a * fill_alpha * in.opacity);
}
```

- [ ] **Step 2: Run full build**

Run: `cargo build -p vexo`
Expected: SUCCESS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/image_shader.wgsl
git commit -m "feat: add opacity support to image shader"
```

---

### Task 6: Add opacity push/pop to Painter

**Files:**
- Modify: `vexo/src/painter.rs:130-155` (paint_recursive, after scroll offset section)

**Interfaces:**
- Consumes: `RenderObject::opacity()` from Task 1, `RenderCommand::PushOpacity`/`PopOpacity` from Task 2

- [ ] **Step 1: Write the minimal implementation**

In `vexo/src/painter.rs`, in `paint_recursive()`, add opacity push/pop after the scroll offset section (after line ~134 where PushOffset is emitted, and before "Paint children" comment). Add the push before children and pop after children:

After the scroll offset push block (line ~134), add:

```rust
        // If this object has an opacity, push it before painting children.
        let opacity = obj.opacity();
        if let Some(opacity_value) = &opacity {
            ctx.push_command(RenderCommand::PushOpacity { opacity: *opacity_value });
        }
```

Before the "Pop scroll offset after children" block (line ~142), add pop for opacity. The pops must be in reverse order of pushes. Current order: transform push, clip push, scroll push, then children, then scroll pop, clip pop, transform pop. Add opacity push after scroll push, and opacity pop before scroll pop:

After painting children (line ~139), add:

```rust
        // Pop opacity after children
        if opacity.is_some() {
            ctx.push_command(RenderCommand::PopOpacity);
        }
```

- [ ] **Step 2: Run full build**

Run: `cargo build -p vexo`
Expected: SUCCESS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/painter.rs
git commit -m "feat: emit PushOpacity/PopOpacity in Painter"
```

---

### Task 7: Create OpacityRenderObject

**Files:**
- Create: `vexo/src/render_objects/opacity.rs`
- Modify: `vexo/src/render_objects/mod.rs`

**Interfaces:**
- Consumes: `RenderObject::opacity()` from Task 1
- Produces: `OpacityRenderObject` — a render object that returns `Some(self.opacity)` from `opacity()`, `vec![]` from `paint()`, pass-through layout

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/render_objects/opacity.rs, inside #[cfg(test)] mod tests
#[test]
fn test_opacity_render_object_opacity() {
    let ro = OpacityRenderObject::new(0.5);
    assert_eq!(ro.opacity(), Some(0.5));
}

#[test]
fn test_opacity_render_object_zero() {
    let ro = OpacityRenderObject::new(0.0);
    assert_eq!(ro.opacity(), Some(0.0));
}

#[test]
fn test_opacity_render_object_full() {
    let ro = OpacityRenderObject::new(1.0);
    assert_eq!(ro.opacity(), Some(1.0));
}

#[test]
fn test_opacity_render_object_set_opacity() {
    let mut ro = OpacityRenderObject::new(0.5);
    assert!(ro.set_opacity(0.7));
    assert_eq!(ro.opacity(), Some(0.7));
    assert!(!ro.set_opacity(0.7)); // no change
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_opacity_render_object`
Expected: FAIL — file doesn't exist yet

- [ ] **Step 3: Write minimal implementation**

Create `vexo/src/render_objects/opacity.rs`:

```rust
use std::any::Any;

use crate::core::{Bounds, Logical, Point};
use crate::input::InputEvent;
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

pub struct OpacityRenderObject {
    opacity: f32,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl OpacityRenderObject {
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) -> bool {
        if (self.opacity - opacity).abs() > f32::EPSILON {
            self.opacity = opacity;
            true
        } else {
            false
        }
    }
}

impl RenderObject for OpacityRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch);

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult { node: existing, size: crate::core::Size::zero() }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult { node, size: crate::core::Size::zero() }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn opacity(&self) -> Option<f32> {
        Some(self.opacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opacity_render_object_opacity() {
        let ro = OpacityRenderObject::new(0.5);
        assert_eq!(ro.opacity(), Some(0.5));
    }

    #[test]
    fn test_opacity_render_object_zero() {
        let ro = OpacityRenderObject::new(0.0);
        assert_eq!(ro.opacity(), Some(0.0));
    }

    #[test]
    fn test_opacity_render_object_full() {
        let ro = OpacityRenderObject::new(1.0);
        assert_eq!(ro.opacity(), Some(1.0));
    }

    #[test]
    fn test_opacity_render_object_set_opacity() {
        let mut ro = OpacityRenderObject::new(0.5);
        assert!(ro.set_opacity(0.7));
        assert_eq!(ro.opacity(), Some(0.7));
        assert!(!ro.set_opacity(0.7));
    }
}
```

Register in `vexo/src/render_objects/mod.rs`:

```rust
mod opacity;

pub use opacity::OpacityRenderObject;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_opacity_render_object`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render_objects/opacity.rs vexo/src/render_objects/mod.rs
git commit -m "feat: add OpacityRenderObject"
```

---

### Task 8: Create OpacityElement

**Files:**
- Create: `vexo/src/elements/opacity.rs`
- Modify: `vexo/src/elements/mod.rs`

**Interfaces:**
- Consumes: `OpacityRenderObject` from Task 7
- Produces: `OpacityElement` — single-child element following `RenderObjectElement` pattern (same as TransformElement)

- [ ] **Step 1: Write minimal implementation**

Create `vexo/src/elements/opacity.rs` — follows the exact same pattern as `TransformElement` in `vexo/src/widgets/transform.rs`:

```rust
use std::any::Any;

use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::{
    Element, ElementContext, ElementKey, EventContext, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};
use crate::render_objects::OpacityRenderObject;

pub struct OpacityElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl OpacityElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for OpacityElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for OpacityElement {
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

impl Element for OpacityElement {
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

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
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

                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
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
```

Register in `vexo/src/elements/mod.rs`:

```rust
mod opacity;

pub use opacity::OpacityElement;
```

- [ ] **Step 2: Run full build**

Run: `cargo build -p vexo`
Expected: SUCCESS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/elements/opacity.rs vexo/src/elements/mod.rs
git commit -m "feat: add OpacityElement"
```

---

### Task 9: Create Opacity widget and Widget trait modifier

**Files:**
- Create: `vexo/src/widgets/opacity.rs`
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Consumes: `OpacityElement` from Task 8, `OpacityRenderObject` from Task 7
- Produces: `Opacity` widget, `.opacity()` modifier on `Widget` trait

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/widgets/opacity.rs, inside #[cfg(test)] mod tests
#[test]
fn test_opacity_creation() {
    let w = Opacity::new(Text::new("Hello"), 0.5);
    assert_eq!(w.opacity_value(), 0.5);
}

#[test]
fn test_opacity_clamping() {
    let w = Opacity::new(Text::new("Hello"), 1.5);
    assert_eq!(w.opacity_value(), 1.0);
    let w2 = Opacity::new(Text::new("Hello"), -0.5);
    assert_eq!(w2.opacity_value(), 0.0);
}

#[test]
fn test_opacity_render_object_creation() {
    let w = Opacity::new(Text::new("Hello"), 0.5);
    let ro = w.create_render_object();
    assert!(ro.as_any().downcast_ref::<OpacityRenderObject>().is_some());
    assert_eq!(ro.as_any().downcast_ref::<OpacityRenderObject>().unwrap().opacity(), Some(0.5));
}

#[test]
fn test_opacity_update_render_object() {
    let w1 = Opacity::new(Text::new("Hello"), 0.5);
    let w2 = Opacity::new(Text::new("Hello"), 0.7);
    let mut ro = crate::render_objects::OpacityRenderObject::new(0.5);

    let result = w1.update_render_object(&mut ro);
    assert_eq!(result, UpdateResult::NONE);

    let result = w2.update_render_object(&mut ro);
    assert!(result.contains(UpdateResult::PAINT));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_opacity_creation`
Expected: FAIL — file doesn't exist yet

- [ ] **Step 3: Write minimal implementation**

Create `vexo/src/widgets/opacity.rs`:

```rust
use std::any::Any;

use crate::core::Bounds;
use crate::elements::OpacityElement;
use crate::render_objects::OpacityRenderObject;
use crate::{
    Element, RenderObject, UpdateResult, Widget, WidgetKey,
};

pub struct Opacity {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    opacity: f32,
}

impl Opacity {
    pub fn new(child: impl Widget + 'static, opacity: f32) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            opacity: opacity.clamp(0.0, 1.0),
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn opacity_value(&self) -> f32 {
        self.opacity
    }
}

impl Clone for Opacity {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            opacity: self.opacity,
        }
    }
}

impl Widget for Opacity {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = OpacityElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(OpacityRenderObject::new(self.opacity))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(opacity_ro) = render_object
            .as_any_mut()
            .downcast_mut::<OpacityRenderObject>()
        {
            if opacity_ro.set_opacity(self.opacity) {
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
    use crate::{Text, render_objects::OpacityRenderObject};

    #[test]
    fn test_opacity_creation() {
        let w = Opacity::new(Text::new("Hello"), 0.5);
        assert_eq!(w.opacity_value(), 0.5);
    }

    #[test]
    fn test_opacity_clamping() {
        let w = Opacity::new(Text::new("Hello"), 1.5);
        assert_eq!(w.opacity_value(), 1.0);
        let w2 = Opacity::new(Text::new("Hello"), -0.5);
        assert_eq!(w2.opacity_value(), 0.0);
    }

    #[test]
    fn test_opacity_render_object_creation() {
        let w = Opacity::new(Text::new("Hello"), 0.5);
        let ro = w.create_render_object();
        assert!(ro.as_any().downcast_ref::<OpacityRenderObject>().is_some());
        assert_eq!(ro.as_any().downcast_ref::<OpacityRenderObject>().unwrap().opacity(), Some(0.5));
    }

    #[test]
    fn test_opacity_update_render_object() {
        let w1 = Opacity::new(Text::new("Hello"), 0.5);
        let w2 = Opacity::new(Text::new("Hello"), 0.7);
        let mut ro = OpacityRenderObject::new(0.5);

        let result = w1.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);

        let result = w2.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT));
    }
}
```

Register in `vexo/src/widgets/mod.rs` — add module declaration and pub(crate) re-export:

```rust
mod opacity;
```

And in the re-exports section (after `pub(crate) use transform::Transform;`):

```rust
pub(crate) use opacity::Opacity;
```

Add `opacity()` modifier to the `Widget` trait (after `fn scale()`, before the closing `}` of the trait):

```rust
    fn opacity(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(Opacity::new(self, value))
    }
```

Re-export `Opacity` publicly from `vexo/src/lib.rs`:

```rust
pub use widgets::Opacity;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_opacity`
Expected: PASS

- [ ] **Step 5: Run full build**

Run: `cargo build -p vexo`
Expected: SUCCESS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/opacity.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat: add Opacity widget and .opacity() Widget trait modifier"
```

---

### Task 10: Integration tests

**Files:**
- Modify: `vexo/tests/` (add integration test for Opacity widget)

**Interfaces:**
- Consumes: All previous tasks

- [ ] **Step 1: Check existing integration test structure**

Run: `ls vexo/tests/`
Then read one existing integration test file to match the pattern.

- [ ] **Step 2: Write integration test for Opacity widget**

Create an integration test that verifies:
1. Opacity widget produces PushOpacity/PopOpacity in render commands
2. Nested opacity values multiply correctly
3. Zero-opacity widget still produces layout

Follow the existing integration test pattern in the project (likely using `MockBackend` or the render command inspection approach).

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p vexo`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/tests/
git commit -m "test: add Opacity widget integration tests"
```

---

### Task 11: Full build and test verification

**Files:**
- No new files

- [ ] **Step 1: Run full workspace build**

Run: `cargo build`
Expected: SUCCESS

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 3: Run desktop demo to verify no regressions**

Ask user to run: `cargo run -p desktop_demo`
Expected: App launches normally, no visual regressions
