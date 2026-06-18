# Image Widget Design Spec

## Problem

Vexo cannot display images. Every production app needs image support — icons, photos, illustrations, placeholders. Without it, the framework cannot build real UIs.

## Requirements

- Display JPEG images from embedded bytes (`include_bytes!`)
- Image widget with full modifier system support (`.background()`, `.padding()`, `.corner_radius()`, etc.)
- Rounded corner clipping on images via corner_radius modifier
- Fill fit mode only (image stretches to layout box)
- Texture atlas for GPU rendering (one draw call for all images)
- Demo in shared_app

## Approach

Texture atlas with shelf-based packing. All images are packed into a single 2048x2048 wgpu texture. A separate GPU pipeline renders textured quads, drawn after solid quads in the same render pass (same pattern as glyphon text rendering).

## Widget, Element, and Render Object

### Image Widget (`vexo/src/widgets/image.rs`)

```rust
pub struct Image {
    image_data: ImageData,
    style: Style,
    layout: Layout,
    key: Option<WidgetKey>,
}
```

- `ImageData` holds decoded RGBA pixels and dimensions (width, height). Decoded once at construction time via the `image` crate from embedded bytes.
- `ImageData::from_bytes(bytes: &[u8])` — decodes JPEG, returns `Result<ImageData, ImageDataError>`. Synchronous, no async (embedded assets).
- Full `style` and `layout` fields for modifier compatibility.
- Inherent modifier methods via `layout_builder_methods!()` macro, plus style-setting methods (`.background()`, `.border()`, `.corner_radius()`, `.clip()`) — same pattern as other widgets.

### ImageElement (`vexo/src/elements/image.rs`)

- Leaf element (no children), similar to `LeafElement`.
- On `mount()`: creates `ImageRenderObject`, registers `ImageData` with `ImageAtlas`.
- On `rebuild()`: if `image_data` changed, updates render object and re-registers with atlas.

### ImageRenderObject (`vexo/src/render_objects/image.rs`)

- Holds `image_key: Option<ImageKey>`, `style: Style`, `layout: Layout`, plus standard bounds/layout_node fields.
- `paint()` emits:
  1. Optional `PushCornerRadius` + background `Rect` + border `Rect` + `PopCornerRadius` (from style)
  2. `RenderCommand::Image { bounds, image_key, corner_radius }`
- `scroll_offset()`: returns `None`
- `clip_bounds()`: returns viewport bounds if style has clip or corner_radius > 0

## Image Atlas and GPU Pipeline

### ImageAtlas (`vexo/src/image_atlas.rs`)

Shelf-based texture atlas packing all images into a single wgpu texture.

```rust
pub struct ImageAtlas {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    shelves: Vec<Shelf>,
    images: HashMap<ImageKey, AtlasRegion>,
    next_key: u64,
}

pub struct AtlasRegion {
    pub x: u32, pub y: u32,
    pub width: u32, pub height: u32,
}

pub struct Shelf {
    y: u32, height: u32,
    x_cursor: u32,
    remaining_width: u32,
}
```

- **Atlas size**: 2048x2048 RGBA8 texture (16MB). Sufficient for embedded-only launch.
- **Packing**: Shelf allocator. Find first shelf with enough remaining width and whose height ≥ image height. If no shelf fits, create a new shelf at the image's height. If atlas is full (not enough vertical space for a new shelf), panic (embedded images are bounded).
- **Registration**: `register(device: &wgpu::Device, queue: &wgpu::Queue, image_data: &ImageData) -> ImageKey`. Writes pixel data to atlas texture via `queue.write_texture()` with region offset.
- **Lookup**: `get_region(key: ImageKey) -> Option<AtlasRegion>`. Used by shader to compute UVs.
- **Lifecycle**: Created once during `WgpuBackend` initialization. Images registered during element mount. No eviction needed for embedded-only launch.

**ImageKey**: `u64` identifier, monotonically incremented. Maps to `AtlasRegion` in the atlas.

### Image Shader (`vexo/src/image_shader.wgsl`)

Separate pipeline for textured quads, drawn after solid quads in the same render pass.

- **Vertex shader**: Takes instance data (position, size, UV origin, UV size, corner_radius, transform). Outputs position in NDC and UV coordinates to fragment.
- **Fragment shader**: Samples atlas texture at interpolated UV. Applies corner_radius clipping via SDF (same rounded-rect technique as solid quad shader). Outputs premultiplied alpha color.
- **Bind group layout**: Group 0 = global uniforms (screen_size, scale_factor). Group 1 = atlas texture + sampler.

### ImageInstance (`vexo/src/image_instance.rs`)

GPU instance data for image quads:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ImageInstance {
    position: [f32; 2],
    size: [f32; 2],
    uv_origin: [f32; 2],
    uv_size: [f32; 2],
    corner_radius: f32,
    transform: [f32; 6],
    _padding: [f32; 2],
}
```

### Rendering Flow (in `WgpuBackend::execute_render_pass()`)

1. Draw solid quads per clip group (existing)
2. Draw image quads per clip group (new — same scissor rect pattern)
3. Draw text (existing — glyphon)

Image quads use their own pipeline + bind group (group 0 = global uniforms, group 1 = atlas texture). Each clip group's image instances drawn with same scissor rect as its solid quads.

## RenderCommand and Pipeline Integration

### New RenderCommand variant (`vexo/src/render/command.rs`)

```rust
pub enum RenderCommand {
    // ...existing variants...
    Image {
        bounds: Bounds<Logical>,
        image_key: ImageKey,
        corner_radius: f32,
    },
}
```

### CommandProcessor changes (`vexo/src/render/command_processor.rs`)

Handle `RenderCommand::Image` by calling `FrameBuilder::add_image()`, offset by current offset stack, transformed by current transform stack.

### FrameBuilder changes (`vexo/src/frame_builder.rs`)

Add `image_requests: Vec<ImageRequest>` to each `ClipGroup`:

```rust
pub struct ClipGroup {
    clip_bounds: Option<Bounds<Logical>>,
    quads: Vec<QuadInstance>,
    text_requests: Vec<TextRequest>,
    image_requests: Vec<ImageRequest>,
}

pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub corner_radius: f32,
    pub transform: [f32; 6],
}
```

New method `add_image(request: ImageRequest)` pushes to current clip group's `image_requests`.

### WgpuBackend changes (`vexo/src/render/wgpu_backend.rs`)

- Store `ImageAtlas`, image pipeline, image instance buffer, image vertex buffer (static quad mesh, same as solid quads).
- `upload_image_geometry()`: Flatten all clip groups' image requests into `Vec<ImageInstance>`, resolve each `ImageKey` to atlas UV coordinates via `ImageAtlas::get_region()`, write to GPU instance buffer.
- In `execute_render_pass()`: After solid quads and before text, for each clip group: set image pipeline + bind groups, set scissor rect, draw image instances.

### TextPipeline / WindowState changes

- `TextPipeline::execute_render()` passes `ImageAtlas` to `WgpuBackend` for image key resolution during geometry upload.
- `WindowState` creates `ImageAtlas` during initialization (alongside device/surface setup).
- `ImageAtlas::register()` accessible during element mounting.

## Public API

```rust
pub use widgets::image::{Image, ImageData};
```

`ImageKey` and `ImageAtlas` are not exported — implementation details.

### Usage examples

```rust
// Basic image from embedded bytes
Image::new(ImageData::from_bytes(include_bytes!("../assets/photo.jpg")))

// Image with modifier chain
Image::new(ImageData::from_bytes(include_bytes!("../assets/photo.jpg")))
    .corner_radius(12.0)
    .padding(8.0)

// Image in a layout
Flex::column()
    .gap(8.0)
    .push(Image::new(my_image_data).width(200.0).corner_radius(8.0))
    .push(Text::new("Caption"))
```

### Demo in shared_app

Add an Image section to the existing demo, showing a JPEG loaded from embedded bytes with rounded corners.

## Edge Cases

- **Atlas overflow**: Panic at registration time if image doesn't fit. Acceptable for embedded-only launch.
- **Zero-size image**: `ImageData::from_bytes()` returns error if decoding produces 0x0 pixels. `Image` widget panics with clear message.
- **Image larger than atlas**: Panic with message indicating atlas size limit. 2048x2048 atlas supports images up to 2048px on either dimension.
- **Multiple images with same bytes**: Each `ImageData::from_bytes()` call produces a new `ImageKey` and registers a new atlas region. No deduplication at launch.
- **Rebuilds with same image**: If `can_update()` returns true (same widget type), element reuses existing `ImageKey`. If image_data changes, old key unregistered and new one registered.
- **Corner radius on images**: Image shader clips pixels outside rounded rect SDF. Visual clipping only — hit testing uses rectangular bounds.

## Testing

- Unit tests for `ImageData::from_bytes()`: valid JPEG decodes correctly, invalid bytes return error.
- Unit tests for `ImageAtlas`: shelf packing correctness, region lookup by key, multiple registrations produce correct UVs.
- Unit tests for `ImageRenderObject::paint()`: emits `RenderCommand::Image` with correct bounds and image_key. Style modifiers emit correct Rect/CornerRadius commands.
- Integration test: Image widget through full pipeline (widget → element → render object → RenderCommands). Verify `Image` command appears alongside Rect commands for style.
- Integration test with `MockBackend`: Verify image commands processed into FrameBuilder image_requests with correct clip groups.
- Visual test via shared_app demo: JPEG renders in desktop demo window.

## Files to Create/Modify

### New files
- `vexo/src/widgets/image.rs` — Image widget + ImageData
- `vexo/src/elements/image.rs` — ImageElement
- `vexo/src/render_objects/image.rs` — ImageRenderObject
- `vexo/src/image_atlas.rs` — ImageAtlas, AtlasRegion, Shelf, ImageKey
- `vexo/src/image_shader.wgsl` — WGSL shader for textured quads
- `vexo/src/image_instance.rs` — ImageInstance struct (Pod/Zeroable for GPU)

### Modified files
- `vexo/src/render/command.rs` — add `Image` variant to `RenderCommand`
- `vexo/src/render/command_processor.rs` — handle `Image` command
- `vexo/src/frame_builder.rs` — add `image_requests` to ClipGroup, add `add_image()`
- `vexo/src/render/wgpu_backend.rs` — image pipeline, atlas, instance buffer, render pass integration
- `vexo/src/window.rs` — create ImageAtlas during init, pass through pipeline
- `vexo/src/widgets/mod.rs` — register Image widget
- `vexo/src/elements/mod.rs` — register ImageElement
- `vexo/src/render_objects/mod.rs` — register ImageRenderObject
- `vexo/src/lib.rs` — re-export Image, ImageData
- `vexo/Cargo.toml` — add `image` crate dependency
- `shared_app/src/lib.rs` — add Image demo section
