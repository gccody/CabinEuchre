// Anti-aliased pixel-art sampling, used for every sprite the scenes draw
// (cards, suit icons, the pixel font). Textures must use linear filtering.
//
// Each texel is drawn as a crisp square, but where a texel edge crosses a
// screen pixel that pixel gets the blend of both sides. Texels stay chunky at
// any scale and rotation, yet rotated edges are smooth instead of stair-stepped.
// Sprites keep a 1-texel transparent border inside their quads, so their
// outline is anti-aliased the same way.
//
// Shapes (rectangles, circles) have a constant `uv`, so `fwidth` is zero and
// they are sampled unchanged.

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(t));
    let texel = uv * size;
    // Texels per screen pixel along each axis.
    let fw = fwidth(texel);
    let seam = floor(texel + vec2<f32>(0.5, 0.5));
    let smooth_texel = seam + clamp((texel - seam) / max(fw, vec2<f32>(1e-5)), vec2<f32>(-0.5), vec2<f32>(0.5));
    let is_shape = fw.x < 1e-6 && fw.y < 1e-6;
    let suv = select(smooth_texel / size, uv, is_shape);
    return textureSample(t, s, suv) * color;
}
