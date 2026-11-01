// CRT post-process, applied while the pixel canvas is drawn to the window.
//
// Uniforms:
//   data[0] = (time, canvas width, canvas height, strength 0..1)
//   data[1] = (window pixels per canvas pixel, scanline rows, -, -)
//
// Upscaling uses "sharp bilinear": each canvas pixel is a crisp square, with
// at most one blended pixel at its edge, so pixels stay even at any window
// size (the canvas must use linear filtering).
//
// Note: `textureSample` must run in uniform control flow, so out-of-screen
// pixels are masked at the end instead of returning early.

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

    // Gentle barrel distortion.
    let c = uv * 2.0 - vec2<f32>(1.0, 1.0);
    let bent = c * (1.0 + dot(c, c) * 0.018 * strength);
    let buv = bent * 0.5 + vec2<f32>(0.5, 0.5);
    let suv = sharp(buv, src, px_scale);

    // Chromatic aberration grows towards the edges.
    let ca = (buv - vec2<f32>(0.5, 0.5)) * 0.003 * strength;
    let r = textureSample(t, s, suv + ca).r;
    let g = textureSample(t, s, suv);
    let b = textureSample(t, s, suv - ca).b;
    var rgb = vec3<f32>(r, g.g, b);

    // Scanlines (one per design row, so they never alias), plus a faint rolling band.
    let scan = 0.5 + 0.5 * cos(buv.y * uniforms.data[1].y * 6.2831853);
    rgb = rgb * mix(1.0, 0.88 + 0.12 * scan, strength);
    let roll = 0.5 + 0.5 * sin(buv.y * 3.0 - time * 1.2);
    rgb = rgb * (1.0 + 0.025 * roll * strength);

    // Gentle glow lift and vignette.
    rgb = rgb + rgb * rgb * 0.08 * strength;
    let vignette = 1.0 - dot(c, c) * 0.16 * strength;
    rgb = rgb * vignette;

    let inside = step(0.0, buv.x) * step(buv.x, 1.0) * step(0.0, buv.y) * step(buv.y, 1.0);
    return vec4<f32>(rgb * inside, 1.0) * color;
}
