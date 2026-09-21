//! The wallpaper: the détends mark, as a field rather than as a logo.
//!
//! A background has one job, and it is not to be looked at. It has to give the
//! glass something to bend — a pane over a uniform field refracts
//! mathematically and reads as a grey rectangle — and it has to make the
//! machine feel like somewhere rather than nowhere. Anything past that is
//! competing with the content, which rule 1 settles.
//!
//! So the mark is drawn enormous, low, and at a contrast where you cannot
//! quite say whether it is there. It is not a logo placed on a desktop; it is
//! the shape the light happens to fall on. You should notice it the way you
//! notice a watermark in paper: by tilting the page, not by looking.
//!
//! It lives in [`Layer::Environment`], underneath everything, so the blur
//! pyramid picks it up and every pane of glass in the system carries a trace of
//! it. That is the whole reason it is worth drawing at all.

use crate::shell::Brand;
use detends_paint::{Frame, Id, Image, Item, Layer, Palette, Primitive, Rect, Seconds, Vec2};

/// How much of the shorter screen dimension the mark spans.
///
/// Large enough that its curves read as geometry rather than as a symbol —
/// past a certain size a mark stops being a logo and becomes a shape.
const SPAN: f32 = 1.15;

/// Where its centre sits, as a fraction of the workspace.
///
/// Low and left, in the quadrant nothing else uses.
///
/// Centred was tried and is wrong: every mode puts its content in a centre
/// column, so a centred mark sits directly behind the clock and the five marks
/// and turns the busiest part of the screen into the noisiest. Here it fills
/// the empty corner under the cool light, balances the warm pool opposite, and
/// never touches anything that has to be read.
const AT: Vec2 = Vec2 { x: 0.13, y: 0.62 };

/// Peak opacity, before the environment dims it.
///
/// A wallpaper you cannot see is not a wallpaper. The first attempts sat near
/// the threshold of visibility on the theory that a background should not be
/// looked at, and the result was a black screen with a rumour on it. This is
/// the level where the mark reads as a large piece of glass catching the
/// room's light — present, clearly the détends mark, and still nothing you
/// have to read past.
const PRESENCE: f32 = 0.125;

/// Draw the mark into the environment.
///
/// Does nothing until the host has loaded the artwork, so a missing or broken
/// asset costs the wallpaper rather than the boot.
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    brand: Option<Brand>,
    size: Vec2,
    now: Seconds,
    hush: f32,
    presence: f32,
) {
    let Some(brand) = brand else { return };

    // The same near-imperceptible drift the environment lights have, at a
    // slightly different period so the two never beat against each other.
    let drift = Vec2 {
        x: (now as f32 * 0.019).sin() * size.x * 0.006,
        y: (now as f32 * 0.014).cos() * size.y * 0.005,
    };

    let span = size.x.min(size.y) * SPAN;
    let height = span / brand.short_aspect.max(0.01);

    let rect = Rect::from_center_size(
        Vec2 {
            x: size.x * AT.x + drift.x,
            y: size.y * AT.y + drift.y,
        },
        Vec2 { x: span, y: height },
    );

    // Focus takes the wallpaper almost entirely away. The room should empty
    // out rather than merely dim (§12) — and a watermark is exactly the kind
    // of thing you stop wanting to see when you are trying to work.
    let opacity = PRESENCE * (1.0 - hush * 0.8) * presence;
    if opacity <= 0.001 {
        return;
    }

    frame.push(
        Item::new(
            Id::of("wallpaper-mark"),
            Layer::Environment,
            Primitive::Image(Image {
                rect,
                texture: brand.short,
                source: Rect::from_min_size(Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 1.0, y: 1.0 }),
                radius: 0.0,
                squircle: 4.0,
                // Tinted rather than white: the mark takes the colour of the
                // text it shares a screen with, so it belongs to the palette
                // instead of sitting on top of it.
                tint: palette.text.fade(opacity),
            }),
        )
        // Below anything else that might want the environment layer later —
        // album art tinting the room, for instance.
        .z(-10),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::{vec2, TextureId};

    fn brand() -> Brand {
        Brand {
            short: TextureId(1),
            short_aspect: 1.0,
            full: TextureId(2),
            full_aspect: 3.0,
        }
    }

    fn render(brand: Option<Brand>, hush: f32, presence: f32) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        draw(
            &mut frame,
            &Palette::dark(),
            brand,
            vec2(1512.0, 982.0),
            0.0,
            hush,
            presence,
        );
        frame
    }

    #[test]
    fn it_draws_the_mark_into_the_environment_layer() {
        let frame = render(Some(brand()), 0.0, 1.0);
        assert_eq!(frame.items.len(), 1);
        assert_eq!(frame.items[0].layer, Layer::Environment);
        assert!(matches!(frame.items[0].primitive, Primitive::Image(_)));
    }

    #[test]
    fn without_artwork_it_draws_nothing_rather_than_failing() {
        assert!(render(None, 0.0, 1.0).items.is_empty());
    }

    #[test]
    fn it_is_visible_without_competing_with_content() {
        // Both halves matter. Too faint and there is no wallpaper at all; too
        // strong and it is something the eye has to read past on every screen.
        let frame = render(Some(brand()), 0.0, 1.0);
        let Primitive::Image(image) = &frame.items[0].primitive else {
            panic!("expected an image");
        };
        assert!(image.tint.a >= 0.06, "alpha {} is invisible", image.tint.a);
        assert!(image.tint.a <= 0.18, "alpha {} competes with content", image.tint.a);
    }

    #[test]
    fn focus_takes_it_almost_entirely_away() {
        let calm = render(Some(brand()), 0.0, 1.0);
        let focused = render(Some(brand()), 1.0, 1.0);

        let alpha = |f: &Frame| match &f.items[0].primitive {
            Primitive::Image(i) => i.tint.a,
            _ => panic!("expected an image"),
        };
        assert!(alpha(&focused) < alpha(&calm) * 0.3, "Focus should empty the room");
    }

    #[test]
    fn it_arrives_with_the_workspace_during_boot() {
        // Presence is the boot curve: the wallpaper must rise behind the mark
        // rather than being there from the first frame.
        assert!(render(Some(brand()), 0.0, 0.0).items.is_empty());
    }

    #[test]
    fn it_keeps_clear_of_the_centre_column_every_mode_draws_into() {
        // The property that keeps it a wallpaper rather than clutter: content
        // is centred, so the mark must not be.
        let frame = render(Some(brand()), 0.0, 1.0);
        let rect = frame.items[0].primitive.bounds();
        assert!(
            rect.center.x < 1512.0 * 0.3,
            "the mark drifted into the content column"
        );
    }
}
