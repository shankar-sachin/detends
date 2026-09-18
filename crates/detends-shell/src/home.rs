//! Home.
//!
//! Where the shell lands after boot, and what you return to. The time, and the
//! five places beneath it.
//!
//! The specification asks for no dock and no taskbar (§16), and this is not
//! one: it is a destination rather than permanent chrome, and it is gone the
//! moment you enter a mode. But something has to show you where you can go.
//! Five keyboard shortcuts nobody announced are not an interface — they are a
//! system that appears to do nothing.

use crate::mode::Mode;
use detends_paint::{
    space, text, Align, Frame, Icon, Id, Item, Layer, Palette, Primitive, Rect, Seconds,
    Spring, Text, Vec2, ICON_STROKE,
};
use detends_time::TimeOfDay;

/// Size of an icon on Home, in logical units.
const ICON: f32 = 58.0;

/// Centre-to-centre spacing between them.
const PITCH: f32 = 128.0;

/// Where each place sits.
///
/// One function, used by both drawing and hit-testing, so what you see and what
/// you can click can never drift apart.
pub fn layout(size: Vec2) -> [(Mode, Rect); 5] {
    // Close enough to the clock that the two read as one composition. Pushed
    // further apart they become two separate things with a hole between them.
    let row_y = size.y * 0.5 + size.y * 0.13;
    let pitch = PITCH.min(size.x / 6.0);
    let span = pitch * 4.0;
    let start = size.x * 0.5 - span * 0.5;

    let mut places = [(Mode::Music, Rect::ZERO); 5];
    for (index, mode) in Mode::ALL.iter().enumerate() {
        let center = Vec2 { x: start + index as f32 * pitch, y: row_y };
        places[index] = (
            *mode,
            Rect::from_center_size(center, Vec2::splat(ICON.min(pitch * 0.62))),
        );
    }
    places
}

/// The larger area a click counts as hitting — the icon plus its label.
///
/// A 58px target is too small to aim at comfortably; the whole cell is the
/// button, even though only the icon is drawn.
pub fn hit_area(icon: Rect) -> Rect {
    Rect::from_center_size(
        Vec2 { x: icon.center.x, y: icon.center.y + space::STEP },
        Vec2 { x: icon.width() + space::WIDE, y: icon.height() + space::WIDE },
    )
}

/// Which place a point lands on, if any.
pub fn hit(size: Vec2, at: Vec2) -> Option<Mode> {
    layout(size)
        .iter()
        .find(|(_, rect)| hit_area(*rect).contains(at))
        .map(|(mode, _)| *mode)
}

pub struct Home {
    /// Which place the keyboard is on.
    selected: usize,
    /// Animates toward `selected`, so the highlight slides rather than jumps.
    cursor: Spring<f32>,
}

impl Default for Home {
    fn default() -> Self {
        Self::new()
    }
}

impl Home {
    pub fn new() -> Self {
        Self {
            selected: 0,
            cursor: Spring::new(detends_paint::springs::SNAP, 0.0),
        }
    }

    pub fn selected(&self) -> Mode {
        Mode::ALL[self.selected.min(4)]
    }

    pub fn select(&mut self, now: Seconds, mode: Mode) {
        if let Some(index) = Mode::ALL.iter().position(|m| *m == mode) {
            self.selected = index;
            self.cursor.target(now, index as f32);
        }
    }

    /// Move the selection, stopping at the ends rather than wrapping.
    ///
    /// Wrapping saves a keystroke and costs the user their place: with five
    /// items you always know where the ends are, and landing back at the start
    /// after pressing right reads as a glitch.
    pub fn step(&mut self, now: Seconds, delta: i32) {
        let next = (self.selected as i32 + delta).clamp(0, 4) as usize;
        if next != self.selected {
            self.selected = next;
            self.cursor.target(now, next as f32);
        }
    }

    pub fn settled(&self, now: Seconds) -> bool {
        self.cursor.at_rest(now)
    }

    pub fn draw(
        &self,
        frame: &mut Frame,
        palette: &Palette,
        time: &TimeOfDay,
        now: Seconds,
        opacity: f32,
        scale: f32,
    ) {
        let size = frame.size;
        let center = Vec2 { x: size.x * 0.5, y: size.y * 0.5 };

        let about = |p: Vec2| Vec2 {
            x: center.x + (p.x - center.x) * scale,
            y: center.y + (p.y - center.y) * scale,
        };

        // The time dominates, as it does in Clock. Home is a calm place to
        // land, not a launcher grid.
        let display = text::DISPLAY.scaled(0.72 * scale);
        frame.push(
            Item::new(
                Id::of("home-time"),
                Layer::Content,
                Primitive::Text(Text {
                    text: time.short().into(),
                    rect: Rect::from_center_size(
                        about(Vec2 { x: center.x, y: center.y - size.y * 0.13 }),
                        Vec2 { x: size.x * 0.9, y: display.size * 1.3 },
                    ),
                    size: display.size,
                    weight: display.weight,
                    tracking: display.tracking,
                    line_height: display.line_height,
                    color: palette.text,
                    align: Align::Center,
                }),
            )
            .opacity(opacity)
            .z(2),
        );

        let heading = text::HEADING;
        frame.push(
            Item::new(
                Id::of("home-date"),
                Layer::Content,
                Primitive::Text(Text {
                    text: time.long_date().into(),
                    rect: Rect::from_center_size(
                        about(Vec2 {
                            x: center.x,
                            y: center.y - size.y * 0.13 + display.size * 0.62,
                        }),
                        Vec2 { x: size.x * 0.9, y: heading.size * 2.0 },
                    ),
                    size: heading.size * scale,
                    weight: heading.weight,
                    tracking: heading.tracking,
                    line_height: heading.line_height,
                    color: palette.text_faint,
                    align: Align::Center,
                }),
            )
            .opacity(opacity)
            .z(2),
        );

        // The five places.
        let cursor = self.cursor.value(now);
        let caption = text::CAPTION;

        for (index, (mode, rect)) in layout(size).iter().enumerate() {
            // Nearness to the cursor, so the highlight is continuous while it
            // travels rather than jumping from one icon to the next.
            let nearness = (1.0 - (cursor - index as f32).abs()).clamp(0.0, 1.0);

            let icon_rect = Rect::from_center_size(
                about(rect.center),
                rect.size() * (scale * (1.0 + nearness * 0.08)),
            );

            frame.push(
                Item::new(
                    Id::of("home-icon").nth(index as u64),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: icon_rect,
                        shape: mode.icon(),
                        stroke: ICON_STROKE,
                        color: palette.text_faint.lerp(palette.text, nearness),
                        // The selected icon catches more light, which is a
                        // quieter way to say "this one" than a box around it.
                        rim: 0.35 + nearness * 0.55,
                    }),
                )
                .opacity(opacity)
                .z(3),
            );

            frame.push(
                Item::new(
                    Id::of("home-label").nth(index as u64),
                    Layer::Content,
                    Primitive::Text(Text {
                        text: mode.name().into(),
                        rect: Rect::from_center_size(
                            about(Vec2 {
                                x: rect.center.x,
                                y: rect.center.y + rect.half.y + space::ROOM + space::TIGHT,
                            }),
                            Vec2 { x: PITCH, y: caption.size * 2.2 },
                        ),
                        size: caption.size * scale,
                        weight: caption.weight,
                        tracking: caption.tracking,
                        line_height: caption.line_height,
                        color: palette
                            .text_faint
                            .lerp(palette.text_soft, nearness)
                            .fade(0.45 + nearness * 0.55),
                        align: Align::Center,
                    }),
                )
                .opacity(opacity)
                .z(3),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;
    use detends_time::Clock;

    const SCREEN: Vec2 = Vec2 { x: 1512.0, y: 982.0 };

    fn render(home: &Home, now: Seconds) -> Frame {
        let mut frame = Frame::new(SCREEN, 2.0);
        let time = Clock::frozen_at(2026, 9, 18, 8, 27, "UTC").unwrap().now();
        home.draw(&mut frame, &Palette::dark(), &time, now, 1.0, 1.0);
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
    fn it_shows_the_time_and_all_five_places() {
        let frame = render(&Home::new(), 0.0);
        let s = texts(&frame);
        assert!(s.iter().any(|t| t == "8:27"));
        assert!(s.iter().any(|t| t.contains("September")));
        for mode in Mode::ALL {
            assert!(s.iter().any(|t| t == mode.name()), "missing {mode:?}");
        }
    }

    #[test]
    fn every_place_gets_its_own_icon() {
        let frame = render(&Home::new(), 0.0);
        let icons: Vec<_> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Icon(icon) => Some(icon.shape),
                _ => None,
            })
            .collect();

        assert_eq!(icons.len(), 5);
        for mode in Mode::ALL {
            assert!(icons.contains(&mode.icon()), "no icon for {mode:?}");
        }
    }

    #[test]
    fn the_icons_sit_in_a_row_beneath_the_time() {
        let places = layout(SCREEN);
        let row_y = places[0].1.center.y;
        for (_, rect) in &places {
            assert!((rect.center.y - row_y).abs() < 0.5, "not on one line");
        }
        assert!(row_y > SCREEN.y * 0.5, "should be below the middle");

        // Evenly spaced, and centred as a group.
        let pitch = places[1].1.center.x - places[0].1.center.x;
        for pair in places.windows(2) {
            assert!((pair[1].1.center.x - pair[0].1.center.x - pitch).abs() < 0.5);
        }
        let mid = (places[0].1.center.x + places[4].1.center.x) * 0.5;
        assert!((mid - SCREEN.x * 0.5).abs() < 0.5, "the row is not centred");
    }

    #[test]
    fn clicking_an_icon_finds_its_place() {
        // The whole point of Home: it can be operated with a pointer.
        for (mode, rect) in layout(SCREEN) {
            assert_eq!(hit(SCREEN, rect.center), Some(mode));
        }
    }

    #[test]
    fn the_target_is_bigger_than_the_icon_but_they_do_not_overlap() {
        let places = layout(SCREEN);
        for (_, rect) in &places {
            let area = hit_area(*rect);
            assert!(area.width() > rect.width(), "target should be forgiving");
        }
        // Adjacent targets must not overlap, or a click lands on the wrong one.
        for pair in places.windows(2) {
            assert!(
                !hit_area(pair[0].1).overlaps(&hit_area(pair[1].1)),
                "neighbouring targets overlap"
            );
        }
    }

    #[test]
    fn clicking_empty_space_selects_nothing() {
        assert_eq!(hit(SCREEN, vec2(10.0, 10.0)), None);
        assert_eq!(hit(SCREEN, vec2(SCREEN.x * 0.5, 40.0)), None);
    }

    #[test]
    fn the_selection_moves_and_stops_at_the_ends() {
        let mut home = Home::new();
        assert_eq!(home.selected(), Mode::Music);

        home.step(0.0, 1);
        assert_eq!(home.selected(), Mode::Clock);

        // Left from the first place stays put rather than wrapping to the last.
        home.step(0.0, -1);
        home.step(0.0, -1);
        assert_eq!(home.selected(), Mode::Music);

        for _ in 0..10 {
            home.step(0.0, 1);
        }
        assert_eq!(home.selected(), Mode::Files);
    }

    #[test]
    fn the_selected_place_is_brighter_than_the_rest() {
        let mut home = Home::new();
        home.select(0.0, Mode::Mail);

        // Long enough for the highlight to arrive.
        let frame = render(&home, 2.0);
        let icons: Vec<_> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Icon(icon) => Some((icon.shape, icon.rim, icon.color.lightness())),
                _ => None,
            })
            .collect();

        let mail = icons.iter().find(|(s, _, _)| *s == Mode::Mail.icon()).unwrap();
        for other in icons.iter().filter(|(s, _, _)| *s != Mode::Mail.icon()) {
            assert!(mail.1 > other.1, "the selected icon should catch more light");
            assert!(mail.2 > other.2, "the selected icon should be brighter");
        }
    }

    #[test]
    fn the_highlight_slides_rather_than_jumping() {
        let mut home = Home::new();
        home.select(0.0, Mode::Files);
        assert!(!home.settled(0.01), "should still be travelling");
        assert!(home.settled(2.0), "should have arrived");
    }

    #[test]
    fn the_row_stays_on_screen_when_the_window_is_narrow() {
        for width in [520.0_f32, 800.0, 1512.0, 3840.0] {
            let size = vec2(width, 720.0);
            for (_, rect) in layout(size) {
                assert!(rect.min().x > 0.0, "ran off the left at {width}");
                assert!(rect.max().x < width, "ran off the right at {width}");
            }
        }
    }
}
