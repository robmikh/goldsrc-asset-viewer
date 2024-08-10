struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) lightmap_tex_coord: vec2<f32>,
};

struct Globals {
    transform: mat4x4<f32>,
};
@group(0)
@binding(0)
var<uniform> r_globals: Globals;

struct DrawParams {
    draw_mode: i32,
};
@group(1)
@binding(0)
var<uniform> r_draw_params: DrawParams;

struct Locals {
    transform: mat4x4<f32>,
};
@group(2)
@binding(0)
var<uniform> r_locals: Locals;
@group(2)
@binding(1)
var r_sampler: sampler;
@group(2)
@binding(2)
var r_atlas: texture_2d<f32>;

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tex_coord: vec2<f32>,
    @location(3) lightmap_tex_coord: vec2<f32>,
) -> VertexOutput {
    var in_position: vec4<f32>;
    in_position.x = position.x;
    in_position.y = position.y;
    in_position.z = position.z;
    in_position.w = 1.0;

    var out: VertexOutput;
    out.tex_coord = tex_coord;
    out.lightmap_tex_coord = lightmap_tex_coord;
    out.position = r_globals.transform * r_locals.transform * in_position;
    return out;
}

@group(3)
@binding(0)
var r_texture: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var tex_color: vec4<f32>;
    tex_color = textureSample(r_texture, r_sampler, in.tex_coord);
    var lightmap_color: vec4<f32>;
    lightmap_color = textureSample(r_atlas, r_sampler, in.lightmap_tex_coord);
    if tex_color.w == 0.0 {
        discard;
    }
    if r_draw_params.draw_mode == 0 {
        // Do nothing, use tex_color as-is
    } else if r_draw_params.draw_mode == 1 {
        tex_color.x = lightmap_color.x;
        tex_color.y = lightmap_color.y;
        tex_color.z = lightmap_color.z;
    } else if r_draw_params.draw_mode == 2 {
        // Blend the texture color and the lightmap color
        tex_color.x = tex_color.x * lightmap_color.x;
        tex_color.y = tex_color.y * lightmap_color.y;
        tex_color.z = tex_color.z * lightmap_color.z;
    }
    return tex_color;
}
