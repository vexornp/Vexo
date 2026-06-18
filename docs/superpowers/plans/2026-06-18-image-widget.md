# Image Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Image widget that displays JPEG images from embedded bytes, with full modifier system support and texture atlas GPU rendering.

**Architecture:** Texture atlas with shelf-based packing into a single 2048x2048 wgpu texture. Separate GPU pipeline for textured quads (drawn after solid quads, before text, in the same render pass). Image widget follows the Text widget pattern: leaf element, render object with style/layout fields, modifier methods via macro.

**Tech Stack:** `image` crate for JPEG decoding, wgpu for GPU texture atlas and textured quad pipeline, bytemuck for GPU instance data.

---

### Task 1: Add `image` crate dependency

**Files:**
- Modify: `Cargo.toml` (workspace dependencies)
- Modify: `vexo/Cargo.toml`

- [ ] **Step 1: Add `image` crate to workspace dependencies**

In `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
# Image Decoding
image = { version = "0.25", default-features = false, features = ["jpeg"] }
```

Using `default-features = false` with only `jpeg` feature to minimize dependency footprint.

- [ ] **Step 2: Add `image` to vexo crate dependencies**

In `vexo/Cargo.toml`, add:

```toml
image = { workspace = true }
```

- [ ] **Step 3: Verify dependency resolves**

Run: `cargo build -p vexo`
Expected: Compiles successfully (no code uses it yet, but dependency is available).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml vexo/Cargo.toml
git commit -m "chore: add image crate dependency for JPEG decoding"
```

---

### Task 2: ImageData, ImageKey, AtlasRegion, and ShelfAllocator types

**Files:**
- Create: `vexo/src/image_data.rs`
- Create: `vexo/src/image_atlas.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create ImageData type**

Create `vexo/src/image_data.rs`:

```rust
#[derive(Clone, Debug)]
pub struct ImageData {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageDataError(String);

impl std::fmt::Display for ImageDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ImageDataError {}

impl ImageData {
    pub fn from_bytes(bytes: &[u8]) -> Result<ImageData, ImageDataError> {
        let img = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| ImageDataError(format!("Failed to guess image format: {}", e)))?
            .decode()
            .map_err(|e| ImageDataError(format!("Failed to decode image: {}", e)))?;

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();

        if width == 0 || height == 0 {
            return Err(ImageDataError("Decoded image has zero dimensions".into()));
        }

        Ok(ImageData {
            pixels: rgba.into_raw(),
            width,
            height,
        })
    }
}
```

- [ ] **Step 2: Create ShelfAllocator with tests**

Create `vexo/src/image_atlas.rs`:

```rust
use std::collections::HashMap;

/// Unique identifier for an image registered in the atlas.
pub type ImageKey = u64;

/// A region within the atlas texture where an image is stored.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A horizontal strip in the shelf allocator.
pub struct Shelf {
    pub y: u32,
    pub height: u32,
    pub x_cursor: u32,
    pub remaining_width: u32,
}

/// Pure-data shelf allocator for atlas packing (no GPU resources).
pub struct ShelfAllocator {
    atlas_width: u32,
    atlas_height: u32,
    shelves: Vec<Shelf>,
    next_key: ImageKey,
    images: HashMap<ImageKey, AtlasRegion>,
}

impl ShelfAllocator {
    pub fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            atlas_width,
            atlas_height,
            shelves: Vec::new(),
            next_key: 0,
            images: HashMap::new(),
        }
    }

    pub fn atlas_width(&self) -> u32 {
        self.atlas_width
    }

    pub fn atlas_height(&self) -> u32 {
        self.atlas_height
    }

    /// Allocate a region for an image of the given size.
    /// Returns the ImageKey and AtlasRegion, or panics if atlas is full.
    pub fn allocate(&mut self, width: u32, height: u32) -> (ImageKey, AtlasRegion) {
        // Find a shelf with enough remaining width whose height >= image height
        for shelf in &mut self.shelves {
            if shelf.remaining_width >= width && shelf.height >= height {
                let region = AtlasRegion {
                    x: shelf.x_cursor,
                    y: shelf.y,
                    width,
                    height,
                };
                shelf.x_cursor += width;
                shelf.remaining_width -= width;
                let key = self.next_key;
                self.next_key += 1;
                self.images.insert(key, region.clone());
                return (key, region);
            }
        }

        // Create a new shelf at current y offset
        let y_offset = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y_offset + height > self.atlas_height {
            panic!(
                "Image atlas is full: cannot fit {}x{} image. Atlas size: {}x{}",
                width, height, self.atlas_width, self.atlas_height
            );
        }

        let shelf = Shelf {
            y: y_offset,
            height,
            x_cursor: 0,
            remaining_width: self.atlas_width,
        };
        self.shelves.push(shelf);

        let shelf = self.shelves.last_mut().unwrap();
        let region = AtlasRegion {
            x: shelf.x_cursor,
            y: shelf.y,
            width,
            height,
        };
        shelf.x_cursor += width;
        shelf.remaining_width -= width;
        let key = self.next_key;
        self.next_key += 1;
        self.images.insert(key, region.clone());
        (key, region)
    }

    /// Look up a previously allocated region.
    pub fn get_region(&self, key: ImageKey) -> Option<&AtlasRegion> {
        self.images.get(&key)
    }

    /// Remove a region from the atlas.
    pub fn remove(&mut self, key: ImageKey) {
        self.images.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_data_from_bytes_invalid() {
        let result = crate::image_data::ImageData::from_bytes(&[0xFF, 0xD8, 0xFF, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn test_image_data_from_bytes_empty() {
        let result = crate::image_data::ImageData::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_image_data_from_bytes_valid_jpeg() {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        let mut jpeg_bytes = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
        encoder.write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        ).unwrap();
        let result = crate::image_data::ImageData::from_bytes(&jpeg_bytes);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.width, 2);
        assert_eq!(data.height, 2);
        assert_eq!(data.pixels.len(), 2 * 2 * 4);
    }

    #[test]
    fn test_shelf_allocator_single_image() {
        let mut allocator = ShelfAllocator::new(2048, 2048);
        let (key, region) = allocator.allocate(100, 50);
        assert_eq!(region.x, 0);
        assert_eq!(region.y, 0);
        assert_eq!(region.width, 100);
        assert_eq!(region.height, 50);
        assert_eq!(allocator.get_region(key), Some(&region));
    }

    #[test]
    fn test_shelf_allocator_multiple_same_height() {
        let mut allocator = ShelfAllocator::new(2048, 2048);
        let (k1, r1) = allocator.allocate(100, 50);
        let (k2, r2) = allocator.allocate(200, 50);
        assert_eq!(r2.x, 100);
        assert_eq!(r2.y, 0);
        assert_eq!(allocator.get_region(k1), Some(&r1));
        assert_eq!(allocator.get_region(k2), Some(&r2));
    }

    #[test]
    fn test_shelf_allocator_different_height_creates_new_shelf() {
        let mut allocator = ShelfAllocator::new(2048, 2048);
        let (_, r1) = allocator.allocate(100, 50);
        let (_, r2) = allocator.allocate(100, 80);
        assert_eq!(r1.y, 0);
        assert_eq!(r2.y, 50);
    }

    #[test]
    #[should_panic(expected = "Image atlas is full")]
    fn test_shelf_allocator_overflow_panics() {
        let mut allocator = ShelfAllocator::new(64, 64);
        allocator.allocate(100, 100);
    }

    #[test]
    fn test_shelf_allocator_remove() {
        let mut allocator = ShelfAllocator::new(2048, 2048);
        let (key, _) = allocator.allocate(100, 50);
        assert!(allocator.get_region(key).is_some());
        allocator.remove(key);
        assert!(allocator.get_region(key).is_none());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo -- image_data shelf_allocator`
Expected: All tests PASS.

- [ ] **Step 4: Register modules in lib.rs**

In `vexo/src/lib.rs`, add module declarations alongside the existing module declarations:

```rust
mod image_data;
pub mod image_atlas;
```

Add re-exports alongside the existing re-exports:

```rust
pub use image_data::{ImageData, ImageDataError};
```

- [ ] **Step 5: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/image_data.rs vexo/src/image_atlas.rs vexo/src/lib.rs
git commit -m "feat: add ImageData, ImageKey, AtlasRegion, ShelfAllocator types"
```

---

### Task 3: RenderCommand::Image, ImageInstance, and FrameBuilder support

**Files:**
- Modify: `vexo/src/render/command.rs`
- Modify: `vexo/src/render/command_processor.rs`
- Create: `vexo/src/image_instance.rs`
- Modify: `vexo/src/frame_builder.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Add Image variant to RenderCommand**

In `vexo/src/render/command.rs`, add import at the top:

```rust
use crate::image_atlas::ImageKey;
```

Add to the `RenderCommand` enum, after `Caret` and before `PushClip`:

```rust
Image {
    bounds: Bounds<Logical>,
    image_key: ImageKey,
    corner_radius: f32,
},
```

- [ ] **Step 2: Handle Image in CommandProcessor**

In `vexo/src/render/command_processor.rs`, add import:

```rust
use crate::image_atlas::ImageKey;
```

Add to the `match` block in `process_commands()`, after the `Caret` arm:

```rust
RenderCommand::Image { bounds, image_key, corner_radius } => {
    let offset_bounds = bounds.offset(current_offset);
    frame_builder.add_image(crate::frame_builder::ImageRequest {
        position: [offset_bounds.left, offset_bounds.top],
        size: [offset_bounds.width(), offset_bounds.height()],
        image_key,
        corner_radius,
        transform: current_transform.to_array(),
    });
}
```

- [ ] **Step 3: Create ImageInstance struct**

Create `vexo/src/image_instance.rs`:

```rust
use crate::core::AffineTransform;
use crate::image_atlas::AtlasRegion;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_origin: [f32; 2],
    pub uv_size: [f32; 2],
    pub corner_radius: f32,
    pub transform: [f32; 6],
    pub _padding: [f32; 2],
}

impl ImageInstance {
    pub fn from_logical(
        pos: [f32; 2],
        size: [f32; 2],
        region: &AtlasRegion,
        atlas_size: [f32; 2],
        corner_radius: f32,
        transform: AffineTransform,
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
            transform: transform.to_array(),
            _padding: [0.0; 2],
        }
    }

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
                wgpu::VertexAttribute { offset: 36, shader_location: 6, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 44, shader_location: 7, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 52, shader_location: 8, format: wgpu::VertexFormat::Float32x2 },
            ],
        }
    }
}
```

- [ ] **Step 4: Add ImageRequest and image support to FrameBuilder**

In `vexo/src/frame_builder.rs`, add import:

```rust
use crate::image_atlas::ImageKey;
```

Add `ImageRequest` struct after `TextRequest`:

```rust
#[derive(Clone)]
pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub corner_radius: f32,
    pub transform: [f32; 6],
}
```

Add `image_requests: Vec<ImageRequest>` field to `ClipGroup`:

```rust
pub struct ClipGroup {
    pub clip_bounds: Option<Bounds>,
    pub quads: Vec<QuadInstance>,
    pub text_requests: Vec<TextRequest>,
    pub image_requests: Vec<ImageRequest>,
}
```

Update `current_group()` — in the "Create a new group" section, add `image_requests`:

```rust
self.clip_groups.push(ClipGroup {
    clip_bounds: clip_key,
    quads: Vec::new(),
    text_requests: Vec::new(),
    image_requests: Vec::new(),
});
```

Add methods to `FrameBuilder`:

```rust
pub fn add_image(&mut self, request: ImageRequest) {
    self.current_group().image_requests.push(request);
}

pub fn image_count(&self) -> usize {
    self.clip_groups.iter().map(|g| g.image_requests.len()).sum()
}

pub fn flatten_image_requests(&self) -> (Vec<ImageRequest>, Vec<DrawRange>) {
    let mut requests = Vec::new();
    let mut draw_ranges = Vec::new();
    for group in &self.clip_groups {
        let first_instance = requests.len() as u32;
        requests.extend_from_slice(&group.image_requests);
        let count = group.image_requests.len() as u32;
        draw_ranges.push(DrawRange { first_instance, count });
    }
    (requests, draw_ranges)
}
```

- [ ] **Step 5: Register image_instance module in lib.rs**

In `vexo/src/lib.rs`, add module declaration:

```rust
mod image_instance;
```

- [ ] **Step 6: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/render/command.rs vexo/src/render/command_processor.rs vexo/src/image_instance.rs vexo/src/frame_builder.rs vexo/src/lib.rs
git commit -m "feat: add RenderCommand::Image, ImageInstance, ImageRequest, FrameBuilder image support"
```

---

### Task 4: Image WGSL shader and GPU pipeline

**Files:**
- Create: `vexo/src/image_shader.wgsl`
- Modify: `vexo/src/resource/file.rs` (add IMAGE_WGSL const)
- Modify: `vexo/src/render/wgpu_backend.rs` (add image pipeline, atlas texture, instance buffer, upload methods)
- Modify: `vexo/src/text_pipeline.rs` (pass image ranges to execute_render_pass)

- [ ] **Step 1: Write the image shader**

Create `vexo/src/image_shader.wgsl`:

```wgsl
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_origin: vec2<f32>,
    @location(2) uv_size: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) corner_radius: f32,
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
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);
    let atlas_uv = in.uv_origin + in.uv * in.uv_size;
    let tex_color = textureSample(image_atlas, image_sampler, atlas_uv);

    if (radius < 0.5) {
        return tex_color;
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

    return vec4<f32>(tex_color.rgb, tex_color.a * fill_alpha);
}
```

- [ ] **Step 2: Register shader in resource module**

In `vexo/src/resource/file.rs`, add alongside the existing `WGSL` const:

```rust
pub const IMAGE_WGSL: &str = include_str!("../image_shader.wgsl");
```

- [ ] **Step 3: Add image pipeline fields to WgpuBackend**

In `vexo/src/render/wgpu_backend.rs`, add imports:

```rust
use crate::image_instance::ImageInstance;
use crate::image_data::ImageData;
use crate::image_atlas::{AtlasRegion, ImageKey, ShelfAllocator};
```

Add fields to `WgpuBackend` struct (after `clear_color`):

```rust
// Image rendering
image_pipeline: wgpu::RenderPipeline,
image_instance_buffer: wgpu::Buffer,
image_instance_buffer_capacity: usize,
image_atlas_bind_group: wgpu::BindGroup,
image_atlas_texture: wgpu::Texture,
image_allocator: ShelfAllocator,
```

Note: `image_atlas_bind_group_layout`, `image_atlas_texture_view`, and `image_atlas_sampler` are only needed during initialization. They don't need to be stored as fields after the bind group is created.

- [ ] **Step 4: Initialize image pipeline in WgpuBackend::init()**

In the `init()` method, after the solid quad pipeline creation and before glyphon init, add:

```rust
// --- Image Atlas and Pipeline ---
const ATLAS_SIZE: u32 = 2048;

let image_allocator = ShelfAllocator::new(ATLAS_SIZE, ATLAS_SIZE);

let image_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("Image Atlas Texture"),
    size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::Rgba8UnormSrgb,
    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    view_formats: &[],
});

let image_atlas_texture_view = image_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

let image_atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    label: Some("Image Atlas Sampler"),
    address_mode_u: wgpu::AddressMode::ClampToEdge,
    address_mode_v: wgpu::AddressMode::ClampToEdge,
    address_mode_w: wgpu::AddressMode::ClampToEdge,
    mag_filter: wgpu::FilterMode::Linear,
    min_filter: wgpu::FilterMode::Linear,
    mipmap_filter: wgpu::FilterMode::Nearest,
    ..Default::default()
});

let image_atlas_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("Image Atlas Bind Group Layout"),
    entries: &[
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ],
});

let image_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Image Atlas Bind Group"),
    layout: &image_atlas_bind_group_layout,
    entries: &[
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&image_atlas_texture_view),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::Sampler(&image_atlas_sampler),
        },
    ],
});

let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Image Shader"),
    source: wgpu::ShaderSource::Wgsl(crate::resource::file::IMAGE_WGSL.into()),
});

let image_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: Some("Image Pipeline Layout"),
    bind_group_layouts: &[&global_bind_group_layout, &image_atlas_bind_group_layout],
    push_constant_ranges: &[],
});

let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    label: Some("Image Render Pipeline"),
    layout: Some(&image_pipeline_layout),
    vertex: wgpu::VertexState {
        module: &image_shader,
        entry_point: Some("vs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        buffers: &[Vertex::desc(), ImageInstance::desc()],
    },
    fragment: Some(wgpu::FragmentState {
        module: &image_shader,
        entry_point: Some("fs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        targets: &[Some(wgpu::ColorTargetState {
            format: config.format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })],
    }),
    primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    },
    depth_stencil: None,
    multisample: wgpu::MultisampleState {
        count: 1,
        mask: !0,
        alpha_to_coverage_enabled: false,
    },
    multiview: None,
    cache: None,
});

const INITIAL_IMAGE_INSTANCE_CAPACITY: usize = 100;

let image_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("Image Instance Buffer"),
    size: (std::mem::size_of::<ImageInstance>() * INITIAL_IMAGE_INSTANCE_CAPACITY) as wgpu::BufferAddress,
    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
});
```

Add these fields to the `Ok(Self { ... })` return:

```rust
image_pipeline,
image_instance_buffer,
image_instance_buffer_capacity: INITIAL_IMAGE_INSTANCE_CAPACITY,
image_atlas_bind_group,
image_atlas_texture,
image_allocator,
```

- [ ] **Step 5: Add image registration and upload methods to WgpuBackend**

Add to `impl WgpuBackend`:

```rust
/// Register image data in the atlas. Returns an ImageKey.
pub fn register_image(&mut self, image_data: &ImageData) -> ImageKey {
    let (key, region) = self.image_allocator.allocate(image_data.width, image_data.height);

    self.queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &self.image_atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x: region.x, y: region.y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        &image_data.pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(image_data.width * 4),
            rows_per_image: Some(image_data.height),
        },
        wgpu::Extent3d {
            width: image_data.width,
            height: image_data.height,
            depth_or_array_layers: 1,
        },
    );

    key
}

/// Remove an image from the atlas allocator.
pub fn unregister_image(&mut self, key: ImageKey) {
    self.image_allocator.remove(key);
}

/// Get the atlas region for an image key.
pub fn get_image_region(&self, key: ImageKey) -> Option<&AtlasRegion> {
    self.image_allocator.get_region(key)
}

fn ensure_image_instance_capacity(&mut self, required: usize) {
    if required <= self.image_instance_buffer_capacity {
        return;
    }
    let new_capacity = required.max(self.image_instance_buffer_capacity * 2);
    let new_size = (std::mem::size_of::<ImageInstance>() * new_capacity) as wgpu::BufferAddress;
    self.image_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Image Instance Buffer"),
        size: new_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    self.image_instance_buffer_capacity = new_capacity;
}

/// Upload image geometry from frame builder to GPU buffers.
pub fn upload_image_geometry(&mut self, frame_builder: &FrameBuilder) {
    let (requests, _ranges) = frame_builder.flatten_image_requests();
    if requests.is_empty() { return; }

    let atlas_size = [
        self.image_allocator.atlas_width() as f32,
        self.image_allocator.atlas_height() as f32,
    ];

    let instances: Vec<ImageInstance> = requests.iter().map(|req| {
        let region = self.image_allocator.get_region(req.image_key)
            .expect("Image key not found in atlas");
        ImageInstance::from_logical(
            req.position,
            req.size,
            region,
            atlas_size,
            req.corner_radius,
            AffineTransform::from_array(req.transform),
        )
    }).collect();

    self.ensure_image_instance_capacity(instances.len());
    self.queue.write_buffer(
        &self.image_instance_buffer,
        0,
        bytemuck::cast_slice(&instances),
    );
}
```

Add import at top of file:

```rust
use crate::core::AffineTransform;
```

- [ ] **Step 6: Modify execute_render_pass to draw image quads**

Change `execute_render_pass` signature to accept image draw ranges:

```rust
pub fn execute_render_pass(
    &mut self,
    clip_groups: &[ClipGroup],
    draw_ranges: &[DrawRange],
    image_draw_ranges: &[DrawRange],
    scale_factor: f32,
    viewport_width: u32,
    viewport_height: u32,
) -> Result<(), RenderError> {
```

After the solid quad drawing loop and before the text rendering section, add:

```rust
// Draw image quads per clip group
{
    let has_images = image_draw_ranges.iter().any(|r| r.count > 0);
    if has_images {
        render_pass.set_pipeline(&self.image_pipeline);
        render_pass.set_bind_group(0, &self.global_bind_group, &[]);
        render_pass.set_bind_group(1, &self.image_atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        for (group, range) in clip_groups.iter().zip(image_draw_ranges.iter()) {
            if range.count == 0 { continue; }

            if let Some(clip) = &group.clip_bounds {
                let x = (clip.left * scale_factor).max(0.0) as u32;
                let y = (clip.top * scale_factor).max(0.0) as u32;
                let right = (clip.right * scale_factor).min(viewport_width as f32) as u32;
                let bottom = (clip.bottom * scale_factor).min(viewport_height as f32) as u32;
                let w = right.saturating_sub(x);
                let h = bottom.saturating_sub(y);
                if w == 0 || h == 0 { continue; }
                render_pass.set_scissor_rect(x, y, w, h);
            } else {
                render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
            }

            render_pass.draw_indexed(0..6, 0, range.first_instance..range.first_instance + range.count);
        }
    }
}
```

- [ ] **Step 7: Update TextPipeline::execute_render() to pass image ranges**

In `vexo/src/text_pipeline.rs`, modify `execute_render()`:

```rust
pub fn execute_render(
    &mut self,
    backend: &mut WgpuBackend,
    frame_builder: &FrameBuilder,
    mut prepared_text: CombinedPreparedText,
    font_system: &mut glyphon::FontSystem,
) -> Result<(), RenderError> {
    backend.upload_geometry(frame_builder);
    backend.upload_image_geometry(frame_builder);

    let flattened = frame_builder.flatten_quads();
    let clip_groups = frame_builder.clip_groups();
    let (_, image_draw_ranges) = frame_builder.flatten_image_requests();

    backend.prepare_text(font_system, prepared_text.as_text_areas());

    let scale_factor = backend.current_config()
        .map(|c| c.scale_factor())
        .unwrap_or(1.0);
    let viewport_width = backend.width();
    let viewport_height = backend.height();

    backend.execute_render_pass(
        clip_groups,
        &flattened.draw_ranges,
        &image_draw_ranges,
        scale_factor,
        viewport_width,
        viewport_height,
    )?;

    Ok(())
}
```

- [ ] **Step 8: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/image_shader.wgsl vexo/src/resource/file.rs vexo/src/render/wgpu_backend.rs vexo/src/text_pipeline.rs
git commit -m "feat: add image GPU pipeline, atlas texture, shader, and render pass integration"
```

---

### Task 5: Image Widget, ImageRenderObject, and pipeline integration

**Files:**
- Create: `vexo/src/widgets/image.rs`
- Create: `vexo/src/render_objects/image.rs`
- Modify: `vexo/src/render_object.rs` (add needs_image_registration / set_image_key defaults)
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/render_objects/mod.rs`
- Modify: `vexo/src/pipeline.rs` (add register_images method)
- Modify: `vexo/src/window.rs` (call register_images before paint)
- Modify: `vexo/src/lib.rs` (re-export Image, ImageData)

- [ ] **Step 1: Add needs_image_registration and set_image_key to RenderObject trait**

In `vexo/src/render_object.rs`, add default methods to the `RenderObject` trait (after `scroll_offset`):

```rust
/// Check if this render object needs its image registered in the atlas.
///
/// Returns Some(ImageData) if the image hasn't been registered yet.
/// The pipeline calls this during the render loop and registers
/// the image via WgpuBackend::register_image().
fn needs_image_registration(&self) -> Option<&crate::image_data::ImageData> {
    None
}

/// Set the image key after registration in the atlas.
///
/// Called by the pipeline after registering the image via WgpuBackend.
fn set_image_key(&mut self, _key: crate::image_atlas::ImageKey) {}
```

- [ ] **Step 2: Create Image widget**

Create `vexo/src/widgets/image.rs`:

```rust
use crate::core::Color;
use crate::image_data::{ImageData, ImageDataError};
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::render_object::RenderObject;
use crate::render_objects::ImageRenderObject;
use crate::style::Style;
use crate::update_result::UpdateResult;
use crate::widgets::Widget;
use crate::elements::LeafElement;
use crate::element::Element;

pub struct Image {
    key: Option<WidgetKey>,
    image_data: ImageData,
    style: Style,
    layout: Layout,
}

impl Image {
    pub fn new(image_data: ImageData) -> Self {
        Self {
            key: None,
            image_data,
            style: Style::default(),
            layout: Layout::default(),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageDataError> {
        let image_data = ImageData::from_bytes(bytes)?;
        Ok(Self::new(image_data))
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn image_data(&self) -> &ImageData {
        &self.image_data
    }

    modifier_methods!();
}

impl Clone for Image {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            image_data: self.image_data.clone(),
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Image {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut element = LeafElement::new();
        element.set_widget(self);
        Box::new(element)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ImageRenderObject::new(
            &self.image_data,
            self.style.clone(),
            self.layout.clone(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        let ro = render_object.as_any_mut().downcast_mut::<ImageRenderObject>().unwrap();
        let mut result = UpdateResult::NONE;
        if ro.set_image_data(&self.image_data) { result |= UpdateResult::PAINT; }
        if ro.set_style(self.style.clone()) { result |= UpdateResult::PAINT; }
        if ro.set_layout(self.layout.clone()) { result |= UpdateResult::LAYOUT; }
        result
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}
```

- [ ] **Step 3: Create ImageRenderObject**

Create `vexo/src/render_objects/image.rs`:

```rust
use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size, Stroke};
use crate::image_atlas::ImageKey;
use crate::image_data::ImageData;
use crate::layout::{Layout, LayoutNodeKey, Dimension};
use crate::layout::engine::LayoutEngine;
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};

pub struct ImageRenderObject {
    image_data: ImageData,
    image_key: Option<ImageKey>,
    style: Style,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl ImageRenderObject {
    pub fn new(image_data: &ImageData, style: Style, layout: Layout) -> Self {
        Self {
            image_data: image_data.clone(),
            image_key: None,
            style,
            layout,
            computed_bounds: None,
            layout_node: None,
        }
    }

    pub fn set_image_data(&mut self, data: &ImageData) -> bool {
        if self.image_data.width != data.width
            || self.image_data.height != data.height
            || self.image_data.pixels != data.pixels
        {
            self.image_data = data.clone();
            self.image_key = None; // Needs re-registration
            true
        } else {
            false
        }
    }

    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }
}

impl RenderObject for ImageRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let intrinsic_width = self.image_data.width as f32;
        let intrinsic_height = self.image_data.height as f32;

        let effective_layout = Layout {
            width: self.layout.width.or(Some(Dimension::Points(intrinsic_width))),
            height: self.layout.height.or(Some(Dimension::Points(intrinsic_height))),
            ..self.layout.clone()
        };

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &effective_layout);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&effective_layout);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
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

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => {
                let mut commands = Vec::new();
                let pos: Position<Logical, Absolute> = ctx.absolute_position();

                let absolute_bounds = Bounds::new(
                    pos.x,
                    pos.y,
                    pos.x + bounds.width(),
                    pos.y + bounds.height(),
                );

                let corner_radius = self.style.corner_radius.map_or(0.0, |cr| cr.radius);

                // 1. Push corner radius if set
                if let Some(ref cr) = self.style.corner_radius {
                    commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
                }

                // 2. Background
                if let Some(bg_color) = self.style.background {
                    commands.push(RenderCommand::rect(absolute_bounds, bg_color));
                }

                // 3. Border
                if let Some(ref border) = self.style.border {
                    commands.push(RenderCommand::rect_with_border(
                        absolute_bounds,
                        Color::TRANSPARENT,
                        border.color,
                        border.width,
                    ));
                }

                // 4. Pop corner radius
                if self.style.corner_radius.is_some() {
                    commands.push(RenderCommand::PopCornerRadius);
                }

                // 5. Image (only if registered in atlas)
                if let Some(key) = self.image_key {
                    commands.push(RenderCommand::Image {
                        bounds: absolute_bounds,
                        image_key: key,
                        corner_radius,
                    });
                }

                commands
            }
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        self.computed_bounds
            .map(|b| b.contains(&position))
            .unwrap_or(false)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }

    fn needs_image_registration(&self) -> Option<&ImageData> {
        if self.image_key.is_none() {
            Some(&self.image_data)
        } else {
            None
        }
    }

    fn set_image_key(&mut self, key: ImageKey) {
        self.image_key = Some(key);
    }
}
```

- [ ] **Step 4: Register widget and render object modules**

In `vexo/src/widgets/mod.rs`, add:

```rust
mod image;
pub use image::Image;
```

In `vexo/src/render_objects/mod.rs`, add:

```rust
mod image;
pub use image::ImageRenderObject;
```

In `vexo/src/lib.rs`, add `Image` to the widget re-exports line (the `pub use widgets::{...}` line). Also add `ImageData` and `ImageDataError` re-exports if not already done in Task 2.

- [ ] **Step 5: Add register_images pass to pipeline**

In `vexo/src/pipeline.rs`, add a method to `ThreeTreePipeline`:

```rust
/// Register images in the atlas for render objects that need it.
pub fn register_images(&mut self, backend: &mut crate::render::WgpuBackend) {
    for (_, ro) in self.render_objects.objects.iter_mut() {
        if let Some(image_data) = ro.needs_image_registration() {
            let key = backend.register_image(image_data);
            ro.set_image_key(key);
        }
    }
}
```

Note: The field is `self.render_objects` which is a `RenderObjectRegistry`. The `objects` field inside is a `SlotMap`. Check whether `SlotMap` provides `iter_mut()` — it does. If `objects` is not public, add an `iter_mut()` method to `RenderObjectRegistry`:

```rust
/// Iterate mutably over all render objects.
pub fn iter_mut(&mut self) -> impl Iterator<Item = (RenderObjectKey, &mut Box<dyn RenderObject>)> {
    self.objects.iter_mut()
}
```

Then the pipeline call becomes:

```rust
for (_, ro) in self.render_objects.iter_mut() {
    if let Some(image_data) = ro.needs_image_registration() {
        let key = backend.register_image(image_data);
        ro.set_image_key(key);
    }
}
```

- [ ] **Step 6: Call register_images in window render loop**

In `vexo/src/window.rs`, in `render_retain()`, find the point after layout and before paint, and add:

```rust
self.three_tree_pipeline.register_images(&mut self.backend);
```

The exact insertion point: after the `layout()` call and before the `paint()` call. Look for the comment or code structure that does:

```rust
// Layout dirty render objects
// ...
// Paint dirty render objects
```

Insert the `register_images` call between these two steps.

- [ ] **Step 7: Build and verify**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/image.rs vexo/src/render_objects/image.rs vexo/src/render_object.rs vexo/src/widgets/mod.rs vexo/src/render_objects/mod.rs vexo/src/pipeline.rs vexo/src/window.rs vexo/src/lib.rs
git commit -m "feat: add Image widget, ImageRenderObject, and image registration pass"
```

---

### Task 6: Visual demo in shared_app

**Files:**
- Modify: `shared_app/Cargo.toml`
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add image dependency to shared_app**

In `shared_app/Cargo.toml`, add:

```toml
image = { workspace = true }
```

- [ ] **Step 2: Create test image data helper and add Image section to demo**

In `shared_app/src/lib.rs`, add a helper function:

```rust
fn create_test_image_data() -> vexo::ImageData {
    let img = image::RgbaImage::from_fn(200, 150, |x, y| {
        let r = (x as f32 / 200.0 * 255.0) as u8;
        let g = (y as f32 / 150.0 * 255.0) as u8;
        let b = 128u8;
        image::Rgba([r, g, b, 255])
    });
    let mut jpeg_bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
    encoder.write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    ).unwrap();
    vexo::ImageData::from_bytes(&jpeg_bytes).unwrap()
}
```

Then modify the `view()` function to add an Image section. The exact structure depends on the current demo layout. Add the image to the existing demo alongside the ScrollView content, something like:

```rust
let test_image = create_test_image_data();

// In the view function, add:
// vexo::Image::new(test_image).width(200.0).corner_radius(8.0)
```

The integration details will depend on reading the current `shared_app/src/lib.rs` at implementation time to see where to insert the image widget.

- [ ] **Step 3: Build and run visually**

Run: `cargo run -p desktop_demo`
Expected: Desktop window opens with the gradient JPEG image displayed with rounded corners.

- [ ] **Step 4: Commit**

```bash
git add shared_app/Cargo.toml shared_app/src/lib.rs
git commit -m "feat: add Image demo to shared_app"
```

---

### Task 7: Tests

**Files:**
- Modify: `vexo/src/render_objects/image.rs` (add #[cfg(test)] block)
- Modify: `vexo/src/frame_builder.rs` (add image tests)

- [ ] **Step 1: Write tests for ImageRenderObject**

Add to `vexo/src/render_objects/image.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, Point, Size};
    use crate::render::RenderCommand;
    use crate::HitTestContext;

    fn make_test_image_data() -> ImageData {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        let mut jpeg_bytes = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
        encoder.write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        ).unwrap();
        ImageData::from_bytes(&jpeg_bytes).unwrap()
    }

    #[test]
    fn test_image_render_object_paint_with_key() {
        let data = make_test_image_data();
        let mut ro = ImageRenderObject::new(&data, Style::default(), Layout::default());
        ro.image_key = Some(42);
        ro.computed_bounds = Some(Bounds::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        let has_image = result.iter().any(|c| matches!(c, RenderCommand::Image { .. }));
        assert!(has_image);
    }

    #[test]
    fn test_image_render_object_paint_with_style() {
        let data = make_test_image_data();
        let mut ro = ImageRenderObject::new(
            &data,
            Style::new().background(Color::BLUE).corner_radius(8.0),
            Layout::default(),
        );
        ro.image_key = Some(1);
        ro.computed_bounds = Some(Bounds::new(Point::new(0.0, 0.0), Size::new(200.0, 150.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        assert!(result.iter().any(|c| matches!(c, RenderCommand::PushCornerRadius { .. })));
        assert!(result.iter().any(|c| matches!(c, RenderCommand::PopCornerRadius)));
        assert!(result.iter().any(|c| matches!(c, RenderCommand::Image { .. })));
    }

    #[test]
    fn test_image_render_object_no_paint_without_key() {
        let data = make_test_image_data();
        let ro = ImageRenderObject::new(&data, Style::default(), Layout::default());

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        let has_image = result.iter().any(|c| matches!(c, RenderCommand::Image { .. }));
        assert!(!has_image);
    }

    #[test]
    fn test_image_render_object_set_image_data_change_detection() {
        let data1 = make_test_image_data();
        let data2 = {
            let img = image::RgbaImage::from_pixel(20, 20, image::Rgba([0, 255, 0, 255]));
            let mut jpeg_bytes = Vec::new();
            let encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
            encoder.write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            ).unwrap();
            ImageData::from_bytes(&jpeg_bytes).unwrap()
        };
        let mut ro = ImageRenderObject::new(&data1, Style::default(), Layout::default());
        assert!(ro.set_image_data(&data2));
        assert!(!ro.set_image_data(&data2)); // Same data, no change
    }

    #[test]
    fn test_image_render_object_needs_image_registration() {
        let data = make_test_image_data();
        let mut ro = ImageRenderObject::new(&data, Style::default(), Layout::default());

        // Initially needs registration
        assert!(ro.needs_image_registration().is_some());

        // After setting key, no longer needs registration
        ro.set_image_key(1);
        assert!(ro.needs_image_registration().is_none());
    }
}
```

- [ ] **Step 2: Write tests for FrameBuilder image support**

Add to `vexo/src/frame_builder.rs` test module (or create one if none exists):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AffineTransform;

    #[test]
    fn test_add_image_request() {
        let mut fb = FrameBuilder::new();
        fb.add_image(ImageRequest {
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            image_key: 1,
            corner_radius: 8.0,
            transform: AffineTransform::identity().to_array(),
        });

        assert_eq!(fb.image_count(), 1);
    }

    #[test]
    fn test_flatten_image_requests() {
        let mut fb = FrameBuilder::new();
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [50.0, 50.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
        });

        fb.push_clip(Bounds::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0)));
        fb.add_image(ImageRequest {
            position: [10.0, 10.0],
            size: [30.0, 30.0],
            image_key: 2,
            corner_radius: 4.0,
            transform: AffineTransform::identity().to_array(),
        });
        fb.pop_clip();

        let (requests, ranges) = fb.flatten_image_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(ranges.len(), 2);
    }
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass, including new image-related tests.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/render_objects/image.rs vexo/src/frame_builder.rs
git commit -m "test: add ImageRenderObject and FrameBuilder image integration tests"
```

---

### Task 8: Final build verification and cleanup

- [ ] **Step 1: Run full workspace build**

Run: `cargo build`
Expected: Clean build with no errors.

- [ ] **Step 2: Run all workspace tests**

Run: `cargo test`
Expected: All workspace tests pass.

- [ ] **Step 3: Run desktop demo for visual verification**

Run: `cargo run -p desktop_demo`
Expected: Image renders correctly with rounded corners.

- [ ] **Step 4: Check for warnings**

Run: `cargo build -p vexo 2>&1 | grep -i warning`
Expected: No new warnings from image-related code.

- [ ] **Step 5: Final commit (if any cleanup needed)**

```bash
git add -A
git commit -m "chore: cleanup image widget implementation"
```
