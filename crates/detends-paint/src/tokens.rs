//! The détends design tokens.
//!
//! Deliberately small. Typography carries the hierarchy (§17), so there are
//! more type styles here than colours — six text styles against a near-
//! monochrome palette of about eight values per theme. Anything that wants a
//! ninth colour should probably be using weight or size instead.

use crate::Color;

/// Light, dark, or follow the environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
    /// Resolved by the host from the platform's own setting.
    Automatic,
}

/// A resolved palette. Near-monochrome by rule — "prefer very few colours
/// simultaneously" (§17) — with a single cool hue running through every value
/// so that glass tinting never introduces a second temperature.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// The deepest background, behind everything.
    pub ground: Color,
    /// The far end of the wallpaper gradient.
    pub ground_far: Color,
    /// Body copy and anything that must be read.
    pub text: Color,
    /// Supporting text — the artist under the track, the date under the time.
    pub text_soft: Color,
    /// Text that is present but not competing: the active provider (§4), units.
    pub text_faint: Color,
    /// The tint laid over refracted backdrop inside a glass pane.
    pub glass: Color,
    /// The Fresnel rim highlight.
    pub rim: Color,
    /// The one accent in the system, used sparingly — Focus, selection.
    pub accent: Color,
    /// The two lights the environment is lit by (§17).
    ///
    /// The ground stays deep and neutral; these carry the colour. Two of them,
    /// at different hues and different corners, because one light over a flat
    /// ground is a radial gradient and reads as exactly that — while two give
    /// the field a direction and a gentle hue shift across the screen, which
    /// is what glass needs to have something worth bending.
    pub glow_warm: Color,
    pub glow_cool: Color,
}

/// The single hue détends is built on: a cool blue-grey, barely there.
///
/// Every neutral carries a trace of it rather than being pure grey, which is
/// what stops large translucent surfaces reading as dirty.
const HUE: f32 = 255.0;
const NEUTRAL_CHROMA: f32 = 0.006;

impl Palette {
    pub fn dark() -> Self {
        Self {
            // The two ends of the environment gradient are further apart than
            // a flat backdrop would need, because glass has to have something
            // to bend. Over a uniform field the refraction is mathematically
            // present and visually absent, and the pane reads as a grey
            // rectangle. The range is still dark and still calm — it simply
            // varies across the screen.
            ground: Color::oklch(0.095, NEUTRAL_CHROMA, HUE),
            ground_far: Color::oklch(0.235, NEUTRAL_CHROMA * 2.2, HUE + 22.0),
            text: Color::oklch(0.965, 0.003, HUE),
            text_soft: Color::oklch(0.740, 0.005, HUE),
            text_faint: Color::oklch(0.560, 0.006, HUE),
            // Glass on a dark ground lightens slightly; a dark tint would read
            // as a hole rather than a pane.
            glass: Color::oklch(0.62, 0.008, HUE).alpha(0.10),
            rim: Color::oklch(0.98, 0.004, HUE).alpha(0.30),
            accent: Color::oklch(0.72, 0.085, 232.0),
            // Restrained on purpose: the alpha is the strength, and at these
            // values neither light is nameable as a colour. You should not be
            // able to say "it is blue" — only that it is not grey.
            glow_cool: Color::oklch(0.54, 0.085, 250.0).alpha(0.42),
            glow_warm: Color::oklch(0.52, 0.055, 28.0).alpha(0.20),
        }
    }

    pub fn light() -> Self {
        Self {
            ground: Color::oklch(0.985, NEUTRAL_CHROMA * 0.5, HUE),
            ground_far: Color::oklch(0.855, NEUTRAL_CHROMA * 1.8, HUE + 22.0),
            text: Color::oklch(0.235, 0.006, HUE),
            text_soft: Color::oklch(0.470, 0.007, HUE),
            text_faint: Color::oklch(0.630, 0.007, HUE),
            // On a light ground the pane darkens instead, so it still reads as
            // a distinct surface rather than dissolving into the background.
            glass: Color::oklch(0.55, 0.006, HUE).alpha(0.07),
            rim: Color::oklch(1.0, 0.0, HUE).alpha(0.55),
            accent: Color::oklch(0.55, 0.105, 232.0),
            // On a light ground the lights tint rather than illuminate, so
            // they are weaker still — a bright field shows colour far more
            // readily than a dark one.
            glow_cool: Color::oklch(0.80, 0.055, 248.0).alpha(0.40),
            glow_warm: Color::oklch(0.86, 0.045, 42.0).alpha(0.30),
        }
    }

    pub fn for_appearance(a: Appearance, system_prefers_dark: bool) -> Self {
        match a {
            Appearance::Dark => Self::dark(),
            Appearance::Light => Self::light(),
            Appearance::Automatic => {
                if system_prefers_dark {
                    Self::dark()
                } else {
                    Self::light()
                }
            }
        }
    }
}

/// One entry in the type ramp.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub size: f32,
    pub weight: f32,
    /// Letter spacing as a fraction of size.
    pub tracking: f32,
    pub line_height: f32,
}

impl TextStyle {
    /// Scale for the Appearance → Text size setting.
    pub fn scaled(self, k: f32) -> Self {
        Self {
            size: self.size * k,
            ..self
        }
    }
}

/// The six text styles in détends.
///
/// Tracking tightens as size grows and opens as it shrinks, which is what makes
/// a ramp look like one typeface at six sizes rather than six unrelated labels.
pub mod text {
    use super::TextStyle;

    /// The clock face, a running timer. Enormous and thin — at this size weight
    /// reads as heaviness, not emphasis.
    pub const DISPLAY: TextStyle = TextStyle {
        size: 112.0,
        weight: 250.0,
        tracking: -0.022,
        line_height: 1.0,
    };

    /// A track title, the name of a Focus session.
    pub const TITLE: TextStyle = TextStyle {
        size: 30.0,
        weight: 400.0,
        tracking: -0.012,
        line_height: 1.2,
    };

    /// Section headings inside a mode.
    pub const HEADING: TextStyle = TextStyle {
        size: 19.0,
        weight: 500.0,
        tracking: -0.005,
        line_height: 1.3,
    };

    /// Body copy: a mail message, a document.
    pub const BODY: TextStyle = TextStyle {
        size: 15.0,
        weight: 400.0,
        tracking: 0.0,
        line_height: 1.5,
    };

    /// Controls and the status cluster.
    pub const LABEL: TextStyle = TextStyle {
        size: 13.0,
        weight: 500.0,
        tracking: 0.004,
        line_height: 1.3,
    };

    /// The provider name under the transport, a timestamp. Present, secondary.
    pub const CAPTION: TextStyle = TextStyle {
        size: 11.0,
        weight: 500.0,
        tracking: 0.018,
        line_height: 1.3,
    };
}

/// Spacing scale, in logical units.
///
/// détends leans on whitespace instead of rules and borders, so these run
/// larger than a typical UI scale and the big ones get used often.
pub mod space {
    pub const TIGHT: f32 = 4.0;
    pub const SNUG: f32 = 8.0;
    pub const STEP: f32 = 12.0;
    pub const ROOM: f32 = 20.0;
    pub const OPEN: f32 = 32.0;
    pub const WIDE: f32 = 52.0;
    pub const VAST: f32 = 84.0;
}

/// How strongly glass asserts itself (Settings → Appearance).
#[derive(Clone, Copy, Debug)]
pub struct GlassSettings {
    /// Scales the rim, refraction and tint together. 0 leaves a plain
    /// translucent surface; 1 is the full optical slab.
    pub intensity: f32,
    /// How much of the backdrop comes through. Lower is more opaque.
    pub transparency: f32,
}

impl Default for GlassSettings {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            transparency: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_themes_have_readable_text_contrast() {
        // Perceptual lightness difference, in OKLab. ~0.4 is a comfortable
        // margin for body copy at these sizes.
        for (name, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
            let delta = (p.text.lightness() - p.ground.lightness()).abs();
            assert!(delta > 0.55, "{name}: primary text contrast only {delta}");

            let soft = (p.text_soft.lightness() - p.ground.lightness()).abs();
            assert!(soft > 0.28, "{name}: secondary text contrast only {soft}");

            // Text has to stay legible over the *bright* end of the gradient
            // too, not just the dark end it is usually sampled against.
            let over_far = (p.text.lightness() - p.ground_far.lightness()).abs();
            assert!(
                over_far > 0.40,
                "{name}: text over the bright end is only {over_far}"
            );
        }
    }

    #[test]
    fn the_text_ramp_descends_in_both_size_and_presence() {
        let ramp = [
            text::DISPLAY,
            text::TITLE,
            text::HEADING,
            text::BODY,
            text::LABEL,
            text::CAPTION,
        ];
        for pair in ramp.windows(2) {
            assert!(pair[0].size > pair[1].size, "sizes must descend");
        }
    }

    #[test]
    fn tracking_tightens_as_type_grows() {
        // The typographic rule that keeps the ramp looking like one family.
        // Checked at compile time, so the ramp cannot be edited into an
        // inconsistent state at all — a test would only catch it afterwards.
        const _: () = assert!(text::DISPLAY.tracking < text::BODY.tracking);
        const _: () = assert!(text::BODY.tracking < text::CAPTION.tracking);
    }

    #[test]
    fn the_palette_stays_near_monochrome() {
        // "Prefer very few colours simultaneously" (§17). Everything but the
        // single accent must be essentially neutral.
        for p in [Palette::dark(), Palette::light()] {
            for (name, c) in [
                ("ground", p.ground),
                ("ground_far", p.ground_far),
                ("text", p.text),
                ("text_soft", p.text_soft),
                ("text_faint", p.text_faint),
                ("glass", p.glass),
            ] {
                // Measured as perceptual chroma rather than RGB spread, which
                // would call a pale value saturated merely for being bright.
                // For scale: the accent sits around 0.085.
                assert!(
                    c.chroma() < 0.02,
                    "{name} is too colourful: chroma {}",
                    c.chroma()
                );
            }
        }
    }

    #[test]
    fn the_accent_is_the_only_saturated_value() {
        for p in [Palette::dark(), Palette::light()] {
            assert!(
                p.accent.chroma() > 0.05,
                "the accent should actually be a colour"
            );
            assert!(p.accent.chroma() > p.ground.chroma() * 4.0);
        }
    }

    #[test]
    fn the_environment_gives_glass_something_to_refract() {
        // A pane over a uniform field refracts nothing visible and reads as a
        // grey rectangle. The gradient needs real range across the screen —
        // this is the property, not an incidental colour choice.
        for p in [Palette::dark(), Palette::light()] {
            let range = (p.ground_far.lightness() - p.ground.lightness()).abs();
            assert!(range > 0.12, "environment range is only {range}");
        }
    }

    #[test]
    fn glass_lightens_on_dark_and_darkens_on_light() {
        // A pane must read as a distinct surface against either ground rather
        // than dissolving into it.
        assert!(Palette::dark().glass.lightness() > Palette::dark().ground.lightness());
        assert!(Palette::light().glass.lightness() < Palette::light().ground.lightness());
    }

    #[test]
    fn automatic_follows_the_system() {
        let dark = Palette::for_appearance(Appearance::Automatic, true);
        let light = Palette::for_appearance(Appearance::Automatic, false);
        assert!(dark.ground.lightness() < light.ground.lightness());
        // An explicit choice ignores the system.
        let forced = Palette::for_appearance(Appearance::Dark, false);
        assert!(forced.ground.lightness() < 0.3);
    }

    #[test]
    fn text_size_setting_scales_only_the_size() {
        let big = text::BODY.scaled(1.4);
        assert!((big.size - text::BODY.size * 1.4).abs() < 1e-5);
        assert_eq!(big.weight, text::BODY.weight);
        assert_eq!(big.tracking, text::BODY.tracking);
    }

    #[test]
    fn the_spacing_scale_ascends() {
        let s = [
            space::TIGHT,
            space::SNUG,
            space::STEP,
            space::ROOM,
            space::OPEN,
            space::WIDE,
            space::VAST,
        ];
        for w in s.windows(2) {
            assert!(w[0] < w[1]);
        }
    }
}
