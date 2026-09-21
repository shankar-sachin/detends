//! Settings.
//!
//! "Settings should remain deliberately small… Do not expose controls simply
//! because they exist internally." (§18)
//!
//! So this is not a preferences database with a sidebar. It is one column of
//! the few things a person actually changes, each wired to state the system
//! already has — appearance, how much glass there is, how much moves. Every
//! control here does something the moment it is touched; there is no Apply and
//! nothing to save.
//!
//! The test of whether something belongs is not "is it configurable" but
//! "would anyone change it twice".

use detends_paint::{
    space, text, Align, Appearance, Fill, Frame, GlassSettings, Icon, IconShape, Id, Item, Layer,
    MotionPreference, Palette, Primitive, Rect, Text, Vec2, ICON_STROKE,
};

/// Something in Settings that can be pressed or dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Appearance(Appearance),
    Motion(MotionPreference),
    Glass,
    Transparency,
}

/// Height of one row, and of a segmented choice.
const ROW: f32 = 62.0;
const SEGMENT: f32 = 30.0;
const TRACK: f32 = 20.0;

/// Where everything sits, for drawing and hit-testing alike.
pub struct Layout {
    pub appearance: [(Control, Rect); 3],
    pub motion: [(Control, Rect); 2],
    pub glass: Rect,
    pub transparency: Rect,
}

pub fn layout(area: Rect) -> Layout {
    let x = area.min().x;
    let width = area.width();
    let mut y = area.min().y + space::ROOM;

    // Appearance: three segments in a row the width of the column.
    y += text::LABEL.size + space::SNUG;
    let segment_width = (width / 3.0).min(140.0);
    let mut appearance = [(Control::Appearance(Appearance::Light), Rect::ZERO); 3];
    for (index, mode) in [Appearance::Light, Appearance::Dark, Appearance::Automatic]
        .iter()
        .enumerate()
    {
        appearance[index] = (
            Control::Appearance(*mode),
            Rect::from_min_size(
                Vec2 { x: x + index as f32 * segment_width, y },
                Vec2 { x: segment_width, y: SEGMENT },
            ),
        );
    }

    y += ROW;
    y += text::LABEL.size + space::SNUG;
    let glass = Rect::from_min_size(Vec2 { x, y }, Vec2 { x: width, y: TRACK });

    y += ROW;
    y += text::LABEL.size + space::SNUG;
    let transparency = Rect::from_min_size(Vec2 { x, y }, Vec2 { x: width, y: TRACK });

    y += ROW;
    y += text::LABEL.size + space::SNUG;
    let mut motion = [(Control::Motion(MotionPreference::Full), Rect::ZERO); 2];
    for (index, preference) in [MotionPreference::Full, MotionPreference::Reduced]
        .iter()
        .enumerate()
    {
        motion[index] = (
            Control::Motion(*preference),
            Rect::from_min_size(
                Vec2 { x: x + index as f32 * segment_width, y },
                Vec2 { x: segment_width, y: SEGMENT },
            ),
        );
    }

    Layout {
        appearance,
        motion,
        glass,
        transparency,
    }
}

/// What a point lands on.
pub fn hit(area: Rect, at: Vec2) -> Option<Control> {
    let layout = layout(area);

    for (control, rect) in layout.appearance.iter().chain(layout.motion.iter()) {
        if rect.contains(at) {
            return Some(*control);
        }
    }
    // Generous vertically: a 20px track is a fine thing to look at and a poor
    // thing to aim at.
    if grown(layout.glass).contains(at) {
        return Some(Control::Glass);
    }
    if grown(layout.transparency).contains(at) {
        return Some(Control::Transparency);
    }
    None
}

fn grown(track: Rect) -> Rect {
    Rect::from_center_size(
        track.center,
        Vec2 { x: track.width(), y: track.height() + space::ROOM },
    )
}

/// Where along a slider a point falls, 0 to 1.
pub fn slider_value(area: Rect, control: Control, at: Vec2) -> Option<f32> {
    let layout = layout(area);
    let track = match control {
        Control::Glass => layout.glass,
        Control::Transparency => layout.transparency,
        _ => return None,
    };
    Some(((at.x - track.min().x) / track.width().max(1.0)).clamp(0.0, 1.0))
}

/// Draw Settings.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    area: Rect,
    appearance: Appearance,
    glass: GlassSettings,
    motion: MotionPreference,
    backend: &str,
    opacity: f32,
) {
    let layout = layout(area);

    let write = |frame: &mut Frame, id: Id, label: String, at: Vec2, width: f32, style, color| {
        let style: detends_paint::TextStyle = style;
        frame.push(
            Item::new(
                id,
                Layer::Content,
                Primitive::Text(Text {
                    text: label.into(),
                    rect: Rect::from_min_size(at, Vec2 { x: width, y: style.size * 1.8 }),
                    size: style.size,
                    weight: style.weight,
                    tracking: style.tracking,
                    line_height: style.line_height,
                    color,
                    align: Align::Left,
                }),
            )
            .opacity(opacity)
            .z(6),
        );
    };

    let heading = |frame: &mut Frame, id: Id, label: &str, track_top: f32| {
        let at = Vec2 { x: area.min().x, y: track_top - text::LABEL.size - space::SNUG };
        write(
            frame,
            id,
            label.to_string(),
            at,
            area.width(),
            text::LABEL,
            palette.text,
        );
    };

    // ---- Appearance -----------------------------------------------------
    heading(
        frame,
        Id::of("set-appearance"),
        "Appearance",
        layout.appearance[0].1.min().y,
    );

    for (index, (control, rect)) in layout.appearance.iter().enumerate() {
        let Control::Appearance(mode) = control else {
            continue;
        };
        segment(
            frame,
            palette,
            Id::of("set-appearance-seg").nth(index as u64),
            *rect,
            match mode {
                Appearance::Light => "Light",
                Appearance::Dark => "Dark",
                Appearance::Automatic => "Auto",
            },
            *mode == appearance,
            opacity,
        );
    }

    // ---- Glass ----------------------------------------------------------
    heading(frame, Id::of("set-glass"), "Glass", layout.glass.min().y);
    slider(
        frame,
        palette,
        Id::of("set-glass-track"),
        layout.glass,
        glass.intensity,
        opacity,
    );

    heading(
        frame,
        Id::of("set-transparency"),
        "Transparency",
        layout.transparency.min().y,
    );
    slider(
        frame,
        palette,
        Id::of("set-transparency-track"),
        layout.transparency,
        glass.transparency,
        opacity,
    );

    // ---- Motion ---------------------------------------------------------
    heading(frame, Id::of("set-motion"), "Motion", layout.motion[0].1.min().y);
    for (index, (control, rect)) in layout.motion.iter().enumerate() {
        let Control::Motion(preference) = control else {
            continue;
        };
        segment(
            frame,
            palette,
            Id::of("set-motion-seg").nth(index as u64),
            *rect,
            match preference {
                MotionPreference::Full => "Full",
                MotionPreference::Reduced => "Reduced",
            },
            *preference == motion,
            opacity,
        );
    }

    // ---- About ----------------------------------------------------------
    //
    // §18 lists "device information" under System. This is the whole of it:
    // what this is and what it is drawing with. A version and a renderer is
    // everything anyone has ever needed from an About box.
    let about_y = layout.motion[0].1.max().y + space::WIDE;
    frame.push(
        Item::new(
            Id::of("set-about-icon"),
            Layer::Content,
            Primitive::Icon(Icon {
                rect: Rect::from_center_size(
                    Vec2 { x: area.min().x + 9.0, y: about_y + 9.0 },
                    Vec2::splat(16.0),
                ),
                shape: IconShape::Focus,
                stroke: ICON_STROKE,
                color: palette.text_faint,
                rim: 0.2,
            }),
        )
        .opacity(opacity)
        .z(6),
    );
    write(
        frame,
        Id::of("set-about"),
        format!("détends {}  ·  {backend}", env!("CARGO_PKG_VERSION")),
        Vec2 { x: area.min().x + 26.0, y: about_y },
        area.width(),
        text::CAPTION,
        palette.text_faint,
    );
}

/// One choice in a segmented control.
fn segment(
    frame: &mut Frame,
    palette: &Palette,
    id: Id,
    rect: Rect,
    label: &str,
    chosen: bool,
    opacity: f32,
) {
    frame.push(
        Item::new(
            id,
            Layer::Content,
            Primitive::Fill(Fill {
                rect: rect.inset(2.0),
                radius: 8.0,
                squircle: 4.0,
                color: if chosen {
                    palette.accent.fade(0.85)
                } else {
                    palette.text_faint.fade(0.12)
                },
            }),
        )
        .opacity(opacity)
        .z(5),
    );

    frame.push(
        Item::new(
            id.nth(1),
            Layer::Content,
            Primitive::Text(Text {
                text: label.to_string().into(),
                rect: Rect::from_center_size(
                    rect.center,
                    Vec2 { x: rect.width(), y: text::CAPTION.size * 2.0 },
                ),
                size: text::CAPTION.size,
                weight: text::CAPTION.weight,
                tracking: text::CAPTION.tracking,
                line_height: text::CAPTION.line_height,
                // Dark on the lit segment, so the chosen one stays legible.
                color: if chosen { palette.ground } else { palette.text_soft },
                align: Align::Center,
            }),
        )
        .opacity(opacity)
        .z(6),
    );
}

fn slider(frame: &mut Frame, palette: &Palette, id: Id, rect: Rect, value: f32, opacity: f32) {
    let value = value.clamp(0.0, 1.0);
    let radius = rect.height() * 0.5;

    frame.push(
        Item::new(
            id,
            Layer::Content,
            Primitive::Fill(Fill {
                rect,
                radius,
                squircle: 2.0,
                color: palette.text_faint.alpha(0.18),
            }),
        )
        .opacity(opacity)
        .z(5),
    );

    if value > 0.001 {
        frame.push(
            Item::new(
                id.nth(1),
                Layer::Content,
                Primitive::Fill(Fill {
                    // Never narrower than it is round, or it collapses into a
                    // lens at the low end.
                    rect: Rect::from_min_size(
                        rect.min(),
                        Vec2 { x: (rect.width() * value).max(rect.height()), y: rect.height() },
                    ),
                    radius,
                    squircle: 2.0,
                    color: palette.text_soft,
                }),
            )
            .opacity(opacity)
            .z(6),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn area() -> Rect {
        Rect::from_min_size(vec2(100.0, 80.0), vec2(420.0, 560.0))
    }

    #[test]
    fn every_control_can_be_pressed() {
        let area = area();
        let layout = layout(area);

        for (control, rect) in layout.appearance.iter().chain(layout.motion.iter()) {
            assert_eq!(hit(area, rect.center), Some(*control));
        }
        assert_eq!(hit(area, layout.glass.center), Some(Control::Glass));
        assert_eq!(hit(area, layout.transparency.center), Some(Control::Transparency));
    }

    #[test]
    fn nothing_overlaps_anything_else() {
        let layout = layout(area());
        let all: Vec<Rect> = layout
            .appearance
            .iter()
            .chain(layout.motion.iter())
            .map(|(_, r)| *r)
            .chain([layout.glass, layout.transparency])
            .collect();

        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                let apart = a.max().x <= b.min().x + 0.01
                    || b.max().x <= a.min().x + 0.01
                    || a.max().y <= b.min().y + 0.01
                    || b.max().y <= a.min().y + 0.01;
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn everything_stays_inside_the_window() {
        let area = area();
        let layout = layout(area);
        for rect in layout
            .appearance
            .iter()
            .chain(layout.motion.iter())
            .map(|(_, r)| *r)
            .chain([layout.glass, layout.transparency])
        {
            assert!(rect.min().x >= area.min().x - 0.01, "{rect:?} runs off the left");
            assert!(rect.max().x <= area.max().x + 0.01, "{rect:?} runs off the right");
            assert!(rect.max().y <= area.max().y + 0.01, "{rect:?} runs off the bottom");
        }
    }

    #[test]
    fn the_sliders_read_left_to_right_and_clamp() {
        let area = area();
        let track = layout(area).glass;

        assert_eq!(slider_value(area, Control::Glass, track.min()), Some(0.0));
        assert!(
            (slider_value(area, Control::Glass, track.max()).unwrap() - 1.0).abs() < 1e-5
        );
        assert_eq!(
            slider_value(area, Control::Glass, vec2(track.min().x - 900.0, track.center.y)),
            Some(0.0)
        );
        assert_eq!(slider_value(area, Control::Appearance(Appearance::Dark), track.center), None);
    }

    #[test]
    fn it_offers_only_what_someone_would_change_twice() {
        // §18: not a preferences database. If this list grows, argue for each.
        let layout = layout(area());
        assert_eq!(layout.appearance.len(), 3);
        assert_eq!(layout.motion.len(), 2);
    }

    #[test]
    fn it_draws_its_headings_and_says_what_this_is() {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        draw(
            &mut frame,
            &Palette::dark(),
            area(),
            Appearance::Dark,
            GlassSettings::default(),
            MotionPreference::Full,
            "aarch64 NEON",
            1.0,
        );

        let s: Vec<String> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();

        for expected in ["Appearance", "Glass", "Transparency", "Motion", "Light", "Dark", "Auto"] {
            assert!(s.iter().any(|x| x == expected), "{expected} is missing: {s:?}");
        }
        assert!(
            s.iter().any(|x| x.contains("détends") && x.contains("NEON")),
            "the about line is missing: {s:?}"
        );
    }
}
