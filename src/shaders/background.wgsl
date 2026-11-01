// Swirling, painterly felt background.
//
// Uniforms:
//   data[0] = (time, resolution.x, resolution.y, swirl strength)
//   data[1], data[2], data[3] = the three palette colors (deep, mid, glow)
//
// Drawn as one full-canvas rectangle, so `uv` is ignored and `screen` is the
// canvas pixel position.

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let time = uniforms.data[0].x;
    let res = uniforms.data[0].yz;
    let swirl = uniforms.data[0].w;
    let deep = uniforms.data[1].rgb;
    let mid = uniforms.data[2].rgb;
    let glow = uniforms.data[3].rgb;

    // Centered, aspect-correct coordinates.
    var p = (screen - 0.5 * res) / length(res);

    // Twist around the center: the angle grows with distance and slowly drifts.
    let r = length(p);
    let a = atan2(p.y, p.x) + time * 0.08 + r * swirl * (5.0 + sin(time * 0.13) * 1.5);
    p = vec2<f32>(cos(a), sin(a)) * r * 30.0;

    // Domain warp: feed the position back through sines for paint-like streaks.
    var q = p;
    for (var i = 0; i < 5; i++) {
        let fi = f32(i);
        q = q + vec2<f32>(
            sin(q.y * 0.55 + time * 0.35 + fi),
            cos(q.x * 0.55 - time * 0.30 - fi)
        ) * 0.9;
    }

    let bands = 0.5 + 0.5 * sin(q.x * 0.30 + q.y * 0.22);
    let rings = 0.5 + 0.5 * cos(length(q) * 0.12 - time * 0.5);

    // Fairly hard edges between bands read as brush strokes rather than gradients.
    var col = mix(deep, mid, smoothstep(0.38, 0.62, bands));
    col = mix(col, glow, smoothstep(0.72, 0.86, rings) * 0.8);

    // Slight posterization keeps the low-res, hand-painted feel.
    col = floor(col * 28.0 + 0.5) / 28.0;
    return vec4<f32>(col, 1.0) * color;
}
