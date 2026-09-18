//! Colour, kept linear.
//!
//! Every colour in détends is stored as **linear** premultiplication-ready RGBA
//! and only encoded to sRGB in the final present pass. Blending, blurring and
//! refraction are all physical operations; doing them on gamma-encoded values
//! is the reason so much translucent UI looks muddy where surfaces overlap.
//!
//! The palette is authored in **OKLCH** rather than picked by hand. Equal
//! lightness steps in OKLCH look equal, so a palette built from a ramp actually
//! reads as a ramp — which is what lets typography carry the hierarchy (§17)
//! without resorting to borders and boxes.

// The OKLab conversion matrices are transcribed at full precision from Björn
// Ottosson's reference definition. They carry more digits than an f32 can hold,
// which is deliberate: they document the source values exactly and stay correct
// if this is ever widened to f64.
#![allow(clippy::excessive_precision)]

use detends_motion::Animatable;

/// Linear-space RGBA.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

impl Color {
    pub const TRANSPARENT: Self = rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = rgba(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = rgba(1.0, 1.0, 1.0, 1.0);

    /// Same colour, different opacity.
    pub const fn alpha(self, a: f32) -> Self {
        rgba(self.r, self.g, self.b, a)
    }

    /// Scale the existing opacity — for fading something already translucent.
    pub fn fade(self, k: f32) -> Self {
        rgba(self.r, self.g, self.b, self.a * k)
    }

    pub fn lerp(self, o: Self, t: f32) -> Self {
        rgba(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }

    /// Build from an sRGB hex literal, e.g. `Color::hex(0xF2F4F7)`, opaque.
    ///
    /// Convenience for transcribing a value from a design tool; the result is
    /// converted to linear immediately.
    pub fn hex(rgb: u32) -> Self {
        let f = |shift: u32| srgb_to_linear(((rgb >> shift) & 0xFF) as f32 / 255.0);
        rgba(f(16), f(8), f(0), 1.0)
    }

    /// Build from OKLCH: perceptual lightness `0..1`, chroma (~`0..0.4`), and
    /// hue in degrees.
    pub fn oklch(l: f32, c: f32, h_deg: f32) -> Self {
        let h = h_deg.to_radians();
        oklab_to_linear(l, c * h.cos(), c * h.sin())
    }

    /// Perceptual chroma — how colourful, independent of how light.
    ///
    /// The right measure for "is this near-monochrome?". Comparing raw RGB
    /// spread instead would call a pale tint saturated purely because it is
    /// bright, since the same chroma opens a wider RGB gap at high lightness.
    pub fn chroma(self) -> f32 {
        let (a, b) = self.oklab_ab();
        a.hypot(b)
    }

    fn oklab_ab(self) -> (f32, f32) {
        let l = (0.4122214708 * self.r + 0.5363325363 * self.g + 0.0514459929 * self.b).cbrt();
        let m = (0.2119034982 * self.r + 0.6806995451 * self.g + 0.1073969566 * self.b).cbrt();
        let s = (0.0883024619 * self.r + 0.2817188376 * self.g + 0.6299787005 * self.b).cbrt();
        (
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        )
    }

    /// Perceptual lightness, for checking contrast without leaving OKLab.
    pub fn lightness(self) -> f32 {
        let l = (0.4122214708 * self.r + 0.5363325363 * self.g + 0.0514459929 * self.b).cbrt();
        let m = (0.2119034982 * self.r + 0.6806995451 * self.g + 0.1073969566 * self.b).cbrt();
        let s = (0.0883024619 * self.r + 0.2817188376 * self.g + 0.6299787005 * self.b).cbrt();
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s
    }
}

/// sRGB transfer function, encoded → linear.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear → sRGB encoded. Used only in the final present pass.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn oklab_to_linear(l_: f32, a: f32, b: f32) -> Color {
    let l = (l_ + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (l_ - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (l_ - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    rgba(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        1.0,
    )
}

impl Animatable for Color {
    fn zero() -> Self {
        Self::TRANSPARENT
    }
    fn add(self, o: Self) -> Self {
        rgba(self.r + o.r, self.g + o.g, self.b + o.b, self.a + o.a)
    }
    fn sub(self, o: Self) -> Self {
        rgba(self.r - o.r, self.g - o.g, self.b - o.b, self.a - o.a)
    }
    fn scale(self, k: f32) -> Self {
        rgba(self.r * k, self.g * k, self.b * k, self.a * k)
    }
    fn magnitude(self) -> f32 {
        self.r
            .abs()
            .max(self.g.abs())
            .max(self.b.abs())
            .max(self.a.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transfer_functions_invert_each_other() {
        for i in 0..=100 {
            let c = i as f32 / 100.0;
            assert!(
                (linear_to_srgb(srgb_to_linear(c)) - c).abs() < 1e-5,
                "at {c}"
            );
        }
    }

    #[test]
    fn hex_decodes_to_linear_not_encoded_values() {
        // Mid grey in sRGB is ~0.216 in linear. Getting this wrong is the
        // classic double-gamma bug, and it shows up as washed-out glass.
        let grey = Color::hex(0x808080);
        assert!((grey.r - 0.2158).abs() < 1e-3, "got {}", grey.r);
        assert_eq!(Color::hex(0xFFFFFF).r, 1.0);
        assert_eq!(Color::hex(0x000000).r, 0.0);
    }

    #[test]
    fn oklch_chroma_round_trips() {
        for c in [0.0_f32, 0.02, 0.08, 0.15] {
            let colour = Color::oklch(0.6, c, 250.0);
            assert!(
                (colour.chroma() - c).abs() < 1e-3,
                "{c} -> {}",
                colour.chroma()
            );
        }
    }

    #[test]
    fn chroma_is_independent_of_lightness() {
        // The property that makes it the right measure for the palette test:
        // the same chroma reads the same whether the colour is dark or pale.
        let dark = Color::oklch(0.15, 0.012, 250.0).chroma();
        let pale = Color::oklch(0.90, 0.012, 250.0).chroma();
        assert!((dark - pale).abs() < 2e-3, "{dark} vs {pale}");
    }

    #[test]
    fn oklch_lightness_round_trips() {
        for l in [0.2_f32, 0.5, 0.8] {
            let c = Color::oklch(l, 0.02, 250.0);
            assert!((c.lightness() - l).abs() < 1e-3, "{l} -> {}", c.lightness());
        }
    }

    #[test]
    fn equal_oklch_steps_are_perceptually_equal() {
        // The property the palette relies on: three evenly spaced lightness
        // values must read as evenly spaced, which sRGB values do not.
        let steps: Vec<f32> = [0.3_f32, 0.5, 0.7]
            .iter()
            .map(|&l| Color::oklch(l, 0.0, 0.0).lightness())
            .collect();
        let d1 = steps[1] - steps[0];
        let d2 = steps[2] - steps[1];
        assert!((d1 - d2).abs() < 1e-3, "uneven ramp: {d1} vs {d2}");
    }

    #[test]
    fn fade_and_alpha_differ_as_documented() {
        let half = Color::WHITE.alpha(0.5);
        assert_eq!(half.fade(0.5).a, 0.25);
        assert_eq!(half.alpha(0.5).a, 0.5);
    }

    #[test]
    fn colors_animate_componentwise() {
        use detends_motion::{springs, Spring};
        let mut s = Spring::new(springs::SNAP, Color::TRANSPARENT);
        let target = Color::hex(0x101418).alpha(0.8);
        s.target(0.0, target);
        let settled = s.params().settle_estimate_for(1.0) as f64;
        assert_eq!(s.value(settled), target);
    }
}
