//! Universal search.
//!
//! "There should be one universal search surface." (§13)
//!
//! One field, `Super + Space`, and it disappears the instant an action
//! completes. Search beats menu hunting (rule 5), which means anything the
//! system can do should be reachable by describing it — not only by finding the
//! control that does it.
//!
//! The parser here is deliberately small and literal. A search field that
//! guesses is worse than one that does not, because a wrong guess executed
//! instantly is far more annoying than no match at all.

use crate::app::App;
use detends_paint::Seconds;

/// Something the user asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Go to one of the five places.
    /// Open an app from the dock's set.
    Open(App),
    /// Start a timer of this many seconds, with an optional name.
    Timer {
        seconds: Seconds,
        name: Option<String>,
    },
    /// Begin a Focus session.
    Focus {
        seconds: Option<Seconds>,
        name: Option<String>,
    },
    /// Toggle a system state.
    Airplane(bool),
    /// Open System Center at a particular control.
    Setting(Setting),
    Power(Power),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Wifi,
    Bluetooth,
    Volume,
    Brightness,
    Appearance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Lock,
    Sleep,
    Restart,
    ShutDown,
}

/// Interpret what was typed.
///
/// Returns at most one command: the interface executes and dismisses, so
/// offering a list to choose from would defeat the point.
pub fn parse(query: &str) -> Option<Command> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }

    // Timers, the most useful thing to be able to say out loud.
    if let Some(command) = parse_timer(&q) {
        return Some(command);
    }

    if let Some(rest) = q.strip_prefix("focus") {
        let rest = rest.trim();
        return Some(Command::Focus {
            seconds: parse_duration(rest),
            name: if rest.is_empty() || parse_duration(rest).is_some() {
                None
            } else {
                Some(rest.to_string())
            },
        });
    }

    // Power, which lives in Search rather than in permanent chrome (§19).
    match q.as_str() {
        "lock" => return Some(Command::Power(Power::Lock)),
        "sleep" => return Some(Command::Power(Power::Sleep)),
        "restart" | "reboot" => return Some(Command::Power(Power::Restart)),
        // "quit" and "exit" are the words a person reaches for when they want
        // out, whatever the system calls it. Search is the reliable way out on
        // a platform whose window manager eats Super+Q before it arrives.
        "shut down" | "shutdown" | "power off" | "quit" | "exit" => {
            return Some(Command::Power(Power::ShutDown))
        }
        "airplane" | "airplane mode" | "flight mode" => return Some(Command::Airplane(true)),
        _ => {}
    }

    // Settings, by the name they carry in System Center.
    let setting = match q.as_str() {
        "wi-fi" | "wifi" | "network" => Some(Setting::Wifi),
        "bluetooth" => Some(Setting::Bluetooth),
        "volume" | "sound" => Some(Setting::Volume),
        "brightness" => Some(Setting::Brightness),
        "appearance" | "dark" | "light" | "theme" => Some(Setting::Appearance),
        _ => None,
    };
    if let Some(setting) = setting {
        return Some(Command::Setting(setting));
    }

    // Finally the five places, by prefix.
    // By prefix, so "sp" reaches Spotify and "fi" reaches Files.
    App::ALL
        .iter()
        .copied()
        .find(|app| app.name().to_lowercase().starts_with(&q))
        .map(Command::Open)
}

fn parse_timer(q: &str) -> Option<Command> {
    let rest = q.strip_prefix("timer")?.trim();
    let seconds = parse_duration(rest)?;

    // "timer 20 minutes deep work" names the timer, which is how a timer
    // becomes something you can look at later and understand.
    let name = rest
        .split_whitespace()
        .skip_while(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) || is_unit(w))
        .collect::<Vec<_>>()
        .join(" ");

    Some(Command::Timer {
        seconds,
        name: if name.is_empty() { None } else { Some(name) },
    })
}

fn is_unit(word: &str) -> bool {
    matches!(
        word.trim_end_matches('s'),
        "second" | "sec" | "minute" | "min" | "hour" | "hr" | "h" | "m"
    )
}

/// Read a duration from the head of a phrase: `20 minutes`, `1h`, `90s`.
fn parse_duration(text: &str) -> Option<Seconds> {
    let mut words = text.split_whitespace().peekable();
    let first = words.next()?;

    // "25m" and "90s" — a number with the unit attached.
    let split = first.find(|c: char| !c.is_ascii_digit() && c != '.');
    let (number, attached) = match split {
        Some(at) if at > 0 => (&first[..at], Some(&first[at..])),
        Some(_) => return None,
        None => (first, None),
    };

    let value: f64 = number.parse().ok()?;
    let unit = attached
        .map(str::to_string)
        .or_else(|| words.next().map(str::to_string));

    // Strip a plural "s" only when something is left behind, so the unit "s"
    // (seconds) survives having its own last letter removed.
    let normalized = unit.as_deref().map(|u| {
        let singular = u.trim_end_matches('s');
        if singular.is_empty() {
            u
        } else {
            singular
        }
    });

    let multiplier = match normalized {
        Some("hour") | Some("hr") | Some("h") => 3600.0,
        Some("minute") | Some("min") | Some("m") => 60.0,
        Some("second") | Some("sec") | Some("s") => 1.0,
        // A bare number is minutes: "timer 20" means twenty minutes, because
        // nobody sets a twenty-second timer by saying "twenty".
        None => 60.0,
        // Something that is not a unit — the phrase was not a duration.
        Some(_) => return None,
    };

    let seconds = value * multiplier;
    (seconds > 0.0).then_some(seconds)
}

/// The search field's state.
#[derive(Default)]
pub struct Search {
    query: String,
    open: bool,
}

impl Search {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
    }

    pub fn push(&mut self, c: char) {
        if !c.is_control() {
            self.query.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.query.pop();
    }

    /// What pressing Enter would do.
    pub fn command(&self) -> Option<Command> {
        parse(&self.query)
    }

    /// The placeholder, shown when nothing has been typed (§13).
    pub fn placeholder() -> &'static str {
        "Search détends…"
    }
}

/// Draw the search field.
///
/// "It opens a small centered glass search field." (§13) Small is the operative
/// word: it is a place to say one thing, not a browser.
pub fn draw(
    frame: &mut detends_paint::Frame,
    palette: &detends_paint::Palette,
    query: &str,
    workspace: detends_paint::Vec2,
    progress: f32,
) {
    use detends_paint::{space, text, Align, Glass, Id, Item, Layer, Primitive, Rect, Text, Vec2};

    if progress <= 0.001 {
        return;
    }

    let width = (workspace.x * 0.38).clamp(360.0, 620.0);
    let height = 72.0;

    // Arrives from slightly small and slightly high, settling into place —
    // the same gesture every surface in détends uses to appear.
    let scale = 0.94 + progress * 0.06;
    let rect = Rect::from_center_size(
        Vec2 {
            x: workspace.x * 0.5,
            y: workspace.y * 0.38 - (1.0 - progress) * 14.0,
        },
        Vec2 {
            x: width,
            y: height,
        },
    )
    .scaled(scale);

    frame.push(
        Item::new(
            Id::of("search-field"),
            Layer::Overlay,
            Primitive::Glass(Glass {
                rect,
                radius: 22.0,
                squircle: 5.0,
                thickness: 15.0,
                bevel: 24.0,
                ior: 1.48,
                dispersion: 0.02,
                frost: 0.75,
                tint: palette.glass,
                rim: 1.0,
            }),
        )
        .opacity(progress),
    );

    let showing_placeholder = query.is_empty();
    let label: std::borrow::Cow<'static, str> = if showing_placeholder {
        Search::placeholder().into()
    } else {
        query.to_string().into()
    };

    frame.push(
        Item::new(
            Id::of("search-query"),
            Layer::Overlay,
            Primitive::Text(Text {
                text: label,
                rect: rect.inset(space::ROOM),
                size: text::HEADING.size,
                weight: text::HEADING.weight,
                tracking: text::HEADING.tracking,
                // Centre the single line vertically inside the field.
                line_height: (height - space::ROOM * 2.0) / text::HEADING.size,
                color: if showing_placeholder {
                    palette.text_faint
                } else {
                    palette.text
                },
                align: Align::Left,
            }),
        )
        .opacity(progress)
        .z(1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_examples_from_the_specification_all_work() {
        // §13 lists these by name.
        assert_eq!(
            parse("timer 20 minutes"),
            Some(Command::Timer {
                seconds: 1200.0,
                name: None
            })
        );
        assert_eq!(parse("Wi-Fi"), Some(Command::Setting(Setting::Wifi)));
        assert_eq!(parse("Clock"), Some(Command::Open(App::Clock)));
    }

    #[test]
    fn durations_are_understood_in_the_ways_people_write_them() {
        for (text, expected) in [
            ("timer 25m", 1500.0),
            ("timer 25 min", 1500.0),
            ("timer 25 minutes", 1500.0),
            ("timer 90s", 90.0),
            ("timer 90 sec", 90.0),
            ("timer 90 secs", 90.0),
            ("timer 90 seconds", 90.0),
            ("timer 1h", 3600.0),
            ("timer 1 hour", 3600.0),
            ("timer 1.5 hours", 5400.0),
        ] {
            assert_eq!(
                parse(text),
                Some(Command::Timer {
                    seconds: expected,
                    name: None
                }),
                "failed on {text:?}"
            );
        }
    }

    #[test]
    fn a_bare_number_means_minutes() {
        // Nobody sets a twenty-second timer by saying "timer 20".
        assert_eq!(
            parse("timer 20"),
            Some(Command::Timer {
                seconds: 1200.0,
                name: None
            })
        );
    }

    #[test]
    fn a_timer_can_be_named() {
        assert_eq!(
            parse("timer 25 minutes deep work"),
            Some(Command::Timer {
                seconds: 1500.0,
                name: Some("deep work".into())
            })
        );
    }

    #[test]
    fn nonsense_matches_nothing_rather_than_guessing() {
        // A wrong guess executed instantly is worse than no match.
        for text in [
            "",
            "   ",
            "asdfgh",
            "timer",
            "timer soon",
            "timer many minutes",
        ] {
            assert_eq!(parse(text), None, "{text:?} should not have matched");
        }
    }

    #[test]
    fn a_zero_or_negative_duration_is_not_a_timer() {
        assert_eq!(parse("timer 0"), None);
        assert_eq!(parse("timer 0 minutes"), None);
    }

    #[test]
    fn every_app_is_reachable_by_name() {
        for app in App::ALL {
            assert_eq!(parse(app.name()), Some(Command::Open(app)));
            assert_eq!(parse(&app.name().to_uppercase()), Some(Command::Open(app)));
        }
    }

    #[test]
    fn focus_can_be_started_with_or_without_a_duration() {
        assert_eq!(
            parse("focus"),
            Some(Command::Focus {
                seconds: None,
                name: None
            })
        );
        assert_eq!(
            parse("focus 45 minutes"),
            Some(Command::Focus {
                seconds: Some(2700.0),
                name: None
            })
        );
        assert_eq!(
            parse("focus physics homework"),
            Some(Command::Focus {
                seconds: None,
                name: Some("physics homework".into())
            })
        );
    }

    #[test]
    fn power_lives_in_search_not_in_permanent_chrome() {
        // §19: the power controls have no permanent home, so Search has to be
        // able to reach all four of them.
        assert_eq!(parse("lock"), Some(Command::Power(Power::Lock)));
        assert_eq!(parse("sleep"), Some(Command::Power(Power::Sleep)));
        assert_eq!(parse("restart"), Some(Command::Power(Power::Restart)));
        assert_eq!(parse("shut down"), Some(Command::Power(Power::ShutDown)));
        assert_eq!(parse("quit"), Some(Command::Power(Power::ShutDown)));
        assert_eq!(parse("exit"), Some(Command::Power(Power::ShutDown)));
    }

    #[test]
    fn settings_are_reachable_by_their_own_names() {
        assert_eq!(
            parse("bluetooth"),
            Some(Command::Setting(Setting::Bluetooth))
        );
        assert_eq!(parse("volume"), Some(Command::Setting(Setting::Volume)));
        assert_eq!(
            parse("brightness"),
            Some(Command::Setting(Setting::Brightness))
        );
        assert_eq!(parse("airplane"), Some(Command::Airplane(true)));
    }

    #[test]
    fn the_field_clears_itself_when_it_opens_and_closes() {
        // §13: it disappears immediately after completing an action — and must
        // not still be holding the last query when it comes back.
        let mut s = Search::default();
        assert!(!s.is_open());

        s.open();
        for c in "timer 20".chars() {
            s.push(c);
        }
        assert!(s.command().is_some());

        s.close();
        assert!(!s.is_open());
        assert_eq!(s.query(), "");

        s.open();
        assert_eq!(s.query(), "", "the old query came back");
    }

    #[test]
    fn typing_and_correcting_behave_normally() {
        let mut s = Search::default();
        s.open();
        for c in "mail".chars() {
            s.push(c);
        }
        assert_eq!(s.command(), Some(Command::Open(App::Mail)));

        s.backspace();
        s.backspace();
        assert_eq!(s.query(), "ma");
        assert_eq!(s.command(), Some(Command::Open(App::Mail)));

        // Backspacing past the start is harmless.
        for _ in 0..10 {
            s.backspace();
        }
        assert_eq!(s.query(), "");
        assert_eq!(s.command(), None);
    }

    #[test]
    fn control_characters_never_enter_the_query() {
        let mut s = Search::default();
        s.open();
        s.push('\n');
        s.push('\t');
        s.push('a');
        assert_eq!(s.query(), "a");
    }
}
