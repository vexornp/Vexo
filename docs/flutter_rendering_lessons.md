# Flutter Rendering Lessons for Vexo

This document captures lessons learned from analyzing Flutter/Impeller's rendering pipeline and how they apply to Vexo.

## Background

**Flutter/Impeller approach:**
- Each rect is a separate entity with 4 vertices in a triangle strip
- Rounded rects are tessellated into more triangles
- Clipping uses stencil buffer + render passes
- All shaders compiled at build time (AOT via `impellerc`)
- Per-entity draw calls, batched by pipeline state

**Vexo approach:**
- Instanced rendering: single unit-quad (4 verts, 6 indices), N instances
- Rounded rects use SDF in fragment shader (same 4 vertices)
- Clipping uses `discard` in fragment shader
- Shaders compiled at runtime via wgpu
- Single `draw_indexed(0..6, 0, 0..N)` for all quads

---

## Priority 1: Fix PushOffset Bug — DONE

**Status:** Fixed in commit `4f0cbb3`

**Problem:** `window.rs` had an inline command loop that silently dropped `PushOffset`/`PopOffset` commands with `// TODO` stubs. The `command_processor.rs` module already had working offset stack logic but wasn't being used.

**Fix:** Replaced the 37-line inline loop with a 4-line call to `process_commands()`, which handles all command types including `PushOffset`/`PopOffset` with its existing offset stack logic.

**Note:** No widget currently emits `PushOffset`/`PopOffset` — they are infrastructure for a future `Offset`/`Transform.translate` widget. The fix removes a latent bug so that when such a widget is added, it will work correctly.

---

## Priority 2: Dynamic Instance Buffer

**Status:** Done — grow-only dynamic buffer with `ensure_instance_capacity()`

**Location:** `vexo/src/render/wgpu_backend.rs` line 231

**Problem:**
```rust
size: (std::mem::size_of::<QuadInstance>() * 10000) as wgpu::BufferAddress,
```

Fixed 10,000 instance limit. If a frame has more quads, it will overflow with no error handling.

**Fix:**
1. Track instance count during frame building
2. If count exceeds current buffer capacity, create a larger buffer
3. Or: split into multiple draw calls when buffer is full
4. Consider using `wgpu::BufferDescriptor::mapped_at_creation` for staging

**Impact:** Robustness - prevents crashes on complex UIs.

---

## Priority 3: Scissor-Based Clipping

**Status:** Done — replaced `discard`-based clipping with wgpu `set_scissor_rect()`

**Location:** `vexo/src/shader.wgsl` lines 59-75

**Current implementation:**
```wgsl
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

**Problems:**
1. **No early-Z optimization** - GPU cannot skip occluded fragments when `discard` is present
2. **Full quad rasterization** - Entire quad is rasterized even when mostly clipped
3. **Branch divergence** - Different fragments in a warp take different paths

**Flutter's approach:**
- Scissor rectangles for axis-aligned clips (near-zero cost)
- Stencil buffer for complex clips (requires render pass)

**Fix for Vexo:**
Since all Vexo clips are axis-aligned rectangles, scissor rects alone suffice:

```rust
// In render loop:
RenderCommand::PushClip { bounds } => {
    // Convert logical to physical pixels
    let x = (bounds.left * scale_factor) as u32;
    let y = (bounds.top * scale_factor) as u32;
    let w = (bounds.width() * scale_factor) as u32;
    let h = (bounds.height() * scale_factor) as u32;
    render_pass.set_scissor_rect(x, y, w, h);
}
RenderCommand::PopClip => {
    // Restore previous scissor (need a stack)
}
```

**Tradeoff:** Scissor changes break single-draw-call batching. But scissor is a cheap state change (not a pipeline change), so the cost is minimal. Options:
1. One draw call per clip region (simple, still fast)
2. Sort quads by clip region, batch draws per region
3. Keep current approach but add scissor in addition to discard (hybrid)

**Impact:** GPU performance - biggest single win for clipped UIs.

---

## Priority 4: Pipeline Caching

**Status:** Shaders compiled at every startup

**Location:** `vexo/src/render/wgpu_backend.rs` lines 139-142

**Current implementation:**
```rust
let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Shader"),
    source: wgpu::ShaderSource::Wgsl(crate::resource::file::WGSL.into()),
});
```

**Problem:**
No caching - WGSL is compiled to backend-specific shader at every `WgpuBackend::new()`. On Metal, the driver may cache. On Vulkan/WebGL, no guarantee.

**Flutter's approach:**
`impellerc` compiles GLSL → SPIRV → Metal/Vulkan/GLES at **build time**, embeds binary blobs in the engine. Zero runtime compilation.

**Fix for Vexo:**
Don't build an AOT compiler (massive infrastructure). Instead:

1. Use wgpu's `PipelineCache` feature (available since wgpu 0.19):
   ```rust
   let cache = device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
       label: Some("Vexo Pipeline Cache"),
       data: cached_data, // Load from disk if exists
   });
   ```

2. On first run: compile shaders, save cache to disk
3. On subsequent runs: load cache, skip compilation

4. Cache location: `~/.cache/vexo/pipeline_cache.bin` or app data dir

**Impact:** Startup time - eliminates shader compilation jank.

---

## Priority 5: Opaque Fullscreen Quad Skip

**Status:** No occlusion culling, all quads rendered back-to-front

**Location:** `vexo/src/render/wgpu_backend.rs` - no depth buffer

**Problem:**
If a fullscreen opaque quad (e.g., background) is drawn last in submission order but first in visual order, all quads underneath are rendered for nothing. The SDF fragment shader runs on every pixel of every quad.

**Flutter's approach:**
Impeller's `CoversArea` optimization: if an entity fully covers the previous render pass with opaque content, the pass is elided entirely.

**Fix for Vexo:**
Simple optimization for the common case:

1. Track whether we've seen an opaque fullscreen quad
2. If the first quad is opaque and covers the entire viewport:
   - Clear the instance buffer (skip all previous quads)
   - Only render quads on top of the background

```rust
// In FrameBuilder or render loop:
let viewport_bounds = Bounds::new(0, 0, viewport_width, viewport_height);

for (i, instance) in quad_instances.iter().enumerate() {
    if instance.color[3] == 1.0 &&  // Opaque
       instance.corner_radius == 0.0 &&  // No rounded corners
       instance_bounds == viewport_bounds {  // Covers screen
        // Skip all previous instances
        quad_instances.drain(0..i);
        break;
    }
}
```

**Tradeoff:** Only works for the specific case of an opaque fullscreen rect. More complex occlusion (partial overlap, non-rect shapes) requires depth buffer and Z-testing.

**Impact:** Reduce overdraw for common UI patterns (background color).

---

## Priority 6: 2x3 Transform Matrix

**Status:** Translation only, no rotation or skew

**Location:** `vexo/src/quad_instance.rs`

**Current implementation:**
```rust
pub struct QuadInstance {
    pub position: [f32; 2],  // Translation only
    pub size: [f32; 2],      // Scale via size
    // ...
}
```

**Problem:**
Elements cannot be rotated or skewed. Limits UI effects like spinning indicators, rotated labels, perspective transforms.

**Flutter's approach:**
Each entity has a 3x3 transform matrix enabling arbitrary 2D transforms.

**Fix for Vexo:**
Add a 2x3 matrix (6 floats) to `QuadInstance`:

```rust
pub struct QuadInstance {
    pub transform: [f32; 6],  // 2x3: [sx, shy, shx, sy, tx, ty]
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub corner_radius: f32,
    pub clip_bounds: [f32; 4],
    pub _padding: [f32; 2],
}
```

**Vertex shader change:**
```wgsl
// Current:
let pixel_pos = scaled_pos + (model_pos * scaled_size);

// With transform:
let local_pos = model_pos * scaled_size;
let pixel_pos = vec2f(
    transform[0] * local_pos.x + transform[2] * local_pos.y + transform[4],
    transform[1] * local_pos.x + transform[3] * local_pos.y + transform[5]
);
```

**Fragment shader change:**
SDF distance calculations need the inverse transform to compute distance in un-rotated space. Either:
1. Precompute inverse on CPU, add 6 more floats to instance data
2. Compute inverse in vertex shader, pass to fragment shader

**Impact:** Feature completeness - enables rotation and skew transforms.

---

## What NOT to Copy from Flutter

### 1. Don't Tessellate Rounded Rects

Flutter tessellates rounded rects into many small triangles. Vexo's SDF approach is simpler, uses less geometry, and anti-aliases for free. The SDF costs more fragment shader work, but for UI-scale quad counts this is negligible on modern GPUs.

**Keep the SDF.**

### 2. Don't Build a DisplayList Layer

Flutter's DisplayList is a retained intermediate representation enabling caching and partial repaint. Vexo's `Vec<RenderCommand>` is rebuilt every frame. For Vexo's current scope (no partial repaint, no caching), this is the right tradeoff.

Add DisplayList-style retention only when profiling shows command generation is a bottleneck.

### 3. Don't Build a Shader Compiler

`impellerc` is thousands of lines of C++ generating C++ bindings from GLSL. Vexo doesn't need this:
- WGSL is the native wgpu shading language
- Rust's type system + `bytemuck` provides compile-time safety for instance data layouts

Use wgpu's pipeline caching instead.

### 4. Don't Switch to Per-Entity Draw Calls

Flutter draws each rect as a separate draw call. Vexo's instanced rendering (one draw for all quads) is **already better** for the common case. Keep it.

---

## Summary Table

| Priority | Change | Effort | Impact | Status |
|----------|--------|--------|--------|--------|
| 1 | Fix PushOffset bug | Small | Correctness | Done (`4f0cbb3`) |
| 2 | Dynamic instance buffer | Small | Robustness | Done |
| 3 | Scissor-based clipping | Medium | GPU perf | Done |
| 4 | Pipeline caching | Small | Startup time | TODO |
| 5 | Opaque fullscreen quad skip | Small | Reduce overdraw | TODO |
| 6 | 2x3 transform matrix | Medium | Features | TODO |

---

## References

- Flutter Impeller source: `https://chromium.googlesource.com/external/github.com/flutter/engine/+/refs/heads/main/impeller/`
- Impeller docs: `impeller/docs/babys_first_triangle.md`, `impeller/docs/blending.md`
- Vexo rendering pipeline: `vexo/src/render/`, `vexo/src/shader.wgsl`
