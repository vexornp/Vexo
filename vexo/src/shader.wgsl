
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) border_width: f32,
    @location(4) size: vec2<f32>,
    @location(5) corner_radius: f32,
    @location(6) clip_bounds: vec4<f32>, // x, y, width, height in logical coords
    @location(7) inst_pos: vec2<f32>, // Instance position for clipping calculation
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
    @location(6) inst_corner_radius: f32,
    @location(7) inst_clip_bounds: vec4<f32>,
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
    out.clip_bounds = inst_clip_bounds;
    out.inst_pos = inst_pos;
    return out;
}


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Clipping: discard pixels outside clip bounds
    // clip_bounds is (x, y, width, height) in logical coordinates
    // If width <= 0 or height <= 0, no clipping is applied
    if (in.clip_bounds.z > 0.0 && in.clip_bounds.w > 0.0) {
        // Calculate the fragment position in logical coordinates
        // inst_pos is the top-left corner, uv is [0,1] across the quad
        let frag_x = in.inst_pos.x + in.uv.x * (in.size.x / globals.scale_factor);
        let frag_y = in.inst_pos.y + in.uv.y * (in.size.y / globals.scale_factor);

        // Check if outside clip bounds
        if (frag_x < in.clip_bounds.x ||
            frag_y < in.clip_bounds.y ||
            frag_x > in.clip_bounds.x + in.clip_bounds.z ||
            frag_y > in.clip_bounds.y + in.clip_bounds.w) {
            discard;
        }
    }

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

    // Calculate border - border is the ring between sdf and sdf + border_px
    let border_px = in.border_width * globals.scale_factor;
    let border_alpha = 1.0 - smoothstep(-1.0, 1.0, sdf + border_px);
    let in_border = 1.0 - smoothstep(-1.0, 1.0, sdf);
    let border_weight = in_border * (1.0 - border_alpha);
    let final_color = mix(in.color, in.border_color, border_weight);
    return vec4<f32>(final_color.rgb, final_color.a * fill_alpha);
}