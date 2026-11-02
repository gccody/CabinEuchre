// The cabin tabletop: walnut planks under a lake-teal felt mat with a stitched
// edge, lit by a pendant lamp that sways very slowly. The light falls off in
// ordered-dither bands on the same texel grid as the pixel art, and a few
// dust motes drift through the beam.
//
// Uniforms:
//   data[0]    = (time, resolution.x, resolution.y, -)
//   data[1..3] = wood dark, wood, wood light
//   data[4..6] = felt deep, felt, felt glow
//   data[7]    = lamp tint
//   data[8]    = the design-space rect the canvas shows (x, y, w, h): 640x360 on
//                a desktop, the whole screen on a phone
//
// Drawn as one full-canvas rectangle, so `uv` is ignored and `screen` is the
// canvas pixel position.

// Texels per design pixel (render::DENSITY).
const DENSITY = 1.5;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn bayer4(p: vec2<f32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let x = u32(p.x) % 4u;
    let y = u32(p.y) % 4u;
    return (m[y * 4u + x] + 0.5) / 16.0;
}

// Signed distance to a rounded rectangle (negative inside).
fn round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - (half - vec2<f32>(r, r));
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn effect(color: vec4<f32>, t: texture_2d<f32>, s: sampler, uv: vec2<f32>, screen: vec2<f32>) -> vec4<f32> {
    let time = uniforms.data[0].x;
    let res = uniforms.data[0].yz;
    let wood_dark = uniforms.data[1].rgb;
    let wood_mid = uniforms.data[2].rgb;
    let wood_light = uniforms.data[3].rgb;
    let felt_deep = uniforms.data[4].rgb;
    let felt_mid = uniforms.data[5].rgb;
    let felt_glow = uniforms.data[6].rgb;
    let lamp = uniforms.data[7].rgb;

    let view = uniforms.data[8];

    // Snap to the pixel-art texel grid so the table is as chunky as the cards.
    let tex = floor((screen / res * view.zw + view.xy) * DENSITY);
    let p = (tex + vec2<f32>(0.5)) / DENSITY;
    // On a phone the design space can start left of the canvas. Whole dither
    // periods keep the grid's indices positive without changing the pattern.
    let grid = tex + vec2<f32>(4096.0);
    let dither = bayer4(grid);

    // Lamp light: an oval pool that swings a few pixels side to side.
    let lamp_pos = vec2<f32>(320.0 + sin(time * 0.55) * 14.0, 150.0 + (1.0 - cos(time * 1.1)) * 2.0);
    let ld = length((p - lamp_pos) * vec2<f32>(1.0, 1.3));
    // A wider table gets a wider pool, so its ends don't fall into darkness.
    let reach = 360.0 * max(max(view.z / 640.0, view.w / 360.0), 1.0);
    var light = clamp(1.0 - ld / reach, 0.0, 1.0);
    light = light * light * (3.0 - 2.0 * light);
    let level = floor(light * 7.0 + dither) / 7.0;

    // The felt mat.
    var mat: f32;
    var sh: f32;
    if (all(view == vec4<f32>(0.0, 0.0, 640.0, 360.0))) {
        // Written out for the exact 640x360 table: the mat's edges and stitches
        // land exactly on their thresholds here, so a shader compiler's rounding
        // of computed values would shift them by a texel.
        mat = round_rect(p - vec2<f32>(320.0, 180.0), vec2<f32>(306.0, 168.0), 40.0);
        sh = round_rect(p - vec2<f32>(323.0, 185.0), vec2<f32>(306.0, 168.0), 40.0);
    } else {
        let mid = view.xy + view.zw * 0.5;
        let half = view.zw * 0.5 - vec2<f32>(14.0, 12.0);
        mat = round_rect(p - mid, half, 40.0);
        sh = round_rect(p - mid - vec2<f32>(3.0, 5.0), half, 40.0);
    }

    var col: vec3<f32>;
    if (mat > 0.0) {
        // Walnut planks with staggered butt joints and wavy grain.
        let plank_h = 22.0;
        let row = floor(p.y / plank_h);
        let ly = p.y - row * plank_h;
        let shift = hash(vec2<f32>(row, 3.0)) * 240.0;
        let seg = floor((p.x + shift) / 240.0);
        let lx = p.x + shift - seg * 240.0;
        let ps = hash(vec2<f32>(row, seg));
        let warp = sin(p.x * 0.02 + ps * 20.0) * 3.0 + sin(p.x * 0.07 + ps * 9.0) * 1.1;
        let grain = sin((ly + warp) * 1.2 + ps * 30.0);
        let w = 0.5 + 0.34 * (ps - 0.5) + 0.22 * grain + (dither - 0.5) * 0.08;
        col = mix(wood_dark, wood_mid, step(0.3, w));
        col = mix(col, wood_light, step(0.68, w));
        if (ly < 1.0 || lx < 1.0) {
            col = wood_dark;
        }
        col = col * (0.62 + 0.6 * level) * mix(vec3<f32>(1.0), lamp, 0.4 * level);
        // The mat's shadow on the wood (cast down and to the right).
        col = col * (1.0 - 0.55 * (1.0 - smoothstep(0.0, 6.0, sh)));
    } else {
        // Felt: lit bands, a faint woven texture and a darker rolled edge.
        col = mix(felt_deep, felt_mid, smoothstep(0.05, 0.55, level));
        col = mix(col, felt_glow, smoothstep(0.6, 1.0, level) * 0.8);
        let weave = hash(tex) * 0.5 + 0.5 * f32((u32(grid.x) + u32(grid.y)) % 2u);
        col = col * (0.97 + 0.05 * weave);
        let inside = -mat;
        if (inside < 3.0) {
            col = felt_deep * 0.8;
        } else if (inside < 4.0) {
            col = col * 1.15;
        }
        // Stitching just inside the edge.
        if (inside >= 7.0 && inside < 8.4) {
            let dash = fract((p.x + p.y) / 7.0);
            if (dash < 0.5) {
                col = mix(col, mix(felt_glow, lamp, 0.35), 0.75);
            }
        }
        col = col * mix(vec3<f32>(1.0), lamp, 0.18 * level);
    }

    // Dust motes drifting up through the lamp's beam.
    let mp = p + vec2<f32>(time * 1.5, time * -4.0);
    let cell = floor(mp / 38.0);
    let h = hash(cell);
    let wander = vec2<f32>(sin(time * 0.7 + h * 6.28) * 5.0, 0.0);
    let mpos = cell * 38.0 + vec2<f32>(hash(cell + vec2<f32>(1.3, 0.0)), hash(cell + vec2<f32>(0.0, 7.1))) * 38.0 + wander;
    let md = abs(mp - mpos);
    let twinkle = 0.5 + 0.5 * sin(time * 1.3 + h * 40.0);
    let mote = step(max(md.x, md.y), 0.7) * step(0.6, h) * twinkle * smoothstep(0.35, 0.9, light);
    col = col + lamp * mote * 0.18;

    return vec4<f32>(col, 1.0) * color;
}
