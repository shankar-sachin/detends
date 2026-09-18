// One oversized triangle covering the viewport.
//
// A triangle rather than two triangles: it avoids the diagonal seam where
// quantisation can differ either side of the shared edge, and it costs one
// fewer vertex.

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    // 0 -> (0,0)   1 -> (2,0)   2 -> (0,2)
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.uv = uv;
    // Flip y: texture space runs downward, clip space upward.
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}
