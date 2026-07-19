
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
    out.color = inst_color;
    out.border_color = inst_border_color;
    out.size = inst_size * globals.scale_factor;
    out.border_width = inst_border_width;
    out.corner_radius = inst_corner_radius * globals.scale_factor;
    out.shadow_color = inst_shadow_color;
    out.shadow_blur = inst_shadow_blur * globals.scale_factor;
    return out;
}


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
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
        return vec4<f32>(in.shadow_color.rgb, falloff * in.shadow_color.a);
    }

    // === EXISTING FILL/BORDER PATH (unchanged) ===
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);

    if (radius < 0.5) {
        if (in.border_width <= 0.0) {
            return in.color;
        }

        let centered_uv = in.uv - 0.5;
        let border_px = in.border_width * globals.scale_factor;
        let uv_border_step = border_px / in.size;
        let edge_dist = abs(centered_uv);
        let is_border_x = smoothstep(0.5 - uv_border_step.x - 0.002, 0.5 - uv_border_step.x, edge_dist.x);
        let is_border_y = smoothstep(0.5 - uv_border_step.y - 0.002, 0.5 - uv_border_step.y, edge_dist.y);
        let is_border = max(is_border_x, is_border_y);
        return mix(in.color, in.border_color, is_border);
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
    return vec4<f32>(final_color.rgb, final_color.a * fill_alpha);
}
