//! Everything time-related, ticking in one place.
//!
//! Owned by the shell rather than by Clock's view, because "alarms and timers
//! should continue functioning regardless of the currently selected mode" (§5).
//! A timer that only counts down while you are looking at it is not a timer.
//!
//! This is also the whole of Clock's state, which is what makes Clock one
//! environment rather than five applications sharing a screen.

use crate::alarm::{Alarm, AlarmId, Repeat};
use crate::stopwatch::Stopwatch;
use crate::timer::{Timer, TimerId};
use jiff::{tz::TimeZone, Timestamp};
use serde::{Deserialize, Serialize};

/// Something that wants the user's attention.
#[derive(Clone, Debug, PartialEq)]
pub enum Fired {
    Timer { id: TimerId, name: Option<String> },
    Alarm { id: AlarmId, label: Option<String> },
}

impl Fired {
    /// What the notification says.
    pub fn title(&self) -> String {
        match self {
            Fired::Timer { name: Some(name), .. } => name.clone(),
            Fired::Timer { .. } => "Timer".into(),
            Fired::Alarm { label: Some(label), .. } => label.clone(),
            Fired::Alarm { .. } => "Alarm".into(),
        }
    }

    pub fn detail(&self) -> &'static str {
        match self {
            Fired::Timer { .. } => "Timer finished",
            Fired::Alarm { .. } => "Alarm",
        }
    }
}

/// A city on the world clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldClock {
    /// An IANA zone name, e.g. `Europe/Paris`.
    pub zone: String,
    /// What to call it. The zone name is a poor label — nobody thinks of Paris
    /// as "Europe/Paris".
    pub label: String,
}

impl WorldClock {
    pub fn new(zone: impl Into<String>, label: impl Into<String>) -> Self {
        Self { zone: zone.into(), label: label.into() }
    }

    pub fn time_zone(&self) -> Option<TimeZone> {
        TimeZone::get(&self.zone).ok()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Schedule {
    pub timers: Vec<Timer>,
    pub alarms: Vec<Alarm>,
    pub stopwatch: Stopwatch,
    pub world: Vec<WorldClock>,
    /// Where ids come from. Monotonic, so an id is never reused even after
    /// something is deleted.
    next_id: u64,
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    // --- timers ----------------------------------------------------------

    pub fn start_timer(
        &mut self,
        now: Timestamp,
        duration: f64,
        name: Option<String>,
    ) -> TimerId {
        let id = TimerId(self.take_id());
        self.timers.push(Timer::start(id, now, duration, name));
        id
    }

    pub fn timer(&self, id: TimerId) -> Option<&Timer> {
        self.timers.iter().find(|t| t.id == id)
    }

    pub fn timer_mut(&mut self, id: TimerId) -> Option<&mut Timer> {
        self.timers.iter_mut().find(|t| t.id == id)
    }

    pub fn dismiss_timer(&mut self, id: TimerId) {
        self.timers.retain(|t| t.id != id);
    }

    /// Timers still counting down, soonest first — which is the order they
    /// matter in.
    pub fn running_timers(&self, now: Timestamp) -> Vec<&Timer> {
        let mut running: Vec<&Timer> = self.timers.iter().filter(|t| t.is_running()).collect();
        running.sort_by(|a, b| {
            a.remaining(now)
                .partial_cmp(&b.remaining(now))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        running
    }

    // --- alarms ----------------------------------------------------------

    pub fn add_alarm(&mut self, now: Timestamp, hour: i8, minute: i8, repeat: Repeat) -> AlarmId {
        let id = AlarmId(self.take_id());
        self.alarms.push(Alarm::new(id, hour, minute, repeat, now));
        id
    }

    pub fn alarm_mut(&mut self, id: AlarmId) -> Option<&mut Alarm> {
        self.alarms.iter_mut().find(|a| a.id == id)
    }

    pub fn remove_alarm(&mut self, id: AlarmId) {
        self.alarms.retain(|a| a.id != id);
    }

    /// The soonest alarm still to come, if any.
    pub fn next_alarm(&self, now: Timestamp, zone: &TimeZone) -> Option<(&Alarm, Timestamp)> {
        self.alarms
            .iter()
            .filter(|a| a.enabled)
            .filter_map(|a| a.next_after(now, zone).map(|at| (a, at)))
            .min_by_key(|(_, at)| *at)
    }

    // --- the tick --------------------------------------------------------

    /// Advance everything, and report what wants attention.
    ///
    /// Called every frame from the shell, whatever mode is on screen.
    pub fn tick(&mut self, now: Timestamp, zone: &TimeZone) -> Vec<Fired> {
        let mut fired = Vec::new();

        for timer in self.timers.iter_mut() {
            if timer.poll(now) {
                fired.push(Fired::Timer {
                    id: timer.id,
                    name: timer.name.clone(),
                });
            }
        }

        for alarm in self.alarms.iter_mut() {
            if alarm.due(now, zone) {
                alarm.last_fired = Some(now);
                // A one-off turns itself off rather than lingering as an alarm
                // that will never sound again.
                if alarm.repeat == Repeat::Once {
                    alarm.enabled = false;
                }
                fired.push(Fired::Alarm {
                    id: alarm.id,
                    label: alarm.label.clone(),
                });
            }
        }

        fired
    }

    /// Whether anything is counting, and so whether the screen needs redrawing
    /// once a second even when nothing else is happening.
    pub fn is_active(&self) -> bool {
        self.stopwatch.is_running() || self.timers.iter().any(|t| t.is_running())
    }

    /// Reconcile after loading: anything that finished while détends was not
    /// running has still finished.
    ///
    /// Without this a timer set for five minutes and then quit through would
    /// come back still counting down from wherever it left off, which is both
    /// wrong and unnerving.
    pub fn reconcile(&mut self, now: Timestamp) {
        for timer in self.timers.iter_mut() {
            timer.poll(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Clock;

    fn zone() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }

    fn at(day: i8, hour: i8, minute: i8) -> Timestamp {
        Clock::frozen_at(2026, 9, day, hour, minute, "America/New_York")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
    }

    #[test]
    fn a_timer_fires_once_and_says_what_it_was_for() {
        let mut schedule = Schedule::new();
        schedule.start_timer(at(18, 9, 0), 600.0, Some("Deep work".into()));

        assert!(schedule.tick(at(18, 9, 5), &zone()).is_empty(), "too early");

        let fired = schedule.tick(at(18, 9, 11), &zone());
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].title(), "Deep work");

        assert!(schedule.tick(at(18, 9, 12), &zone()).is_empty(), "it fired twice");
    }

    #[test]
    fn an_unnamed_timer_still_says_something_useful() {
        let mut schedule = Schedule::new();
        schedule.start_timer(at(18, 9, 0), 60.0, None);
        let fired = schedule.tick(at(18, 9, 2), &zone());
        assert_eq!(fired[0].title(), "Timer");
    }

    #[test]
    fn several_timers_run_at_once_and_are_ordered_by_urgency() {
        let mut schedule = Schedule::new();
        let now = at(18, 9, 0);
        schedule.start_timer(now, 600.0, Some("Long".into()));
        schedule.start_timer(now, 60.0, Some("Short".into()));
        schedule.start_timer(now, 300.0, Some("Middle".into()));

        let running = schedule.running_timers(now);
        let names: Vec<&str> = running
            .iter()
            .map(|t| t.name.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(names, vec!["Short", "Middle", "Long"]);
    }

    #[test]
    fn a_repeating_alarm_stays_on_and_a_one_off_switches_itself_off() {
        let mut schedule = Schedule::new();
        let daily = schedule.add_alarm(at(18, 6, 0), 7, 30, Repeat::Daily);
        let once = schedule.add_alarm(at(18, 6, 0), 7, 30, Repeat::Once);

        let fired = schedule.tick(at(18, 7, 31), &zone());
        assert_eq!(fired.len(), 2);

        assert!(schedule.alarm_mut(daily).unwrap().enabled, "the daily alarm turned itself off");
        assert!(!schedule.alarm_mut(once).unwrap().enabled, "the one-off stayed on");
    }

    #[test]
    fn the_next_alarm_is_the_soonest_one() {
        let mut schedule = Schedule::new();
        schedule.add_alarm(at(18, 6, 0), 9, 0, Repeat::Daily);
        schedule.add_alarm(at(18, 6, 0), 7, 30, Repeat::Daily);
        schedule.add_alarm(at(18, 6, 0), 18, 0, Repeat::Daily);

        let (alarm, _) = schedule.next_alarm(at(18, 6, 30), &zone()).expect("an alarm");
        assert_eq!(alarm.readout(), "7:30 AM");
    }

    #[test]
    fn ids_are_never_reused() {
        // Otherwise dismissing one timer and starting another could address the
        // wrong one.
        let mut schedule = Schedule::new();
        let first = schedule.start_timer(at(18, 9, 0), 60.0, None);
        schedule.dismiss_timer(first);
        let second = schedule.start_timer(at(18, 9, 0), 60.0, None);
        assert_ne!(first, second);
    }

    #[test]
    fn a_timer_that_finished_while_detends_was_closed_has_still_finished() {
        let mut schedule = Schedule::new();
        schedule.start_timer(at(18, 9, 0), 300.0, Some("Bread".into()));

        // Quit, and come back an hour later.
        schedule.reconcile(at(18, 10, 0));
        assert!(schedule.timers[0].has_rung());
        assert_eq!(schedule.timers[0].readout(at(18, 10, 0)), "0:00");
    }

    #[test]
    fn it_only_asks_for_frames_when_something_is_counting() {
        // §21: a calm system stops drawing when nothing is happening.
        let mut schedule = Schedule::new();
        assert!(!schedule.is_active());

        schedule.add_alarm(at(18, 6, 0), 7, 30, Repeat::Daily);
        assert!(!schedule.is_active(), "a pending alarm is not a reason to redraw");

        let id = schedule.start_timer(at(18, 9, 0), 60.0, None);
        assert!(schedule.is_active());

        schedule.dismiss_timer(id);
        assert!(!schedule.is_active());

        schedule.stopwatch.start(at(18, 9, 0));
        assert!(schedule.is_active());
    }

    #[test]
    fn world_clocks_resolve_their_zones() {
        let paris = WorldClock::new("Europe/Paris", "Paris");
        assert!(paris.time_zone().is_some());
        assert!(WorldClock::new("Mars/Olympus", "Olympus").time_zone().is_none());
    }
}
