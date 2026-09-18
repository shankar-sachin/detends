// The détends optical slab.
//
// A pane here is a physical object, not a style: it has a thickness, a rounded
// bevel at its edge, and an index of refraction. Everything the material does
// follows from those three numbers.
//
// The important consequence is that refraction is **zero in the flat interior
// by construction** — not by a mask that fades it out, but because a flat
// surface has a vertical normal and a vertical normal bends nothing. That is
// what separates a slab of glass from a blurred rectangle: content under the
// middle stays clean and readable, and only the rim bends what is behind it.

struct Uniforms {
    // Logical size of the workspace.
    resolution: vec2<f32>,
    // Logical-to-physical ratio.
    scale: f32,
    // Seconds, for anything that drifts.
    time: f32,
    // Appearance -> Glass intensity, scales rim/refraction/tint together.
    intensity: f32,
    // Appearance -> Transparency.
    transparency: f32,
    _pad: vec2<f32>,
};

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // radius, squircle exponent, thickness, bevel width
    @location(2) shape: vec4<f32>,
    // ior, dispersion, frost, rim strength
    @location(3) optics: vec4<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) opacity: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    // Position in logical workspace units.
    @location(0) local: vec2<f32>,
    @location(1) center: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) shape: vec4<f32>,
    @location(4) optics: vec4<f32>,
    @location(5) tint: vec4<f32>,
    @location(6) opacity: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var backdrop: texture_2d<f32>;   // what is behind, sharp
@group(0) @binding(2) var blurred: texture_2d<f32>;    // the blur pyramid result
@group(0) @binding(3) var samp: sampler;
@group(0) @binding(4) var grain: texture_2d<f32>;      // blue noise tile

// Room around the pane for the rim highlight to breathe.
const MARGIN: f32 = 2.0;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, inst: Instance) -> VertexOut {
    // Two triangles, no index buffer.
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0,  1.0), vec2(1.0, -1.0), vec2( 1.0, 1.0),
    );
    let corner = corners[vertex];
    let local = inst.center + corner * (inst.half_size + vec2<f32>(MARGIN));

    var out: VertexOut;
    out.local = local;
    out.center = inst.center;
    out.half_size = inst.half_size;
    out.shape = inst.shape;
    out.optics = inst.optics;
    out.tint = inst.tint;
    out.opacity = inst.opacity;

    let ndc = local / u.resolution * 2.0 - 1.0;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    return out;
}

// Signed distance to a superellipse-cornered box. Negative inside.
//
// The exponent `n` controls corner character: 2 is a circular arc, 4-6 gives
// the continuously-curved squircle that reads as machined rather than drawn.
fn sd_squircle(p: vec2<f32>, half_size: vec2<f32>, radius: f32, n: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - (half_size - vec2<f32>(r));
    let qp = max(q, vec2<f32>(0.0));
    let outside = pow(pow(qp.x, n) + pow(qp.y, n), 1.0 / n);
    return outside + min(max(q.x, q.y), 0.0) - r;
}

// Gradient of the above, analytically.
//
// Cheaper than finite differences and, more importantly, exact — a numerically
// estimated normal shimmers as the pane moves, which is precisely the kind of
// low-level noise that makes an interface feel unsettled.
fn sd_squircle_grad(p: vec2<f32>, half_size: vec2<f32>, radius: f32, n: f32) -> vec2<f32> {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - (half_size - vec2<f32>(r));
    let s = sign(p);

    if (q.x > 0.0 && q.y > 0.0) {
        // In a corner: the superellipse's own gradient.
        let g = vec2<f32>(pow(q.x, n - 1.0), pow(q.y, n - 1.0));
        let len = max(length(g), 1e-6);
        return (g / len) * s;
    }
    // Along an edge: straight out, perpendicular to whichever side is nearer.
    if (q.x > q.y) {
        return vec2<f32>(s.x, 0.0);
    }
    return vec2<f32>(0.0, s.y);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let p = in.local - in.center;
    let radius = in.shape.x;
    let squircle = in.shape.y;
    let thickness = in.shape.z * u.intensity;
    let bevel = max(in.shape.w, 0.001);

    let d = sd_squircle(p, in.half_size, radius, squircle);

    // Analytic coverage antialiasing. `fwidth` on an exact SDF is correct and
    // costs nothing, which is why this pipeline needs no MSAA at all.
    let aa = max(fwidth(d), 1e-5);
    let coverage = 1.0 - smoothstep(-aa, aa, d);
    if (coverage <= 0.0) {
        discard;
    }

    // --- the height field -------------------------------------------------
    // A rounded rim rising to full thickness over `bevel` units. `t` is 0 at
    // the very edge and 1 once fully inside.
    let t = clamp(-d / bevel, 0.0, 1.0);
    let inv = 1.0 - t;
    let profile = sqrt(max(1.0 - inv * inv, 1e-6));
    let height = thickness * profile;

    // dh/dt rises steeply at the rim — physically right for a rounded edge,
    // but unbounded exactly at d = 0, so it is clamped before use.
    let dh_dt = min(thickness * inv / profile, 64.0);
    let grad = sd_squircle_grad(p, in.half_size, radius, squircle);
    // Chain rule: t = -d/bevel, so dt/dp = -grad/bevel.
    let dh_dp = dh_dt * (-grad / bevel);

    // Surface normal of the slab's top face. In the flat interior dh_dt is 0,
    // so this is exactly (0,0,1) and the refraction below vanishes.
    let normal = normalize(vec3<f32>(-dh_dp, 1.0));

    // --- refraction -------------------------------------------------------
    // A gentle perspective on the incoming ray, so panes near the edge of the
    // screen catch the light slightly differently from those in the middle.
    let ndc = (in.local / u.resolution) * 2.0 - 1.0;
    let incident = normalize(vec3<f32>(ndc * 0.25, -1.0));

    let ior = max(in.optics.x, 1.0001);
    let refracted = refract(incident, normal, 1.0 / ior);

    // Slab parallax: the ray crosses `height` of glass and exits displaced by
    // height * tan(theta). This is the line that makes it read as a solid
    // object with depth rather than a warped surface.
    var offset = vec2<f32>(0.0);
    if (dot(refracted, refracted) > 0.0) {
        offset = refracted.xy * (height / max(abs(refracted.z), 0.15));
    }
    // Grazing rays would otherwise smear across the whole screen.
    offset = clamp(offset, vec2<f32>(-48.0), vec2<f32>(48.0));

    let texel = 1.0 / u.resolution;
    let base_uv = in.local * texel;
    let offset_uv = offset * texel;

    // --- dispersion -------------------------------------------------------
    // Three taps at slightly different displacements. Because `offset` is
    // already zero in the interior, the colour split appears only at the rim
    // without needing a mask of its own — the physics does the masking.
    let spread = in.optics.y * u.intensity;
    let frost = clamp(in.optics.z, 0.0, 1.0);

    let uv_r = base_uv + offset_uv * (1.0 - spread);
    let uv_g = base_uv + offset_uv;
    let uv_b = base_uv + offset_uv * (1.0 + spread);

    // Frost mixes between the sharp backdrop and the blurred pyramid, so a
    // pane can go from nearly clear to deeply diffused with one number.
    let sharp = vec3<f32>(
        textureSample(backdrop, samp, uv_r).r,
        textureSample(backdrop, samp, uv_g).g,
        textureSample(backdrop, samp, uv_b).b,
    );
    let soft = vec3<f32>(
        textureSample(blurred, samp, uv_r).r,
        textureSample(blurred, samp, uv_g).g,
        textureSample(blurred, samp, uv_b).b,
    );
    var color = mix(sharp, soft, frost);

    // --- tint -------------------------------------------------------------
    let tint_alpha = in.tint.a * u.intensity * u.transparency;
    color = mix(color, in.tint.rgb, tint_alpha);

    // --- Fresnel and the rim ---------------------------------------------
    // Reflectance rises steeply as the view grazes the surface, which is why
    // the bevel lights up and the flat middle does not. This does more for
    // making the pane feel physical than the dispersion does.
    let cos_theta = clamp(dot(normal, -incident), 0.0, 1.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - cos_theta, 5.0);

    // A fixed light from the upper left, the direction détends is lit from.
    let light = normalize(vec3<f32>(-0.45, -0.75, 0.55));
    let spec = pow(max(dot(normal, light), 0.0), 48.0);

    let rim_strength = in.optics.w * u.intensity;
    color += vec3<f32>(fresnel * 0.5 + spec * 0.85) * rim_strength;

    // --- grain ------------------------------------------------------------
    // A wide blur across a smooth gradient is the single most band-prone thing
    // a compositor draws. A little noise here, carried through the rest of the
    // pipeline in 16-bit, keeps the final dither from having to do all the
    // work alone.
    let grain_size = vec2<f32>(textureDimensions(grain));
    let noise = textureSample(grain, samp, in.local / grain_size).r - 0.5;
    color += vec3<f32>(noise * 0.0025);

    return vec4<f32>(color, 1.0) * coverage * in.opacity;
}
