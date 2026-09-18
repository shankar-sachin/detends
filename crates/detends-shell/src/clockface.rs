//! Clock.
//!
//! "Clock combines all time-related utilities… Do not make these feel like five
//! separate applications." (§5)
//!
//! So it is one surface. The time stays dominant, as it is on Home; the four
//! utilities sit beneath it as a single row, and whichever is in use opens
//! below that. There are no tabs, no panes and no chrome — moving between them
//! changes what is written under the clock and nothing else.

use detends_paint::{
    space, text, Align, Color, Frame, Icon, IconShape, Id, Item, Layer, Palette, Primitive, Rect,
    Seconds, Spring, Text, Vec2, ICON_STROKE,
};
use detends_time::{format_duration, Schedule, TimeOfDay};
use jiff::{tz::TimeZone, Timestamp};

/// Which utility is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Timers,
    Alarms,
    Stopwatch,
    World,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Timers,
        Section::Alarms,
        Section::Stopwatch,
        Section::World,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Section::Timers => "Timers",
            Section::Alarms => "Alarms",
            Section::Stopwatch => "Stopwatch",
            Section::World => "World",
        }
    }

    pub fn icon(self) -> IconShape {
        match self {
            Section::Timers => IconShape::Timer,
            Section::Alarms => IconShape::Alarm,
            Section::Stopwatch => IconShape::Stopwatch,
            Section::World => IconShape::Globe,
        }
    }
}

/// Icon size in the section row.
const TAB_ICON: f32 = 26.0;
const TAB_PITCH: f32 = 116.0;

/// Where the four utilities sit, for drawing and for hit-testing alike.
pub fn tabs(area: Rect) -> [(Section, Rect); 4] {
    // Close under the date, so the clock and its utilities read as one
    // composition rather than as two things with a hole between them.
    let y = area.center.y - area.height() * 0.17;
    let pitch = TAB_PITCH.min(area.width() / 5.0);
    let span = pitch * 3.0;
    let start = area.center.x - span * 0.5;

    let mut out = [(Section::Timers, Rect::ZERO); 4];
    for (index, section) in Section::ALL.iter().enumerate() {
        out[index] = (
            *section,
            Rect::from_center_size(
                Vec2 { x: start + index as f32 * pitch, y },
                Vec2::splat(TAB_ICON.min(pitch * 0.34)),
            ),
        );
    }
    out
}

/// Which utility a point lands on.
pub fn hit(area: Rect, at: Vec2) -> Option<Section> {
    tabs(area)
        .iter()
        .find(|(_, rect)| {
            Rect::from_center_size(
                rect.center,
                Vec2 { x: rect.width() + space::WIDE, y: rect.height() + space::OPEN },
            )
            .contains(at)
        })
        .map(|(section, _)| *section)
}

pub struct ClockFace {
    section: usize,
    cursor: Spring<f32>,
}

impl Default for ClockFace {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockFace {
    pub fn new() -> Self {
        Self {
            section: 0,
            cursor: Spring::new(detends_paint::springs::SNAP, 0.0),
        }
    }

    pub fn section(&self) -> Section {
        Section::ALL[self.section.min(3)]
    }

    pub fn select(&mut self, now: Seconds, section: Section) {
        if let Some(index) = Section::ALL.iter().position(|s| *s == section) {
            self.section = index;
            self.cursor.target(now, index as f32);
        }
    }

    pub fn step(&mut self, now: Seconds, delta: i32) {
        let next = (self.section as i32 + delta).clamp(0, 3) as usize;
        if next != self.section {
            self.section = next;
            self.cursor.target(now, next as f32);
        }
    }

    pub fn settled(&self, now: Seconds) -> bool {
        self.cursor.at_rest(now)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        frame: &mut Frame,
        palette: &Palette,
        area: Rect,
        time: &TimeOfDay,
        schedule: &Schedule,
        stamp: Timestamp,
        zone: &TimeZone,
        now: Seconds,
        opacity: f32,
        scale: f32,
    ) {
        let center = area.center;
        let about = |p: Vec2| Vec2 {
            x: center.x + (p.x - center.x) * scale,
            y: center.y + (p.y - center.y) * scale,
        };

        let write = |frame: &mut Frame,
                         id: Id,
                         label: std::borrow::Cow<'static, str>,
                         at: Vec2,
                         width: f32,
                         style: detends_paint::TextStyle,
                         color: Color,
                         align: Align| {
            frame.push(
                Item::new(
                    id,
                    Layer::Content,
                    Primitive::Text(Text {
                        text: label,
                        rect: Rect::from_center_size(
                            about(at),
                            Vec2 { x: width, y: style.size * 2.0 },
                        ),
                        size: style.size * scale,
                        weight: style.weight,
                        tracking: style.tracking,
                        line_height: style.line_height,
                        color,
                        align,
                    }),
                )
                .opacity(opacity)
                .z(2),
            );
        };

        // The time, as dominant here as it is on Home.
        let display = text::DISPLAY.scaled(0.58);
        write(
            frame,
            Id::of("clock-time"),
            time.short().into(),
            Vec2 { x: center.x, y: area.min().y + area.height() * 0.16 },
            area.width() * 0.9,
            display,
            palette.text,
            Align::Center,
        );
        write(
            frame,
            Id::of("clock-date"),
            time.long_date().into(),
            Vec2 {
                x: center.x,
                y: area.min().y + area.height() * 0.16 + display.size * 0.62,
            },
            area.width() * 0.9,
            text::HEADING,
            palette.text_faint,
            Align::Center,
        );

        // The four utilities, as one quiet row.
        let cursor = self.cursor.value(now);
        for (index, (section, rect)) in tabs(area).iter().enumerate() {
            let near = (1.0 - (cursor - index as f32).abs()).clamp(0.0, 1.0);

            frame.push(
                Item::new(
                    Id::of("clock-tab").nth(index as u64),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(
                            about(rect.center),
                            rect.size() * (scale * (1.0 + near * 0.06)),
                        ),
                        shape: section.icon(),
                        stroke: ICON_STROKE,
                        color: palette.text_faint.lerp(palette.text, near),
                        rim: 0.25 + near * 0.5,
                    }),
                )
                .opacity(opacity)
                .z(3),
            );

            write(
                frame,
                Id::of("clock-tab-label").nth(index as u64),
                section.name().into(),
                Vec2 { x: rect.center.x, y: rect.center.y + rect.half.y + space::ROOM },
                TAB_PITCH,
                text::CAPTION,
                palette
                    .text_faint
                    .lerp(palette.text_soft, near)
                    .fade(0.4 + near * 0.6),
                Align::Center,
            );
        }

        // Whatever is open, written underneath.
        let body_top = area.center.y - area.height() * 0.04;
        let rows = self.body(schedule, stamp, zone);

        if rows.is_empty() {
            write(
                frame,
                Id::of("clock-empty"),
                self.empty_line().into(),
                Vec2 { x: center.x, y: body_top + space::OPEN },
                area.width() * 0.7,
                text::BODY,
                palette.text_faint,
                Align::Center,
            );
            return;
        }

        for (index, (lead, trail, emphasis)) in rows.iter().enumerate() {
            let y = body_top + space::OPEN + index as f32 * space::WIDE;
            let width = (area.width() * 0.32).clamp(280.0, 460.0);
            let style = if *emphasis { text::HEADING } else { text::BODY };

            write(
                frame,
                Id::of("clock-row-lead").nth(index as u64),
                lead.clone().into(),
                Vec2 { x: center.x - width * 0.25, y },
                width * 0.5,
                style,
                if *emphasis { palette.text } else { palette.text_soft },
                Align::Left,
            );
            write(
                frame,
                Id::of("clock-row-trail").nth(index as u64),
                trail.clone().into(),
                Vec2 { x: center.x + width * 0.25, y },
                width * 0.5,
                style,
                if *emphasis { palette.text_soft } else { palette.text_faint },
                Align::Right,
            );
        }
    }

    fn empty_line(&self) -> &'static str {
        match self.section() {
            Section::Timers => "No timers",
            Section::Alarms => "No alarms",
            Section::Stopwatch => "Ready",
            Section::World => "No cities",
        }
    }

    /// The rows the open utility shows: a label, a value, and whether it is the
    /// one that matters.
    fn body(
        &self,
        schedule: &Schedule,
        stamp: Timestamp,
        zone: &TimeZone,
    ) -> Vec<(String, String, bool)> {
        match self.section() {
            Section::Timers => schedule
                .running_timers(stamp)
                .iter()
                .take(5)
                .enumerate()
                .map(|(index, timer)| {
                    (
                        timer.name.clone().unwrap_or_else(|| "Timer".into()),
                        timer.readout(stamp),
                        // The one finishing soonest is the one being waited on.
                        index == 0,
                    )
                })
                .collect(),

            Section::Alarms => {
                let next = schedule.next_alarm(stamp, zone).map(|(a, _)| a.id);
                schedule
                    .alarms
                    .iter()
                    .take(5)
                    .map(|alarm| {
                        let label = alarm
                            .label
                            .clone()
                            .unwrap_or_else(|| alarm.repeat.label().to_string());
                        let value = if alarm.enabled {
                            alarm.readout()
                        } else {
                            format!("{}  ·  off", alarm.readout())
                        };
                        (label, value, Some(alarm.id) == next)
                    })
                    .collect()
            }

            Section::Stopwatch => {
                let mut rows = vec![(
                    if schedule.stopwatch.is_running() {
                        "Running".to_string()
                    } else {
                        "Stopped".to_string()
                    },
                    schedule.stopwatch.readout(stamp),
                    true,
                )];
                // Most recent lap first — the one just recorded is the one
                // being looked at.
                for (index, split) in schedule.stopwatch.splits().iter().enumerate().rev().take(4) {
                    rows.push((format!("Lap {}", index + 1), format_duration(*split), false));
                }
                rows
            }

            Section::World => schedule
                .world
                .iter()
                .take(5)
                .filter_map(|city| {
                    let zone = city.time_zone()?;
                    let there = TimeOfDay::new(stamp.to_zoned(zone));
                    // With the meridiem, not without. The whole point of a
                    // world clock is knowing whether it is a reasonable hour
                    // to call someone, and "1:18" does not answer that.
                    Some((city.label.clone(), there.with_meridiem(), false))
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;
    use detends_time::{Clock, Repeat, WorldClock};

    fn area() -> Rect {
        Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0)).inset(42.0)
    }

    fn zone() -> TimeZone {
        TimeZone::get("UTC").unwrap()
    }

    fn stamp(hour: i8, minute: i8) -> Timestamp {
        Clock::frozen_at(2026, 9, 18, hour, minute, "UTC")
            .unwrap()
            .now()
            .zoned()
            .timestamp()
    }

    fn render(face: &ClockFace, schedule: &Schedule, at: Timestamp) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let time = TimeOfDay::new(at.to_zoned(zone()));
        face.draw(
            &mut frame,
            &Palette::dark(),
            area(),
            &time,
            schedule,
            at,
            &zone(),
            2.0,
            1.0,
            1.0,
        );
        frame
    }

    fn texts(frame: &Frame) -> Vec<String> {
        frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn it_offers_all_four_utilities_on_one_surface() {
        // §5: one environment, not five applications.
        let frame = render(&ClockFace::new(), &Schedule::new(), stamp(9, 27));
        let s = texts(&frame);
        for section in Section::ALL {
            assert!(s.iter().any(|t| t == section.name()), "missing {section:?}");
        }
        // And the time still dominates.
        assert!(s.iter().any(|t| t == "9:27"));
    }

    #[test]
    fn every_utility_has_its_own_icon() {
        let frame = render(&ClockFace::new(), &Schedule::new(), stamp(9, 0));
        let icons: Vec<_> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Icon(icon) => Some(icon.shape),
                _ => None,
            })
            .collect();
        for section in Section::ALL {
            assert!(icons.contains(&section.icon()), "no icon for {section:?}");
        }
    }

    #[test]
    fn running_timers_are_listed_soonest_first() {
        let mut schedule = Schedule::new();
        schedule.start_timer(stamp(9, 0), 600.0, Some("Long".into()));
        schedule.start_timer(stamp(9, 0), 120.0, Some("Bread".into()));

        let s = texts(&render(&ClockFace::new(), &schedule, stamp(9, 1)));
        let bread = s.iter().position(|t| t == "Bread").expect("Bread");
        let long = s.iter().position(|t| t == "Long").expect("Long");
        assert!(bread < long, "the soonest timer should be first");
        assert!(s.iter().any(|t| t == "1:00"), "no countdown shown: {s:?}");
    }

    #[test]
    fn an_empty_utility_says_so_rather_than_showing_nothing() {
        let s = texts(&render(&ClockFace::new(), &Schedule::new(), stamp(9, 0)));
        assert!(s.iter().any(|t| t == "No timers"), "got {s:?}");
    }

    #[test]
    fn alarms_show_their_time_and_repeat() {
        let mut face = ClockFace::new();
        face.select(0.0, Section::Alarms);

        let mut schedule = Schedule::new();
        schedule.add_alarm(stamp(6, 0), 7, 30, Repeat::Weekdays);

        let s = texts(&render(&face, &schedule, stamp(9, 0)));
        assert!(s.iter().any(|t| t == "7:30 AM"), "got {s:?}");
        assert!(s.iter().any(|t| t == "Weekdays"));
    }

    #[test]
    fn a_switched_off_alarm_says_it_is_off() {
        let mut face = ClockFace::new();
        face.select(0.0, Section::Alarms);

        let mut schedule = Schedule::new();
        let id = schedule.add_alarm(stamp(6, 0), 7, 30, Repeat::Daily);
        schedule.alarm_mut(id).unwrap().set_enabled(false, stamp(6, 0));

        let s = texts(&render(&face, &schedule, stamp(9, 0)));
        assert!(s.iter().any(|t| t.contains("off")), "got {s:?}");
    }

    #[test]
    fn the_stopwatch_shows_its_reading_and_laps() {
        let mut face = ClockFace::new();
        face.select(0.0, Section::Stopwatch);

        let mut schedule = Schedule::new();
        schedule.stopwatch.start(stamp(9, 0));
        schedule.stopwatch.lap(stamp(9, 0) + jiff::SignedDuration::from_secs(12));

        let s = texts(&render(&face, &schedule, stamp(9, 1)));
        assert!(s.iter().any(|t| t == "Running"), "got {s:?}");
        assert!(s.iter().any(|t| t.starts_with("Lap 1")));
    }

    #[test]
    fn the_world_clock_reads_one_instant_in_many_cities() {
        let mut face = ClockFace::new();
        face.select(0.0, Section::World);

        let mut schedule = Schedule::new();
        schedule.world.push(WorldClock::new("Europe/Paris", "Paris"));
        schedule.world.push(WorldClock::new("Asia/Tokyo", "Tokyo"));

        // 12:00 UTC.
        let s = texts(&render(&face, &schedule, stamp(12, 0)));
        assert!(s.iter().any(|t| t == "Paris"));
        assert!(s.iter().any(|t| t == "2:00 PM"), "Paris is UTC+2 here: {s:?}");
        assert!(s.iter().any(|t| t == "9:00 PM"), "Tokyo is UTC+9: {s:?}");
    }

    #[test]
    fn world_clock_times_say_whether_it_is_morning_or_evening() {
        // A world clock exists to tell you whether it is a reasonable hour
        // somewhere. "1:18" does not answer that question.
        let mut face = ClockFace::new();
        face.select(0.0, Section::World);

        let mut schedule = Schedule::new();
        schedule.world.push(WorldClock::new("Asia/Tokyo", "Tokyo"));

        let s = texts(&render(&face, &schedule, stamp(16, 0)));
        assert!(
            s.iter().any(|t| t.contains("AM") || t.contains("PM")),
            "ambiguous world clock: {s:?}"
        );
    }

    #[test]
    fn a_city_with_an_unknown_zone_is_skipped_rather_than_breaking_the_list() {
        let mut face = ClockFace::new();
        face.select(0.0, Section::World);

        let mut schedule = Schedule::new();
        schedule.world.push(WorldClock::new("Mars/Olympus", "Olympus"));
        schedule.world.push(WorldClock::new("Europe/Paris", "Paris"));

        let s = texts(&render(&face, &schedule, stamp(12, 0)));
        assert!(s.iter().any(|t| t == "Paris"), "the good city was lost too");
        assert!(!s.iter().any(|t| t == "Olympus"));
    }

    #[test]
    fn the_utilities_can_be_pressed() {
        for (section, rect) in tabs(area()) {
            assert_eq!(hit(area(), rect.center), Some(section));
        }
        assert_eq!(hit(area(), vec2(20.0, 20.0)), None);
    }

    #[test]
    fn the_selection_moves_and_stops_at_the_ends() {
        let mut face = ClockFace::new();
        assert_eq!(face.section(), Section::Timers);

        face.step(0.0, -1);
        assert_eq!(face.section(), Section::Timers, "it wrapped");

        for _ in 0..9 {
            face.step(0.0, 1);
        }
        assert_eq!(face.section(), Section::World);
    }
}
