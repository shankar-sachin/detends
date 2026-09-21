//! Music.
//!
//! The composition is the one §4 draws:
//!
//! ```text
//!                      [ART]
//!
//!                    Resonance
//!                       HOME
//!
//!                ◀      Ⅱ      ▶
//!
//!                 ━━━━━●━━━━
//!
//!                     Spotify
//! ```
//!
//! One column, centred, with the artwork dominating and the provider last and
//! faintest — "the active provider should be visible but visually secondary".
//! Shuffle and repeat sit out at the edges of the transport row rather than in
//! it, because they are settings that persist and the three in the middle are
//! things you do.
//!
//! Nothing here knows which provider is playing. It draws a [`Playback`], and
//! a `Playback` from the local provider and one from Spotify are the same kind
//! of thing — which is the whole point of §4's rule.

use detends_music::{Command, Playback, Repeat, Status};
use detends_paint::{
    space, text, Align, Color, Fill, Frame, Icon, IconShape, Id, Image, Item, Layer, Palette,
    Primitive, Rect, Seconds, Text, TextStyle, TextureId, Vec2, ICON_STROKE,
};

/// Something in Music that can be pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Previous,
    PlayPause,
    Next,
    Shuffle,
    Repeat,
    /// The timeline. Where along it was pressed is worked out separately.
    Timeline,
}

const TRANSPORT: f32 = 30.0;
const SIDE_ICON: f32 = 19.0;

/// Where everything sits, for drawing and hit-testing alike.
pub struct Layout {
    pub artwork: Rect,
    pub transport: [(Hit, Rect); 3],
    pub sides: [(Hit, Rect); 2],
    pub timeline: Rect,
}

pub fn layout(area: Rect) -> Layout {
    let centre = area.center;

    // The artwork is the subject, so it takes as much room as the column can
    // give it without crowding what is written underneath.
    let side = (area.height() * 0.34).clamp(180.0, 340.0);
    let artwork = Rect::from_center_size(
        Vec2 { x: centre.x, y: area.min().y + area.height() * 0.30 },
        Vec2::splat(side),
    );

    // Far enough below the artwork to clear the title *and* the artist under
    // it. At anything tighter the artist line and the transport row touch,
    // which reads as one crowded block rather than two things.
    let transport_y = artwork.max().y + space::WIDE * 3.3;
    let pitch = (area.width() * 0.09).clamp(74.0, 118.0);

    let transport = [
        (
            Hit::Previous,
            Rect::from_center_size(
                Vec2 { x: centre.x - pitch, y: transport_y },
                Vec2::splat(TRANSPORT),
            ),
        ),
        (
            Hit::PlayPause,
            // The one you press most often, and so the largest.
            Rect::from_center_size(
                Vec2 { x: centre.x, y: transport_y },
                Vec2::splat(TRANSPORT * 1.28),
            ),
        ),
        (
            Hit::Next,
            Rect::from_center_size(
                Vec2 { x: centre.x + pitch, y: transport_y },
                Vec2::splat(TRANSPORT),
            ),
        ),
    ];

    // Out at the edges: persistent settings, not actions.
    let reach = pitch * 2.15;
    let sides = [
        (
            Hit::Shuffle,
            Rect::from_center_size(
                Vec2 { x: centre.x - reach, y: transport_y },
                Vec2::splat(SIDE_ICON),
            ),
        ),
        (
            Hit::Repeat,
            Rect::from_center_size(
                Vec2 { x: centre.x + reach, y: transport_y },
                Vec2::splat(SIDE_ICON),
            ),
        ),
    ];

    let width = (area.width() * 0.26).clamp(240.0, 420.0);
    let timeline = Rect::from_center_size(
        Vec2 { x: centre.x, y: transport_y + space::WIDE * 1.4 },
        // Generous vertically so it can be grabbed; the drawn track is 3px.
        Vec2 { x: width, y: space::ROOM },
    );

    Layout {
        artwork,
        transport,
        sides,
        timeline,
    }
}

/// What a point lands on.
pub fn hit(area: Rect, at: Vec2) -> Option<Hit> {
    let layout = layout(area);
    layout
        .transport
        .iter()
        .chain(layout.sides.iter())
        .find(|(_, rect)| rect.contains(at))
        .map(|(hit, _)| *hit)
        .or_else(|| layout.timeline.contains(at).then_some(Hit::Timeline))
}

/// Where along the timeline a point falls, 0 to 1.
pub fn seek_fraction(area: Rect, at: Vec2) -> f32 {
    let track = layout(area).timeline;
    ((at.x - track.min().x) / track.width().max(1.0)).clamp(0.0, 1.0)
}

/// Turn a press into a command.
pub fn command_for(hit: Hit, area: Rect, at: Vec2) -> Command {
    match hit {
        Hit::Previous => Command::Previous,
        Hit::PlayPause => Command::PlayPause,
        Hit::Next => Command::Next,
        Hit::Shuffle => Command::ToggleShuffle,
        Hit::Repeat => Command::CycleRepeat,
        Hit::Timeline => Command::Seek(seek_fraction(area, at)),
    }
}

/// Draw Music.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    area: Rect,
    state: &Playback,
    artwork: Option<TextureId>,
    _now: Seconds,
    opacity: f32,
    scale: f32,
) {
    let centre = area.center;
    let about = |p: Vec2| Vec2 {
        x: centre.x + (p.x - centre.x) * scale,
        y: centre.y + (p.y - centre.y) * scale,
    };

    let write = |frame: &mut Frame,
                 id: Id,
                 label: std::borrow::Cow<'static, str>,
                 at: Vec2,
                 width: f32,
                 style: TextStyle,
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

    let layout = layout(area);

    // The cover.
    //
    // Deliberately *not* a pane of glass. It sits inside a window that is
    // already glass, and two glass surfaces overlapping in one layer cannot
    // composite — the earlier version drew one here and it came out as a
    // smudge. A record sleeve is a printed thing anyway.
    let cover = Rect::from_center_size(about(layout.artwork.center), layout.artwork.size() * scale);

    match artwork {
        Some(texture) => {
            frame.push(
                Item::new(
                    Id::of("music-artwork"),
                    Layer::Content,
                    Primitive::Image(Image {
                        rect: cover,
                        texture,
                        source: Rect::from_min_size(
                            Vec2 { x: 0.0, y: 0.0 },
                            Vec2 { x: 1.0, y: 1.0 },
                        ),
                        radius: 14.0 * scale,
                        squircle: 5.0,
                        tint: Color::WHITE,
                    }),
                )
                .opacity(opacity)
                .z(1),
            );
        }
        None => {
            frame.push(
                Item::new(
                    Id::of("music-artwork"),
                    Layer::Content,
                    Primitive::Fill(Fill {
                        rect: cover,
                        radius: 14.0 * scale,
                        squircle: 5.0,
                        color: palette.text_faint.fade(0.10),
                    }),
                )
                .opacity(opacity)
                .z(1),
            );

            frame.push(
                Item::new(
                    Id::of("music-artwork-mark"),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(
                            about(layout.artwork.center),
                            Vec2::splat(layout.artwork.width() * 0.22) * scale,
                        ),
                        shape: IconShape::Music,
                        stroke: ICON_STROKE,
                        color: palette.text_faint.fade(0.5),
                        rim: 0.3,
                    }),
                )
                .opacity(opacity)
                .z(2),
            );
        }
    }

    // Title and artist, or whatever the status has to say instead.
    let title_y = layout.artwork.max().y + space::WIDE;
    match &state.track {
        Some(track) => {
            write(
                frame,
                Id::of("music-title"),
                track.title.clone().into(),
                Vec2 { x: centre.x, y: title_y },
                area.width() * 0.7,
                text::TITLE,
                palette.text,
                Align::Center,
            );
            write(
                frame,
                Id::of("music-artist"),
                track.artist.clone().into(),
                Vec2 { x: centre.x, y: title_y + text::TITLE.size * 1.5 },
                area.width() * 0.7,
                text::HEADING,
                palette.text_soft,
                Align::Center,
            );
        }
        None => {
            write(
                frame,
                Id::of("music-nothing"),
                state.status.message().into(),
                Vec2 { x: centre.x, y: title_y + text::TITLE.size * 0.6 },
                area.width() * 0.7,
                text::HEADING,
                palette.text_faint,
                Align::Center,
            );
        }
    }

    // The transport.
    let live = state.track.is_some();
    for (hit, rect) in layout.transport.iter() {
        let shape = match hit {
            Hit::Previous => IconShape::Previous,
            Hit::Next => IconShape::Next,
            _ if state.playing => IconShape::Pause,
            _ => IconShape::Play,
        };

        frame.push(
            Item::new(
                Id::of("music-transport").nth(*hit as u64),
                Layer::Content,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(about(rect.center), rect.size() * scale),
                    shape,
                    stroke: ICON_STROKE,
                    color: if live { palette.text } else { palette.text_faint },
                    rim: if *hit == Hit::PlayPause { 0.75 } else { 0.4 },
                }),
            )
            .opacity(opacity)
            .z(3),
        );
    }

    // Shuffle and repeat: lit when on, present when off. A setting that
    // vanishes when inactive cannot be turned back on.
    for (hit, rect) in layout.sides.iter() {
        let (shape, on) = match hit {
            Hit::Shuffle => (IconShape::Shuffle, state.shuffle),
            _ => (IconShape::Repeat, state.repeat != Repeat::Off),
        };

        frame.push(
            Item::new(
                Id::of("music-side").nth(*hit as u64),
                Layer::Content,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(about(rect.center), rect.size() * scale),
                    shape,
                    stroke: ICON_STROKE,
                    color: if on { palette.accent } else { palette.text_faint.fade(0.7) },
                    rim: if on { 0.6 } else { 0.2 },
                }),
            )
            .opacity(opacity)
            .z(3),
        );
    }

    // Repeat-one says so, rather than looking identical to repeat-all.
    if state.repeat == Repeat::One {
        let (_, rect) = layout.sides[1];
        write(
            frame,
            Id::of("music-repeat-one"),
            "1".into(),
            Vec2 { x: rect.center.x, y: rect.center.y + SIDE_ICON * 0.95 },
            SIDE_ICON * 3.0,
            text::CAPTION,
            palette.accent,
            Align::Center,
        );
    }

    // The timeline.
    let track_rect = layout.timeline;
    let line = 3.0;
    frame.push(
        Item::new(
            Id::of("music-timeline"),
            Layer::Content,
            Primitive::Fill(Fill {
                rect: Rect::from_center_size(
                    about(track_rect.center),
                    Vec2 { x: track_rect.width(), y: line } * scale,
                ),
                radius: line * 0.5 * scale,
                squircle: 2.0,
                color: palette.text_faint.fade(0.35),
            }),
        )
        .opacity(opacity)
        .z(1),
    );

    let progress = state.progress();
    if progress > 0.0 {
        let filled = track_rect.width() * progress;
        frame.push(
            Item::new(
                Id::of("music-elapsed"),
                Layer::Content,
                Primitive::Fill(Fill {
                    rect: Rect::from_min_size(
                        about(Vec2 {
                            x: track_rect.min().x,
                            y: track_rect.center.y - line * 0.5,
                        }),
                        Vec2 { x: filled, y: line } * scale,
                    ),
                    radius: line * 0.5 * scale,
                    squircle: 2.0,
                    color: palette.text_soft,
                }),
            )
            .opacity(opacity)
            .z(2),
        );
    }

    if live {
        // The head, so the timeline reads as draggable rather than as a bar.
        frame.push(
            Item::new(
                Id::of("music-head"),
                Layer::Content,
                Primitive::Fill(Fill {
                    rect: Rect::from_center_size(
                        about(Vec2 {
                            x: track_rect.min().x + track_rect.width() * progress,
                            y: track_rect.center.y,
                        }),
                        Vec2::splat(9.0) * scale,
                    ),
                    radius: 4.5 * scale,
                    squircle: 2.0,
                    color: palette.text,
                }),
            )
            .opacity(opacity)
            .z(3),
        );

        let times_y = track_rect.center.y + space::ROOM;
        write(
            frame,
            Id::of("music-position"),
            state.elapsed().into(),
            Vec2 { x: track_rect.min().x + 26.0, y: times_y },
            60.0,
            text::CAPTION,
            palette.text_faint,
            Align::Left,
        );
        write(
            frame,
            Id::of("music-remaining"),
            state.remaining().into(),
            Vec2 { x: track_rect.max().x - 26.0, y: times_y },
            60.0,
            text::CAPTION,
            palette.text_faint,
            Align::Right,
        );
    }

    // The provider, last and faintest (§4). The device beside it when one is
    // known, because "Spotify" and "Spotify, on the Kitchen speaker" are
    // different facts and the second is the useful one.
    let source = match &state.device {
        Some(device) if !device.is_empty() => format!("{}  ·  {device}", state.provider),
        _ => state.provider.clone(),
    };
    if !source.is_empty() {
        write(
            frame,
            Id::of("music-provider"),
            source.into(),
            Vec2 { x: centre.x, y: track_rect.center.y + space::WIDE * 1.5 },
            area.width() * 0.6,
            text::CAPTION,
            palette.text_faint.fade(0.8),
            Align::Center,
        );
    }

    // A failure is worth saying out loud, under everything else.
    if let Status::Failed(why) = &state.status {
        write(
            frame,
            Id::of("music-trouble"),
            why.clone().into(),
            Vec2 { x: centre.x, y: area.max().y - space::WIDE },
            area.width() * 0.7,
            text::CAPTION,
            palette.accent.fade(0.9),
            Align::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_music::{MediaId, Track};
    use detends_paint::vec2;

    fn area() -> Rect {
        Rect::from_min_size(vec2(0.0, 0.0), vec2(1512.0, 982.0)).inset(60.0)
    }

    fn playing() -> Playback {
        Playback {
            status: Status::Ready,
            track: Some(Track {
                id: MediaId::new("x"),
                title: "Resonance".into(),
                artist: "HOME".into(),
                album: "Odyssey".into(),
                duration: 215.0,
                artwork: None,
            }),
            playing: true,
            position: 35.0,
            provider: "Spotify".into(),
            ..Default::default()
        }
    }

    fn strings(state: &Playback) -> Vec<String> {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        draw(&mut frame, &Palette::dark(), area(), state, None, 0.0, 1.0, 1.0);
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
    fn it_draws_the_composition_from_the_specification() {
        let s = strings(&playing());
        assert!(s.iter().any(|t| t == "Resonance"), "{s:?}");
        assert!(s.iter().any(|t| t == "HOME"));
        assert!(s.iter().any(|t| t.contains("Spotify")));
    }

    #[test]
    fn the_provider_is_named_but_last() {
        // §4: visible, but visually secondary.
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        draw(&mut frame, &Palette::dark(), area(), &playing(), None, 0.0, 1.0, 1.0);

        let y_of = |needle: &str| {
            frame
                .items
                .iter()
                .find_map(|i| match &i.primitive {
                    Primitive::Text(t) if t.text.contains(needle) => Some(t.rect.center.y),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{needle} was not drawn"))
        };
        assert!(y_of("Spotify") > y_of("Resonance"), "the provider is not last");
    }

    #[test]
    fn the_device_is_named_when_one_is_known() {
        let mut state = playing();
        state.device = Some("Kitchen".into());
        let s = strings(&state);
        assert!(s.iter().any(|t| t.contains("Kitchen")), "{s:?}");
    }

    #[test]
    fn with_nothing_playing_it_says_what_to_do_next() {
        let idle = Playback {
            status: Status::Disconnected,
            ..Default::default()
        };
        let s = strings(&idle);
        assert!(
            s.iter().any(|t| t == "No music account connected"),
            "{s:?}"
        );
    }

    #[test]
    fn a_failure_is_shown_rather_than_swallowed() {
        let mut state = playing();
        state.status = Status::Failed("Spotify needs Premium".into());
        let s = strings(&state);
        assert!(s.iter().any(|t| t.contains("Premium")), "{s:?}");
    }

    #[test]
    fn every_control_can_be_pressed() {
        let area = area();
        let layout = layout(area);
        for (control, rect) in layout.transport.iter().chain(layout.sides.iter()) {
            assert_eq!(hit(area, rect.center), Some(*control));
        }
        assert_eq!(hit(area, layout.timeline.center), Some(Hit::Timeline));
        assert_eq!(hit(area, vec2(5.0, 5.0)), None);
    }

    #[test]
    fn the_transport_is_previous_play_next_in_that_order() {
        let layout = layout(area());
        let xs: Vec<f32> = layout.transport.iter().map(|(_, r)| r.center.x).collect();
        assert!(xs[0] < xs[1] && xs[1] < xs[2]);
        assert_eq!(layout.transport[1].0, Hit::PlayPause);
    }

    #[test]
    fn play_pause_is_the_largest_target_because_it_is_pressed_most() {
        let layout = layout(area());
        let play = layout.transport[1].1.width();
        assert!(play > layout.transport[0].1.width());
    }

    #[test]
    fn shuffle_and_repeat_sit_outside_the_transport() {
        // They are settings, not actions, so they do not join the row.
        let layout = layout(area());
        let left = layout.transport[0].1.center.x;
        let right = layout.transport[2].1.center.x;
        assert!(layout.sides[0].1.center.x < left);
        assert!(layout.sides[1].1.center.x > right);
    }

    #[test]
    fn pressing_the_timeline_seeks_to_where_it_was_pressed() {
        let area = area();
        let track = layout(area).timeline;

        assert_eq!(seek_fraction(area, track.min()), 0.0);
        // Not an exact comparison: the right-hand end lands a float's breadth
        // short of 1.0, which is correct and not worth contorting the maths for.
        assert!((seek_fraction(area, track.max()) - 1.0).abs() < 1e-5);
        assert!((seek_fraction(area, track.center) - 0.5).abs() < 0.01);

        match command_for(Hit::Timeline, area, track.center) {
            Command::Seek(f) => assert!((f - 0.5).abs() < 0.01),
            other => panic!("expected a seek, got {other:?}"),
        }
    }

    #[test]
    fn each_control_asks_for_the_command_it_looks_like() {
        let area = area();
        let at = area.center;
        assert_eq!(command_for(Hit::PlayPause, area, at), Command::PlayPause);
        assert_eq!(command_for(Hit::Next, area, at), Command::Next);
        assert_eq!(command_for(Hit::Previous, area, at), Command::Previous);
        assert_eq!(command_for(Hit::Shuffle, area, at), Command::ToggleShuffle);
        assert_eq!(command_for(Hit::Repeat, area, at), Command::CycleRepeat);
    }

    #[test]
    fn repeat_one_is_distinguishable_from_repeat_all() {
        let mut state = playing();
        state.repeat = Repeat::All;
        assert!(!strings(&state).iter().any(|t| t == "1"));

        state.repeat = Repeat::One;
        assert!(strings(&state).iter().any(|t| t == "1"), "repeat-one looks like repeat-all");
    }

    #[test]
    fn the_timeline_shows_both_ends_of_the_clock() {
        let s = strings(&playing());
        assert!(s.iter().any(|t| t == "0:35"), "{s:?}");
        assert!(s.iter().any(|t| t == "-3:00"), "{s:?}");
    }

    #[test]
    fn nothing_overflows_the_area_it_was_given() {
        let area = area();
        let layout = layout(area);
        for rect in [layout.artwork, layout.timeline] {
            assert!(rect.min().y >= area.min().y, "{rect:?} above the area");
            assert!(rect.max().y <= area.max().y, "{rect:?} below the area");
        }
    }
}
