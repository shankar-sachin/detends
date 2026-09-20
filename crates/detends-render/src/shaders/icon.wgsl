// The détends icon set.
//
// Drawn as signed distance fields rather than typed as glyphs, because Inter —
// like most text faces — has almost no symbol coverage: ✈ ◎ ✉ ◷ ▤ are all
// absent, and an interface that typed its icons would silently render blank
// boxes.
//
// Drawing them also means they are exact at any size, antialiased for free by
// `fwidth`, and able to catch the same light as the glass.
//
// Every icon is designed on the same 24×24 grid, expressed here in normalised
// coordinates where the grid spans −1…+1 and +y points down, matching screen
// space. Every icon is drawn at one stroke weight. A set whose weights drift is
// the clearest sign of one that was collected rather than designed.

struct Uniforms {
    resolution: vec2<f32>,
    scale: f32,
    time: f32,
};

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    // shape index, stroke width (icon space), rim strength, opacity
    @location(2) params: vec4<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    // Integer varyings must be flat in WGSL; keeping the shape index in a
    // float avoids the interpolation rules entirely.
    @location(1) params: vec4<f32>,
    @location(2) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, inst: Instance) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
        vec2(-1.0,  1.0), vec2(1.0, -1.0), vec2( 1.0, 1.0),
    );
    let corner = corners[vertex];

    // Icons are square. A non-square rectangle centres the icon on the shorter
    // axis rather than stretching it — proportions that drift with layout are
    // another way a set stops looking designed.
    let side = min(inst.half_size.x, inst.half_size.y);
    let local = inst.center + corner * side;

    var out: VertexOut;
    out.local = corner;
    out.params = inst.params;
    out.color = inst.color;

    let ndc = local / u.resolution * 2.0 - 1.0;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    return out;
}

// ---------------------------------------------------------------- primitives

fn sd_circle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

// Approximate, but exact enough for a thin stroke: scaling space distorts
// distance, and multiplying back by the smaller radius undoes most of it.
fn sd_ellipse(p: vec2<f32>, r: vec2<f32>) -> f32 {
    return (length(p / r) - 1.0) * min(r.x, r.y);
}

fn sd_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// A line with round caps, which is what gives the set its joins for free.
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// A real quadratic Bézier, so curved strokes stay curves rather than becoming
// a chain of short straight segments that shows as faceting at large sizes.
fn sd_bezier(pos: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let A = b - a;
    let B = a - 2.0 * b + c;
    let C = A * 2.0;
    let D = a - pos;

    let kk = 1.0 / max(dot(B, B), 1e-6);
    let kx = kk * dot(A, B);
    let ky = kk * (2.0 * dot(A, A) + dot(D, B)) / 3.0;
    let kz = kk * dot(D, A);

    let p = ky - kx * kx;
    let p3 = p * p * p;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    var h = q * q + 4.0 * p3;

    if (h >= 0.0) {
        h = sqrt(h);
        let x = (vec2<f32>(h, -h) - vec2<f32>(q)) / 2.0;
        let uv = sign(x) * pow(abs(x), vec2<f32>(1.0 / 3.0));
        let t = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        return length(D + (C + B * t) * t);
    }

    let z = sqrt(-p);
    let v = acos(q / (p * z * 2.0)) / 3.0;
    let m = cos(v);
    let n = sin(v) * 1.732050808;
    let t = clamp(vec2<f32>(m + m, -n - m) * z - vec2<f32>(kx), vec2<f32>(0.0), vec2<f32>(1.0));
    return sqrt(min(
        dot(D + (C + B * t.x) * t.x, D + (C + B * t.x) * t.x),
        dot(D + (C + B * t.y) * t.y, D + (C + B * t.y) * t.y),
    ));
}

fn op_union(a: f32, b: f32) -> f32 { return min(a, b); }

// Turn a shape's boundary into a stroke of the given width.
fn outline(d: f32, w: f32) -> f32 { return abs(d) - w * 0.5; }

// --------------------------------------------------------------------- icons
//
// Coordinates are read off the 24-unit grid and divided by 12.

fn icon_music(p: vec2<f32>, w: f32) -> f32 {
    // A filled head with a stroked stem and flag: the conventional way a note
    // is drawn, and legible far smaller than an outlined head would be.
    let head = sd_ellipse(p - vec2<f32>(-0.30, 0.46), vec2<f32>(0.30, 0.235));
    let stem = outline(sd_segment(p, vec2<f32>(0.00, 0.42), vec2<f32>(0.00, -0.66)), w);
    let flag = outline(
        sd_bezier(p, vec2<f32>(0.00, -0.66), vec2<f32>(0.52, -0.52), vec2<f32>(0.40, -0.08)),
        w,
    );
    return op_union(head, op_union(stem, flag));
}

fn icon_clock(p: vec2<f32>, w: f32) -> f32 {
    let face = outline(sd_circle(p, 0.76), w);
    // A right angle, not ten-past-ten. Two hands of similar length meeting at
    // an acute angle read as a checkmark rather than as a clock — the hands
    // have to differ in both length and axis to be unmistakable.
    let hour = outline(sd_segment(p, vec2<f32>(0.0, 0.0), vec2<f32>(0.0, -0.36)), w);
    let minute = outline(sd_segment(p, vec2<f32>(0.0, 0.0), vec2<f32>(0.46, 0.0)), w);
    // The hub gives the hands somewhere to meet, which is what stops the join
    // reading as a corner.
    let hub = sd_circle(p, w * 0.62);
    return op_union(face, op_union(hub, op_union(hour, minute)));
}

fn icon_mail(p: vec2<f32>, w: f32) -> f32 {
    let body = outline(sd_box(p, vec2<f32>(0.84, 0.60), 0.16), w);
    let left = outline(sd_segment(p, vec2<f32>(-0.78, -0.50), vec2<f32>(0.0, 0.06)), w);
    let right = outline(sd_segment(p, vec2<f32>(0.78, -0.50), vec2<f32>(0.0, 0.06)), w);
    return op_union(body, op_union(left, right));
}

fn icon_studio(p: vec2<f32>, w: f32) -> f32 {
    // A page with a turned corner.
    //
    // A rounded rectangle holding a couple of lines simply *is* a phone, at any
    // size and to everyone — no amount of adjusting proportions fixes that. The
    // turned corner is the one detail that says paper and cannot say device, so
    // the outline is drawn as a path with the corner cut away rather than as a
    // box.
    let tl = vec2<f32>(-0.54, -0.80);
    let fold_top = vec2<f32>(0.16, -0.80);
    let fold_out = vec2<f32>(0.54, -0.42);
    let br = vec2<f32>(0.54, 0.80);
    let bl = vec2<f32>(-0.54, 0.80);
    let fold_in = vec2<f32>(0.16, -0.42);

    var edge = sd_segment(p, tl, fold_top);
    edge = min(edge, sd_segment(p, fold_top, fold_out));
    edge = min(edge, sd_segment(p, fold_out, br));
    edge = min(edge, sd_segment(p, br, bl));
    edge = min(edge, sd_segment(p, bl, tl));

    // The fold itself, which is what makes the cut corner read as turned paper
    // rather than as a chipped rectangle.
    var fold = sd_segment(p, fold_top, fold_in);
    fold = min(fold, sd_segment(p, fold_in, fold_out));

    let line_a = sd_segment(p, vec2<f32>(-0.26, 0.06), vec2<f32>(0.26, 0.06));
    let line_b = sd_segment(p, vec2<f32>(-0.26, 0.38), vec2<f32>(0.06, 0.38));

    let strokes = min(min(edge, fold), min(line_a, line_b));
    return strokes - w * 0.5;
}

fn icon_files(p: vec2<f32>, w: f32) -> f32 {
    // Tab and body unioned *before* stroking, so the seam between them is
    // interior to the shape and never drawn — a folder, not two boxes.
    let tab = sd_box(p - vec2<f32>(-0.42, -0.50), vec2<f32>(0.42, 0.18), 0.10);
    let body = sd_box(p - vec2<f32>(0.0, 0.14), vec2<f32>(0.84, 0.54), 0.14);
    return outline(op_union(tab, body), w);
}

fn icon_airplane(p: vec2<f32>, w: f32) -> f32 {
    // Seen from above: fuselage, swept wings, tailplane.
    let body = outline(sd_segment(p, vec2<f32>(0.0, -0.82), vec2<f32>(0.0, 0.74)), w);
    let wing_l = outline(sd_segment(p, vec2<f32>(-0.80, 0.26), vec2<f32>(0.0, -0.06)), w);
    let wing_r = outline(sd_segment(p, vec2<f32>(0.80, 0.26), vec2<f32>(0.0, -0.06)), w);
    let tail_l = outline(sd_segment(p, vec2<f32>(-0.28, 0.66), vec2<f32>(0.0, 0.50)), w);
    let tail_r = outline(sd_segment(p, vec2<f32>(0.28, 0.66), vec2<f32>(0.0, 0.50)), w);
    return op_union(body, op_union(op_union(wing_l, wing_r), op_union(tail_l, tail_r)));
}

fn icon_focus(p: vec2<f32>, w: f32) -> f32 {
    let ring = outline(sd_circle(p, 0.70), w);
    let core = sd_circle(p, 0.22);
    return op_union(ring, core);
}

fn icon_play(p: vec2<f32>, w: f32) -> f32 {
    let a = vec2<f32>(-0.40, -0.66);
    let b = vec2<f32>(0.66, 0.0);
    let c = vec2<f32>(-0.40, 0.66);
    let e1 = sd_segment(p, a, b);
    let e2 = sd_segment(p, b, c);
    let e3 = sd_segment(p, c, a);
    return op_union(e1, op_union(e2, e3)) - w * 0.5;
}

fn icon_pause(p: vec2<f32>, w: f32) -> f32 {
    let l = outline(sd_segment(p, vec2<f32>(-0.30, -0.62), vec2<f32>(-0.30, 0.62)), w * 1.35);
    let r = outline(sd_segment(p, vec2<f32>(0.30, -0.62), vec2<f32>(0.30, 0.62)), w * 1.35);
    return op_union(l, r);
}

fn triangle(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, w: f32) -> f32 {
    let e1 = sd_segment(p, a, b);
    let e2 = sd_segment(p, b, c);
    let e3 = sd_segment(p, c, a);
    return op_union(e1, op_union(e2, e3)) - w * 0.5;
}

fn icon_next(p: vec2<f32>, w: f32) -> f32 {
    let tri = triangle(
        p,
        vec2<f32>(-0.62, -0.56), vec2<f32>(0.26, 0.0), vec2<f32>(-0.62, 0.56),
        w,
    );
    let bar = outline(sd_segment(p, vec2<f32>(0.52, -0.56), vec2<f32>(0.52, 0.56)), w * 1.25);
    return op_union(tri, bar);
}

fn icon_previous(p: vec2<f32>, w: f32) -> f32 {
    return icon_next(vec2<f32>(-p.x, p.y), w);
}

fn icon_timer(p: vec2<f32>, w: f32) -> f32 {
    let q = p - vec2<f32>(0.0, 0.14);
    let face = outline(sd_circle(q, 0.66), w);
    // The stem a kitchen timer is wound by.
    let stem = outline(sd_segment(p, vec2<f32>(0.0, -0.58), vec2<f32>(0.0, -0.84)), w);
    let cap = outline(sd_segment(p, vec2<f32>(-0.22, -0.84), vec2<f32>(0.22, -0.84)), w);
    // One hand, counting down.
    let hand = outline(sd_segment(q, vec2<f32>(0.0, 0.0), vec2<f32>(0.30, -0.30)), w);
    return op_union(face, op_union(op_union(stem, cap), hand));
}

fn icon_alarm(p: vec2<f32>, w: f32) -> f32 {
    let q = p - vec2<f32>(0.0, 0.16);
    let face = outline(sd_circle(q, 0.62), w);
    // The two bells, angled off the top.
    let bell_l = outline(sd_segment(p, vec2<f32>(-0.74, -0.46), vec2<f32>(-0.40, -0.68)), w);
    let bell_r = outline(sd_segment(p, vec2<f32>(0.74, -0.46), vec2<f32>(0.40, -0.68)), w);
    let hand_a = outline(sd_segment(q, vec2<f32>(0.0, 0.0), vec2<f32>(0.0, -0.34)), w);
    let hand_b = outline(sd_segment(q, vec2<f32>(0.0, 0.0), vec2<f32>(0.26, 0.14)), w);
    return op_union(
        face,
        op_union(op_union(bell_l, bell_r), op_union(hand_a, hand_b)),
    );
}

fn icon_stopwatch(p: vec2<f32>, w: f32) -> f32 {
    let q = p - vec2<f32>(0.0, 0.16);
    let face = outline(sd_circle(q, 0.64), w);
    let crown = outline(sd_segment(p, vec2<f32>(0.0, -0.52), vec2<f32>(0.0, -0.82)), w * 1.2);
    // The side button is what tells it apart from the timer at a glance.
    let button = outline(sd_segment(p, vec2<f32>(0.52, -0.42), vec2<f32>(0.68, -0.58)), w * 1.2);
    let hand = outline(sd_segment(q, vec2<f32>(0.0, 0.0), vec2<f32>(0.0, -0.40)), w);
    return op_union(face, op_union(op_union(crown, button), hand));
}

fn icon_globe(p: vec2<f32>, w: f32) -> f32 {
    // Sphere, equator, one meridian. Adding the tropics turns it into a ball
    // of string at any size worth looking at — three lines say "globe" and a
    // fifth says nothing the first three did not.
    let sphere = outline(sd_circle(p, 0.76), w);
    let equator = outline(sd_segment(p, vec2<f32>(-0.76, 0.0), vec2<f32>(0.76, 0.0)), w);
    // Drawn as an ellipse so it reads as curvature rather than as a line down
    // the middle.
    let meridian = outline(sd_ellipse(p, vec2<f32>(0.38, 0.76)), w);
    return op_union(sphere, op_union(equator, meridian));
}

// ---- Files ---------------------------------------------------------------
//
// The destinations, and the three détends document types. §9 asks for "distinct,
// elegant icons" for native Studio files, and distinct is the operative word:
// Page, Deck and Grid share one silhouette — the same sheet with a folded
// corner — and differ only in the mark inside it. That way the family reads as a
// family at a glance, and the member is legible on a second look.

// The sheet every document type is drawn on: a page with its corner turned.
fn document_sheet(p: vec2<f32>, w: f32) -> f32 {
    let body = outline(sd_box(p, vec2<f32>(0.62, 0.84), 0.12), w);
    // The fold, as two strokes meeting at the corner.
    let fold_a = outline(sd_segment(p, vec2<f32>(0.18, -0.84), vec2<f32>(0.18, -0.44)), w);
    let fold_b = outline(sd_segment(p, vec2<f32>(0.18, -0.44), vec2<f32>(0.62, -0.44)), w);
    return op_union(body, op_union(fold_a, fold_b));
}

fn icon_document(p: vec2<f32>, w: f32) -> f32 {
    return document_sheet(p, w);
}

// Page: lines of text, shortened at the end like a real paragraph.
fn icon_page(p: vec2<f32>, w: f32) -> f32 {
    let sheet = document_sheet(p, w);
    let l1 = outline(sd_segment(p, vec2<f32>(-0.30, -0.12), vec2<f32>(0.30, -0.12)), w);
    let l2 = outline(sd_segment(p, vec2<f32>(-0.30, 0.16), vec2<f32>(0.30, 0.16)), w);
    let l3 = outline(sd_segment(p, vec2<f32>(-0.30, 0.44), vec2<f32>(0.06, 0.44)), w);
    return op_union(sheet, op_union(l1, op_union(l2, l3)));
}

// Deck: a slide — one framed rectangle, the way a deck is one thing per page.
fn icon_deck(p: vec2<f32>, w: f32) -> f32 {
    let sheet = document_sheet(p, w);
    let slide = outline(sd_box(p - vec2<f32>(0.0, 0.14), vec2<f32>(0.32, 0.24), 0.05), w);
    return op_union(sheet, slide);
}

// Grid: two rules crossing, which is the whole idea of a spreadsheet.
fn icon_grid(p: vec2<f32>, w: f32) -> f32 {
    let sheet = document_sheet(p, w);
    let h = outline(sd_segment(p, vec2<f32>(-0.34, 0.14), vec2<f32>(0.34, 0.14)), w);
    let v = outline(sd_segment(p, vec2<f32>(0.0, -0.20), vec2<f32>(0.0, 0.48)), w);
    let top = outline(sd_segment(p, vec2<f32>(-0.34, -0.20), vec2<f32>(0.34, -0.20)), w);
    return op_union(sheet, op_union(h, op_union(v, top)));
}

// Folder: the Files mark itself, so a folder in a listing and the place it
// lives in are visibly the same idea.
fn icon_folder(p: vec2<f32>, w: f32) -> f32 {
    return icon_files(p, w);
}

// Recents: a clock face turning back — the hand points anticlockwise.
fn icon_recent(p: vec2<f32>, w: f32) -> f32 {
    let ring = outline(sd_circle(p, 0.76), w);
    let hand_h = outline(sd_segment(p, vec2<f32>(0.0, 0.0), vec2<f32>(-0.34, 0.0)), w);
    let hand_m = outline(sd_segment(p, vec2<f32>(0.0, 0.0), vec2<f32>(0.0, -0.46)), w);
    return op_union(ring, op_union(hand_h, hand_m));
}

// Downloads: an arrow coming down onto a line.
fn icon_download(p: vec2<f32>, w: f32) -> f32 {
    let shaft = outline(sd_segment(p, vec2<f32>(0.0, -0.74), vec2<f32>(0.0, 0.26)), w);
    let head_l = outline(sd_segment(p, vec2<f32>(-0.34, -0.08), vec2<f32>(0.0, 0.26)), w);
    let head_r = outline(sd_segment(p, vec2<f32>(0.34, -0.08), vec2<f32>(0.0, 0.26)), w);
    let floor = outline(sd_segment(p, vec2<f32>(-0.62, 0.70), vec2<f32>(0.62, 0.70)), w);
    return op_union(op_union(shaft, floor), op_union(head_l, head_r));
}

// Screenshots: the corner marks of a capture selection.
fn icon_screenshot(p: vec2<f32>, w: f32) -> f32 {
    let a = 0.72;
    let b = 0.30;
    // Four L-shaped corners, drawn as eight strokes.
    let tl = op_union(
        outline(sd_segment(p, vec2<f32>(-a, -a), vec2<f32>(-b, -a)), w),
        outline(sd_segment(p, vec2<f32>(-a, -a), vec2<f32>(-a, -b)), w));
    let tr = op_union(
        outline(sd_segment(p, vec2<f32>(a, -a), vec2<f32>(b, -a)), w),
        outline(sd_segment(p, vec2<f32>(a, -a), vec2<f32>(a, -b)), w));
    let bl = op_union(
        outline(sd_segment(p, vec2<f32>(-a, a), vec2<f32>(-b, a)), w),
        outline(sd_segment(p, vec2<f32>(-a, a), vec2<f32>(-a, b)), w));
    let br = op_union(
        outline(sd_segment(p, vec2<f32>(a, a), vec2<f32>(b, a)), w),
        outline(sd_segment(p, vec2<f32>(a, a), vec2<f32>(a, b)), w));
    return op_union(op_union(tl, tr), op_union(bl, br));
}

// Recently Deleted: a bin. Lid, body, and the two rules down its face.
fn icon_trash(p: vec2<f32>, w: f32) -> f32 {
    let lid = outline(sd_segment(p, vec2<f32>(-0.74, -0.46), vec2<f32>(0.74, -0.46)), w);
    let handle = outline(sd_segment(p, vec2<f32>(-0.24, -0.72), vec2<f32>(0.24, -0.72)), w);
    let stem_l = outline(sd_segment(p, vec2<f32>(-0.24, -0.72), vec2<f32>(-0.24, -0.46)), w);
    let stem_r = outline(sd_segment(p, vec2<f32>(0.24, -0.72), vec2<f32>(0.24, -0.46)), w);
    let side_l = outline(sd_segment(p, vec2<f32>(-0.56, -0.46), vec2<f32>(-0.44, 0.76)), w);
    let side_r = outline(sd_segment(p, vec2<f32>(0.56, -0.46), vec2<f32>(0.44, 0.76)), w);
    let floor = outline(sd_segment(p, vec2<f32>(-0.44, 0.76), vec2<f32>(0.44, 0.76)), w);
    return op_union(
        op_union(op_union(lid, handle), op_union(stem_l, stem_r)),
        op_union(op_union(side_l, side_r), floor));
}

fn icon_distance(shape: i32, p: vec2<f32>, w: f32) -> f32 {
    switch shape {
        case 0:  { return icon_music(p, w); }
        case 1:  { return icon_clock(p, w); }
        case 2:  { return icon_mail(p, w); }
        case 3:  { return icon_studio(p, w); }
        case 4:  { return icon_files(p, w); }
        case 5:  { return icon_airplane(p, w); }
        case 6:  { return icon_focus(p, w); }
        case 7:  { return icon_play(p, w); }
        case 8:  { return icon_pause(p, w); }
        case 9:  { return icon_previous(p, w); }
        case 10: { return icon_next(p, w); }
        case 11: { return icon_timer(p, w); }
        case 12: { return icon_alarm(p, w); }
        case 13: { return icon_stopwatch(p, w); }
        case 14: { return icon_globe(p, w); }
        case 15: { return icon_recent(p, w); }
        case 16: { return icon_download(p, w); }
        case 17: { return icon_screenshot(p, w); }
        case 18: { return icon_trash(p, w); }
        case 19: { return icon_folder(p, w); }
        case 20: { return icon_document(p, w); }
        case 21: { return icon_page(p, w); }
        case 22: { return icon_deck(p, w); }
        case 23: { return icon_grid(p, w); }
        default: { return 1.0; }
    }
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let shape = i32(in.params.x + 0.5);
    let weight = in.params.y;
    let rim = in.params.z;
    let opacity = in.params.w;

    let d = icon_distance(shape, in.local, weight);

    // Analytic coverage. `fwidth` on an exact field is correct, which is why
    // nothing in détends needs multisampling.
    let aa = max(fwidth(d), 1e-5);
    let coverage = 1.0 - smoothstep(-aa, aa, d);
    if (coverage <= 0.0) {
        discard;
    }

    var color = in.color.rgb;

    // The same light that lights the panes, from the upper left. The stroke
    // brightens where it faces the light and dims where it turns away, so an
    // icon reads as a bent length of glass rather than as a drawn line.
    let gradient = vec2<f32>(dpdx(d), dpdy(d));
    if (rim > 0.0 && dot(gradient, gradient) > 1e-12) {
        let n = normalize(gradient);
        let light = normalize(vec2<f32>(-0.55, -0.84));
        let facing = dot(n, light);
        color += vec3<f32>(max(facing, 0.0) * 0.55 * rim);
        color -= vec3<f32>(max(-facing, 0.0) * 0.16 * rim);
    }

    let alpha = in.color.a * coverage * opacity;
    return vec4<f32>(max(color, vec3<f32>(0.0)) * alpha, alpha);
}
