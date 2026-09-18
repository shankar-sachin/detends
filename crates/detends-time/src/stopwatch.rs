//! The stopwatch.

use crate::timer::{format_duration, seconds_between};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stopwatch {
    /// When the current run began, if it is running.
    running_since: Option<Timestamp>,
    /// Time from previous runs.
    accumulated: f64,
    /// Elapsed time at each lap, measured from zero.
    laps: Vec<f64>,
}

impl Stopwatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    pub fn elapsed(&self, now: Timestamp) -> f64 {
        match self.running_since {
            Some(since) => self.accumulated + seconds_between(since, now).max(0.0),
            None => self.accumulated,
        }
    }

    pub fn start(&mut self, now: Timestamp) {
        if self.running_since.is_none() {
            self.running_since = Some(now);
        }
    }

    pub fn stop(&mut self, now: Timestamp) {
        if let Some(since) = self.running_since.take() {
            self.accumulated += seconds_between(since, now).max(0.0);
        }
    }

    pub fn toggle(&mut self, now: Timestamp) {
        if self.is_running() {
            self.stop(now);
        } else {
            self.start(now);
        }
    }

    /// Back to zero, laps and all.
    pub fn reset(&mut self) {
        self.running_since = None;
        self.accumulated = 0.0;
        self.laps.clear();
    }

    /// Mark a lap. Does nothing while stopped — a lap on a stopped watch would
    /// record the same time twice.
    pub fn lap(&mut self, now: Timestamp) {
        if self.is_running() {
            self.laps.push(self.elapsed(now));
        }
    }

    /// Lap times measured from zero.
    pub fn laps(&self) -> &[f64] {
        &self.laps
    }

    /// The duration of each lap on its own, which is what a lap list shows.
    pub fn splits(&self) -> Vec<f64> {
        let mut previous = 0.0;
        self.laps
            .iter()
            .map(|at| {
                let split = at - previous;
                previous = *at;
                split
            })
            .collect()
    }

    /// `1:23.4` — tenths, because a stopwatch without them is not a stopwatch.
    pub fn readout(&self, now: Timestamp) -> String {
        let elapsed = self.elapsed(now);
        let tenths = ((elapsed * 10.0) as u64) % 10;
        format!("{}.{tenths}", format_duration(elapsed.floor()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Clock;

    fn at(second: i64) -> Timestamp {
        Clock::frozen_at(2026, 9, 18, 9, 0, "UTC")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
            + jiff::SignedDuration::from_millis(second * 1000)
    }

    #[test]
    fn it_starts_stops_and_resumes() {
        let mut watch = Stopwatch::new();
        assert_eq!(watch.elapsed(at(0)), 0.0);

        watch.start(at(0));
        assert!(watch.is_running());
        assert!((watch.elapsed(at(5)) - 5.0).abs() < 0.01);

        watch.stop(at(5));
        assert!(!watch.is_running());
        // Time passing while stopped changes nothing.
        assert!((watch.elapsed(at(20)) - 5.0).abs() < 0.01);

        watch.start(at(20));
        assert!((watch.elapsed(at(25)) - 10.0).abs() < 0.01);
    }

    #[test]
    fn starting_twice_does_not_lose_the_first_start() {
        let mut watch = Stopwatch::new();
        watch.start(at(0));
        watch.start(at(5));
        assert!((watch.elapsed(at(10)) - 10.0).abs() < 0.01, "the clock jumped");
    }

    #[test]
    fn laps_record_totals_and_splits() {
        let mut watch = Stopwatch::new();
        watch.start(at(0));
        watch.lap(at(10));
        watch.lap(at(25));
        watch.lap(at(30));

        let laps = watch.laps();
        assert_eq!(laps.len(), 3);
        assert!((laps[0] - 10.0).abs() < 0.01);
        assert!((laps[2] - 30.0).abs() < 0.01);

        let splits = watch.splits();
        assert!((splits[0] - 10.0).abs() < 0.01);
        assert!((splits[1] - 15.0).abs() < 0.01);
        assert!((splits[2] - 5.0).abs() < 0.01);
    }

    #[test]
    fn a_lap_on_a_stopped_watch_records_nothing() {
        // Otherwise pressing lap while stopped fills the list with duplicates.
        let mut watch = Stopwatch::new();
        watch.lap(at(5));
        assert!(watch.laps().is_empty());
    }

    #[test]
    fn resetting_clears_everything() {
        let mut watch = Stopwatch::new();
        watch.start(at(0));
        watch.lap(at(5));
        watch.reset();

        assert_eq!(watch.elapsed(at(30)), 0.0);
        assert!(watch.laps().is_empty());
        assert!(!watch.is_running());
    }

    #[test]
    fn the_readout_shows_tenths() {
        let mut watch = Stopwatch::new();
        watch.start(at(0));
        let reading = watch.readout(at(83));
        assert!(reading.starts_with("1:23"), "got {reading}");
        assert!(reading.contains('.'), "a stopwatch needs tenths: {reading}");
    }
}
