//! A spring, sampled at absolute time.

use crate::{coeffs, params::rest_threshold, Animatable, Params};

/// A value that moves toward a target under spring physics.
///
/// Sampled at an absolute timestamp rather than advanced by a delta. Nothing
/// is mutated by reading it, so the same spring can be evaluated many times in
/// a frame — by layout, then by paint — and always agree.
///
/// ```
/// # use detends_motion::{Spring, springs};
/// let mut opacity = Spring::new(springs::SNAP, 0.0);
/// opacity.target(0.0, 1.0);
///
/// assert!(opacity.value(0.05) > 0.0);          // moving
/// assert!(opacity.value(1.0) == 1.0);          // arrived and snapped
/// assert!(opacity.at_rest(1.0));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Spring<T: Animatable> {
    params: Params,
    /// When the current motion began, in seconds on the shell's clock.
    origin_time: f64,
    /// Value and velocity at `origin_time`.
    origin_value: T,
    origin_velocity: T,
    target: T,
    /// How far this leg of the motion set out to travel, which sets how close
    /// the spring must get before it counts as arrived.
    travel: f32,
}

impl<T: Animatable> Spring<T> {
    /// A spring at rest, holding `value`.
    pub fn new(params: Params, value: T) -> Self {
        Self {
            params,
            origin_time: 0.0,
            origin_value: value,
            origin_velocity: T::zero(),
            target: value,
            travel: 0.0,
        }
    }

    /// Point the spring at a new target, preserving where it is and how fast
    /// it is already travelling.
    ///
    /// This is the whole reason for the analytic solver: interrupting a
    /// transition halfway redirects it smoothly instead of restarting it. A
    /// user mashing `Super+2`, `Super+4`, `Super+1` should see one continuous
    /// movement, never three.
    pub fn target(&mut self, now: f64, target: T) {
        if self.target.sub(target).magnitude() < f32::EPSILON {
            return; // already heading there; don't reset the clock
        }
        let (value, velocity) = self.sample(now);
        self.origin_time = now;
        self.origin_value = value;
        self.origin_velocity = velocity;
        self.travel = value.sub(target).magnitude();
        self.target = target;
    }

    /// Jump to a value with no motion at all.
    ///
    /// For appearing from nowhere, or for honouring reduced motion at a call
    /// site where even a short settle would be wrong.
    pub fn reset(&mut self, now: f64, value: T) {
        self.origin_time = now;
        self.origin_value = value;
        self.origin_velocity = T::zero();
        self.target = value;
        self.travel = 0.0;
    }

    /// Retune the spring in flight, keeping position and velocity.
    pub fn retune(&mut self, now: f64, params: Params) {
        let (value, velocity) = self.sample(now);
        self.origin_time = now;
        self.origin_value = value;
        self.origin_velocity = velocity;
        self.params = params;
    }

    /// Where the spring is at `now`.
    pub fn value(&self, now: f64) -> T {
        self.sample(now).0
    }

    /// How fast it is travelling at `now`, per second.
    pub fn velocity(&self, now: f64) -> T {
        self.sample(now).1
    }

    /// Whether the spring has visually arrived.
    ///
    /// Both conditions matter: a spring passing through its target at speed is
    /// not at rest. Velocity is compared against `ε·ω` so that the threshold
    /// scales with how fast the spring is meant to move.
    pub fn at_rest(&self, now: f64) -> bool {
        let threshold = rest_threshold(self.travel);
        let (value, velocity) = self.sample(now);
        value.sub(self.target).magnitude() < threshold
            && velocity.magnitude() < threshold * self.params.omega
    }

    /// The value this spring is heading toward.
    pub fn goal(&self) -> T {
        self.target
    }

    pub fn params(&self) -> Params {
        self.params
    }

    /// Position and velocity in one evaluation, since callers almost always
    /// want both and the coefficients are shared.
    fn sample(&self, now: f64) -> (T, T) {
        let dt = (now - self.origin_time).max(0.0) as f32;
        let c = coeffs(self.params, dt);

        let displacement = self.origin_value.sub(self.target);
        let value = self
            .target
            .add(displacement.scale(c.a))
            .add(self.origin_velocity.scale(c.b));
        let velocity = displacement.scale(c.c).add(self.origin_velocity.scale(c.d));

        // Snap once arrived, so a settled spring returns its target exactly.
        // Without this, springs asymptote forever and the shell never gets to
        // declare itself idle — which is what lets it stop rendering.
        let threshold = rest_threshold(self.travel);
        if value.sub(self.target).magnitude() < threshold
            && velocity.magnitude() < threshold * self.params.omega
        {
            (self.target, T::zero())
        } else {
            (value, velocity)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::springs;

    #[test]
    fn a_new_spring_is_already_at_rest() {
        let s = Spring::new(springs::SNAP, 3.0_f32);
        assert!(s.at_rest(0.0));
        assert_eq!(s.value(0.0), 3.0);
        assert_eq!(s.value(10.0), 3.0);
        assert_eq!(s.velocity(0.5), 0.0);
    }

    #[test]
    fn it_travels_to_its_target_and_stops_exactly_there() {
        let mut s = Spring::new(springs::SETTLE, 0.0_f32);
        s.target(0.0, 100.0);

        assert!(!s.at_rest(0.01));
        assert!(s.value(0.05) > 0.0 && s.value(0.05) < 100.0);

        let settled = s.params().settle_estimate_for(100.0) as f64;
        assert!(
            s.value(s.params().perceptual_arrival() as f64) > 98.0,
            "should look arrived early"
        );
        assert!(s.at_rest(settled));
        assert_eq!(s.value(settled), 100.0, "must land exactly, not asymptote");
        assert_eq!(s.velocity(settled), 0.0);
    }

    #[test]
    fn sampling_does_not_depend_on_how_often_we_sample() {
        // The property that makes 60Hz and 120Hz identical. Walking a spring
        // in tiny steps must give the same answer as jumping straight there.
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 1.0);

        let direct = s.value(0.3);
        let mut walked = 0.0;
        for i in 1..=360 {
            walked = s.value(i as f64 * (0.3 / 360.0));
        }
        assert!((direct - walked).abs() < 1e-6, "{direct} vs {walked}");
    }

    #[test]
    fn irregular_frame_timing_does_not_accumulate_error() {
        // Simulate a stuttering frame clock; the answer at a given instant must
        // not depend on the jitter that preceded it.
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 1.0);

        let mut t = 0.0_f64;
        let jitter = [0.004, 0.019, 0.008, 0.033, 0.002, 0.016];
        for (i, _) in (0..120).enumerate() {
            t += jitter[i % jitter.len()];
            let _ = s.value(t);
        }
        assert!((s.value(t) - Spring::<f32>::sampled_fresh(springs::GLIDE, t)).abs() < 1e-6);
    }

    impl Spring<f32> {
        /// A spring with the same history, evaluated without any intermediate
        /// sampling — the control for the jitter test above.
        fn sampled_fresh(params: Params, t: f64) -> f32 {
            let mut fresh = Spring::new(params, 0.0_f32);
            fresh.target(0.0, 1.0);
            fresh.value(t)
        }
    }

    #[test]
    fn retargeting_preserves_velocity() {
        // The anti-jerk property. At the moment of redirection the spring must
        // still be moving at exactly the speed it was.
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 1.0);

        let before = s.velocity(0.1);
        assert!(before > 0.0, "should be in flight");

        s.target(0.1, -1.0);
        let after = s.velocity(0.1);

        assert!(
            (before - after).abs() < 1e-5,
            "{before} -> {after} is a jerk"
        );
    }

    #[test]
    fn retargeting_preserves_position() {
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 10.0);
        let before = s.value(0.08);
        s.target(0.08, -5.0);
        assert!((before - s.value(0.08)).abs() < 1e-5, "position jumped");
    }

    #[test]
    fn retargeting_to_the_same_place_does_not_restart_the_motion() {
        let mut s = Spring::new(springs::SETTLE, 0.0_f32);
        s.target(0.0, 1.0);
        let midway = s.value(0.1);

        s.target(0.1, 1.0); // same target, repeatedly — as a held key would
        s.target(0.15, 1.0);

        assert!((s.value(0.1) - midway).abs() < 1e-6, "motion was restarted");
        assert!(s.value(0.2) > midway, "motion should have continued");
    }

    #[test]
    fn reset_arrives_instantly_with_no_velocity() {
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 1.0);
        let _ = s.value(0.05);

        s.reset(0.05, 42.0);
        assert_eq!(s.value(0.05), 42.0);
        assert_eq!(s.velocity(0.05), 0.0);
        assert!(s.at_rest(0.05));
    }

    #[test]
    fn retune_keeps_the_value_continuous() {
        // Switching to reduced motion mid-transition must not teleport.
        let mut s = Spring::new(springs::HUSH, 0.0_f32);
        s.target(0.0, 1.0);
        let before = s.value(0.2);

        s.retune(0.2, springs::SNAP);
        assert!((before - s.value(0.2)).abs() < 1e-5);
        assert_eq!(s.params(), springs::SNAP);
    }

    #[test]
    fn arrays_animate_componentwise_and_stay_in_step() {
        let mut s = Spring::new(springs::SETTLE, [0.0_f32, 0.0, 0.0, 0.0]);
        s.target(0.0, [1.0, 2.0, 3.0, 4.0]);

        let mid = s.value(0.12);
        // Same solver coefficients, so the components stay in exact proportion.
        for i in 1..4 {
            let ratio = mid[i] / mid[0];
            assert!(
                (ratio - (i + 1) as f32).abs() < 1e-4,
                "components drifted: {mid:?}"
            );
        }

        let settled = s.params().settle_estimate_for(4.0) as f64;
        assert_eq!(s.value(settled), [1.0, 2.0, 3.0, 4.0]);
        assert!(s.at_rest(settled));
    }

    #[test]
    fn a_spring_passing_through_its_target_is_not_at_rest() {
        let mut s = Spring::new(springs::GLIDE, 0.0_f32);
        s.target(0.0, 1.0);
        // GLIDE overshoots, so it crosses 1.0 while still moving quickly.
        let mut crossed_while_moving = false;
        for i in 1..400 {
            let t = i as f64 * 0.001;
            if (s.value(t) - 1.0).abs() < 0.01 && !s.at_rest(t) {
                crossed_while_moving = true;
                break;
            }
        }
        assert!(
            crossed_while_moving,
            "expected a fast pass through the target"
        );
    }
}
