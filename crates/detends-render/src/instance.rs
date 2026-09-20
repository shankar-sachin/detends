//! GPU-side instance and uniform layouts.
//!
//! Every struct here mirrors one in the WGSL. They are kept together so that a
//! change to a shader's inputs is a change to one adjacent pair rather than a
//! hunt through the renderer.

use detends_paint::{Color, Fill, Glass, Icon, Image, Rect};

/// Per-pane data for `glass.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlassInstance {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    /// radius, squircle exponent, thickness, bevel width
    pub shape: [f32; 4],
    /// ior, dispersion, frost, rim strength
    pub optics: [f32; 4],
    pub tint: [f32; 4],
    pub opacity: f32,
    pub _pad: [f32; 3],
}

impl GlassInstance {
    pub fn from_paint(g: &Glass, opacity: f32) -> Self {
        Self {
            center: [g.rect.center.x, g.rect.center.y],
            half_size: [g.rect.half.x, g.rect.half.y],
            shape: [
                g.radius,
                g.squircle.max(2.0),
                g.thickness,
                g.bevel.max(0.001),
            ],
            optics: [
                g.ior.max(1.0001),
                g.dispersion,
                g.frost.clamp(0.0, 1.0),
                g.rim,
            ],
            tint: [g.tint.r, g.tint.g, g.tint.b, g.tint.a],
            opacity,
            _pad: [0.0; 3],
        }
    }

    pub const LAYOUT: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2, // center
        1 => Float32x2, // half_size
        2 => Float32x4, // shape
        3 => Float32x4, // optics
        4 => Float32x4, // tint
        5 => Float32,   // opacity
    ];
}

/// Per-quad data for `flat.wgsl`, covering both fills and images.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FlatInstance {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    /// radius, squircle, uses_texture, opacity
    pub shape: [f32; 4],
    pub color: [f32; 4],
    /// Source rect in the texture, normalised: x, y, w, h.
    pub source: [f32; 4],
}

impl FlatInstance {
    pub fn from_fill(f: &Fill, opacity: f32) -> Self {
        Self {
            center: [f.rect.center.x, f.rect.center.y],
            half_size: [f.rect.half.x, f.rect.half.y],
            shape: [f.radius, f.squircle.max(2.0), 0.0, opacity],
            color: [f.color.r, f.color.g, f.color.b, f.color.a],
            source: [0.0, 0.0, 1.0, 1.0],
        }
    }

    pub fn from_image(i: &Image, opacity: f32) -> Self {
        Self {
            center: [i.rect.center.x, i.rect.center.y],
            half_size: [i.rect.half.x, i.rect.half.y],
            shape: [i.radius, i.squircle.max(2.0), 1.0, opacity],
            color: [i.tint.r, i.tint.g, i.tint.b, i.tint.a],
            source: source_rect(&i.source),
        }
    }

    pub const LAYOUT: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2, // center
        1 => Float32x2, // half_size
        2 => Float32x4, // shape
        3 => Float32x4, // color
        4 => Float32x4, // source
    ];
}

fn source_rect(r: &Rect) -> [f32; 4] {
    let min = r.min();
    let size = r.size();
    // A default-constructed Rect is empty; treat that as "the whole texture"
    // so callers do not have to spell out the common case.
    if size.x <= 0.0 || size.y <= 0.0 {
        [0.0, 0.0, 1.0, 1.0]
    } else {
        [min.x, min.y, size.x, size.y]
    }
}

/// Per-icon data for `icon.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IconInstance {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    /// shape index, stroke width in icon space, rim strength, opacity
    pub params: [f32; 4],
    pub color: [f32; 4],
}

impl IconInstance {
    pub fn from_paint(icon: &Icon, opacity: f32) -> Self {
        Self {
            center: [icon.rect.center.x, icon.rect.center.y],
            half_size: [icon.rect.half.x, icon.rect.half.y],
            params: [
                icon.shape as u32 as f32,
                // The icon's design grid spans −1…+1, so a stroke expressed as
                // a fraction of the icon's size is twice that in icon space.
                (icon.stroke * 2.0).max(0.001),
                icon.rim.clamp(0.0, 1.0),
                opacity.clamp(0.0, 1.0),
            ],
            color: [icon.color.r, icon.color.g, icon.color.b, icon.color.a],
        }
    }

    pub const LAYOUT: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2, // center
        1 => Float32x2, // half_size
        2 => Float32x4, // params
        3 => Float32x4, // color
    ];
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlassUniforms {
    pub resolution: [f32; 2],
    pub scale: f32,
    pub time: f32,
    pub intensity: f32,
    pub transparency: f32,
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EnvUniforms {
    pub resolution: [f32; 2],
    pub time: f32,
    pub focus: f32,
    pub near: [f32; 4],
    pub far: [f32; 4],
    /// The two light sources, as `rgb` + strength in `a`.
    pub glow_warm: [f32; 4],
    pub glow_cool: [f32; 4],
    pub presence: f32,
    pub _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurLevel {
    pub inv_source: [f32; 2],
    pub scale: f32,
    pub _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PresentUniforms {
    pub encode: f32,
    pub fade: f32,
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FlatUniforms {
    pub resolution: [f32; 2],
    pub scale: f32,
    pub time: f32,
}

/// Linear colour to a raw array, for uniform upload.
pub fn color_array(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::{vec2, Rect};

    #[test]
    fn uniform_blocks_meet_the_sixteen_byte_rule() {
        // Uniform buffer bindings must be a multiple of 16 bytes, and a
        // mismatch here is a validation error at pipeline creation rather than
        // anything legible.
        for size in [
            std::mem::size_of::<GlassUniforms>(),
            std::mem::size_of::<EnvUniforms>(),
            std::mem::size_of::<BlurLevel>(),
            std::mem::size_of::<PresentUniforms>(),
            std::mem::size_of::<FlatUniforms>(),
        ] {
            assert_eq!(size % 16, 0, "{size} bytes is not a multiple of 16");
        }
    }

    #[test]
    fn the_vertex_layouts_cover_every_byte_of_their_instance() {
        // A gap between the declared attributes and the struct size means the
        // shader would read the wrong field with no error at all.
        let glass_span: u64 = GlassInstance::LAYOUT
            .iter()
            .map(|a| a.offset + a.format.size())
            .max()
            .unwrap();
        assert!(glass_span <= std::mem::size_of::<GlassInstance>() as u64);

        let flat_span: u64 = FlatInstance::LAYOUT
            .iter()
            .map(|a| a.offset + a.format.size())
            .max()
            .unwrap();
        assert_eq!(flat_span, std::mem::size_of::<FlatInstance>() as u64);
    }

    #[test]
    fn icon_conversion_carries_the_shape_and_weight() {
        use detends_paint::{IconShape, ICON_STROKE};
        let icon = Icon {
            rect: Rect::from_center_size(vec2(100.0, 50.0), vec2(40.0, 40.0)),
            shape: IconShape::Files,
            stroke: ICON_STROKE,
            rim: 0.5,
            ..Default::default()
        };
        let i = IconInstance::from_paint(&icon, 0.8);
        assert_eq!(i.center, [100.0, 50.0]);
        assert_eq!(i.params[0], IconShape::Files as u32 as f32);
        assert!((i.params[1] - ICON_STROKE * 2.0).abs() < 1e-6);
        assert_eq!(i.params[3], 0.8);
    }

    #[test]
    fn every_icon_shape_survives_the_round_trip_to_a_float() {
        use detends_paint::IconShape;
        // The shape index travels as a float because integer varyings must be
        // flat in WGSL. Every index has to land back on itself after the
        // shader rounds it.
        for shape in IconShape::ALL {
            let encoded = shape as u32 as f32;
            assert_eq!((encoded + 0.5) as u32, shape as u32, "{shape:?} did not round-trip");
        }
    }

    #[test]
    fn a_zero_stroke_is_clamped_so_an_icon_is_never_invisible() {
        let icon = Icon { stroke: 0.0, ..Default::default() };
        assert!(IconInstance::from_paint(&icon, 1.0).params[1] > 0.0);
    }

    #[test]
    fn glass_conversion_carries_the_optics_through() {
        let g = Glass {
            rect: Rect::from_min_size(vec2(10.0, 20.0), vec2(100.0, 40.0)),
            ior: 1.48,
            dispersion: 0.02,
            frost: 0.6,
            rim: 0.8,
            ..Default::default()
        };
        let i = GlassInstance::from_paint(&g, 0.5);
        assert_eq!(i.center, [60.0, 40.0]);
        assert_eq!(i.half_size, [50.0, 20.0]);
        assert_eq!(i.optics, [1.48, 0.02, 0.6, 0.8]);
        assert_eq!(i.opacity, 0.5);
    }

    #[test]
    fn degenerate_optics_are_clamped_to_something_physical() {
        let g = Glass {
            ior: 0.5,
            squircle: 0.0,
            bevel: 0.0,
            frost: 5.0,
            ..Default::default()
        };
        let i = GlassInstance::from_paint(&g, 1.0);
        assert!(i.optics[0] > 1.0, "an ior below 1 would invert refraction");
        assert!(
            i.shape[1] >= 2.0,
            "a squircle exponent below 2 is not a shape"
        );
        assert!(
            i.shape[3] > 0.0,
            "a zero bevel divides by zero in the shader"
        );
        assert_eq!(i.optics[2], 1.0, "frost must stay in 0..1");
    }

    #[test]
    fn an_empty_source_rect_means_the_whole_texture() {
        assert_eq!(source_rect(&Rect::ZERO), [0.0, 0.0, 1.0, 1.0]);
        let half = Rect::from_min_size(vec2(0.0, 0.0), vec2(0.5, 1.0));
        assert_eq!(source_rect(&half), [0.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn fills_and_images_are_distinguished_by_the_texture_flag() {
        let fill = FlatInstance::from_fill(&Fill::default(), 1.0);
        assert_eq!(fill.shape[2], 0.0);
        let image = FlatInstance::from_image(
            &Image {
                rect: Rect::ZERO,
                texture: detends_paint::TextureId(0),
                source: Rect::ZERO,
                radius: 0.0,
                squircle: 4.0,
                tint: detends_paint::Color::WHITE,
            },
            1.0,
        );
        assert_eq!(image.shape[2], 1.0);
    }
}
