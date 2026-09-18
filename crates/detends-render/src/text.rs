//! Text, via glyphon.
//!
//! Typography carries the hierarchy in détends (§17), so this is not an
//! afterthought layer on top of the graphics — it is most of the interface.
//!
//! Shaping is cached per display-list id. Text is by far the most expensive
//! thing a frame does, and almost none of it changes between frames: a clock
//! reshapes once a second, everything else effectively never. The stable ids
//! on the display list exist largely so this cache can work.

use detends_paint::{Align, Frame, Id, Layer, Primitive, Text};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// The variable typeface détends is set in.
///
/// Bundled rather than taken from the system: San Francisco is Apple's voice
/// and rule 17 asks détends to have its own, it does not exist on Linux, and a
/// bundled face renders identically everywhere.
const INTER: &[u8] = include_bytes!("../../../assets/fonts/InterVariable.ttf");

/// Resolve the family name the bundled face actually registers under.
///
/// Not hardcoded, because it is not what you would guess: Inter's variable
/// build registers as "Inter Variable", so asking for "Inter" silently falls
/// back to a system font and the whole typographic identity quietly evaporates
/// with no error anywhere.
///
/// Resolved from a database holding nothing but this face, so the answer does
/// not depend on what else happens to be loaded or in what order — and stays
/// correct if the typeface is ever swapped.
fn resolve_family(font: &[u8]) -> String {
    let mut db = glyphon::fontdb::Database::new();
    db.load_font_data(font.to_vec());
    let name = db
        .faces()
        .next()
        .and_then(|face| face.families.first().map(|(name, _)| name.clone()));
    name.unwrap_or_else(|| "sans-serif".to_string())
}

pub struct TextStack {
    font_system: glyphon::FontSystem,
    swash: glyphon::SwashCache,
    viewport: glyphon::Viewport,
    atlas: glyphon::TextAtlas,
    /// One renderer per compositing layer.
    ///
    /// Not one for the whole frame: glass layers are composited in sequence,
    /// so text belonging to mode content has to be drawn *before* the System
    /// Center's glass goes over the top. A single renderer at the end would
    /// put every label above every panel, and the mode's text would show
    /// through an overlay that is supposed to cover it.
    renderers: Vec<glyphon::TextRenderer>,
    shaped: HashMap<Id, Shaped>,
    /// Frame counter, so buffers nobody drew for a while can be dropped.
    generation: u64,
    /// The family name the bundled face actually registered under.
    family: String,
}

struct Shaped {
    buffer: glyphon::Buffer,
    /// Hash of everything that would change the shaping.
    signature: u64,
    last_used: u64,
}

impl TextStack {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut font_system = glyphon::FontSystem::new();
        font_system.db_mut().load_font_data(INTER.to_vec());
        let family = resolve_family(INTER);
        log::info!("détends is set in {family}");

        let cache = glyphon::Cache::new(device);
        let viewport = glyphon::Viewport::new(device, &cache);

        // `Web` rather than `Accurate`: détends composites into a linear
        // working target and encodes to sRGB once at the end, which is the
        // arrangement `Web` expects. `Accurate` assumes an sRGB render target
        // and would double-correct.
        let mut atlas = glyphon::TextAtlas::with_color_mode(
            device,
            queue,
            &cache,
            format,
            glyphon::ColorMode::Web,
        );

        // The renderers must be built against *this* atlas, not a second one —
        // it is the atlas `prepare` grows and they sample. They share it, and
        // the glyph cache with it, which is the point of one atlas.
        let renderers = Layer::ALL
            .iter()
            .map(|_| {
                glyphon::TextRenderer::new(
                    &mut atlas,
                    device,
                    // No MSAA anywhere in détends: glyph edges come from atlas
                    // coverage and glass edges are analytically antialiased
                    // from an exact SDF, so multisampling would cost bandwidth
                    // for nothing.
                    wgpu::MultisampleState::default(),
                    None,
                )
            })
            .collect();

        Self {
            font_system,
            swash: glyphon::SwashCache::new(),
            viewport,
            atlas,
            renderers,
            shaped: HashMap::new(),
            generation: 0,
            family,
        }
    }

    /// Shape everything in the frame and upload it. Must run before the render
    /// pass begins — it writes to the queue and may grow the atlas.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
        physical: (u32, u32),
    ) -> Result<(), glyphon::PrepareError> {
        self.generation += 1;
        self.viewport.update(
            queue,
            glyphon::Resolution {
                width: physical.0,
                height: physical.1,
            },
        );

        let scale = frame.scale_factor;

        // Shape (or reuse) a buffer for every text item.
        for item in &frame.items {
            let Primitive::Text(text) = &item.primitive else {
                continue;
            };
            self.shape(item.id, text, scale);
        }

        // Then build each layer's draw list. Split from shaping above because
        // that needs `&mut font_system` while these borrow the buffers.
        let generation = self.generation;

        for (index, layer) in Layer::ALL.iter().enumerate() {
            let shaped = &self.shaped;
            let areas = frame
                .items
                .iter()
                .filter(|item| item.layer == *layer)
                .filter_map(|item| {
                    let Primitive::Text(text) = &item.primitive else {
                        return None;
                    };
                    let entry = shaped.get(&item.id)?;
                    if entry.last_used != generation {
                        return None;
                    }

                    let min = text.rect.min();
                    let max = text.rect.max();

                    Some(glyphon::TextArea {
                        buffer: &entry.buffer,
                        left: min.x * scale,
                        top: min.y * scale,
                        scale,
                        bounds: glyphon::TextBounds {
                            left: (min.x * scale) as i32,
                            top: (min.y * scale) as i32,
                            right: (max.x * scale).ceil() as i32,
                            bottom: (max.y * scale).ceil() as i32,
                        },
                        // Opacity is folded in here rather than at shape time,
                        // which is why fading text costs nothing.
                        default_color: to_glyphon_color(text.color.fade(item.opacity)),
                        custom_glyphs: &[],
                    })
                });

            self.renderers[index].prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash,
            )?;
        }

        Ok(())
    }

    /// Draw into an existing pass.
    ///
    /// Shares the caller's render pass rather than opening one of its own: on
    /// a tile-based GPU an extra pass means storing and reloading the whole
    /// framebuffer out of tile memory, which is a real cost for no benefit.
    pub fn render(
        &self,
        layer: Layer,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> Result<(), glyphon::RenderError> {
        self.renderers[layer as usize].render(&self.atlas, &self.viewport, pass)
    }

    /// Release atlas space and drop buffers nobody has drawn for a while.
    pub fn trim(&mut self) {
        self.atlas.trim();
        // Two seconds of frames at 120Hz is generous, and bounded.
        let cutoff = self.generation.saturating_sub(240);
        self.shaped.retain(|_, s| s.last_used >= cutoff);
    }

    fn shape(&mut self, id: Id, text: &Text, scale: f32) {
        let signature = signature_of(text, scale);

        if let Some(existing) = self.shaped.get_mut(&id) {
            if existing.signature == signature {
                existing.last_used = self.generation;
                return;
            }
        }

        let metrics = glyphon::Metrics::new(text.size, text.size * text.line_height);
        let mut buffer = glyphon::Buffer::new(&mut self.font_system, metrics);

        buffer.set_size(Some(text.rect.width()), Some(text.rect.height()));

        let attrs = glyphon::Attrs::new()
            .family(glyphon::Family::Name(&self.family))
            .weight(glyphon::Weight(text.weight as u16))
            // cosmic-text adds letter spacing to an advance that has already
            // been divided by units-per-em, so this value is in **em**, not
            // pixels. Tracking is defined as a fraction of the size, which is
            // the same thing — so it is passed straight through.
            //
            // Multiplying by the size here (the obvious-looking mistake) scales
            // the spacing by the font size twice: at 30px a tracking of -0.012
            // becomes -10.8px per glyph and the word collapses into itself.
            .letter_spacing(text.tracking);

        let alignment = Some(match text.align {
            Align::Left => glyphon::cosmic_text::Align::Left,
            Align::Center => glyphon::cosmic_text::Align::Center,
            Align::Right => glyphon::cosmic_text::Align::Right,
        });

        // `Shaping::Advanced` rather than `Basic`: détends has to render World
        // Clock city names and mail from anywhere, so correct shaping for
        // non-Latin scripts is not optional.
        buffer.set_text(&text.text, &attrs, glyphon::Shaping::Advanced, alignment);
        buffer.shape_until_scroll(&mut self.font_system, false);

        self.shaped.insert(
            id,
            Shaped {
                buffer,
                signature,
                last_used: self.generation,
            },
        );
    }

    #[cfg(test)]
    pub fn cached_count(&self) -> usize {
        self.shaped.len()
    }
}

/// Everything that would change the shaped result. Colour and opacity are
/// deliberately absent — they are applied at draw time, so recolouring text
/// costs nothing.
fn signature_of(text: &Text, scale: f32) -> u64 {
    let mut h = DefaultHasher::new();
    text.text.hash(&mut h);
    text.size.to_bits().hash(&mut h);
    text.weight.to_bits().hash(&mut h);
    text.tracking.to_bits().hash(&mut h);
    text.line_height.to_bits().hash(&mut h);
    text.rect.width().to_bits().hash(&mut h);
    text.rect.height().to_bits().hash(&mut h);
    (text.align as u8).hash(&mut h);
    scale.to_bits().hash(&mut h);
    h.finish()
}

/// Linear colour to glyphon's 8-bit sRGB.
fn to_glyphon_color(c: detends_paint::Color) -> glyphon::Color {
    let e = |v: f32| (detends_paint::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
    glyphon::Color::rgba(
        e(c.r),
        e(c.g),
        e(c.b),
        (c.a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::{vec2, Rect};

    fn text_of(s: &'static str) -> Text {
        Text {
            text: s.into(),
            rect: Rect::from_min_size(vec2(0.0, 0.0), vec2(200.0, 40.0)),
            ..Default::default()
        }
    }

    #[test]
    fn the_bundled_face_registers_as_an_inter_family() {
        // Guards the exact failure above: if the family name ever stops
        // matching what is actually loaded, text silently falls back to a
        // system font and détends loses its typography without an error.
        let family = resolve_family(INTER);
        assert!(
            family.contains("Inter"),
            "resolved to {family:?}, not Inter"
        );
    }

    #[test]
    fn a_corrupt_font_falls_back_rather_than_panicking() {
        assert_eq!(resolve_family(b"not a font"), "sans-serif");
    }

    #[test]
    fn the_bundled_font_is_present_and_plausible() {
        assert!(INTER.len() > 100_000, "Inter looks truncated");
        // TrueType magic: either 0x00010000 or "true".
        assert!(
            INTER.starts_with(&[0x00, 0x01, 0x00, 0x00]) || INTER.starts_with(b"true"),
            "not a TrueType file"
        );
    }

    #[test]
    fn identical_text_shares_a_signature() {
        assert_eq!(
            signature_of(&text_of("Resonance"), 2.0),
            signature_of(&text_of("Resonance"), 2.0)
        );
    }

    #[test]
    fn changing_the_string_reshapes() {
        assert_ne!(
            signature_of(&text_of("28:47"), 2.0),
            signature_of(&text_of("28:46"), 2.0)
        );
    }

    #[test]
    fn changing_the_scale_reshapes() {
        // A window dragged to a different display must re-shape, or glyphs
        // land on the wrong subpixel grid and go soft.
        assert_ne!(
            signature_of(&text_of("Inbox"), 1.0),
            signature_of(&text_of("Inbox"), 2.0)
        );
    }

    #[test]
    fn recolouring_does_not_reshape() {
        // Fading text in and out is constant work, not a reshape per frame.
        // This is what makes the Focus dimming free.
        let mut a = text_of("Deep Work");
        let b = a.clone();
        a.color = detends_paint::Color::BLACK;
        assert_eq!(signature_of(&a, 2.0), signature_of(&b, 2.0));
    }

    #[test]
    fn weight_and_tracking_are_part_of_the_signature() {
        let mut heavy = text_of("Music");
        heavy.weight = 700.0;
        assert_ne!(
            signature_of(&heavy, 2.0),
            signature_of(&text_of("Music"), 2.0)
        );

        let mut tracked = text_of("Music");
        tracked.tracking = 0.05;
        assert_ne!(
            signature_of(&tracked, 2.0),
            signature_of(&text_of("Music"), 2.0)
        );
    }

    #[test]
    fn tracking_is_applied_in_em_not_pixels() {
        // Locks down the units. If tracking were ever multiplied by the font
        // size again, large text would collapse into an illegible pile while
        // small text looked merely a little loose — a bug that is very easy to
        // stare past.
        use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

        let mut fs = FontSystem::new();
        fs.db_mut().load_font_data(INTER.to_vec());
        let family = resolve_family(INTER);

        let span = |fs: &mut FontSystem, size: f32, tracking: f32| -> f32 {
            let mut buf = Buffer::new(fs, Metrics::new(size, size * 1.35));
            buf.set_size(Some(2000.0), Some(size * 2.0));
            let attrs = Attrs::new()
                .family(Family::Name(&family))
                .weight(Weight(400))
                .letter_spacing(tracking);
            buf.set_text("Resonance", &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(fs, false);
            buf.layout_runs().next().map_or(0.0, |r| r.line_w)
        };

        for size in [11.0_f32, 30.0, 112.0] {
            let plain = span(&mut fs, size, 0.0);
            let tracked = span(&mut fs, size, -0.012);

            assert!(
                plain > size * 3.0,
                "{size}px did not lay out at all: {plain}"
            );
            assert!(tracked < plain, "negative tracking should tighten");

            // Nine glyphs, so -0.012em should remove about 0.1em in total —
            // a nudge, never a collapse.
            let removed = (plain - tracked) / size;
            assert!(
                (0.05..0.2).contains(&removed),
                "{size}px lost {removed}em to tracking; units are wrong"
            );
        }
    }

    #[test]
    fn layout_is_proportional_to_size() {
        // The invariant that first exposed the tracking bug: a string's width
        // divided by its size is the same at every size.
        use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

        let mut fs = FontSystem::new();
        fs.db_mut().load_font_data(INTER.to_vec());
        let family = resolve_family(INTER);

        let ratio = |fs: &mut FontSystem, size: f32| -> f32 {
            let mut buf = Buffer::new(fs, Metrics::new(size, size * 1.35));
            buf.set_size(Some(4000.0), Some(size * 2.0));
            let attrs = Attrs::new()
                .family(Family::Name(&family))
                .weight(Weight(400));
            buf.set_text("Resonance", &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(fs, false);
            buf.layout_runs().next().map_or(0.0, |r| r.line_w) / size
        };

        let reference = ratio(&mut fs, 15.0);
        for size in [11.0_f32, 30.0, 60.0, 112.0] {
            let r = ratio(&mut fs, size);
            assert!(
                (r - reference).abs() < 0.02,
                "{size}px ratio {r} vs {reference}"
            );
        }
    }

    #[test]
    fn colour_conversion_encodes_to_srgb() {
        use detends_paint::Color;
        // Linear 0.216 is sRGB mid-grey, 128.
        let mid = to_glyphon_color(Color::hex(0x808080));
        assert!((mid.r() as i32 - 128).abs() <= 1, "got {}", mid.r());
        assert_eq!(to_glyphon_color(Color::WHITE).r(), 255);
        assert_eq!(to_glyphon_color(Color::BLACK).r(), 0);
    }

    #[test]
    fn opacity_survives_the_conversion() {
        use detends_paint::Color;
        assert_eq!(to_glyphon_color(Color::WHITE.alpha(0.5)).a(), 128);
    }
}
