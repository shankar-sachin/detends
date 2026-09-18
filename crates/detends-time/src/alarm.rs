//! Alarms.
//!
//! A time of day plus a repeat rule, resolved against a zone. Working in local
//! civil time rather than in absolute instants is what makes "07:30 every
//! weekday" mean 07:30 all year, rather than drifting an hour twice a year.

use jiff::civil::Weekday;
use jiff::{tz::TimeZone, Timestamp, Zoned};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AlarmId(pub u64);

/// When an alarm comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    /// Once, then it turns itself off.
    Once,
    Daily,
    /// Monday to Friday.
    Weekdays,
    /// Saturday and Sunday.
    Weekends,
}

impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => "Once",
            Repeat::Daily => "Every day",
            Repeat::Weekdays => "Weekdays",
            Repeat::Weekends => "Weekends",
        }
    }

    fn includes(self, day: Weekday) -> bool {
        let weekend = matches!(day, Weekday::Saturday | Weekday::Sunday);
        match self {
            Repeat::Once | Repeat::Daily => true,
            Repeat::Weekdays => !weekend,
            Repeat::Weekends => weekend,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alarm {
    pub id: AlarmId,
    pub label: Option<String>,
    pub hour: i8,
    pub minute: i8,
    pub repeat: Repeat,
    pub enabled: bool,
    /// The last instant this alarm went off, so it never fires twice for one
    /// occurrence.
    pub last_fired: Option<Timestamp>,
    /// When the alarm was set, or last switched on.
    ///
    /// Without this, an alarm created at 07:00 for 07:30 would immediately
    /// consider itself overdue — it has never fired, and yesterday's 07:30 is
    /// in the past. Occurrences before the alarm existed are not its business.
    pub armed_at: Timestamp,
}

impl Alarm {
    pub fn new(id: AlarmId, hour: i8, minute: i8, repeat: Repeat, now: Timestamp) -> Self {
        Self {
            id,
            label: None,
            hour: hour.clamp(0, 23),
            minute: minute.clamp(0, 59),
            repeat,
            enabled: true,
            last_fired: None,
            armed_at: now,
        }
    }

    /// Switch the alarm on or off.
    ///
    /// Switching it on re-arms it, so an alarm turned on at 08:00 does not
    /// immediately sound for the 07:30 it slept through.
    pub fn set_enabled(&mut self, enabled: bool, now: Timestamp) {
        self.enabled = enabled;
        if enabled {
            self.armed_at = now;
        }
    }

    pub fn named(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// `7:30 AM`.
    pub fn readout(&self) -> String {
        let h12 = match self.hour % 12 {
            0 => 12,
            other => other,
        };
        let suffix = if self.hour < 12 { "AM" } else { "PM" };
        format!("{h12}:{:02} {suffix}", self.minute)
    }

    /// The next instant this alarm should sound, at or after `after`.
    ///
    /// Searches forward a day at a time rather than computing one, because
    /// repeat rules skip days and daylight-saving transitions make "tomorrow at
    /// this time" a different number of hours away depending on the date.
    pub fn next_after(&self, after: Timestamp, zone: &TimeZone) -> Option<Timestamp> {
        if !self.enabled {
            return None;
        }

        let local = after.to_zoned(zone.clone());
        for offset in 0..8 {
            let day = local
                .date()
                .checked_add(jiff::Span::new().days(offset))
                .ok()?;
            if !self.repeat.includes(day.weekday()) {
                continue;
            }

            // A local time inside a daylight-saving gap is shifted forward
            // rather than skipped, so an alarm set for it still goes off.
            let Ok(when) = day
                .at(self.hour, self.minute, 0, 0)
                .to_zoned(zone.clone())
            else {
                continue;
            };

            let stamp = when.timestamp();
            if stamp > after {
                return Some(stamp);
            }
        }
        None
    }

    /// Whether the alarm should sound now, given it last sounded when it did.
    pub fn due(&self, now: Timestamp, zone: &TimeZone) -> bool {
        if !self.enabled {
            return false;
        }
        // Look backwards from now for the most recent occurrence.
        let Some(previous) = self.most_recent(now, zone) else {
            return false;
        };
        // Later than both the last time it rang and the moment it was armed.
        let floor = match self.last_fired {
            Some(fired) if fired > self.armed_at => fired,
            _ => self.armed_at,
        };
        previous > floor
    }

    fn most_recent(&self, now: Timestamp, zone: &TimeZone) -> Option<Timestamp> {
        let local: Zoned = now.to_zoned(zone.clone());
        for offset in 0..8 {
            let day = local
                .date()
                .checked_sub(jiff::Span::new().days(offset))
                .ok()?;
            if !self.repeat.includes(day.weekday()) {
                continue;
            }
            let Ok(when) = day
                .at(self.hour, self.minute, 0, 0)
                .to_zoned(zone.clone())
            else {
                continue;
            };
            let stamp = when.timestamp();
            if stamp <= now {
                return Some(stamp);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Clock;

    fn zone() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }

    fn at(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> Timestamp {
        Clock::frozen_at(year, month, day, hour, minute, "America/New_York")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
    }

    fn alarm(hour: i8, minute: i8, repeat: Repeat) -> Alarm {
        Alarm::new(AlarmId(1), hour, minute, repeat, at(2026, 9, 18, 6, 0))
    }

    #[test]
    fn it_reads_out_in_twelve_hour_time() {
        assert_eq!(alarm(7, 30, Repeat::Daily).readout(), "7:30 AM");
        assert_eq!(alarm(0, 5, Repeat::Daily).readout(), "12:05 AM");
        assert_eq!(alarm(12, 0, Repeat::Daily).readout(), "12:00 PM");
        assert_eq!(alarm(17, 14, Repeat::Daily).readout(), "5:14 PM");
    }

    #[test]
    fn a_new_alarm_is_not_immediately_overdue() {
        // Set at 07:00 for 07:30: it has never fired, and yesterday's 07:30 is
        // in the past, but it must not go off the moment it is created.
        let alarm = Alarm::new(AlarmId(1), 7, 30, Repeat::Daily, at(2026, 9, 18, 7, 0));
        assert!(!alarm.due(at(2026, 9, 18, 7, 0), &zone()));
        assert!(!alarm.due(at(2026, 9, 18, 7, 29), &zone()));
        assert!(alarm.due(at(2026, 9, 18, 7, 31), &zone()));
    }

    #[test]
    fn switching_an_alarm_on_re_arms_it() {
        // Turned on at 08:00, it must not immediately sound for the 07:30 it
        // was switched off through.
        let mut a = Alarm::new(AlarmId(1), 7, 30, Repeat::Daily, at(2026, 9, 17, 6, 0));
        a.set_enabled(false, at(2026, 9, 18, 6, 0));
        a.set_enabled(true, at(2026, 9, 18, 8, 0));
        assert!(!a.due(at(2026, 9, 18, 8, 1), &zone()));
        assert!(a.due(at(2026, 9, 19, 7, 31), &zone()));
    }

    #[test]
    fn a_daily_alarm_comes_back_tomorrow() {
        let alarm = alarm(7, 30, Repeat::Daily);
        // 18 September 2026 is a Friday, 09:00.
        let next = alarm.next_after(at(2026, 9, 18, 9, 0), &zone()).unwrap();
        assert_eq!(next, at(2026, 9, 19, 7, 30));
    }

    #[test]
    fn it_comes_back_today_if_the_time_has_not_passed() {
        let alarm = alarm(17, 0, Repeat::Daily);
        let next = alarm.next_after(at(2026, 9, 18, 9, 0), &zone()).unwrap();
        assert_eq!(next, at(2026, 9, 18, 17, 0));
    }

    #[test]
    fn a_weekday_alarm_skips_the_weekend() {
        // Friday evening: the next weekday occurrence is Monday.
        let alarm = alarm(7, 30, Repeat::Weekdays);
        let next = alarm.next_after(at(2026, 9, 18, 20, 0), &zone()).unwrap();
        assert_eq!(next, at(2026, 9, 21, 7, 30), "it should have skipped to Monday");
    }

    #[test]
    fn a_weekend_alarm_skips_the_week() {
        let alarm = alarm(9, 0, Repeat::Weekends);
        // Monday: the next weekend occurrence is Saturday.
        let next = alarm.next_after(at(2026, 9, 21, 10, 0), &zone()).unwrap();
        assert_eq!(next, at(2026, 9, 26, 9, 0));
    }

    #[test]
    fn a_disabled_alarm_never_comes_back() {
        let mut alarm = alarm(7, 30, Repeat::Daily);
        alarm.set_enabled(false, at(2026, 9, 18, 6, 0));
        assert!(alarm.next_after(at(2026, 9, 18, 9, 0), &zone()).is_none());
        assert!(!alarm.due(at(2026, 9, 18, 8, 0), &zone()));
    }

    #[test]
    fn it_becomes_due_once_its_time_has_passed() {
        let alarm = alarm(7, 30, Repeat::Daily);
        assert!(!alarm.due(at(2026, 9, 18, 7, 0), &zone()), "too early");
        assert!(alarm.due(at(2026, 9, 18, 7, 31), &zone()));
    }

    #[test]
    fn it_does_not_sound_twice_for_one_occurrence() {
        let mut alarm = alarm(7, 30, Repeat::Daily);
        let now = at(2026, 9, 18, 7, 31);
        assert!(alarm.due(now, &zone()));

        alarm.last_fired = Some(now);
        assert!(!alarm.due(at(2026, 9, 18, 8, 0), &zone()), "it rang twice");

        // But it does come back the next day.
        assert!(alarm.due(at(2026, 9, 19, 7, 31), &zone()));
    }

    #[test]
    fn it_keeps_its_local_time_across_a_daylight_saving_change() {
        // US clocks go back on 1 November 2026. An alarm set for 07:30 must
        // still be 07:30 afterwards, not 06:30.
        let alarm = alarm(7, 30, Repeat::Daily);
        let next = alarm.next_after(at(2026, 10, 31, 9, 0), &zone()).unwrap();

        let local = next.to_zoned(zone());
        assert_eq!(local.hour(), 7, "the alarm drifted across the transition");
        assert_eq!(local.minute(), 30);
    }

    #[test]
    fn a_once_alarm_still_reports_a_next_time_until_it_is_turned_off() {
        // Turning it off is the schedule's job after it sounds; the alarm
        // itself just answers when it would next be due.
        let alarm = alarm(7, 30, Repeat::Once);
        assert!(alarm.next_after(at(2026, 9, 18, 9, 0), &zone()).is_some());
    }
}
