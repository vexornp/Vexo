# UiBatcher Architecture Analysis

## Current Design Overview

The `UiBatcher` collects render data for efficient GPU instanced rendering:

```rust
pub struct UiBatcher {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_requests: Vec<TextRequest>,
    pub editor_requests: Vec<EditorRequest>,
    pub quad_instances: Vec<QuadInstance>,
    screen_size: Size<Logical>,
    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
}
```

### Current Render Flow

1. **Widget → RenderCommand**: Widgets implement `Paint` trait to generate `RenderCommand`s
2. **RenderCommand → UiBatcher**: `process_commands()` translates commands to batcher calls
3. **UiBatcher → GPU**: `WgpuBackend` uploads `quad_instances` to instance buffer
4. **GPU Render**: Single instanced draw call: `draw_indexed(0..6, 0, 0..instance_count)`

### Current QuadInstance Structure

```rust
#[repr(C)]
pub struct QuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub corner_radius: f32,
    pub clip_bounds: [f32; 4],
    pub _padding: [f32; 2],
}
```

---

## What's Working Well

### 1. Instanced Rendering ✓

- Single vertex buffer with 4 vertices (unit quad: 0,0 → 1,1)
- Single index buffer with 6 indices (two triangles)
- All quads drawn in one draw call via instancing
- GPU-efficient: minimal vertex shader invocations

### 2. SDF-based Rendering ✓

- Rounded rectangles computed in fragment shader using signed distance fields
- Anti-aliased edges via `smoothstep`
- Borders rendered without extra geometry
- Clean mathematical approach

### 3. Separation of Concerns ✓

- `RenderCommand` decouples widgets from GPU knowledge
- `UiBatcher` is a pure data collector (no GPU dependencies)
- `WgpuBackend` handles all GPU specifics
- Testable without GPU via `MockBackend`

---

## Architectural Issues & Improvements

### Issue 1: No Batching by State (Major)

**Current Problem**: All quads use the same shader, which does per-fragment clipping:

```wgsl
// Fragment shader - executed for EVERY pixel
if (in.clip_bounds.z > 0.0 && in.clip_bounds.w > 0.0) {
    let frag_x = in.inst_pos.x + in.uv.x * (in.size.x / globals.scale_factor);
    let frag_y = in.inst_pos.y + in.uv.y * (in.size.y / globals.scale_factor);

    if (frag_x < in.clip_bounds.x ||
        frag_y < in.clip_bounds.y ||
        frag_x > in.clip_bounds.x + in.clip_bounds.z ||
        frag_y > in.clip_bounds.y + in.clip_bounds.w) {
        discard;
    }
}
```

Even quads without clipping pay the branch cost. Quads without corner radius also pay SDF calculation cost.

**Proposed Solution**: Separate batches by state

```rust
pub struct UiBatcher {
    // Quads without clipping - simpler shader
    simple_quads: Vec<SimpleQuadInstance>,
    // Quads with clipping - use clip shader
    clipped_quads: Vec<ClippedQuadInstance>,
    // Quads with corner radius - use SDF shader
    rounded_quads: Vec<RoundedQuadInstance>,
}

// Or use batch grouping:
pub struct RenderBatch {
    pub pipeline: BatchPipeline,
    pub instances: Vec<QuadInstance>,
    pub clip_rect: Option<Bounds>,
}

pub enum BatchPipeline {
    Simple,      // No clip, no radius - fastest shader
    Rounded,     // Has corner radius - SDF shader
    Clipped,     // Has clip rect - clip shader
}
```

**Benefit**: Fewer shader variants per draw call, better GPU cache locality

---

### Issue 2: No Z-ordering / Depth Sorting

**Current Problem**: Quads rendered in insertion order, no depth handling:

```rust
pub quad_instances: Vec<QuadInstance>,  // Insertion order
```

For correct alpha blending with overlapping transparent elements, quads should be sorted back-to-front.

**Proposed Solutions**:

Option A: Z-sorting
```rust
impl UiBatcher {
    pub fn finalize(&mut self) {
        // Sort by z-index before rendering
        self.quad_instances.sort_by_key(|q| q.z_index);
    }
}
```

Option B: Depth buffer
```rust
// In WgpuBackend
let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("Depth Texture"),
    size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
    format: wgpu::TextureFormat::Depth24Plus,
    // ...
});

// Render pass with depth
depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
    view: &depth_view,
    depth_ops: Some(wgpu::Operations {
        load: wgpu::LoadOp::Clear(1.0),
        store: wgpu::StoreOp::Store,
    }),
    // ...
}),
```

---

### Issue 3: Fixed Buffer Size

**Current Problem**: Hardcoded 10,000 instance limit

```rust
// vexo/src/render/wgpu_backend.rs:210
let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
    size: (std::mem::size_of::<QuadInstance>() * 10000) as wgpu::BufferAddress,
    // ...
});
```

**Proposed Solution**: Dynamic buffer resizing

```rust
pub struct WgpuBackend {
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
}

impl WgpuBackend {
    pub fn ensure_instance_capacity(&mut self, required: usize) {
        if required > self.instance_capacity {
            let new_capacity = required.next_power_of_two().max(1024);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer"),
                size: (std::mem::size_of::<QuadInstance>() * new_capacity) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_capacity;
        }
    }
}
```

---

### Issue 4: No Texture Atlas for Images

**Current Problem**: Only handles solid colors, no image/icon support

**Proposed Solution**: Texture atlas

```rust
pub struct TextureAtlas {
    texture: wgpu::Texture,
    allocator: AtlasAllocator,  // e.g., rectangle packing algorithm
    entries: HashMap<ImageId, AtlasEntry>,
}

pub struct AtlasEntry {
    uv_min: [f32; 2],  // Top-left UV coordinates
    uv_max: [f32; 2],  // Bottom-right UV coordinates
}

pub struct TexturedQuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub tint_color: [f32; 4],
}
```

Shader addition:
```wgsl
@group(1) @binding(0) var texture_atlas: texture_2d<f32>;
@group(1) @binding(1) var sampler_atlas: sampler;

@fragment
fn fs_textured(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mix(in.uv_min, in.uv_max, in.vertex_uv);
    let tex_color = textureSample(texture_atlas, sampler_atlas, uv);
    return tex_color * in.tint_color;
}
```

---

### Issue 5: Clip Stack is CPU-side, Per-Quad Storage

**Current Problem**: Clip bounds stored in every `QuadInstance`, wasting memory and bandwidth

```rust
pub clip_bounds: [f32; 4],  // 16 bytes per quad
```

For 10,000 quads = 160KB of clip data, even if most quads don't need clipping.

**Proposed Solution**: GPU clip stack

```wgsl
// In shader
struct ClipRect {
    bounds: vec4<f32>,
}

@group(0) @binding(1) var<storage> clip_stack: array<ClipRect, 16>;
@group(0) @binding(2) var<uniform> clip_depth: u32;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Check against current clip depth
    for (var i = 0u; i < clip_depth; i++) {
        let clip = clip_stack[i];
        if (outside_clip(in.position, clip.bounds)) {
            discard;
        }
    }
    // ... rest of shader
}
```

Rust side:
```rust
pub struct ClipStack {
    stack: Vec<Bounds>,
    gpu_buffer: wgpu::Buffer,
}

impl ClipStack {
    pub fn push(&mut self, bounds: Bounds) {
        self.stack.push(bounds);
        self.upload_to_gpu();
    }
}
```

---

### Issue 6: No Culling

**Current Problem**: All quads rendered even if off-screen

**Proposed Solution**: Frustum culling

```rust
pub struct UiBatcher {
    visible_area: Bounds<Logical>,
}

impl UiBatcher {
    pub fn add_rect(&mut self, bounds: Bounds, ...) {
        // Skip off-screen quads entirely
        if !self.visible_area.intersects(&bounds) {
            return;
        }

        // Clip partially-visible quads
        let clipped_bounds = bounds.clamp_to(&self.visible_area);

        self.quad_instances.push(QuadInstance {
            position: [clipped_bounds.left, clipped_bounds.top],
            size: [clipped_bounds.width(), clipped_bounds.height()],
            // ...
        });
    }
}
```

---

## Recommended Architecture Improvements

### Proposed Batch-Grouped Design

```
┌─────────────────────────────────────────────────────────────────┐
│                    IMPROVED BATCHER DESIGN                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  UiBatcher                                                      │
│  ├── batches: Vec<RenderBatch>                                 │
│  │   ├── RenderBatch::Simple { quads, pipeline }              │
│  │   ├── RenderBatch::Clipped { quads, clip_rect, pipeline }  │
│  │   └── RenderBatch::Textured { quads, texture, pipeline }   │
│  │                                                             │
│  ├── culling_rect: Bounds       // Visible area               │
│  ├── texture_atlas: TextureAtlas // For images/icons          │
│  └── stats: RenderStats         // For profiling              │
│                                                                 │
│  RenderBatch                                                    │
│  ├── pipeline: BatchPipeline                                   │
│  ├── instances: Vec<QuadInstance>                              │
│  ├── clip_rect: Option<Bounds>                                 │
│  └── texture: Option<TextureId>                                │
│                                                                 │
│  BatchPipeline (enum)                                           │
│  ├── Simple      // No clip, no radius - fastest              │
│  ├── Rounded     // Has corner radius                          │
│  ├── Clipped     // Has clip rect                              │
│  └── Textured    // Has texture                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Proposed Implementation

```rust
pub struct UiBatcher {
    batches: Vec<RenderBatch>,
    current_batch: Option<RenderBatch>,
    visible_area: Bounds<Logical>,
    texture_atlas: Option<TextureAtlas>,
}

pub struct RenderBatch {
    pipeline: BatchPipeline,
    instances: Vec<QuadInstance>,
    clip_rect: Option<Bounds>,
    texture_id: Option<TextureId>,
    z_range: (u32, u32),  // Min/max z for sorting
}

#[derive Clone, Copy, PartialEq, Eq)]
pub enum BatchPipeline {
    Simple,      // No clip, no radius
    Rounded,     // Has corner radius (SDF)
    Clipped,     // Has clip rect
    Textured,    // Has texture
}

impl UiBatcher {
    pub fn add_rect(&mut self, bounds: Bounds, fill: Color, ...) {
        // 1. Culling
        if !self.visible_area.intersects(&bounds) {
            return;
        }

        // 2. Classify quad
        let pipeline = self.classify_quad(corner_radius, clip_bounds, texture);

        // 3. Add to appropriate batch
        let batch = self.find_or_create_batch(pipeline, clip_bounds, texture);
        batch.instances.push(QuadInstance::new(bounds, fill, ...));
    }

    fn classify_quad(&self, corner_radius: f32, clip: Option<Bounds>, texture: Option<TextureId>) -> BatchPipeline {
        if texture.is_some() { return BatchPipeline::Textured; }
        if clip.is_some() { return BatchPipeline::Clipped; }
        if corner_radius > 0.0 { return BatchPipeline::Rounded; }
        BatchPipeline::Simple
    }

    fn finalize(&mut self) -> Vec<RenderBatch> {
        // Sort batches by z-index
        self.batches.sort_by_key(|b| b.z_range.0);

        // Sort instances within each batch
        for batch in &mut self.batches {
            batch.instances.sort_by_key(|i| i.z_index);
        }

        self.batches.clone()
    }
}
```

---

## Summary Table

| Aspect | Current State | Recommended Improvement |
|--------|---------------|------------------------|
| Batching | Single batch, all quads | Sorted by pipeline/state |
| Shader variants | One complex shader | Multiple simpler shaders |
| Culling | None | Frustum culling |
| Buffer size | Fixed 10,000 | Dynamic resize |
| Textures | Not supported | Texture atlas |
| Clipping | Per-fragment, per-quad | GPU clip stack |
| Depth/Z-order | None | Z-sort or depth buffer |
| Memory | Clip bounds in every quad | Shared clip stack |

---

## Priority Order for Implementation

1. **High Priority**: Dynamic buffer resize (prevent crashes on complex UIs)
2. **High Priority**: Frustum culling (immediate performance gain)
3. **Medium Priority**: Batch sorting by pipeline (shader optimization)
4. **Medium Priority**: Z-ordering (correct transparency)
5. **Low Priority**: Texture atlas (feature addition)
6. **Low Priority**: GPU clip stack (memory optimization)

---

## Files to Modify

- `vexo/src/renderer.rs` - UiBatcher implementation
- `vexo/src/quad_instance.rs` - QuadInstance structure
- `vexo/src/render/wgpu_backend.rs` - Buffer management, render pass
- `vexo/src/shader.wgsl` - Shader variants
- `vexo/src/render/command_processor.rs` - Batch classification logic