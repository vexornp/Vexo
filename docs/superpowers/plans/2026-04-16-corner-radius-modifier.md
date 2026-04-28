# Corner Radius Widget Modifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement visual rounded corner rendering for the `CornerRadius` widget modifier using SDF (Signed Distance Field) in the WGSL shader.

**Architecture:** Add a stack-based corner radius context to `UiBatcher`. The `CornerRadius` modifier pushes/pops the radius, and `add_rect` uses the context value. The shader uses SDF to render rounded rectangles with anti-aliasing.

**Tech Stack:** Rust, WGSL (wgpu shader), Taffy layout

---

## File Structure

```
vexo/src/
├── renderer.rs           # Modify: Add corner_radius_stack, push/pop methods
├── shader.wgsl           # Modify: Add SDF rounded rect rendering
└── widgets/
    └── modifiers.rs      # Modify: Update CornerRadius::draw() to use context
```

---

### Task 1: Add corner radius context to UiBatcher

**Files:**
- Modify: `vexo/src/renderer.rs`

- [ ] **Step 1: Add corner_radius_stack field to UiBatcher**

Edit `vexo/src/renderer.rs` to add the stack field to the struct:

```rust
pub struct UiBatcher {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_requests: Vec<TextRequest>,
    pub editor_requests: Vec<EditorRequest>,
    pub quad_instances: Vec<quad_instance::QuadInstance>,

    screen_width: f32,
    screen_height: f32,
    corner_radius_stack: Vec<f32>,  // Stack for nested radius contexts
}
```

- [ ] **Step 2: Initialize corner_radius_stack in new()**

Update the `new()` method:

```rust
pub fn new() -> Self {
    Self {
        vertices: Vec::new(),
        indices: Vec::new(),
        text_requests: Vec::new(),
        editor_requests: Vec::new(),
        quad_instances: Vec::new(),
        screen_width: 1.0,
        screen_height: 1.0,
        corner_radius_stack: Vec::new(),
    }
}
```

- [ ] **Step 3: Add push/pop/current methods**

Add these methods to `impl UiBatcher` after the `set_screen_size` method:

```rust
/// Push a corner radius onto the context stack.
/// Used by CornerRadius modifier to set radius for child widgets.
pub fn push_corner_radius(&mut self, radius: f32) {
    self.corner_radius_stack.push(radius);
}

/// Pop the corner radius from the context stack.
/// Called after drawing children to restore previous context.
pub fn pop_corner_radius(&mut self) {
    self.corner_radius_stack.pop();
}

/// Get the current corner radius from the context stack.
/// Returns 0.0 if no radius is set.
pub fn current_corner_radius(&self) -> f32 {
    self.corner_radius_stack.last().copied().unwrap_or(0.0)
}
```

- [ ] **Step 4: Update add_rect to use context radius**

Modify the `add_rect` method to use context radius when the explicit parameter is 0.0:

```rust
pub fn add_rect(
    &mut self,
    pos: [f32; 2],
    size: [f32; 2],
    color: impl Into<Color>,
    border_color: impl Into<Color>,
    border_width: f32,
    corner_radius: f32,
) {
    let color: Color = color.into();
    let border_color: Color = border_color.into();

    // Use explicit radius if > 0, otherwise use context
    let radius = if corner_radius > 0.0 {
        corner_radius
    } else {
        self.current_corner_radius()
    };

    self.quad_instances.push(quad_instance::QuadInstance {
        position: pos,
        size,
        color: color.to_array(),
        border_color: border_color.to_array(),
        border_width,
        corner_radius: radius,
        _padding: [0.0; 2],
    });
}
```

- [ ] **Step 5: Build to verify**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 6: Commit**

```bash
git add vexo/src/renderer.rs
git commit -m "feat(renderer): add corner radius context stack to UiBatcher"
```

---

### Task 2: Update CornerRadius modifier to use context

**Files:**
- Modify: `vexo/src/widgets/modifiers.rs`

- [ ] **Step 1: Update CornerRadius::draw() to push/pop radius**

Find the `CornerRadius` impl block for `Widget<M>` and update the `draw` method:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: Point<Logical>,
    focused_id: Option<WidgetId>,
    cursor_blink: &crate::CursorBlinkState,
    ctx: &mut WidgetContext,
) {
    // Push radius onto context stack
    renderer.push_corner_radius(self.radius);

    // Draw child with radius context set
    self.child.draw(taffy, node, renderer, offset, focused_id, cursor_blink, ctx);

    // Pop radius from context stack
    renderer.pop_corner_radius();
}
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/widgets/modifiers.rs
git commit -m "feat(modifiers): CornerRadius now pushes radius to renderer context"
```

---

### Task 3: Implement SDF rounded rect in shader

**Files:**
- Modify: `vexo/src/shader.wgsl`

- [ ] **Step 1: Add corner_radius to VertexOutput struct**

Update the `VertexOutput` struct to include corner_radius:

```wgsl
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) border_width: f32,
    @location(4) size: vec2<f32>,
    @location(5) corner_radius: f32,
};
```

- [ ] **Step 2: Update vs_main to accept and pass corner_radius**

Update the vertex shader function signature and body:

```wgsl
@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_color: vec4<f32>,
    @location(5) inst_border_width: f32,
    @location(6) inst_corner_radius: f32,
) -> VertexOutput {
    // Multiply incoming logical points by the scale factor to get physical pixels
    let scaled_pos = inst_pos * globals.scale_factor;
    let scaled_size = inst_size * globals.scale_factor;

    // 1. Calculate pixel position:
    let pixel_pos = scaled_pos + (model_pos * scaled_size);

    // Normalize to NDC (-1.0 to 1.0)
    let nx = (pixel_pos.x / globals.screen_size.x) * 2.0 - 1.0;
    let ny = 1.0 - (pixel_pos.y / globals.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = model_pos;
    out.color = inst_color;
    out.border_color = inst_border_color;
    out.size = scaled_size;
    out.border_width = inst_border_width;
    out.corner_radius = inst_corner_radius * globals.scale_factor;
    return out;
}
```

- [ ] **Step 3: Implement SDF rounded rect in fragment shader**

Replace the entire `fs_main` function:

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Clamp radius to at most half the smallest dimension
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);

    // If no corner radius, use original rectangular rendering
    if (radius < 0.5) {
        let centered_uv = in.uv - 0.5;
        let border_px = in.border_width * globals.scale_factor;
        let uv_border_step = border_px / in.size;
        let edge_dist = abs(centered_uv);
        let is_border_x = smoothstep(0.5 - uv_border_step.x - 0.002, 0.5 - uv_border_step.x, edge_dist.x);
        let is_border_y = smoothstep(0.5 - uv_border_step.y - 0.002, 0.5 - uv_border_step.y, edge_dist.y);
        let is_border = max(is_border_x, is_border_y);
        return mix(in.color, in.border_color, is_border);
    }

    // SDF for rounded rectangle
    // UV is 0-1, convert to pixel coordinates relative to center
    let pixel_pos = in.uv * in.size;
    let half_size = in.size * 0.5;
    let center_pos = pixel_pos - half_size;

    // SDF: distance from rounded rectangle edge
    let inner_dist = abs(center_pos) - (half_size - radius);
    let corner_dist = length(max(inner_dist, vec2<f32>(0.0))) - radius;
    let sdf = min(max(inner_dist.x, inner_dist.y), 0.0) + corner_dist;

    // Fill alpha with 1px anti-aliasing
    let fill_alpha = 1.0 - smoothstep(-1.0, 1.0, sdf);

    // If completely outside, discard
    if (fill_alpha <= 0.0) {
        discard;
    }

    // Calculate border
    let border_px = in.border_width * globals.scale_factor;
    let inner_sdf = sdf + border_px;
    let border_alpha = 1.0 - smoothstep(-1.0, 1.0, inner_sdf);

    // Mix fill and border colors
    let fill_color = vec4<f32>(in.color.rgb, in.color.a * fill_alpha);
    let border_contribution = border_alpha * fill_alpha;

    return mix(fill_color, in.border_color, border_contribution);
}
```

- [ ] **Step 4: Build to verify**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/shader.wgsl
git commit -m "feat(shader): implement SDF rounded rectangle rendering"
```

---

### Task 4: Update example app to demonstrate corner radius

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Add corner radius usage to example view**

Read `shared_app/src/lib.rs` and find the `view` function. Add an example with corner radius. Look for existing modifier usage and add `.corner_radius()`:

Find a widget with `.background()` and add `.corner_radius(8.0)` after it.

Example change (adjust based on actual code):
```rust
// Before
text!("Modified Text", size: 24.0)
    .padding(10.0)
    .background(Color::rgb(0.2, 0.2, 0.4))
    .border(Color::WHITE, 2.0),

// After
text!("Modified Text", size: 24.0)
    .padding(10.0)
    .background(Color::rgb(0.2, 0.2, 0.4))
    .border(Color::WHITE, 2.0)
    .corner_radius(8.0),
```

- [ ] **Step 2: Build and run to verify**

Run: `cargo run -p desktop_demo`
Expected: App launches with rounded corners visible on the modified widget

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(example): demonstrate corner radius modifier"
```

---

### Task 5: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: Success with no errors

- [ ] **Step 2: Run desktop demo and visually verify**

Run: `cargo run -p desktop_demo`

Visually verify:
- Rounded corners appear smooth (anti-aliased)
- Border follows rounded edge
- Background is clipped to rounded shape

- [ ] **Step 3: Final commit (if any fixes needed)**

```bash
git status
# If changes, commit them
```

---

## Summary

This implementation adds visual rounded corner rendering to the `CornerRadius` modifier:

1. **Renderer context** - Stack-based radius context in `UiBatcher`
2. **Modifier integration** - `CornerRadius` pushes/pops radius during draw
3. **Shader SDF** - Fragment shader renders rounded rects with anti-aliasing

**Usage:**
```rust
text!("Hello")
    .padding(10.0)
    .background(Color::WHITE)
    .border(Color::BLACK, 2.0)
    .corner_radius(8.0)
```
