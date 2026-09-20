// The détends environment.
//
// Not a photograph and not a pattern — a large, slow field of light for glass
// to sit on and refract. It has to be smooth enough that the blur pyramid has
// something gentle to work with, and quiet enough that it never competes with
// content (rule 1).
//
// The drift is deliberately near the threshold of perception. You should not
// be able to catch it moving; you should only notice, returning after a while,
// that it is somewhere else.

struct Env {
    resolution: vec2<f32>,
    time: f32,
    // 0 while nothing is happening, rising during Focus. Focus makes détends
    // quieter, so the environment contracts rather than announcing itself.
    focus: f32,
    near: vec4<f32>,
    far: vec4<f32>,
    // The two light sources: rgb, with strength in a.
    glow_warm: vec4<f32>,
    glow_cool: vec4<f32>,
    /// How much of the environment has arrived, 0 to 1.
    ///
    /// Separate from the global fade in the present pass, because during boot
    /// the workspace must rise *behind* the mark — fading everything would
    /// take the mark down with it.
    presence: f32,
    // Three separate scalars, not a vec3: WGSL aligns vec3 to 16 bytes, which
    // would push this struct to 80 while Rust's [f32; 3] keeps it at 64. The
    // mismatch is invisible until the GPU rejects the binding.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> env: Env;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.uv = uv;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

// Smooth value noise. Cheap, and band-free because it is interpolated with a
// quintic rather than a linear curve.
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(
        mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let aspect = env.resolution.x / max(env.resolution.y, 1.0);
    let p = vec2<f32>((in.uv.x - 0.5) * aspect, in.uv.y - 0.5);

    // A soft pool of light, off-centre so the composition is never symmetrical.
    let drift = vec2<f32>(
        sin(env.time * 0.021) * 0.06,
        cos(env.time * 0.017) * 0.045,
    );
    let source = vec2<f32>(-0.18, -0.24) + drift;

    // Two very large noise octaves bend the falloff so it never looks like a
    // radial gradient from a drawing tool.
    let warp = noise(p * 1.35 + env.time * 0.012) * 0.16
             + noise(p * 2.70 - env.time * 0.008) * 0.07;

    var falloff = length(p - source) * 1.18 + warp;
    // During Focus the field closes in, dimming the periphery.
    falloff = falloff * (1.0 + env.focus * 0.45);

    let t = clamp(falloff, 0.0, 1.0);
    // Smootherstep, so there is no visible edge where the gradient tops out.
    let eased = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);

    var color = mix(env.far.rgb, env.near.rgb, 1.0 - eased);

    // Two pools of coloured light, added on top of the neutral field.
    //
    // Added rather than mixed, because light is additive and mixing toward a
    // hue would grey the ground out instead of lifting it. Each falls off over
    // a large radius with the same noise warp bending it, so the two never
    // read as two circles — only as a field that is warmer one side and cooler
    // the other.
    let cool_at = vec2<f32>(-0.30, -0.26) + drift * 1.4;
    let warm_at = vec2<f32>(0.36, 0.30) - drift;

    let cool_d = length(p - cool_at) * 1.06 + warp * 0.8;
    let warm_d = length(p - warm_at) * 1.24 + warp * 0.6;

    // Squared falloff: gentle in the middle, and genuinely gone at the edge,
    // so neither light lands on the periphery where it would fight the
    // vignette that keeps attention centred.
    let cool = pow(clamp(1.0 - cool_d, 0.0, 1.0), 2.9);
    let warm = pow(clamp(1.0 - warm_d, 0.0, 1.0), 3.1);

    // Focus dims the lights faster than it contracts the field: the room goes
    // quiet before it goes dark (§12).
    let lit = 1.0 - env.focus * 0.72;

    color += env.glow_cool.rgb * (cool * env.glow_cool.a * lit);
    color += env.glow_warm.rgb * (warm * env.glow_warm.a * lit);

    // A trace of noise in the environment itself. Sixteen-bit targets do not
    // band, but the blur pyramid's lower levels are where smooth gradients
    // start to step, and a little grain at the source prevents it.
    color += vec3<f32>((noise(in.uv * env.resolution * 0.5) - 0.5) * 0.0015);

    return vec4<f32>(color * env.presence, 1.0);
}
