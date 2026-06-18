
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
