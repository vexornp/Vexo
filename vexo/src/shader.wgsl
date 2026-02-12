
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) border_width: f32,
    @location(4) size: vec2<f32>,
};

struct GlobalUniforms {
    screen_size: vec2<f32>,
    scale_factor: f32,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;

@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_color: vec4<f32>,
    @location(5) inst_border_width: f32,
) -> VertexOutput {
    // Multiply incoming logical points by the scale factor to get physical pixels
    let scaled_pos = inst_pos * globals.scale_factor;
    let scaled_size = inst_size * globals.scale_factor;

    // 1. Calculate pixel position:
    // If model_pos is 0.0 to 1.0, we just do:
    let pixel_pos = scaled_pos + (model_pos * scaled_size);

    // Normalize to NDC (-1.0 to 1.0)
    let nx = (pixel_pos.x / globals.screen_size.x) * 2.0 - 1.0;
    let ny = 1.0 - (pixel_pos.y / globals.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = model_pos;
    out.color = inst_color;
    out.border_color = inst_border_color;
    out.size = scaled_size; // Pass scaled size to fragment shader for border calculations

    /// Make a fixed pixel border (e.g., exactly 2 pixels wide), 
    /// We should adjust the border_width in your vertex shader before passing it to the fragment shade
    /// Convert pixel width to UV-space width for this specific instance
    out.border_width = inst_border_width;
    return out;
}


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let centered_uv = in.uv - 0.5; // Center UV around (0,0)

    // How many UV units represent the desired border width?
    // inst_border_width is logical points, so multiply by scale_factor for physical pixels
    let border_px = in.border_width * globals.scale_factor;

    // Convert pixel border width to UV space based on the size of the rectangle
    let uv_border_step = border_px / in.size; // How much UV corresponds to 1 pixel for this instance
    
    // This gives us the UV distance that corresponds to the desired pixel border width
    // (Prevents the border from looking stretched on wide rectangles)
    // let border_uv_thickness = in.border_width * uv_pixel_step;

    // Calculate edge distance
    let edge_dist = abs(centered_uv);

    // Determine if we're in the border region
    let is_border_x = smoothstep(0.5 - uv_border_step.x - 0.002, 0.5 - uv_border_step.x, edge_dist.x);
    let is_border_y = smoothstep(0.5 - uv_border_step.y - 0.002, 0.5 - uv_border_step.y, edge_dist.y);
    let is_border = max(is_border_x, is_border_y);

    // Mix the base color and the border color based on the distance
    return mix(in.color, in.border_color, is_border);
}