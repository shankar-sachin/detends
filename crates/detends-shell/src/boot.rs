//! Boot.
//!
//! "Black or environment-appropriate background. Centered détends mark. No
//! Linux boot messages. No unnecessary progress indicators. Transition directly
//! from boot into the détends Shell." (§20)
//!
//! So: no progress bar, no version string, no spinner, no status text. The
//! machine is either starting or it has a problem worth a person's attention,
//! and a progress indicator communicates neither.
//!
//! Four beats:
//!
//! ```text
//!   1. the short mark flies in          rising, settling on SETTLE
//!   2. it holds                         a beat, so it registers as arrived
//!   3. it expands into the full mark    one object growing, not a crossfade
//!   4. the workspace rises behind it    overlapping, so boot is one movement
//! ```
//!
//! The expansion is the moment worth getting right. Both marks share a centre,
//! the short one growing outward as the full one arrives from slightly small —
//! so the eye reads a single thing unfolding rather than two images swapping.

use detends_paint::{
    springs, text, Align, Color, Frame, Id, Item, Layer, Palette, Primitive, Rect, Seconds, Spring,
    Text, TextStyle, TextureId, Vec2,
};

const SHORT: Id = Id::of("boot-mark-short");
const FULL: Id = Id::of("boot-mark-full");

/// How long the short mark holds alone before unfolding.
const HOLD: Seconds = 1.0;

/// What a mark is drawn from.
///
/// Text until the artwork lands; the choreography is identical either way,
/// which is the point of building it before the logo exists.
#[derive(Clone, Debug)]
pub enum Mark {
    /// A typographic placeholder.
    Wordmark {
        text: &'static str,
        style: TextStyle,
    },
    /// The real thing: an image the renderer owns.
    Artwork { texture: TextureId, size: Vec2 },
}

impl Mark {
    /// Natural size at scale 1.
    fn size(&self, frame: &Frame) -> Vec2 {
        match self {
            Mark::Wordmark { text, style } => Vec2 {
                // Generous: the text is centred inside this box, so it only
                // has to be wide enough not to wrap.
                x: (style.size * text.chars().count() as f32).min(frame.size.x * 0.8),
                y: style.size * 1.6,
            },
            Mark::Artwork { size, .. } => *size,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The short mark is arriving, or holding.
    Presenting,
    /// Unfolding into the full mark.
    Expanding,
    /// The workspace is rising behind it.
    HandingOver,
    Done,
}

pub struct Boot {
    started: Seconds,
    phase: Phase,
    expanded_at: Seconds,

    short: Mark,
    full: Mark,

    /// Opacity of the short mark.
    short_presence: Spring<f32>,
    /// Scale of the short mark. Grows through the expansion.
    short_scale: Spring<f32>,
    /// How far the short mark still has to rise into place.
    short_rise: Spring<f32>,

    /// Opacity of the full mark.
    full_presence: Spring<f32>,
    full_scale: Spring<f32>,

    /// How much of the workspace has arrived.
    workspace: Spring<f32>,
}

impl Boot {
    pub fn new(now: Seconds) -> Self {
        Self::with_marks(
            now,
            // The short mark: the first letter, standing in for a monogram.
            Mark::Wordmark {
                text: "d",
                style: text::DISPLAY.scaled(0.62),
            },
            Mark::Wordmark {
                text: "détends",
                style: text::DISPLAY.scaled(0.42),
            },
        )
    }

    pub fn with_marks(now: Seconds, short: Mark, full: Mark) -> Self {
        let mut short_presence = Spring::new(springs::SETTLE, 0.0);
        let mut short_scale = Spring::new(springs::SETTLE, 0.86);
        let mut short_rise = Spring::new(springs::SETTLE, 1.0);

        // Flies in immediately. Nothing is worth waiting for here — whatever
        // needs loading can load behind it.
        short_presence.target(now, 1.0);
        short_scale.target(now, 1.0);
        short_rise.target(now, 0.0);

        Self {
            started: now,
            phase: Phase::Presenting,
            expanded_at: now,
            short,
            full,
            short_presence,
            short_scale,
            short_rise,
            full_presence: Spring::new(springs::SETTLE, 0.0),
            full_scale: Spring::new(springs::SETTLE, 0.93),
            workspace: Spring::new(springs::GLIDE, 0.0),
        }
    }

    /// Swap in real artwork once it has been loaded.
    pub fn set_marks(&mut self, short: Mark, full: Mark) {
        self.short = short;
        self.full = full;
    }

    pub fn update(&mut self, now: Seconds) -> Phase {
        match self.phase {
            Phase::Presenting => {
                // Unfold once the short mark has arrived *and* had its beat.
                if now - self.started >= HOLD && self.short_presence.at_rest(now) {
                    // The short mark opens outward as it dissolves, so the eye
                    // follows one expanding object into the full mark rather
                    // than watching one image replaced by another.
                    //
                    // Modestly, though. Growing it far enough to span the
                    // wordmark leaves a large dark ghost sitting behind the
                    // letters for most of the transition — the gesture reads
                    // better when the mark is already leaving by the time the
                    // word becomes legible.
                    self.short_scale.target(now, 1.42);
                    // Retuned to leave faster than the wordmark arrives, so the
                    // two never both sit at full strength.
                    self.short_presence.retune(now, springs::SNAP);
                    self.short_presence.target(now, 0.0);

                    self.full_scale.target(now, 1.0);
                    self.full_presence.target(now, 1.0);

                    self.expanded_at = now;
                    self.phase = Phase::Expanding;
                }
            }
            Phase::Expanding => {
                if self.full_presence.at_rest(now) && now - self.expanded_at >= 0.35 {
                    self.full_presence.target(now, 0.0);
                    self.full_scale.target(now, 1.05);
                    self.workspace.target(now, 1.0);
                    self.phase = Phase::HandingOver;
                }
            }
            Phase::HandingOver => {
                if self.full_presence.at_rest(now) && self.workspace.at_rest(now) {
                    self.phase = Phase::Done;
                }
            }
            Phase::Done => {}
        }
        self.phase
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// How much of the workspace should be visible, 0 to 1.
    pub fn workspace_presence(&self, now: Seconds) -> f32 {
        self.workspace.value(now).clamp(0.0, 1.0)
    }

    pub fn animating(&self, now: Seconds) -> bool {
        self.phase != Phase::Done
            || !self.short_presence.at_rest(now)
            || !self.full_presence.at_rest(now)
            || !self.workspace.at_rest(now)
    }

    pub fn draw(&self, frame: &mut Frame, palette: &Palette, now: Seconds) {
        if self.phase == Phase::Done {
            return;
        }

        let center = Vec2 {
            x: frame.size.x * 0.5,
            y: frame.size.y * 0.5,
        };

        // The short mark, rising into place then expanding away.
        let short_presence = self.short_presence.value(now).clamp(0.0, 1.0);
        if short_presence > 0.001 {
            // Flies in from below — a short travel, so it reads as arriving
            // rather than as sliding.
            let rise = self.short_rise.value(now) * 26.0;
            self.draw_mark(
                frame,
                SHORT,
                &self.short,
                Vec2 {
                    x: center.x,
                    y: center.y + rise,
                },
                self.short_scale.value(now),
                short_presence,
                palette,
                101,
            );
        }

        // The full mark, unfolding from the same centre.
        let full_presence = self.full_presence.value(now).clamp(0.0, 1.0);
        if full_presence > 0.001 {
            self.draw_mark(
                frame,
                FULL,
                &self.full,
                center,
                self.full_scale.value(now),
                full_presence,
                palette,
                100,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_mark(
        &self,
        frame: &mut Frame,
        id: Id,
        mark: &Mark,
        center: Vec2,
        scale: f32,
        presence: f32,
        palette: &Palette,
        z: i16,
    ) {
        let size = mark.size(frame);
        let rect = Rect::from_center_size(center, size).scaled(scale);

        let primitive = match mark {
            Mark::Wordmark { text: label, style } => Primitive::Text(Text {
                text: (*label).into(),
                rect,
                size: style.size * scale,
                weight: style.weight,
                tracking: style.tracking,
                line_height: style.line_height,
                // Lit rather than printed.
                color: Color::WHITE.alpha(0.94).lerp(palette.text, 0.3),
                align: Align::Center,
            }),
            Mark::Artwork { texture, .. } => Primitive::Image(detends_paint::Image {
                rect,
                texture: *texture,
                source: Rect::ZERO,
                radius: 0.0,
                squircle: 4.0,
                tint: Color::WHITE,
            }),
        };

        frame.push(
            Item::new(id, Layer::Overlay, primitive)
                .opacity(presence)
                .z(z),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn run_to_done(boot: &mut Boot) -> Seconds {
        let mut t = 0.0;
        for _ in 0..4000 {
            t += 1.0 / 120.0;
            if boot.update(t) == Phase::Done {
                return t;
            }
        }
        panic!("boot never finished");
    }

    fn reach(boot: &mut Boot, phase: Phase) -> Seconds {
        let mut t = 0.0;
        for _ in 0..4000 {
            t += 1.0 / 120.0;
            if boot.update(t) == phase {
                return t;
            }
        }
        panic!("never reached {phase:?}");
    }

    #[test]
    fn it_runs_through_every_beat_in_order() {
        let mut boot = Boot::new(0.0);
        assert_eq!(boot.phase(), Phase::Presenting);

        let mut seen = vec![boot.phase()];
        let mut t = 0.0;
        for _ in 0..4000 {
            t += 1.0 / 120.0;
            let phase = boot.update(t);
            if *seen.last().unwrap() != phase {
                seen.push(phase);
            }
            if phase == Phase::Done {
                break;
            }
        }
        assert_eq!(
            seen,
            vec![
                Phase::Presenting,
                Phase::Expanding,
                Phase::HandingOver,
                Phase::Done
            ]
        );
    }

    #[test]
    fn the_short_mark_holds_for_about_a_second_before_unfolding() {
        let mut boot = Boot::new(0.0);
        let expanded = reach(&mut boot, Phase::Expanding);
        assert!(expanded >= HOLD, "unfolded after only {expanded}s");
        assert!(expanded < HOLD + 0.6, "held too long: {expanded}s");
    }

    #[test]
    fn the_short_mark_flies_in_rather_than_appearing() {
        let mut boot = Boot::new(0.0);
        boot.update(0.01);

        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        boot.draw(&mut frame, &Palette::dark(), 0.01);
        let early = frame.items[0].primitive.bounds().center.y;

        // ...and has settled onto the centre line by the time it holds.
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        boot.update(HOLD);
        boot.draw(&mut frame, &Palette::dark(), HOLD);
        let settled = frame.items[0].primitive.bounds().center.y;

        assert!(
            early > settled + 5.0,
            "did not travel: {early} -> {settled}"
        );
        assert!(
            (settled - 491.0).abs() < 1.0,
            "did not land centred: {settled}"
        );
    }

    #[test]
    fn the_expansion_is_one_object_unfolding_not_a_swap() {
        // Both marks must be visible together, sharing a centre, with the short
        // one larger than it was. That overlap is what sells it as a single
        // thing growing rather than two images crossfading.
        let mut boot = Boot::new(0.0);
        let expanded = reach(&mut boot, Phase::Expanding);

        let t = expanded + 0.12;
        boot.update(t);
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        boot.draw(&mut frame, &Palette::dark(), t);

        assert_eq!(frame.items.len(), 2, "both marks should be present");

        let short = frame
            .items
            .iter()
            .find(|i| i.id == SHORT)
            .expect("short mark");
        let full = frame
            .items
            .iter()
            .find(|i| i.id == FULL)
            .expect("full mark");

        assert!(short.opacity > 0.02, "short mark vanished too early");
        assert!(full.opacity > 0.02, "full mark had not arrived");

        let sc = short.primitive.bounds().center;
        let fc = full.primitive.bounds().center;
        assert!(
            (sc.x - fc.x).abs() < 1.0 && (sc.y - fc.y).abs() < 1.0,
            "centres drifted apart"
        );

        assert!(
            boot.short_scale.value(t) > 1.0,
            "the short mark should be growing"
        );
    }

    #[test]
    fn the_workspace_arrives_before_the_mark_has_gone() {
        let mut boot = Boot::new(0.0);
        let handover = reach(&mut boot, Phase::HandingOver);

        let t = handover + 0.1;
        boot.update(t);
        assert!(
            boot.full_presence.value(t) > 0.05,
            "the mark had already gone"
        );
        assert!(
            boot.workspace_presence(t) > 0.05,
            "the workspace had not started"
        );
    }

    #[test]
    fn the_whole_sequence_is_brief() {
        let mut boot = Boot::new(0.0);
        let finished = run_to_done(&mut boot);
        // Long enough to be a moment, short enough never to be a wait.
        assert!(finished > 1.2 && finished < 4.0, "boot took {finished}s");
    }

    #[test]
    fn nothing_is_drawn_once_boot_is_over() {
        let mut boot = Boot::new(0.0);
        let finished = run_to_done(&mut boot);
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        boot.draw(&mut frame, &Palette::dark(), finished);
        assert!(frame.items.is_empty(), "boot left something behind");
    }

    #[test]
    fn it_reports_animating_until_it_is_finished() {
        let mut boot = Boot::new(0.0);
        let mut t = 0.0;
        for _ in 0..4000 {
            let done = boot.update(t) == Phase::Done;
            assert_eq!(boot.animating(t), !done, "disagreed at {t}s");
            if done {
                return;
            }
            t += 1.0 / 120.0;
        }
        panic!("boot never finished");
    }

    #[test]
    fn artwork_drops_in_without_changing_the_choreography() {
        // The whole reason the marks are pluggable: the real logo must not
        // require the timing to be rebuilt.
        let mut boot = Boot::with_marks(
            0.0,
            Mark::Artwork {
                texture: TextureId(1),
                size: vec2(140.0, 140.0),
            },
            Mark::Artwork {
                texture: TextureId(2),
                size: vec2(520.0, 150.0),
            },
        );
        let expanded = reach(&mut boot, Phase::Expanding);
        assert!(expanded >= HOLD);

        let t = expanded + 0.12;
        boot.update(t);
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        boot.draw(&mut frame, &Palette::dark(), t);

        assert_eq!(frame.items.len(), 2);
        assert!(frame
            .items
            .iter()
            .all(|i| matches!(i.primitive, Primitive::Image(_))));

        let finished = run_to_done(&mut boot);
        assert!(finished > 1.2 && finished < 4.0);
    }

    #[test]
    fn marks_stay_centred_on_every_screen() {
        let boot = Boot::new(0.0);
        for size in [
            vec2(1512.0, 982.0),
            vec2(3840.0, 2160.0),
            vec2(800.0, 600.0),
        ] {
            let mut frame = Frame::new(size, 2.0);
            boot.draw(&mut frame, &Palette::dark(), 0.3);
            let bounds = frame.items[0].primitive.bounds();
            assert!(
                (bounds.center.x - size.x * 0.5).abs() < 0.5,
                "off-centre at {size:?}"
            );
        }
    }
}
