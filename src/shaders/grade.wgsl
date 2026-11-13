// The lamplight grade: what makes the table look lit by one warm lamp. Appended to
// the shaders that need it (render.rs).
//
// A warm grade (shadows lean amber-brown, highlights stay creamy), a soft oval
// vignette like a room lit by one lamp, and fine film grain that changes at
// 12 fps so it reads as texture rather than noise.
//
// It's applied in one of two places. Normally every draw grades its own color
// on the way to the window (`grade_here`), so a frame is a single pass. The
// vignette and the grain are the same sum whether they're applied before or after
// blending, and the warm tint differs by under two parts in 255 where something
// see-through is blended, which can't be seen. The tabletop goes further: its
// warm tint and vignette are worked out with the rest of it, once per texel 30
// times a second (background.wgsl), and only the grain is left for every pixel.
//
// A window too small for the pixel art is drawn into a canvas first, and
// lamplight.wgsl grades that on its way to the window.

fn grade_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn grade_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}

// Warm grade: tint the darks toward amber, lift the mids a touch.
fn grade_warm(rgb: vec3<f32>, luma: f32, strength: f32, lamp: vec3<f32>) -> vec3<f32> {
    let warm = rgb * mix(vec3<f32>(1.0), lamp, 0.22 * (1.0 - luma));
    let lifted = warm + (sqrt(max(warm, vec3<f32>(0.0))) - warm) * 0.08;
    return mix(rgb, lifted, strength);
}

// Oval vignette, darkest in the corners. `uv` is the position across the view (0..1).
fn grade_vignette(uv: vec2<f32>, strength: f32) -> f32 {
    let c = (uv - vec2<f32>(0.5, 0.47)) * vec2<f32>(1.0, 1.2);
    return 1.0 - smoothstep(0.25, 0.85, length(c)) * 0.42 * strength;
}

// Film grain to add to a color whose luma was `luma` before grading: stronger in
// the shadows. `screen` is the window pixel, `frame` the grain's frame number
// (floor(time * 12)).
fn grade_grain(screen: vec2<f32>, frame: f32, luma: f32, strength: f32) -> f32 {
    let n = grade_hash(floor(screen / 2.0) + vec2<f32>(frame * 17.0, frame * 31.0)) - 0.5;
    return n * 0.045 * strength * (1.1 - luma);
}

// The whole grade. `strength` is the Atmosphere option.
fn grade(rgb_in: vec3<f32>, uv: vec2<f32>, screen: vec2<f32>, frame: f32, strength: f32, lamp: vec3<f32>) -> vec3<f32> {
    let luma = grade_luma(rgb_in);
    var rgb = grade_warm(rgb_in, luma, strength, lamp);
    rgb = rgb * grade_vignette(uv, strength);
    rgb = rgb + grade_grain(screen, frame, luma, strength);
    return max(rgb, vec3<f32>(0.0));
}

// Grades a color drawn straight to the window, if this frame is drawn that way.
//
// Uniforms (the same slots in every shader that draws to the window):
//   data[12] = (grain frame, strength 0..1, 1 to grade here, -)
//   data[13] = the view's rect in the window, in pixels (x, y, w, h)
//   data[14] = lamp tint
fn grade_here(rgb: vec3<f32>, screen: vec2<f32>) -> vec3<f32> {
    let g = uniforms.data[12];
    if (g.z < 0.5) {
        return rgb;
    }
    let view = uniforms.data[13];
    return grade(rgb, (screen - view.xy) / view.zw, screen, g.x, g.y, uniforms.data[14].rgb);
}
