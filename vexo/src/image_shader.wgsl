
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_origin: vec2<f32>,
    @location(2) uv_size: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(5) opacity: f32,
};

struct GlobalUniforms {
    screen_size: vec2<f32>,
    scale_factor: f32,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;
@group(1) @binding(0) var image_atlas: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct RClipUniform {
    count: vec4<f32>,              // .x = number of active entries (0..8)
    bounds: array<vec4<f32>, 8>,   // (left, top, right, bottom) per entry
    radii: array<vec4<f32>, 2>,    // 8 radii packed 4-per-vec4
};

@group(2) @binding(0) var<uniform> rclip: RClipUniform;

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
///
/// Transform caveat: `rclip.bounds` is the transformed AABB of the
/// clip rect (computed in `command_processor.rs`), not the actual
/// transformed rounded rect. Under rotation the SDF is correct
/// (a rotated rect's AABB is still axis-aligned). Under non-uniform
/// scale the visual clip would be an ellipse, but the SDF still
/// treats it as a circle/rounded-rect in pixel space — the clip
/// region is the AABB, not the transformed shape. The design spec
/// (`docs/superpowers/specs/2026-07-21-clip-rrect-widget-design.md`,
/// "Transform-aware rclip") explicitly defers tighter handling to a
/// follow-up. This matches `PushClip`'s existing behavior.
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

@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_uv_origin: vec2<f32>,
    @location(4) inst_uv_size: vec2<f32>,
    @location(9) inst_opacity: f32,
    @location(6) inst_transform_ab: vec2<f32>,
    @location(7) inst_transform_cd: vec2<f32>,
    @location(8) inst_transform_ef: vec2<f32>,
    @location(5) inst_z: f32,
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
    out.clip_position = vec4<f32>(nx, ny, inst_z, 1.0);
    out.uv = model_pos;
    out.uv_origin = inst_uv_origin;
    out.uv_size = inst_uv_size;
    out.size = inst_size * globals.scale_factor;
    out.opacity = inst_opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // `in.clip_position.xy` is already framebuffer pixel coordinates
    // (post-viewport-transform, top-left origin in WebGPU). Use directly
    // as the absolute physical-pixel position for the rclip SDF.
    let abs_pixel_pos = in.clip_position.xy;

    let atlas_uv = in.uv_origin + in.uv * in.uv_size;
    let tex_color = textureSample(image_atlas, image_sampler, atlas_uv);

    return vec4<f32>(tex_color.rgb, tex_color.a * in.opacity * rclip_alpha(abs_pixel_pos));
}
