// Lamplight post-process, applied while the pixel canvas is drawn to the window.
//
// Uniforms:
//   data[0] = (time, canvas width, canvas height, strength 0..1)
//   data[1] = (window pixels per canvas pixel, -, -, -)
//   data[2] = lamp tint
//
// A warm grade (shadows lean amber-brown, highlights stay creamy), a soft oval
// vignette like a room lit by one lamp, and fine film grain that changes at
// 12 fps so it reads as texture rather than noise.
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

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let time = uniforms.data[0].x;
    let src = uniforms.data[0].yz;
    let strength = uniforms.data[0].w;
    let px_scale = max(uniforms.data[1].x, 1.0);
    let lamp = uniforms.data[2].rgb;

    var rgb = textureSample(t, s, sharp(uv, src, px_scale)).rgb;
    let luma = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));

    // Warm grade: tint the darks toward amber, lift the mids a touch.
    let warm = rgb * mix(vec3<f32>(1.0), lamp, 0.22 * (1.0 - luma));
    let lifted = warm + (sqrt(max(warm, vec3<f32>(0.0))) - warm) * 0.08;
    rgb = mix(rgb, lifted, strength);

    // Oval vignette, darkest in the corners.
    let c = (uv - vec2<f32>(0.5, 0.47)) * vec2<f32>(1.0, 1.2);
    let vignette = 1.0 - smoothstep(0.25, 0.85, length(c)) * 0.42 * strength;
    rgb = rgb * vignette;

    // Film grain, stronger in the shadows.
    let frame = floor(time * 12.0);
    let n = hash(floor(screen / 2.0) + vec2<f32>(frame * 17.0, frame * 31.0)) - 0.5;
    rgb = rgb + n * 0.045 * strength * (1.1 - luma);

    return vec4<f32>(max(rgb, vec3<f32>(0.0)), 1.0) * color;
}
