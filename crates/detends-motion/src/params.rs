//! Spring parameters and the détends motion vocabulary.

/// The floor on the rest threshold, for springs that barely travel at all.
pub const REST_FLOOR: f32 = 0.0005;

/// The rest threshold as a fraction of how far a spring is travelling.
const REST_FRACTION: f32 = 0.00025;

/// How close a spring must be to its target before it counts as arrived,
/// given how far it set out to travel.
///
/// This has to be scale-aware, because one spring type animates both pixel
/// geometry (which can travel a thousand units) and opacity (which travels
/// one). A threshold tuned for pixels would snap opacity visibly; a threshold
/// tuned for opacity keeps geometry rendering long after it has visually
/// arrived, which defeats the whole point of being able to go idle.
///
/// A quarter of a thousandth of the travel is sub-pixel across a full screen
/// and imperceptible on a colour, with an absolute floor so that a spring
/// moving almost nowhere still terminates.
pub fn rest_threshold(travel: f32) -> f32 {
    REST_FLOOR.max(REST_FRACTION * travel.abs())
}

/// A damped spring, described the way a designer thinks about one:
/// how tight it is, and how much it overshoots.
///
/// Internally this is the ODE `x'' + 2ζω·x' + ω²·(x − target) = 0`
/// with unit mass, so `ω = √k`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Undamped angular frequency, rad/s.
    pub omega: f32,
    /// Damping ratio. `< 1` overshoots, `1` is critical, `> 1` crawls in.
    pub zeta: f32,
}

impl Params {
    /// Build from stiffness and damping ratio. Mass is always 1.
    pub fn new(stiffness: f32, zeta: f32) -> Self {
        debug_assert!(stiffness > 0.0, "stiffness must be positive");
        debug_assert!(zeta > 0.0, "damping ratio must be positive");
        Self {
            omega: stiffness.sqrt(),
            zeta,
        }
    }

    /// How long until a change of `distance` units comes fully to rest.
    ///
    /// This is when the spring stops *rendering*, not when it looks like it
    /// arrived — see [`Self::perceptual_arrival`] for that, which is
    /// considerably sooner.
    ///
    /// This genuinely depends on how far the spring travels: arriving within a
    /// fixed sub-pixel threshold takes longer from further away. An estimate
    /// that ignored distance would quietly under-report on large movements,
    /// which is exactly the kind of small dishonesty that turns into a
    /// "sometimes it feels sluggish" bug report months later.
    pub fn settle_estimate_for(&self, distance: f32) -> f32 {
        let distance = distance.abs();
        let threshold = rest_threshold(distance);
        if distance <= threshold {
            return 0.0;
        }

        let rate = (self.zeta * self.omega).max(f32::EPSILON);

        // The decaying exponential gives a first guess...
        let mut t = (distance / threshold).ln() / rate;

        // ...but it is always optimistic, because every damping regime carries
        // an extra factor the bare exponential ignores: `(1 + ωt)` at critical
        // damping, a sinusoid underdamped, a second exponential overdamped.
        // Rather than fudge a safety constant, walk forward against the real
        // solution until it actually satisfies the rest test.
        let step = 0.25 / rate;
        for _ in 0..256 {
            let c = crate::solver::coeffs(*self, t);
            let arrived = c.a.abs() * distance < threshold;
            let stopped = c.c.abs() * distance < threshold * self.omega;
            if arrived && stopped {
                break;
            }
            t += step;
        }
        t
    }

    /// Settle time for a change of one unit — opacity, a normalised progress,
    /// a small nudge.
    ///
    /// For anything measured in pixels, prefer [`Self::settle_estimate_for`].
    pub fn settle_estimate(&self) -> f32 {
        self.settle_estimate_for(1.0)
    }

    /// When the movement stops reading as movement — within 1% of its target.
    ///
    /// This is the number that describes how the system *feels*, and it is the
    /// one to reach for when tuning. Because the remaining fraction is what
    /// matters rather than the remaining distance, it is the same whether the
    /// spring travels forty pixels or twelve hundred.
    pub fn perceptual_arrival(&self) -> f32 {
        self.perceptual_arrival_for(1.0)
    }

    /// Perceptual arrival for a specific travel distance. Identical for all
    /// distances; the parameter exists for symmetry and for documenting intent
    /// at call sites.
    pub fn perceptual_arrival_for(&self, distance: f32) -> f32 {
        let _ = distance;
        let rate = (self.zeta * self.omega).max(f32::EPSILON);
        let mut t = (1.0_f32 / 0.01).ln() / rate;
        let step = 0.25 / rate;
        for _ in 0..256 {
            if crate::solver::coeffs(*self, t).a.abs() < 0.01 {
                break;
            }
            t += step;
        }
        t
    }
}

/// The four springs détends animates in. Everything in the system uses one of
/// these — a transition that needs a fifth is a transition worth questioning.
///
/// Consistency here is what makes the whole interface feel like one object
/// rather than a collection of separately-tuned animations.
pub mod springs {
    use super::Params;

    /// Toggles, taps, selection. Arrives almost immediately, never overshoots.
    pub const SNAP: Params = Params {
        omega: 20.493902,
        zeta: 1.00,
    };

    /// Mode switches. The one spring allowed a trace of overshoot, because
    /// the workspace should feel like it has mass.
    pub const GLIDE: Params = Params {
        omega: 14.491377,
        zeta: 0.92,
    };

    /// System Center, Search — large surfaces arriving. Calm, no bounce.
    pub const SETTLE: Params = Params {
        omega: 10.954452,
        zeta: 1.00,
    };

    /// Wallpaper drift, Focus dimming. Slow enough to be felt, not watched.
    pub const HUSH: Params = Params {
        omega: 8.3666,
        zeta: 1.00,
    };
}

/// How much motion the user wants (Settings → Appearance).
///
/// Reduced motion is an accessibility requirement, not a preference, so it is
/// threaded through the solver itself rather than bolted onto call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotionPreference {
    #[default]
    Full,
    /// Collapse springs to a brief, direct settle. State changes stay legible;
    /// nothing travels, overshoots, or draws the eye.
    Reduced,
}

impl MotionPreference {
    /// Apply the preference to a spring.
    pub fn apply(self, params: Params) -> Params {
        match self {
            Self::Full => params,
            // Stiff and critically damped: arrives in ~120ms with no overshoot
            // and no perceptible travel.
            Self::Reduced => Params {
                omega: 42.0,
                zeta: 1.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_springs_match_their_stiffness() {
        // The constants are written out literally so they can be `const`;
        // this guards them against drifting from their documented stiffness.
        for (named, stiffness, zeta) in [
            (springs::SNAP, 420.0, 1.00),
            (springs::GLIDE, 210.0, 0.92),
            (springs::SETTLE, 120.0, 1.00),
            (springs::HUSH, 70.0, 1.00),
        ] {
            let computed = Params::new(stiffness, zeta);
            assert!(
                (named.omega - computed.omega).abs() < 1e-4,
                "omega drift: {named:?} vs {computed:?}"
            );
            assert_eq!(named.zeta, computed.zeta);
        }
    }

    #[test]
    fn everything_looks_arrived_quickly() {
        // This is the assertion that actually describes the feel of détends.
        // Perceptual arrival is scale-invariant for a spring, so one bound
        // covers a toggle and a full-screen transition alike.
        for (s, name) in [
            (springs::SNAP, "SNAP"),
            (springs::GLIDE, "GLIDE"),
            (springs::SETTLE, "SETTLE"),
            (springs::HUSH, "HUSH"),
        ] {
            let t = s.perceptual_arrival();
            assert!(t > 0.15 && t < 0.85, "{name} looks arrived after {t}s");
        }
    }

    #[test]
    fn perceptual_arrival_does_not_depend_on_distance() {
        // A pleasant property of springs, and one worth locking down: a panel
        // sliding 40px and one crossing the screen take the same time to read
        // as "there", so the system feels consistent regardless of geometry.
        for s in [
            springs::SNAP,
            springs::GLIDE,
            springs::SETTLE,
            springs::HUSH,
        ] {
            let near = s.perceptual_arrival_for(40.0);
            let far = s.perceptual_arrival_for(1200.0);
            assert!((near - far).abs() < 1e-3, "{s:?}: {near}s vs {far}s");
        }
    }

    #[test]
    fn coming_to_rest_takes_longer_than_looking_arrived() {
        for s in [
            springs::SNAP,
            springs::GLIDE,
            springs::SETTLE,
            springs::HUSH,
        ] {
            assert!(s.perceptual_arrival_for(400.0) < s.settle_estimate_for(400.0));
        }
    }

    #[test]
    fn the_springs_are_ordered_from_quick_to_calm() {
        let at = |p: Params| p.settle_estimate_for(100.0);
        assert!(at(springs::SNAP) < at(springs::GLIDE));
        assert!(at(springs::GLIDE) < at(springs::SETTLE));
        assert!(at(springs::SETTLE) < at(springs::HUSH));
    }

    #[test]
    fn the_estimate_actually_satisfies_the_rest_test() {
        // The estimate is only worth having if a spring really has arrived by
        // the time it says so.
        for p in [
            springs::SNAP,
            springs::GLIDE,
            springs::SETTLE,
            springs::HUSH,
        ] {
            for distance in [1.0_f32, 42.0, 100.0, 1200.0] {
                let t = p.settle_estimate_for(distance);
                let c = crate::solver::coeffs(p, t);
                assert!(
                    c.a.abs() * distance < rest_threshold(distance),
                    "{p:?} over {distance}: still {} away at {t}s",
                    c.a.abs() * distance
                );
            }
        }
    }

    #[test]
    fn a_long_move_does_not_take_longer_to_settle_than_a_short_one() {
        // Because the rest threshold scales with the travel, a spring crossing
        // the screen comes to rest in the same time as one nudging a toggle.
        // Consistency like this is what keeps the system feeling like one
        // object rather than a set of separately-tuned animations.
        let p = springs::SETTLE;
        let near = p.settle_estimate_for(10.0);
        let far = p.settle_estimate_for(1000.0);
        assert!((near - far).abs() < 0.05, "{near}s vs {far}s");
    }

    #[test]
    fn very_small_movements_hit_the_floor_and_settle_sooner() {
        // Below the floor the threshold stops shrinking, so a movement smaller
        // than half a thousandth of a pixel is over almost immediately rather
        // than chasing an ever-finer target.
        let p = springs::SETTLE;
        assert!(p.settle_estimate_for(0.001) < p.settle_estimate_for(100.0));
    }

    #[test]
    fn a_change_already_within_the_threshold_settles_immediately() {
        assert_eq!(springs::SNAP.settle_estimate_for(0.0), 0.0);
        assert_eq!(springs::SNAP.settle_estimate_for(REST_FLOOR / 2.0), 0.0);
    }

    #[test]
    fn the_rest_threshold_stays_sub_pixel_and_never_hits_zero() {
        assert!(rest_threshold(1200.0) < 0.5, "would snap visibly");
        assert!(
            rest_threshold(1.0) <= REST_FLOOR * 2.0,
            "too coarse for opacity"
        );
        assert!(rest_threshold(0.0) > 0.0, "must terminate");
    }
}
