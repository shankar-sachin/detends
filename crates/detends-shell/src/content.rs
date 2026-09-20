//! What each of the five places looks like.
//!
//! Placeholder content, real layout. The compositions here are the ones the
//! specification describes, so that the interaction language can be judged now
//! and the modes filled in later without the geometry moving.
//!
//! Every mode follows the same discipline: one thing dominates, type carries the
//! hierarchy, and large empty areas stay empty (§17).

use crate::mode::Mode;
use detends_paint::{
    space, text, Align, Color, Fill, Frame, Glass, Id, Item, Layer, Palette, Primitive, Rect, Text,
    TextStyle, Vec2,
};
use detends_time::TimeOfDay;

/// Everything a mode needs to draw itself.
pub struct Canvas<'a> {
    pub frame: &'a mut Frame,
    pub palette: &'a Palette,
    /// The area a mode may use, inside the status cluster's margins.
    pub area: Rect,
    pub opacity: f32,
    /// Scale applied around the centre, for arriving and departing modes.
    pub scale: f32,
    pub time: &'a TimeOfDay,
}

impl Canvas<'_> {
    fn place(&self, rect: Rect) -> Rect {
        // Modes scale about the workspace centre, so a transition moves the
        // whole composition as one object rather than sliding its parts.
        let c = self.area.center;
        Rect {
            center: Vec2 {
                x: c.x + (rect.center.x - c.x) * self.scale,
                y: c.y + (rect.center.y - c.y) * self.scale,
            },
            half: rect.half * self.scale,
        }
    }

    fn text(
        &mut self,
        id: Id,
        label: impl Into<std::borrow::Cow<'static, str>>,
        rect: Rect,
        style: TextStyle,
        color: Color,
        align: Align,
    ) {
        let rect = self.place(rect);
        let opacity = self.opacity;
        self.frame.push(
            Item::new(
                id,
                Layer::Content,
                Primitive::Text(Text {
                    text: label.into(),
                    rect,
                    size: style.size * self.scale,
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
    }

    fn pane(&mut self, id: Id, rect: Rect, radius: f32) {
        let rect = self.place(rect);
        let opacity = self.opacity;
        let tint = self.palette.glass;
        self.frame.push(
            Item::new(
                id,
                Layer::Content,
                Primitive::Glass(Glass {
                    rect,
                    radius: radius * self.scale,
                    squircle: 5.0,
                    thickness: 14.0,
                    bevel: 22.0,
                    ior: 1.48,
                    dispersion: 0.018,
                    frost: 0.6,
                    tint,
                    rim: 0.9,
                }),
            )
            .opacity(opacity),
        );
    }

    fn fill(&mut self, id: Id, rect: Rect, color: Color, radius: f32) {
        let rect = self.place(rect);
        let opacity = self.opacity;
        self.frame.push(
            Item::new(
                id,
                Layer::Content,
                Primitive::Fill(Fill {
                    rect,
                    radius,
                    squircle: 4.0,
                    color,
                }),
            )
            .opacity(opacity)
            .z(1),
        );
    }
}

pub fn draw(mode: Mode, canvas: &mut Canvas<'_>) {
    match mode {
        Mode::Music => music(canvas),
        Mode::Clock => clock(canvas),
        Mode::Mail => mail(canvas),
        Mode::Studio => studio(canvas),
        Mode::Files => files(canvas),
    }
}

/// §4: artwork, track, artist, transport, timeline, provider — and the active
/// provider visible but visually secondary.
fn music(c: &mut Canvas<'_>) {
    let centre = c.area.center;
    let art = (c.area.height() * 0.34).clamp(180.0, 340.0);

    c.pane(
        Id::of("music-art"),
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: centre.y - art * 0.62,
            },
            Vec2::splat(art),
        ),
        art * 0.06,
    );

    let title = text::TITLE;
    c.text(
        Id::of("music-track"),
        "Resonance",
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: centre.y + art * 0.10,
            },
            Vec2 {
                x: c.area.width() * 0.7,
                y: title.size * 1.7,
            },
        ),
        title,
        c.palette.text,
        Align::Center,
    );

    let caption = text::CAPTION;
    c.text(
        Id::of("music-artist"),
        "HOME",
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: centre.y + art * 0.10 + space::ROOM + space::TIGHT,
            },
            Vec2 {
                x: c.area.width() * 0.7,
                y: caption.size * 2.0,
            },
        ),
        caption,
        c.palette.text_faint,
        Align::Center,
    );

    let transport_y = centre.y + art * 0.10 + space::WIDE + space::STEP;
    c.text(
        Id::of("music-transport"),
        "◀      ⅠⅠ      ▶",
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: transport_y,
            },
            Vec2 {
                x: c.area.width() * 0.5,
                y: text::HEADING.size * 2.0,
            },
        ),
        text::HEADING,
        c.palette.text_soft,
        Align::Center,
    );

    // Timeline: a hairline, with the played portion in the text colour. Two
    // fills rather than a control, because there is nothing to operate until
    // there is a real track.
    let track_w = (c.area.width() * 0.34).clamp(220.0, 420.0);
    let track_y = transport_y + space::OPEN + space::SNUG;
    c.fill(
        Id::of("music-track-bg"),
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: track_y,
            },
            Vec2 { x: track_w, y: 2.0 },
        ),
        c.palette.text_faint.alpha(0.25),
        1.0,
    );
    c.fill(
        Id::of("music-track-fg"),
        Rect::from_min_size(
            Vec2 {
                x: centre.x - track_w * 0.5,
                y: track_y - 1.0,
            },
            Vec2 {
                x: track_w * 0.42,
                y: 2.0,
            },
        ),
        c.palette.text_soft,
        1.0,
    );

    // The provider: present, and firmly secondary.
    c.text(
        Id::of("music-provider"),
        "Spotify",
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: track_y + space::ROOM + space::SNUG,
            },
            Vec2 {
                x: c.area.width() * 0.4,
                y: caption.size * 2.0,
            },
        ),
        caption,
        c.palette.text_faint,
        Align::Center,
    );
}

/// §5: visually sparse. The time, and nothing that is not the time.
fn clock(c: &mut Canvas<'_>) {
    let centre = c.area.center;
    let display = text::DISPLAY;

    c.text(
        Id::of("clock-time"),
        c.time.short(),
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: centre.y - display.size * 0.18,
            },
            Vec2 {
                x: c.area.width() * 0.9,
                y: display.size * 1.3,
            },
        ),
        display,
        c.palette.text,
        Align::Center,
    );

    c.text(
        Id::of("clock-date"),
        c.time.long_date(),
        Rect::from_center_size(
            Vec2 {
                x: centre.x,
                y: centre.y + display.size * 0.52,
            },
            Vec2 {
                x: c.area.width() * 0.9,
                y: text::HEADING.size * 2.0,
            },
        ),
        text::HEADING,
        c.palette.text_faint,
        Align::Center,
    );
}

/// §6: the message dominates the reading interface.
fn mail(c: &mut Canvas<'_>) {
    let centre = c.area.center;
    let width = (c.area.width() * 0.52).clamp(420.0, 760.0);
    let left = centre.x - width * 0.5;
    let top = c.area.min().y + c.area.height() * 0.22;

    c.text(
        Id::of("mail-subject"),
        "Thursday's measurements",
        Rect::from_min_size(
            Vec2 { x: left, y: top },
            Vec2 {
                x: width,
                y: text::TITLE.size * 1.6,
            },
        ),
        text::TITLE,
        c.palette.text,
        Align::Left,
    );

    c.text(
        Id::of("mail-from"),
        "ALEX MERCIER · 4:02 PM",
        Rect::from_min_size(
            Vec2 {
                x: left,
                y: top + space::OPEN + space::TIGHT,
            },
            Vec2 {
                x: width,
                y: text::CAPTION.size * 2.0,
            },
        ),
        text::CAPTION,
        c.palette.text_faint,
        Align::Left,
    );

    c.text(
        Id::of("mail-body"),
        "The second run came out cleaner than the first — the drift we were \
         chasing turns out to have been the mount, not the sensor. I've attached \
         both sets so you can see the difference.\n\nNo rush on this one.",
        Rect::from_min_size(
            Vec2 {
                x: left,
                y: top + space::WIDE + space::ROOM,
            },
            Vec2 {
                x: width,
                y: c.area.height() * 0.4,
            },
        ),
        text::BODY,
        c.palette.text_soft,
        Align::Left,
    );
}

/// §7: three creation modes, and the native formats they produce.
fn studio(c: &mut Canvas<'_>) {
    let centre = c.area.center;
    let card = Vec2 {
        x: (c.area.width() * 0.16).clamp(150.0, 240.0),
        y: (c.area.height() * 0.26).clamp(170.0, 260.0),
    };
    let gap = space::ROOM;
    let total = card.x * 3.0 + gap * 2.0;
    let start = centre.x - total * 0.5 + card.x * 0.5;

    for (index, (name, extension)) in [("Page", ".dpg"), ("Deck", ".dek"), ("Grid", ".dgr")]
        .iter()
        .enumerate()
    {
        let x = start + index as f32 * (card.x + gap);
        let rect = Rect::from_center_size(Vec2 { x, y: centre.y }, card);
        c.pane(Id::of("studio-card").nth(index as u64), rect, 24.0);

        c.text(
            Id::of("studio-name").nth(index as u64),
            *name,
            Rect::from_center_size(
                Vec2 {
                    x,
                    y: centre.y + card.y * 0.16,
                },
                Vec2 {
                    x: card.x,
                    y: text::HEADING.size * 1.8,
                },
            ),
            text::HEADING,
            c.palette.text,
            Align::Center,
        );

        c.text(
            Id::of("studio-ext").nth(index as u64),
            *extension,
            Rect::from_center_size(
                Vec2 {
                    x,
                    y: centre.y + card.y * 0.16 + space::ROOM,
                },
                Vec2 {
                    x: card.x,
                    y: text::CAPTION.size * 2.0,
                },
            ),
            text::CAPTION,
            c.palette.text_faint,
            Align::Center,
        );
    }
}

/// §9: destinations, not a folder tree.
fn files(c: &mut Canvas<'_>) {
    let centre = c.area.center;
    let width = (c.area.width() * 0.3).clamp(260.0, 420.0);
    let row = space::WIDE;
    let places = ["Recents", "Studio", "Downloads", "Screenshots"];
    let top = centre.y - (places.len() as f32 * row) * 0.5;

    for (index, place) in places.iter().enumerate() {
        let y = top + index as f32 * row + row * 0.5;
        c.text(
            Id::of("files-place").nth(index as u64),
            *place,
            Rect::from_min_size(
                Vec2 {
                    x: centre.x - width * 0.5,
                    y: y - text::HEADING.size,
                },
                Vec2 {
                    x: width,
                    y: text::HEADING.size * 2.0,
                },
            ),
            text::HEADING,
            if index == 0 {
                c.palette.text
            } else {
                c.palette.text_soft
            },
            Align::Left,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;
    use detends_time::Clock;

    fn render(mode: Mode) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let palette = Palette::dark();
        let time = Clock::frozen_at(2026, 9, 17, 17, 14, "UTC").unwrap().now();
        let area = Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0)).inset(60.0);
        let mut canvas = Canvas {
            frame: &mut frame,
            palette: &palette,
            area,
            opacity: 1.0,
            scale: 1.0,
            time: &time,
        };
        draw(mode, &mut canvas);
        frame
    }

    fn strings(frame: &Frame) -> Vec<String> {
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
    fn every_mode_draws_something() {
        for mode in Mode::ALL {
            assert!(!render(mode).items.is_empty(), "{mode:?} drew nothing");
        }
    }

    #[test]
    fn music_shows_the_composition_from_the_specification() {
        let s = strings(&render(Mode::Music));
        assert!(s.iter().any(|t| t == "Resonance"));
        assert!(s.iter().any(|t| t == "HOME"));
        assert!(s.iter().any(|t| t == "Spotify"));
    }

    #[test]
    fn the_provider_is_secondary_to_the_track() {
        // §4: "The active provider should be visible but visually secondary."
        let frame = render(Mode::Music);
        let size_of = |needle: &str| {
            frame.items.iter().find_map(|i| match &i.primitive {
                Primitive::Text(t) if t.text == needle => Some(t.size),
                _ => None,
            })
        };
        let track = size_of("Resonance").expect("track");
        let provider = size_of("Spotify").expect("provider");
        assert!(
            provider < track * 0.5,
            "provider {provider} vs track {track}"
        );
    }

    #[test]
    fn studio_uses_the_native_extensions() {
        let s = strings(&render(Mode::Studio));
        for extension in [".dpg", ".dek", ".dgr"] {
            assert!(s.iter().any(|t| t == extension), "missing {extension}");
        }
        // Never the long forms.
        assert!(!s.iter().any(|t| t.contains(".dpage")));
    }

    #[test]
    fn clock_shows_the_real_time() {
        let s = strings(&render(Mode::Clock));
        assert!(s.iter().any(|t| t == "5:14"), "got {s:?}");
        assert!(s.iter().any(|t| t.contains("September")));
    }

    #[test]
    fn clock_is_the_sparsest_mode() {
        // §5: "Clock should be visually sparse."
        let clock = render(Mode::Clock).items.len();
        for other in [Mode::Music, Mode::Studio, Mode::Files] {
            assert!(
                clock <= render(other).items.len(),
                "Clock is busier than {other:?}"
            );
        }
    }

    #[test]
    fn files_lists_destinations_rather_than_a_tree() {
        let s = strings(&render(Mode::Files));
        for place in ["Recents", "Studio", "Downloads", "Screenshots"] {
            assert!(s.iter().any(|t| t == place), "missing {place}");
        }
    }

    #[test]
    fn mail_lets_the_message_dominate() {
        // §6: the message itself should dominate the reading interface, so the
        // body is the longest thing on screen by a wide margin.
        let s = strings(&render(Mode::Mail));
        let longest = s.iter().map(|t| t.len()).max().unwrap_or(0);
        let subject = s.iter().find(|t| t.contains("measurements")).unwrap().len();
        assert!(longest > subject * 4, "the body does not dominate");
    }

    #[test]
    fn no_mode_overlaps_its_own_glass() {
        // The invariant the renderer's single-draw-per-layer path relies on.
        for mode in Mode::ALL {
            let frame = render(mode);
            assert!(
                frame.debug_check_layers().is_ok(),
                "{mode:?}: {:?}",
                frame.debug_check_layers()
            );
        }
    }

    #[test]
    fn everything_stays_inside_the_workspace() {
        for mode in Mode::ALL {
            let frame = render(mode);
            for item in &frame.items {
                let b = item.primitive.bounds();
                assert!(
                    b.min().x > -20.0 && b.max().x < 1532.0,
                    "{mode:?} overflows horizontally"
                );
                assert!(
                    b.min().y > -20.0 && b.max().y < 1002.0,
                    "{mode:?} overflows vertically"
                );
            }
        }
    }
}
