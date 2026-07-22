
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) border_width: f32,
    @location(4) size: vec2<f32>,
    @location(5) corner_radius: f32,
    @location(6) shadow_color: vec4<f32>,
    @location(7) shadow_blur: f32,
};

struct GlobalUniforms {
    screen_size: vec2<f32>,
    scale_factor: f32,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;

struct RClipUniform {
    count: vec4<f32>,              // .x = number of active entries (0..8)
    bounds: array<vec4<f32>, 8>,   // (left, top, right, bottom) per entry
    radii: array<vec4<f32>, 2>,    // 8 radii packed 4-per-vec4
};

@group(1) @binding(0) var<uniform> rclip: RClipUniform;

@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_color: vec4<f32>,
    @location(5) inst_border_width: f32,
    @location(6) inst_corner_radius: f32,
    @location(7) inst_transform_ab: vec2<f32>,
    @location(8) inst_transform_cd: vec2<f32>,
    @location(9) inst_transform_ef: vec2<f32>,
    @location(10) inst_shadow_color: vec4<f32>,
    @location(11) inst_shadow_blur: f32,
    @location(12) inst_z: f32,
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
    out.color = inst_color;
    out.border_color = inst_border_color;
    out.size = inst_size * globals.scale_factor;
    out.border_width = inst_border_width;
    out.corner_radius = inst_corner_radius * globals.scale_factor;
    out.shadow_color = inst_shadow_color;
    out.shadow_blur = inst_shadow_blur * globals.scale_factor;
    return out;
}


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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // `in.clip_position.xy` is already framebuffer pixel coordinates
    // (post-viewport-transform, top-left origin in WebGPU). Use directly
    // as the absolute physical-pixel position for the rclip SDF.
    let abs_pixel_pos = in.clip_position.xy;

    // === SHADOW PATH (when shadow_color.a > 0) ===
    if (in.shadow_color.a > 0.0) {
        let blur_px = in.shadow_blur;
        let silhouette_size = max(in.size - vec2<f32>(2.0 * blur_px), vec2<f32>(0.0));
        let silhouette_half = silhouette_size * 0.5;
        let silhouette_radius = min(in.corner_radius, min(silhouette_size.x, silhouette_size.y) * 0.5);

        let pixel_pos = in.uv * in.size;
        let center_pos = pixel_pos - (in.size * 0.5);

        let inner_dist = abs(center_pos) - (silhouette_half - silhouette_radius);
        let corner_dist = length(max(inner_dist, vec2<f32>(0.0))) - silhouette_radius;
        let shadow_sdf = min(max(inner_dist.x, inner_dist.y), 0.0) + corner_dist;

        let sigma = max(blur_px * 0.5, 0.5);
        let d = max(shadow_sdf, 0.0);
        let falloff = exp(-d * d / (2.0 * sigma * sigma));
        return vec4<f32>(in.shadow_color.rgb, falloff * in.shadow_color.a * rclip_alpha(abs_pixel_pos));
    }

    // === EXISTING FILL/BORDER PATH (unchanged) ===
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);

    if (radius < 0.5) {
        if (in.border_width <= 0.0) {
            return vec4<f32>(in.color.rgb, in.color.a * rclip_alpha(abs_pixel_pos));
        }

        let centered_uv = in.uv - 0.5;
        let border_px = in.border_width * globals.scale_factor;
        let uv_border_step = border_px / in.size;
        let edge_dist = abs(centered_uv);
        let is_border_x = smoothstep(0.5 - uv_border_step.x - 0.002, 0.5 - uv_border_step.x, edge_dist.x);
        let is_border_y = smoothstep(0.5 - uv_border_step.y - 0.002, 0.5 - uv_border_step.y, edge_dist.y);
        let is_border = max(is_border_x, is_border_y);
        let result = mix(in.color, in.border_color, is_border);
        return vec4<f32>(result.rgb, result.a * rclip_alpha(abs_pixel_pos));
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

    let border_px = in.border_width * globals.scale_factor;
    let border_alpha = 1.0 - smoothstep(-1.0, 1.0, sdf + border_px);
    let in_border = 1.0 - smoothstep(-1.0, 1.0, sdf);
    let border_weight = in_border * (1.0 - border_alpha);
    let final_color = mix(in.color, in.border_color, border_weight);
    return vec4<f32>(final_color.rgb, final_color.a * fill_alpha * rclip_alpha(abs_pixel_pos));
}
