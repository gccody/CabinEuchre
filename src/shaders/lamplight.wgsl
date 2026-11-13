// Lamplight post-process, applied while the pixel canvas is drawn to the window.
// Only used when the window is too small to draw the scene straight into (see
// grade.wgsl, which holds the grade itself and is appended to this file).
//
// Uniforms:
//   data[0] = (time, canvas width, canvas height, strength 0..1)
//   data[1] = (window pixels per canvas pixel, -, -, -)
//   data[2] = lamp tint
//
// Upscaling uses "sharp bilinear": each canvas pixel is a crisp square, with
// at most one blended pixel at its edge, so pixels stay even at any window
// size (the canvas must use linear filtering).

fn sharp(uv: vec2<f32>, size: vec2<f32>, px_scale: f32) -> vec2<f32> {
    let texel = uv * size;
    let base = floor(texel);
    let d = fract(texel) - vec2<f32>(0.5, 0.5);
    let region = 0.5 - 0.5 / px_scale;
    let f = (d - clamp(d, vec2<f32>(-region), vec2<f32>(region))) * px_scale + vec2<f32>(0.5, 0.5);
    return (base + f) / size;
}

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let time = uniforms.data[0].x;
    let src = uniforms.data[0].yz;
    let strength = uniforms.data[0].w;
    let px_scale = max(uniforms.data[1].x, 1.0);
    let lamp = uniforms.data[2].rgb;

    let rgb = textureSample(t, s, sharp(uv, src, px_scale)).rgb;
    return vec4<f32>(grade(rgb, uv, screen, floor(time * 12.0), strength, lamp), 1.0) * color;
}
