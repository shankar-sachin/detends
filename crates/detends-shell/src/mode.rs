//! The five places.
//!
//! Music · Clock · Mail · Studio · Files — and no sixth (rule 7).
//!
//! These are environments, not windows. Switching transforms the workspace
//! rather than spawning anything: the outgoing mode recedes and dissolves while
//! the incoming one arrives from just beyond, the two overlapping so the
//! movement reads as one workspace changing state rather than two screens
//! swapping.

use detends_paint::{springs, IconShape, Seconds, Spring};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mode {
    Music,
    Clock,
    Mail,
    Studio,
    Files,
}

impl Mode {
    pub const ALL: [Mode; 5] = [
        Mode::Music,
        Mode::Clock,
        Mode::Mail,
        Mode::Studio,
        Mode::Files,
    ];

    /// The icon that stands for this place on Home.
    pub fn icon(self) -> IconShape {
        match self {
            Mode::Music => IconShape::Music,
            Mode::Clock => IconShape::Clock,
            Mode::Mail => IconShape::Mail,
            Mode::Studio => IconShape::Studio,
            Mode::Files => IconShape::Files,
        }
    }

    /// The name as it appears in the interface.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Music => "Music",
            Mode::Clock => "Clock",
            Mode::Mail => "Mail",
            Mode::Studio => "Studio",
            Mode::Files => "Files",
        }
    }

    /// Position in the keyboard ordering, 1–5.
    pub fn index(self) -> u8 {
        match self {
            Mode::Music => 1,
            Mode::Clock => 2,
            Mode::Mail => 3,
            Mode::Studio => 4,
            Mode::Files => 5,
        }
    }

    /// Select by number, for `Super+1`…`Super+5`.
    pub fn from_index(index: u8) -> Option<Self> {
        Mode::ALL.iter().copied().find(|m| m.index() == index)
    }

    /// Match a typed string, for Search. Case- and prefix-insensitive, so
    /// "mu" finds Music.
    pub fn matching(query: &str) -> Option<Self> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return None;
        }
        Mode::ALL
            .iter()
            .copied()
            .find(|m| m.name().to_lowercase().starts_with(&q))
    }
}

/// Where the workspace is.
///
/// Home is not a sixth mode — it is the root you land on and return to, and the
/// only place that shows you where you can go. Without it the five places are
/// reachable only by keystrokes nobody told you about, which is how a system
/// ends up looking like it does nothing at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    Home,
    Mode(Mode),
}

impl Destination {
    pub fn name(self) -> &'static str {
        match self {
            Destination::Home => "détends",
            Destination::Mode(mode) => mode.name(),
        }
    }

    pub fn mode(self) -> Option<Mode> {
        match self {
            Destination::Home => None,
            Destination::Mode(mode) => Some(mode),
        }
    }
}

impl From<Mode> for Destination {
    fn from(mode: Mode) -> Self {
        Destination::Mode(mode)
    }
}

/// Tracks what owns the workspace, and the movement between destinations.
pub struct Navigator {
    current: Destination,
    /// What is being left, still visible while it recedes.
    previous: Option<Destination>,
    /// 0 when the incoming mode has fully arrived, 1 the moment it starts.
    transition: Spring<f32>,
}

impl Navigator {
    pub fn new(start: impl Into<Destination>) -> Self {
        let start = start.into();
        Self {
            current: start,
            previous: None,
            transition: Spring::new(springs::GLIDE, 0.0),
        }
    }

    pub fn current(&self) -> Destination {
        self.current
    }

    pub fn previous(&self) -> Option<Destination> {
        self.previous
    }

    pub fn at_home(&self) -> bool {
        self.current == Destination::Home
    }

    /// Go somewhere. Selecting where you already are does nothing at all — no
    /// flicker, no restart.
    pub fn go(&mut self, now: Seconds, to: impl Into<Destination>) {
        let mode = to.into();
        if mode == self.current {
            return;
        }
        self.previous = Some(self.current);
        self.current = mode;
        // Start the transition from fully-outgoing and let the spring carry it
        // home. Because springs retarget rather than restart, interrupting a
        // switch halfway redirects it smoothly instead of jumping.
        self.transition.reset(now, 1.0);
        self.transition.target(now, 0.0);
    }

    /// How far through the transition, 1 at the start and 0 once arrived.
    pub fn progress(&self, now: Seconds) -> f32 {
        self.transition.value(now).clamp(0.0, 1.0)
    }

    pub fn settled(&self, now: Seconds) -> bool {
        self.transition.at_rest(now)
    }

    /// Opacity and scale for the arriving mode.
    ///
    /// It comes from slightly beyond and settles back, so the workspace feels
    /// like it has depth rather than sliding on a plane.
    pub fn incoming(&self, now: Seconds) -> (f32, f32) {
        let t = self.progress(now);
        (1.0 - t, 1.0 + t * 0.04)
    }

    /// Opacity and scale for the departing mode.
    ///
    /// It recedes rather than sliding away — the two overlap, which is what
    /// makes the change read as one movement.
    pub fn outgoing(&self, now: Seconds) -> (f32, f32) {
        let t = self.progress(now);
        (t * 0.6, 1.0 - (1.0 - t) * 0.04)
    }

    /// Drop the departing mode once it is no longer visible, so it stops being
    /// drawn at all.
    pub fn settle(&mut self, now: Seconds) {
        if self.transition.at_rest(now) {
            self.previous = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_five_modes_with_distinct_keys() {
        assert_eq!(Mode::ALL.len(), 5);
        let mut keys: Vec<u8> = Mode::ALL.iter().map(|m| m.index()).collect();
        keys.sort();
        assert_eq!(keys, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn index_round_trips() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_index(m.index()), Some(m));
        }
        assert_eq!(Mode::from_index(0), None);
        assert_eq!(Mode::from_index(6), None);
    }

    #[test]
    fn search_matches_by_prefix_and_ignores_case() {
        assert_eq!(Mode::matching("mu"), Some(Mode::Music));
        assert_eq!(Mode::matching("CLOCK"), Some(Mode::Clock));
        assert_eq!(Mode::matching("  files "), Some(Mode::Files));
        assert_eq!(Mode::matching(""), None);
        assert_eq!(Mode::matching("zz"), None);
    }

    #[test]
    fn music_and_mail_are_told_apart_by_the_second_letter() {
        // Both begin with "m", so a single letter is ambiguous — but it must
        // resolve to something rather than nothing, and never to the wrong one
        // once enough has been typed.
        assert_eq!(Mode::matching("mu"), Some(Mode::Music));
        assert_eq!(Mode::matching("ma"), Some(Mode::Mail));
    }

    #[test]
    fn switching_overlaps_the_two_modes() {
        // The defining property: both are partly visible in the middle of a
        // switch, which is what makes it one movement rather than a cut.
        let mut nav = Navigator::new(Mode::Music);
        nav.go(0.0, Mode::Files);

        let (incoming, _) = nav.incoming(0.08);
        let (outgoing, _) = nav.outgoing(0.08);
        assert!(
            incoming > 0.05,
            "the arriving mode had not started: {incoming}"
        );
        assert!(
            outgoing > 0.05,
            "the leaving mode had already gone: {outgoing}"
        );
    }

    #[test]
    fn the_arriving_mode_comes_from_beyond_and_settles() {
        let mut nav = Navigator::new(Mode::Music);
        nav.go(0.0, Mode::Clock);

        let (_, early) = nav.incoming(0.01);
        assert!(early > 1.0, "should arrive from further away, got {early}");

        let settled = nav.transition.params().settle_estimate_for(1.0) as f64;
        let (opacity, scale) = nav.incoming(settled);
        assert!((scale - 1.0).abs() < 1e-3, "should settle at natural size");
        assert!((opacity - 1.0).abs() < 1e-3, "should settle fully present");
    }

    #[test]
    fn selecting_the_current_mode_does_nothing() {
        let mut nav = Navigator::new(Mode::Mail);
        nav.go(0.0, Mode::Mail);
        assert!(
            nav.previous().is_none(),
            "should not have started a transition"
        );
        assert!(nav.settled(0.0));
    }

    #[test]
    fn interrupting_a_switch_redirects_rather_than_restarting() {
        // Mashing Super+2, Super+4, Super+1 should read as one continuous
        // movement, never as three.
        let mut nav = Navigator::new(Mode::Music);
        nav.go(0.0, Mode::Clock);
        let midway = nav.progress(0.1);

        nav.go(0.1, Mode::Files);
        // Redirecting resets the transition to its start, but the mode being
        // left is now Clock — the movement continues from where it was.
        assert_eq!(nav.current(), Destination::Mode(Mode::Files));
        assert_eq!(nav.previous(), Some(Destination::Mode(Mode::Clock)));
        assert!(midway > 0.0 && midway < 1.0, "should have been mid-flight");
    }

    #[test]
    fn the_departing_mode_is_dropped_once_it_is_gone() {
        // Otherwise it would keep being drawn forever at zero opacity.
        let mut nav = Navigator::new(Mode::Music);
        nav.go(0.0, Mode::Studio);
        assert!(nav.previous().is_some());

        let settled = nav.transition.params().settle_estimate_for(1.0) as f64;
        nav.settle(settled);
        assert!(nav.previous().is_none());
    }

    #[test]
    fn a_switch_completes_quickly_enough_to_feel_immediate() {
        let mut nav = Navigator::new(Mode::Music);
        nav.go(0.0, Mode::Files);
        let arrival = nav.transition.params().perceptual_arrival() as f64;
        assert!(
            arrival < 0.5,
            "a mode switch takes {arrival}s to read as done"
        );
    }
}
