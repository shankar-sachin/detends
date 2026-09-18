//! The wall clock.

use jiff::{tz::TimeZone, Timestamp, Zoned};

/// A reading of the wall clock, already resolved to a zone.
///
/// Carried as a value rather than fetched on demand so that a frame's clock,
/// its alarms and its status cluster all agree on what time it is — and so
/// every one of them can be tested at an arbitrary instant.
#[derive(Clone, Debug)]
pub struct TimeOfDay {
    zoned: Zoned,
}

impl TimeOfDay {
    pub fn new(zoned: Zoned) -> Self {
        Self { zoned }
    }

    pub fn hour(&self) -> i8 {
        self.zoned.hour()
    }

    pub fn minute(&self) -> i8 {
        self.zoned.minute()
    }

    pub fn second(&self) -> i8 {
        self.zoned.second()
    }

    /// `5:14`, twelve-hour with no leading zero.
    ///
    /// The cluster shows the time at a glance, so it is as short as it can be
    /// while staying unambiguous — no seconds, no padding, no AM/PM.
    pub fn short(&self) -> String {
        let h = self.zoned.hour();
        let h12 = match h % 12 {
            0 => 12,
            other => other,
        };
        format!("{h12}:{:02}", self.zoned.minute())
    }

    /// `5:14 PM`, for System Center, where there is room to be explicit.
    pub fn with_meridiem(&self) -> String {
        let suffix = if self.zoned.hour() < 12 { "AM" } else { "PM" };
        format!("{} {suffix}", self.short())
    }

    /// `Thursday, September 17`.
    pub fn long_date(&self) -> String {
        const DAYS: [&str; 7] = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];

        let weekday = DAYS[(self.zoned.weekday().to_monday_zero_offset()) as usize % 7];
        let month = MONTHS[(self.zoned.month() as usize - 1) % 12];
        format!("{weekday}, {month} {}", self.zoned.day())
    }

    /// Fraction of the way through the day, 0 to 1.
    ///
    /// The environment can lean on this later so the workspace warms and cools
    /// with the hour without anybody configuring anything.
    pub fn day_fraction(&self) -> f32 {
        let seconds = self.zoned.hour() as f32 * 3600.0
            + self.zoned.minute() as f32 * 60.0
            + self.zoned.second() as f32;
        seconds / 86_400.0
    }

    pub fn zoned(&self) -> &Zoned {
        &self.zoned
    }
}

/// Reads the wall clock.
///
/// An explicit object rather than a free function so tests can pin it to an
/// instant, which is what makes alarm and timer behaviour testable at all.
#[derive(Clone, Debug)]
pub struct Clock {
    zone: TimeZone,
    /// When set, the clock always reports this instant.
    pinned: Option<Timestamp>,
}

impl Clock {
    /// The system clock in the local zone.
    pub fn system() -> Self {
        Self {
            zone: TimeZone::system(),
            pinned: None,
        }
    }

    /// A clock in a named zone, e.g. `"Europe/Paris"`.
    pub fn in_zone(name: &str) -> Option<Self> {
        TimeZone::get(name)
            .ok()
            .map(|zone| Self { zone, pinned: None })
    }

    /// A clock frozen at an instant, for tests.
    pub fn pinned(zone: TimeZone, at: Timestamp) -> Self {
        Self {
            zone,
            pinned: Some(at),
        }
    }

    /// A clock frozen at a named local time.
    ///
    /// Public rather than test-only so that nothing downstream has to depend on
    /// `jiff` merely to pin a clock — keeping the date library an implementation
    /// detail of this crate is most of the reason this crate exists.
    ///
    /// Returns `None` only for an unknown zone.
    ///
    /// A local time inside a daylight-saving gap is *shifted forward* rather
    /// than rejected — 02:30 on a spring-forward morning resolves to 03:30.
    /// That is deliberate: it is what a person setting an alarm expects, and
    /// the alternative is an alarm that silently does not exist once a year.
    pub fn frozen_at(
        year: i16,
        month: i8,
        day: i8,
        hour: i8,
        minute: i8,
        zone: &str,
    ) -> Option<Self> {
        let tz = TimeZone::get(zone).ok()?;
        let stamp = jiff::civil::date(year, month, day)
            .at(hour, minute, 0, 0)
            .to_zoned(tz.clone())
            .ok()?
            .timestamp();
        Some(Self {
            zone: tz,
            pinned: Some(stamp),
        })
    }

    pub fn now(&self) -> TimeOfDay {
        let stamp = self.pinned.unwrap_or_else(Timestamp::now);
        TimeOfDay::new(stamp.to_zoned(self.zone.clone()))
    }

    /// The same instant, read in another zone — the basis of World Clock.
    pub fn now_in(&self, zone: &TimeZone) -> TimeOfDay {
        let stamp = self.pinned.unwrap_or_else(Timestamp::now);
        TimeOfDay::new(stamp.to_zoned(zone.clone()))
    }

    pub fn zone(&self) -> &TimeZone {
        &self.zone
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i16, m: i8, d: i8, h: i8, min: i8, zone: &str) -> Clock {
        Clock::frozen_at(y, m, d, h, min, zone).expect("a valid instant")
    }

    #[test]
    fn the_short_time_matches_the_specification() {
        // §10 shows "5:14" — no leading zero, no seconds, no meridiem.
        let c = at(2026, 9, 17, 17, 14, "America/New_York");
        assert_eq!(c.now().short(), "5:14");
        assert_eq!(c.now().with_meridiem(), "5:14 PM");
    }

    #[test]
    fn midnight_and_noon_read_as_twelve_not_zero() {
        assert_eq!(at(2026, 9, 17, 0, 5, "UTC").now().short(), "12:05");
        assert_eq!(
            at(2026, 9, 17, 0, 5, "UTC").now().with_meridiem(),
            "12:05 AM"
        );
        assert_eq!(at(2026, 9, 17, 12, 0, "UTC").now().short(), "12:00");
        assert_eq!(
            at(2026, 9, 17, 12, 0, "UTC").now().with_meridiem(),
            "12:00 PM"
        );
    }

    #[test]
    fn minutes_are_always_two_digits() {
        assert_eq!(at(2026, 9, 17, 9, 3, "UTC").now().short(), "9:03");
    }

    #[test]
    fn the_long_date_matches_the_specification() {
        // §10 shows "Thursday, September 17".
        let c = at(2026, 9, 17, 17, 14, "America/New_York");
        assert_eq!(c.now().long_date(), "Thursday, September 17");
    }

    #[test]
    fn world_clock_reads_one_instant_in_many_zones() {
        // The property that makes World Clock correct rather than five clocks:
        // every zone shows the *same moment*, not a separately-read time.
        let c = at(2026, 9, 17, 12, 0, "UTC");
        let paris = TimeZone::get("Europe/Paris").unwrap();
        let tokyo = TimeZone::get("Asia/Tokyo").unwrap();

        assert_eq!(c.now().short(), "12:00");
        assert_eq!(c.now_in(&paris).short(), "2:00"); // UTC+2 in September
        assert_eq!(c.now_in(&tokyo).short(), "9:00"); // UTC+9

        // Same underlying instant in all three.
        assert_eq!(
            c.now().zoned().timestamp(),
            c.now_in(&tokyo).zoned().timestamp()
        );
    }

    #[test]
    fn a_zone_that_does_not_exist_is_rejected_rather_than_guessed() {
        assert!(Clock::in_zone("Europe/Paris").is_some());
        assert!(Clock::in_zone("Mars/Olympus_Mons").is_none());
        assert!(Clock::frozen_at(2026, 9, 17, 12, 0, "Mars/Olympus_Mons").is_none());
    }

    #[test]
    fn a_time_inside_a_daylight_saving_gap_shifts_forward() {
        // US clocks jump 02:00 -> 03:00 on 8 March 2026, so 02:30 never
        // happens. An alarm set for then should still go off, at the first
        // moment that does exist — never vanish for the year.
        let shifted = at(2026, 3, 8, 2, 30, "America/New_York");
        assert_eq!(shifted.now().short(), "3:30");

        // Times either side are untouched.
        assert_eq!(
            at(2026, 3, 8, 1, 30, "America/New_York").now().short(),
            "1:30"
        );
        assert_eq!(
            at(2026, 3, 8, 3, 30, "America/New_York").now().short(),
            "3:30"
        );
    }

    #[test]
    fn a_time_that_happens_twice_resolves_to_the_first() {
        // The autumn fold: 01:30 occurs twice on 1 November 2026. Taking the
        // earlier one means an alarm fires once, on the first pass.
        assert_eq!(
            at(2026, 11, 1, 1, 30, "America/New_York").now().short(),
            "1:30"
        );
    }

    #[test]
    fn the_day_fraction_runs_from_midnight_to_midnight() {
        assert!(at(2026, 9, 17, 0, 0, "UTC").now().day_fraction() < 0.001);
        assert!((at(2026, 9, 17, 12, 0, "UTC").now().day_fraction() - 0.5).abs() < 0.001);
        assert!(at(2026, 9, 17, 23, 59, "UTC").now().day_fraction() > 0.999);
    }

    #[test]
    fn a_pinned_clock_does_not_move() {
        // What makes alarms and timers testable without waiting.
        let c = at(2026, 9, 17, 17, 14, "UTC");
        let first = c.now().short();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(c.now().short(), first);
    }

    #[test]
    fn the_system_clock_is_readable() {
        // Smoke test: whatever zone this machine is in, it must resolve.
        let now = Clock::system().now();
        assert!((0..24).contains(&now.hour()));
        assert!(!now.long_date().is_empty());
    }
}
