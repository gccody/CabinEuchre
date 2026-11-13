// Lays the tabletop (background.wgsl, one pixel per texel) under the scene: over
// the whole view in the window, or over the scene canvas.
//
// Uniforms:
//   data[0] = (view width, view height, first texel x, first texel y), the size in
//             the target's pixels
//   data[1] = the design-space rect the view shows (x, y, w, h)
//   data[2] = (last texel column, last texel row, -, -) of the tabletop canvas
//   data[3] = (view x, view y, -, -): where the view starts in the target, in pixels
//   data[12]     = (grain frame, lamplight strength, 1 when drawing to the window, -)
//
// Drawn to the window, the tabletop comes with the lamplight's warm tint and
// vignette already in it and its luma in alpha (background.wgsl), and gets the
// film grain here (grade.wgsl, appended to this file).
//
// Each pixel takes the texel its center falls in. That's worked out here rather
// than left to the sampler, which rounds a center that lands exactly on a texel's
// edge either way: in a 2400 px wide view that's every fifth column.

// Texels per design pixel (render::DENSITY).
const DENSITY = 1.5;

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let res = uniforms.data[0].xy;
    let first = uniforms.data[0].zw;
    let view = uniforms.data[1];
    let tex = floor(((screen - uniforms.data[3].xy) / res * view.zw + view.xy) * DENSITY) - first;
    let at = clamp(tex, vec2<f32>(0.0), uniforms.data[2].xy);
    let texel = textureLoad(t, vec2<i32>(at), 0);
    let g = uniforms.data[12];
    if (g.z < 0.5) {
        return vec4<f32>(texel.rgb, 1.0) * color;
    }
    let rgb = texel.rgb + grade_grain(screen, g.x, texel.a, g.y);
    return vec4<f32>(max(rgb, vec3<f32>(0.0)), 1.0) * color;
}
