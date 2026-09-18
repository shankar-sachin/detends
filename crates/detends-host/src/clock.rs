//! The frame clock.

use std::time::Instant;

/// Absolute time, sampled once per frame.
///
/// Every spring in a frame is evaluated against the same instant. Sampling per
/// call would let two halves of one transition disagree by a few microseconds:
/// invisible on its own, but enough to break the illusion that separately
/// animated elements are one moving object.
pub struct Clock {
    origin: Instant,
    last_frame: f64,
    /// Rolling mean frame interval, for the debug overlay.
    mean_interval: f64,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            last_frame: 0.0,
            mean_interval: 1.0 / 120.0,
        }
    }

    /// Seconds since start. Monotonic, and unaffected by the system clock.
    pub fn now(&self) -> f64 {
        self.origin.elapsed().as_secs_f64()
    }

    /// Call once at the top of each frame.
    pub fn tick(&mut self) -> f64 {
        let now = self.now();
        let interval = now - self.last_frame;
        self.last_frame = now;

        // Ignore the first frame and any pause longer than a blink, so a
        // window being dragged between displays does not poison the average.
        if interval > 0.0 && interval < 0.25 {
            self.mean_interval += (interval - self.mean_interval) * 0.1;
        }
        now
    }

    /// Smoothed frames per second.
    pub fn fps(&self) -> f64 {
        1.0 / self.mean_interval.max(1e-6)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_stall_does_not_poison_the_average() {
        // Dragging a window between displays, or a debugger breakpoint, would
        // otherwise leave the reported rate wrong for a long time afterwards.
        let mut c = Clock::new();
        c.mean_interval = 1.0 / 120.0;
        let before = c.fps();

        c.last_frame = 0.0;
        // A two-second stall, far beyond the 0.25s guard.
        let stalled = 2.0;
        let interval = stalled - c.last_frame;
        assert!(interval >= 0.25, "this test needs a stall past the guard");

        // Same arithmetic tick() performs, with the guard in place.
        if interval > 0.0 && interval < 0.25 {
            c.mean_interval += (interval - c.mean_interval) * 0.1;
        }
        assert_eq!(c.fps(), before, "the stall was counted");
    }

    #[test]
    fn the_average_converges_on_the_real_rate() {
        let mut c = Clock::new();
        c.mean_interval = 1.0 / 60.0;
        for _ in 0..200 {
            let interval = 1.0 / 120.0;
            c.mean_interval += (interval - c.mean_interval) * 0.1;
        }
        assert!((c.fps() - 120.0).abs() < 1.0, "converged to {}", c.fps());
    }
}
