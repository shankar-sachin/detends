// Dual-Kawase blur.
//
// A wide, cheap, smooth blur built from two small kernels run down and back up
// a resolution pyramid. Each tap sits at a half-texel offset so that bilinear
// filtering turns it into a 2x2 average for free — which is why this must be
// sampled with a linear filter, and why a nearest-neighbour sampler silently
// produces a completely different and much uglier kernel.
//
// Every offset is derived from the **source** texture's size, in both
// directions. Deriving them from the destination is the classic bug here and
// makes the result look boxy.

struct Level {
    // 1.0 / source texture size.
    inv_source: vec2<f32>,
    // Widens the radius without adding passes. Non-integer values stay
    // temporally stable, where adding a pyramid level does not.
    scale: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> level: Level;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

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

// Five taps: the centre weighted four, plus the four diagonals.
@fragment
fn downsample(in: VertexOut) -> @location(0) vec4<f32> {
    let h = level.inv_source * 0.5 * level.scale;

    var sum = textureSample(source, samp, in.uv) * 4.0;
    sum += textureSample(source, samp, in.uv + vec2<f32>(-h.x, -h.y));
    sum += textureSample(source, samp, in.uv + vec2<f32>( h.x, -h.y));
    sum += textureSample(source, samp, in.uv + vec2<f32>(-h.x,  h.y));
    sum += textureSample(source, samp, in.uv + vec2<f32>( h.x,  h.y));
    return sum / 8.0;
}

// Eight taps: axis-aligned at double offset weighted one, diagonals weighted
// two. The asymmetry with the downsample kernel is what gives the dual filter
// its smoothness for so few samples.
@fragment
fn upsample(in: VertexOut) -> @location(0) vec4<f32> {
    let h = level.inv_source * 0.5 * level.scale;

    var sum = textureSample(source, samp, in.uv + vec2<f32>(-h.x * 2.0, 0.0));
    sum += textureSample(source, samp, in.uv + vec2<f32>( h.x * 2.0, 0.0));
    sum += textureSample(source, samp, in.uv + vec2<f32>(0.0, -h.y * 2.0));
    sum += textureSample(source, samp, in.uv + vec2<f32>(0.0,  h.y * 2.0));

    sum += textureSample(source, samp, in.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    sum += textureSample(source, samp, in.uv + vec2<f32>( h.x, -h.y)) * 2.0;
    sum += textureSample(source, samp, in.uv + vec2<f32>(-h.x,  h.y)) * 2.0;
    sum += textureSample(source, samp, in.uv + vec2<f32>( h.x,  h.y)) * 2.0;

    return sum / 12.0;
}
