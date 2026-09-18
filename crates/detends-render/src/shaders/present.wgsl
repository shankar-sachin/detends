// The one place détends leaves linear light.
//
// Everything upstream — environment, blur, glass, text — is linear. Here it is
// encoded to sRGB and dithered, in that order.
//
// The order is the whole point. Dithering has to happen *after* the transfer
// curve, at the scale of one step of the final 8-bit value. A hardware sRGB
// surface format would encode after this shader runs, compressing the dither
// non-uniformly and leaving visible banding in the darks — which is exactly
// where a calm, near-black environment lives.

struct Present {
    // 1 to encode and dither, 0 to pass linear through for an HDR surface.
    encode: f32,
    // Fades the whole workspace to black for shutdown and sleep (§19).
    fade: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> present: Present;
@group(0) @binding(1) var composite: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var grain: texture_2d<f32>;

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

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = c <= vec3<f32>(0.0031308);
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, cutoff);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var color = textureSample(composite, samp, in.uv).rgb;
    color *= present.fade;

    if (present.encode < 0.5) {
        // Extended-range linear straight out. No encode, so no dither either:
        // there is no 8-bit quantisation to break up.
        return vec4<f32>(color, 1.0);
    }

    color = linear_to_srgb(color);

    // Triangular-PDF dither from two blue-noise taps, amplitude one 8-bit step.
    // Blue noise rather than white because its energy sits in high spatial
    // frequencies, where the eye is least sensitive — it reads as texture
    // rather than as static.
    //
    // The pattern is static, not animated per frame. Temporal dither on a
    // motionless screen shimmers, which is the opposite of calm.
    let size = vec2<f32>(textureDimensions(grain));
    let a = textureSample(grain, samp, in.position.xy / size).r;
    let b = textureSample(grain, samp, (in.position.xy + vec2<f32>(37.0, 17.0)) / size).r;
    let tpdf = (a + b) - 1.0;

    color += vec3<f32>(tpdf / 255.0);

    return vec4<f32>(color, 1.0);
}
