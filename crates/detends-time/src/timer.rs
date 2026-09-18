//! Countdown timers.
//!
//! Anchored to an instant on the wall clock rather than to a running total of
//! elapsed seconds. That choice is what makes a timer survive quitting the
//! system: the answer to "how long is left" is a subtraction against now, so it
//! stays right across a restart, a sleep, or a process that was not running for
//! twenty minutes.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimerId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TimerState {
    /// Counting down toward an instant.
    Running { ends_at: Timestamp },
    /// Held, with this much left to run.
    Paused { remaining: f64 },
    /// Reached zero and not yet dismissed.
    Rang { at: Timestamp },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    pub id: TimerId,
    /// What it is for. A timer you can look at later and understand.
    pub name: Option<String>,
    /// How long it was set for, in seconds.
    pub duration: f64,
    pub state: TimerState,
}

impl Timer {
    pub fn start(id: TimerId, now: Timestamp, duration: f64, name: Option<String>) -> Self {
        Self {
            id,
            name,
            duration: duration.max(0.0),
            state: TimerState::Running {
                ends_at: add_seconds(now, duration.max(0.0)),
            },
        }
    }

    /// Seconds left, never negative.
    pub fn remaining(&self, now: Timestamp) -> f64 {
        match &self.state {
            TimerState::Running { ends_at } => seconds_between(now, *ends_at).max(0.0),
            TimerState::Paused { remaining } => *remaining,
            TimerState::Rang { .. } => 0.0,
        }
    }

    /// How far through, 0 to 1. Useful for a ring or a bar.
    pub fn progress(&self, now: Timestamp) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (1.0 - self.remaining(now) / self.duration).clamp(0.0, 1.0) as f32
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, TimerState::Running { .. })
    }

    pub fn has_rung(&self) -> bool {
        matches!(self.state, TimerState::Rang { .. })
    }

    pub fn pause(&mut self, now: Timestamp) {
        if let TimerState::Running { .. } = self.state {
            self.state = TimerState::Paused {
                remaining: self.remaining(now),
            };
        }
    }

    pub fn resume(&mut self, now: Timestamp) {
        if let TimerState::Paused { remaining } = self.state {
            self.state = TimerState::Running {
                ends_at: add_seconds(now, remaining),
            };
        }
    }

    /// Back to the top, still running.
    pub fn restart(&mut self, now: Timestamp) {
        self.state = TimerState::Running {
            ends_at: add_seconds(now, self.duration),
        };
    }

    /// Mark it as having rung, if it has. Returns whether it just did.
    pub fn poll(&mut self, now: Timestamp) -> bool {
        if let TimerState::Running { ends_at } = self.state {
            if now >= ends_at {
                self.state = TimerState::Rang { at: ends_at };
                return true;
            }
        }
        false
    }

    /// `24:18`, or `1:02:45` past an hour.
    pub fn readout(&self, now: Timestamp) -> String {
        format_duration(self.remaining(now))
    }
}

/// `24:18`, growing an hours field only when it needs one.
pub fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub(crate) fn add_seconds(at: Timestamp, seconds: f64) -> Timestamp {
    let nanos = (seconds * 1e9).round() as i64;
    at.checked_add(jiff::SignedDuration::from_nanos(nanos))
        .unwrap_or(at)
}

pub(crate) fn seconds_between(from: Timestamp, to: Timestamp) -> f64 {
    // Subtracting two timestamps in jiff yields a calendar `Span`, which is
    // deliberately not a fixed number of seconds. An absolute difference is
    // what is wanted here, so ask for it directly.
    to.duration_since(from).as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Clock;

    fn at(minute: i8, second: i8) -> Timestamp {
        Clock::frozen_at(2026, 9, 18, 9, minute, "UTC")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
            + jiff::SignedDuration::from_secs(second as i64)
    }

    #[test]
    fn it_counts_down_and_rings() {
        let start = at(0, 0);
        let mut timer = Timer::start(TimerId(1), start, 25.0 * 60.0, Some("Deep work".into()));

        assert_eq!(timer.readout(start), "25:00");
        assert_eq!(timer.readout(at(0, 42)), "24:18");
        assert!(!timer.poll(at(10, 0)));

        assert!(timer.poll(at(25, 1)), "it should have rung");
        assert!(timer.has_rung());
        assert_eq!(timer.readout(at(30, 0)), "0:00", "must not go negative");
    }

    #[test]
    fn it_rings_only_once() {
        let mut timer = Timer::start(TimerId(1), at(0, 0), 10.0, None);
        assert!(timer.poll(at(0, 11)));
        assert!(!timer.poll(at(0, 12)), "it rang twice");
    }

    #[test]
    fn it_survives_not_being_looked_at_for_hours() {
        // Anchored to an instant, so a process that was not running still knows
        // the timer finished.
        let mut timer = Timer::start(TimerId(1), at(0, 0), 60.0, None);
        assert!(timer.poll(at(59, 0)), "it should have rung while nobody watched");
    }

    #[test]
    fn pausing_holds_the_remaining_time() {
        let mut timer = Timer::start(TimerId(1), at(0, 0), 300.0, None);
        timer.pause(at(1, 0));
        assert_eq!(timer.readout(at(1, 0)), "4:00");

        // Time passing while paused changes nothing.
        assert_eq!(timer.readout(at(9, 0)), "4:00");
        assert!(!timer.is_running());

        timer.resume(at(9, 0));
        assert!(timer.is_running());
        assert_eq!(timer.readout(at(10, 0)), "3:00");
    }

    #[test]
    fn progress_runs_from_nothing_to_everything() {
        let timer = Timer::start(TimerId(1), at(0, 0), 100.0, None);
        assert!(timer.progress(at(0, 0)) < 0.01);
        assert!((timer.progress(at(0, 50)) - 0.5).abs() < 0.02);
        assert!(timer.progress(at(5, 0)) > 0.99, "must not exceed one");
    }

    #[test]
    fn restarting_starts_over_from_the_full_duration() {
        let mut timer = Timer::start(TimerId(1), at(0, 0), 120.0, None);
        timer.poll(at(3, 0));
        assert!(timer.has_rung());

        timer.restart(at(3, 0));
        assert!(timer.is_running());
        assert_eq!(timer.readout(at(3, 0)), "2:00");
    }

    #[test]
    fn the_readout_grows_an_hours_field_only_when_needed() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(59.0), "0:59");
        assert_eq!(format_duration(3599.0), "59:59");
        assert_eq!(format_duration(3600.0), "1:00:00");
        assert_eq!(format_duration(-5.0), "0:00", "must not go negative");
    }

    #[test]
    fn a_timer_of_no_length_is_already_finished() {
        let timer = Timer::start(TimerId(1), at(0, 0), 0.0, None);
        assert_eq!(timer.remaining(at(0, 0)), 0.0);
        assert_eq!(timer.progress(at(0, 0)), 1.0);
    }
}
