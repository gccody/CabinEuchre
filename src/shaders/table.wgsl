// Lays the tabletop (background.wgsl, one pixel per texel) over the scene canvas.
//
// Uniforms:
//   data[0] = (canvas width, canvas height, first texel x, first texel y)
//   data[1] = the design-space rect the canvas shows (x, y, w, h)
//   data[2] = (last texel column, last texel row, -, -) of the tabletop canvas
//
// Each canvas pixel takes the texel its center falls in. That's worked out here
// rather than left to the sampler, which rounds a center that lands exactly on a
// texel's edge either way: at a 2400 px wide canvas that's every fifth column.

// Texels per design pixel (render::DENSITY).
const DENSITY = 1.5;

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let res = uniforms.data[0].xy;
    let first = uniforms.data[0].zw;
    let view = uniforms.data[1];
    let tex = floor((screen / res * view.zw + view.xy) * DENSITY) - first;
    let at = clamp(tex, vec2<f32>(0.0), uniforms.data[2].xy);
    return vec4<f32>(textureLoad(t, vec2<i32>(at), 0).rgb, 1.0) * color;
}
