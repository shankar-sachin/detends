//! The seam between the shell and the renderer.
//!
//! Each frame the shell produces a [`Frame`] — a flat, owned description of
//! what should be on screen — and the renderer consumes it. The shell has no
//! idea a GPU exists; the renderer has no idea what a mode or a Focus session
//! is.
//!
//! This is the most important structural decision in détends. When the Linux
//! compositor arrives, it produces the same [`Frame`] and the renderer is
//! untouched. The frame is `'static + Send`, so it can also be built on another
//! thread or double-buffered without fighting lifetimes.

use crate::{Color, Rect, Vec2};
use std::borrow::Cow;

/// Identifies a primitive across frames.
///
/// Stability is what lets the renderer cache expensive work — shaped text in
/// particular — and what lets animation bind to an element rather than to a
/// position in a list. Two primitives in different frames with the same id are
/// understood to be the same thing, moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u64);

impl Id {
    /// Derive an id from a stable name, so call sites can say what they mean
    /// instead of tracking counters.
    pub const fn of(name: &str) -> Self {
        // FNV-1a, const-evaluable so ids can live in constants.
        let bytes = name.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            i += 1;
        }
        Self(hash)
    }

    /// Derive a child id, for repeated elements like list rows.
    ///
    /// Offset by one before mixing so that `nth(0)` is distinct from the parent
    /// — otherwise the very first row of a list would silently share its
    /// container's identity, and the renderer would treat the two as one thing.
    pub const fn nth(self, n: u64) -> Self {
        Self(self.0 ^ n.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }
}

/// Which compositing layer a primitive belongs to.
///
/// Glass cannot be composited correctly in a single pass: a panel sitting over
/// another panel must refract what is actually beneath it, including the first
/// panel's own glass and highlights. So the renderer draws one layer, re-blurs
/// the result, then draws the next.
///
/// **Surfaces within a layer must not overlap.** That constraint is what keeps
/// each layer a single instanced draw, and it is checked in debug builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum Layer {
    /// The wallpaper and whatever the active mode renders into the background.
    Environment = 0,
    /// Mode content and the status cluster — the permanent furniture.
    Content = 1,
    /// System Center, Search, notifications. Things that arrive over the top.
    Overlay = 2,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::Environment, Layer::Content, Layer::Overlay];
}

/// A pane of détends glass.
///
/// The parameters describe a physical slab rather than a style: it has an index
/// of refraction and a thickness, and the look follows from those. See
/// `docs/GLASS.md`.
#[derive(Clone, Copy, Debug)]
pub struct Glass {
    pub rect: Rect,
    /// Corner radius in logical units.
    pub radius: f32,
    /// Superellipse exponent. 2 is a circular arc; 4–6 gives the continuous
    /// curvature that reads as a squircle. Rule 17 asks for few rounded
    /// rectangles, so the ones that exist should be optically correct.
    pub squircle: f32,
    /// Slab thickness, in logical units. Drives how far the rim displaces what
    /// is behind it — this is what makes it read as glass rather than as blur.
    pub thickness: f32,
    /// Width of the rounded bevel at the edge. Beyond this, the surface is
    /// flat and refracts nothing.
    pub bevel: f32,
    /// Index of refraction. 1.45–1.52 is optical glass.
    pub ior: f32,
    /// How far apart the red and blue samples are taken at the rim.
    pub dispersion: f32,
    /// Blend between blur pyramid levels: 0 is nearly clear, 1 is deeply
    /// frosted.
    pub frost: f32,
    /// Colour laid over the refracted backdrop. Kept low-saturation (§17).
    pub tint: Color,
    /// Strength of the Fresnel rim highlight.
    pub rim: f32,
}

impl Default for Glass {
    fn default() -> Self {
        Self {
            rect: Rect::ZERO,
            radius: 22.0,
            squircle: 5.0,
            thickness: 14.0,
            bevel: 18.0,
            ior: 1.48,
            dispersion: 0.018,
            frost: 0.55,
            tint: Color::TRANSPARENT,
            rim: 1.0,
        }
    }
}

/// Horizontal alignment within the text run's rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// A run of text.
///
/// The string is carried by value as a [`Cow`], so static labels cost nothing
/// and dynamic ones (a ticking clock) simply allocate. The frame stays `Send`
/// and `'static`, which a borrowed shaped buffer would prevent.
#[derive(Clone, Debug)]
pub struct Text {
    pub text: Cow<'static, str>,
    /// The box the text is laid out within.
    pub rect: Rect,
    pub size: f32,
    /// Variable-font weight, 100–900.
    pub weight: f32,
    /// Letter spacing, as a fraction of the font size. Negative tightens.
    pub tracking: f32,
    /// Extra line height beyond the font's own, as a multiplier.
    pub line_height: f32,
    pub color: Color,
    pub align: Align,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            text: Cow::Borrowed(""),
            rect: Rect::ZERO,
            size: 15.0,
            weight: 400.0,
            tracking: 0.0,
            line_height: 1.35,
            color: Color::WHITE,
            align: Align::Left,
        }
    }
}

/// A rectangle of flat colour. Rare by design — détends prefers type and glass
/// to boxes — but needed for separators, the wallpaper base, and fades.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fill {
    pub rect: Rect,
    pub radius: f32,
    pub squircle: f32,
    pub color: Color,
}

/// An image drawn from a texture the renderer owns.
///
/// Present from the start even though the only images today are the wallpaper
/// and placeholder album art, because under a Wayland compositor every client
/// window arrives as exactly this — and discovering that later would mean
/// rewriting the primitive enum.
#[derive(Clone, Copy, Debug)]
pub struct Image {
    pub rect: Rect,
    /// Opaque handle resolved by the renderer's texture registry.
    pub texture: TextureId,
    /// Sub-rectangle of the source, in normalised 0..1 coordinates.
    pub source: Rect,
    pub radius: f32,
    pub squircle: f32,
    /// Multiplied into the sampled colour; use white for an untouched image.
    pub tint: Color,
}

/// A texture owned by the renderer. Opaque to the shell on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

#[derive(Clone, Debug)]
pub enum Primitive {
    Glass(Glass),
    Fill(Fill),
    Text(Text),
    Image(Image),
    Icon(Icon),
}

impl Primitive {
    pub fn bounds(&self) -> Rect {
        match self {
            Primitive::Glass(g) => g.rect,
            Primitive::Fill(f) => f.rect,
            Primitive::Text(t) => t.rect,
            Primitive::Image(i) => i.rect,
            Primitive::Icon(i) => i.rect,
        }
    }
}

/// One primitive, with everything the renderer needs to place it.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: Id,
    pub layer: Layer,
    pub primitive: Primitive,
    /// Drawn in ascending order within a layer.
    pub z: i16,
    /// Multiplied into the primitive's own alpha. Kept separate so a whole
    /// group can fade without disturbing per-element opacity.
    pub opacity: f32,
}

impl Item {
    pub fn new(id: Id, layer: Layer, primitive: Primitive) -> Self {
        Self {
            id,
            layer,
            primitive,
            z: 0,
            opacity: 1.0,
        }
    }

    pub fn z(mut self, z: i16) -> Self {
        self.z = z;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

/// Everything the renderer needs for one frame.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    /// Logical size of the workspace.
    pub size: Vec2,
    /// Logical-to-physical ratio. The shell works in logical units throughout
    /// and never learns the backing scale; the renderer applies this.
    pub scale_factor: f32,
    pub items: Vec<Item>,
    /// Regions that changed since the previous frame. `None` means everything.
    /// A Wayland compositor wants this for efficient presentation; on a
    /// desktop window it is a pure saving.
    pub damage: Option<Vec<Rect>>,
    /// Whether anything is still in motion. When false and no clock has
    /// ticked, the host stops asking for frames entirely — a calm system
    /// should not pin a core to redraw a static screen.
    pub animating: bool,
}

impl Frame {
    pub fn new(size: Vec2, scale_factor: f32) -> Self {
        Self {
            size,
            scale_factor,
            items: Vec::new(),
            damage: None,
            animating: false,
        }
    }

    /// Reuse an existing frame's allocation. The shell keeps two frames and
    /// alternates, so steady-state rendering allocates nothing.
    pub fn reset(&mut self, size: Vec2, scale_factor: f32) {
        self.size = size;
        self.scale_factor = scale_factor;
        self.items.clear();
        self.damage = None;
        self.animating = false;
    }

    pub fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    /// Note that something is still moving. Any caller that animates must say
    /// so, or the host will stop drawing mid-transition.
    pub fn keep_animating(&mut self) {
        self.animating = true;
    }

    /// Sort into draw order: layer, then z, then insertion order.
    ///
    /// A stable sort, so equal-z items keep the order the shell emitted them
    /// in — which is the order that reads naturally at the call site.
    pub fn sort(&mut self) {
        self.items.sort_by_key(|i| (i.layer, i.z));
    }

    /// Check the one invariant the renderer's fast path depends on: glass
    /// surfaces within a layer must not overlap, since each layer is composited
    /// as a single instanced draw against one blurred backdrop.
    ///
    /// Debug-only. Overlapping glass does not crash, it just refracts the wrong
    /// thing — a subtle wrongness that is far easier to catch here than to
    /// diagnose by eye later.
    #[cfg(debug_assertions)]
    pub fn debug_check_layers(&self) -> Result<(), String> {
        for layer in Layer::ALL {
            let panes: Vec<&Item> = self
                .items
                .iter()
                .filter(|i| i.layer == layer && matches!(i.primitive, Primitive::Glass(_)))
                .collect();
            for (a, b) in panes
                .iter()
                .enumerate()
                .flat_map(|(n, a)| panes[n + 1..].iter().map(move |b| (a, b)))
            {
                if a.primitive.bounds().overlaps(&b.primitive.bounds()) {
                    return Err(format!(
                        "glass {:?} and {:?} overlap in {layer:?}; put one in a higher layer",
                        a.id, b.id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vec2;

    fn glass_at(x: f32, w: f32) -> Primitive {
        Primitive::Glass(Glass {
            rect: Rect::from_min_size(vec2(x, 0.0), vec2(w, 50.0)),
            ..Default::default()
        })
    }

    #[test]
    fn named_ids_are_stable_and_distinct() {
        assert_eq!(Id::of("system-center"), Id::of("system-center"));
        assert_ne!(Id::of("system-center"), Id::of("search"));
        // Usable in const position, which is the point of the const hash.
        const CLUSTER: Id = Id::of("status-cluster");
        assert_eq!(CLUSTER, Id::of("status-cluster"));
    }

    #[test]
    fn child_ids_do_not_collide() {
        let base = Id::of("inbox");
        let rows: Vec<Id> = (0..256).map(|n| base.nth(n)).collect();
        let mut unique = rows.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), rows.len(), "row ids collided");
        assert!(!rows.contains(&base));
    }

    #[test]
    fn sorting_is_by_layer_then_z_and_otherwise_stable() {
        let mut f = Frame::new(vec2(100.0, 100.0), 2.0);
        f.push(Item::new(Id(1), Layer::Overlay, glass_at(0.0, 10.0)));
        f.push(Item::new(Id(2), Layer::Environment, glass_at(0.0, 10.0)).z(5));
        f.push(Item::new(Id(3), Layer::Environment, glass_at(0.0, 10.0)).z(-1));
        f.push(Item::new(Id(4), Layer::Content, glass_at(0.0, 10.0)));
        f.push(Item::new(Id(5), Layer::Content, glass_at(0.0, 10.0)));
        f.sort();
        let order: Vec<u64> = f.items.iter().map(|i| i.id.0).collect();
        // Environment (by z), then Content (insertion order preserved for
        // equal z), then Overlay.
        assert_eq!(order, vec![3, 2, 4, 5, 1], "got {order:?}");
    }

    #[test]
    fn reset_keeps_the_allocation() {
        let mut f = Frame::new(vec2(10.0, 10.0), 1.0);
        for n in 0..32 {
            f.push(Item::new(Id(n), Layer::Content, glass_at(0.0, 1.0)));
        }
        let capacity = f.items.capacity();
        f.reset(vec2(20.0, 20.0), 2.0);
        assert!(f.items.is_empty());
        assert_eq!(
            f.items.capacity(),
            capacity,
            "steady state must not realloc"
        );
        assert!(!f.animating);
        assert_eq!(f.scale_factor, 2.0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn overlapping_glass_in_one_layer_is_rejected() {
        let mut f = Frame::new(vec2(500.0, 100.0), 1.0);
        f.push(Item::new(Id(1), Layer::Content, glass_at(0.0, 100.0)));
        f.push(Item::new(Id(2), Layer::Content, glass_at(50.0, 100.0)));
        assert!(f.debug_check_layers().is_err(), "overlap should be caught");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn glass_may_overlap_across_layers() {
        // This is the legitimate case: a panel over mode content. The renderer
        // re-blurs between layers precisely so this composites correctly.
        let mut f = Frame::new(vec2(500.0, 100.0), 1.0);
        f.push(Item::new(Id(1), Layer::Content, glass_at(0.0, 100.0)));
        f.push(Item::new(Id(2), Layer::Overlay, glass_at(50.0, 100.0)));
        assert!(f.debug_check_layers().is_ok());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn side_by_side_glass_in_one_layer_is_fine() {
        let mut f = Frame::new(vec2(500.0, 100.0), 1.0);
        f.push(Item::new(Id(1), Layer::Content, glass_at(0.0, 100.0)));
        f.push(Item::new(Id(2), Layer::Content, glass_at(120.0, 100.0)));
        assert!(f.debug_check_layers().is_ok());
    }

    #[test]
    fn a_frame_is_send_and_static() {
        fn assert_send<T: Send + 'static>() {}
        assert_send::<Frame>();
    }

    #[test]
    fn static_labels_do_not_allocate() {
        let t = Text {
            text: "Wi-Fi".into(),
            ..Default::default()
        };
        assert!(matches!(t.text, Cow::Borrowed(_)));
    }
}

/// One of the drawn icons.
///
/// Icons are signed-distance shapes evaluated in the shader rather than glyphs
/// from a font. Inter — like most text faces — has almost no symbol coverage:
/// `✈`, `◎`, `✉` and `◷` are all absent, so an interface that typed its icons
/// would silently render blank boxes. Drawing them also means they are crisp at
/// any size, tintable, animatable, and able to carry the same rim highlight as
/// the glass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum IconShape {
    // The five places.
    Music = 0,
    Clock = 1,
    Mail = 2,
    Studio = 3,
    Files = 4,

    // System states, replacing glyphs the font does not have.
    Airplane = 5,
    Focus = 6,

    // Transport.
    Play = 7,
    Pause = 8,
    Previous = 9,
    Next = 10,

    // Clock's own utilities.
    Timer = 11,
    Alarm = 12,
    Stopwatch = 13,
    Globe = 14,

    // Files: the destinations, and the détends document family (§9).
    Recent = 15,
    Download = 16,
    Screenshot = 17,
    Trash = 18,
    Folder = 19,
    Document = 20,
    Page = 21,
    Deck = 22,
    Grid = 23,
}

impl IconShape {
    pub const ALL: [IconShape; 24] = [
        IconShape::Music,
        IconShape::Clock,
        IconShape::Mail,
        IconShape::Studio,
        IconShape::Files,
        IconShape::Airplane,
        IconShape::Focus,
        IconShape::Play,
        IconShape::Pause,
        IconShape::Previous,
        IconShape::Next,
        IconShape::Timer,
        IconShape::Alarm,
        IconShape::Stopwatch,
        IconShape::Globe,
        IconShape::Recent,
        IconShape::Download,
        IconShape::Screenshot,
        IconShape::Trash,
        IconShape::Folder,
        IconShape::Document,
        IconShape::Page,
        IconShape::Deck,
        IconShape::Grid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            IconShape::Music => "Music",
            IconShape::Clock => "Clock",
            IconShape::Mail => "Mail",
            IconShape::Studio => "Studio",
            IconShape::Files => "Files",
            IconShape::Airplane => "Airplane",
            IconShape::Focus => "Focus",
            IconShape::Play => "Play",
            IconShape::Pause => "Pause",
            IconShape::Previous => "Previous",
            IconShape::Next => "Next",
            IconShape::Timer => "Timer",
            IconShape::Alarm => "Alarm",
            IconShape::Stopwatch => "Stopwatch",
            IconShape::Globe => "Globe",
            IconShape::Recent => "Recent",
            IconShape::Download => "Download",
            IconShape::Screenshot => "Screenshot",
            IconShape::Trash => "Trash",
            IconShape::Folder => "Folder",
            IconShape::Document => "Document",
            IconShape::Page => "Page",
            IconShape::Deck => "Deck",
            IconShape::Grid => "Grid",
        }
    }
}

/// A drawn icon.
#[derive(Clone, Copy, Debug)]
pub struct Icon {
    /// The square the icon is drawn inside. Non-square rectangles are centred
    /// on the shorter axis rather than stretched — an icon set with drifting
    /// proportions is the clearest sign of a careless one.
    pub rect: Rect,
    pub shape: IconShape,
    /// Stroke weight as a fraction of the icon's size.
    ///
    /// Held constant across the whole set: one weight is what makes a set of
    /// icons read as designed together rather than merely collected.
    pub stroke: f32,
    pub color: Color,
    /// How much of the glass rim highlight the stroke catches, 0 to 1. Enough
    /// that the icon reads as bent glass; not so much that it competes with
    /// the panes.
    pub rim: f32,
}

/// The stroke weight the whole set is drawn at, as a fraction of icon size.
///
/// 1.7 units on the 24-unit grid. Two units — the obvious choice — reads heavy
/// once an icon is large, which is at odds with glass letterforms this thin.
pub const ICON_STROKE: f32 = 1.7 / 24.0;

impl Default for Icon {
    fn default() -> Self {
        Self {
            rect: Rect::ZERO,
            shape: IconShape::Music,
            stroke: ICON_STROKE,
            color: Color::WHITE,
            rim: 0.5,
        }
    }
}
