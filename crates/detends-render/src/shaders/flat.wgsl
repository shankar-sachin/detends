// Flat fills and images.
//
// Rare in détends by design — the system leans on type and glass rather than
// boxes — but needed for separators, album art, the wallpaper base, and
// anything a Wayland client will eventually hand over as a texture.

struct Uniforms {
    resolution: vec2<f32>,
    scale: f32,
    time: f32,
};

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // radius, squircle, uses_texture, opacity
    @location(2) shape: vec4<f32>,
    @location(3) color: vec4<f32>,
    // Source rect within the texture, normalised.
    @location(4) source: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) center: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) shape: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) uv: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, inst: Instance) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0,  1.0), vec2(1.0, -1.0), vec2( 1.0, 1.0),
    );
    let corner = corners[vertex];
    let local = inst.center + corner * (inst.half_size + vec2<f32>(1.0));

    var out: VertexOut;
    out.local = local;
    out.center = inst.center;
    out.half_size = inst.half_size;
    out.shape = inst.shape;
    out.color = inst.color;

    let unit = corner * 0.5 + 0.5;
    out.uv = inst.source.xy + unit * inst.source.zw;

    let ndc = local / u.resolution * 2.0 - 1.0;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    return out;
}

fn sd_squircle(p: vec2<f32>, half_size: vec2<f32>, radius: f32, n: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - (half_size - vec2<f32>(r));
    let qp = max(q, vec2<f32>(0.0));
    let outside = pow(pow(qp.x, n) + pow(qp.y, n), 1.0 / n);
    return outside + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let p = in.local - in.center;
    let d = sd_squircle(p, in.half_size, in.shape.x, in.shape.y);

    let aa = max(fwidth(d), 1e-5);
    let coverage = 1.0 - smoothstep(-aa, aa, d);
    if (coverage <= 0.0) {
        discard;
    }

    var color = in.color;
    if (in.shape.z > 0.5) {
        color = color * textureSample(image, samp, in.uv);
    }

    // Premultiplied, matching the blend state and what glyphon expects.
    let alpha = color.a * coverage * in.shape.w;
    return vec4<f32>(color.rgb * alpha, alpha);
}
