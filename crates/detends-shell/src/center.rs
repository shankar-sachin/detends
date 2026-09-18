//! System Center.
//!
//! "Clicking the cluster opens one unified glass System Center." (§10)
//!
//! It holds persistent system states and nothing else (rule 3). There is no
//! screenshot button and no screen-recording button: those are actions, and
//! actions belong in shortcuts and Search (rule 4).
//!
//! The panel grows out of the status cluster rather than appearing in the
//! middle of the screen, because animation communicates relationships (rule
//! 10) — the thing you clicked becomes the thing you are looking at.

use crate::system::{Focus, System};
use detends_paint::{
    space, text, Align, Color, Fill, Frame, Glass, Id, Item, Layer, Palette, Primitive, Rect, Text,
    Vec2,
};
use detends_time::TimeOfDay;

pub const PANEL: Id = Id::of("system-center");

/// Panel size, in logical units.
const WIDTH: f32 = 340.0;
const HEIGHT: f32 = 380.0;

/// Where the panel wants to sit: under the cluster, aligned to its right edge.
pub fn resting_place(cluster: Rect, workspace: Vec2) -> Rect {
    let right = cluster.max().x;
    let top = cluster.max().y + space::STEP;
    Rect::from_min_size(
        Vec2 {
            x: (right - WIDTH).max(space::ROOM),
            y: top.min(workspace.y - HEIGHT - space::ROOM),
        },
        Vec2 {
            x: WIDTH,
            y: HEIGHT,
        },
    )
}

/// Draw the panel.
///
/// `progress` runs 0 (closed, still the cluster) to 1 (fully open). Geometry is
/// interpolated between the two, so the panel is literally the cluster
/// stretching — not a separate surface fading in over it.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    system: &System,
    focus: Option<&Focus>,
    time: &TimeOfDay,
    cluster: Rect,
    workspace: Vec2,
    progress: f32,
) {
    if progress <= 0.001 {
        return;
    }

    let target = resting_place(cluster, workspace);
    let rect = lerp_rect(cluster, target, progress);

    frame.push(
        Item::new(
            PANEL,
            // Above mode content, so it refracts the workspace beneath it.
            Layer::Overlay,
            Primitive::Glass(Glass {
                rect,
                radius: lerp(cluster.height() * 0.5, 30.0, progress),
                squircle: 5.0,
                thickness: lerp(6.0, 16.0, progress),
                bevel: lerp(9.0, 26.0, progress),
                ior: 1.48,
                dispersion: 0.018,
                frost: 0.72,
                tint: palette.glass,
                rim: 0.95,
            }),
        )
        .opacity(1.0),
    );

    // The contents arrive only once there is room for them, so they never
    // appear crushed inside a panel that is still opening.
    let reveal = ((progress - 0.45) / 0.55).clamp(0.0, 1.0);
    if reveal <= 0.001 {
        return;
    }

    let pad = space::ROOM;
    let x = rect.min().x + pad;
    let inner = rect.width() - pad * 2.0;
    let mut y = rect.min().y + pad;

    // Time and battery, the two things worth knowing at a glance.
    push_text(
        frame,
        Id::of("sc-time"),
        time.with_meridiem(),
        Vec2 { x, y },
        inner,
        text::TITLE,
        palette.text,
        Align::Left,
        reveal,
    );
    push_text(
        frame,
        Id::of("sc-battery"),
        format!("{}%", system.battery.percent()),
        Vec2 { x, y },
        inner,
        text::TITLE,
        if system.battery.low() {
            palette.accent
        } else {
            palette.text_soft
        },
        Align::Right,
        reveal,
    );

    y += text::TITLE.size * 1.35;
    push_text(
        frame,
        Id::of("sc-date"),
        time.long_date(),
        Vec2 { x, y },
        inner,
        text::LABEL,
        palette.text_faint,
        Align::Left,
        reveal,
    );

    y += space::OPEN + space::SNUG;

    // Two columns of toggles, exactly as the specification lays them out.
    let column = inner * 0.5;
    let rows: [[(&str, String, bool); 2]; 2] = [
        [
            ("Wi-Fi", connection_label(system), !system.airplane),
            ("Airplane", on_off(system.airplane), system.airplane),
        ],
        [
            (
                "Bluetooth",
                system.bluetooth.unwrap_or("Off").to_string(),
                system.bluetooth.is_some(),
            ),
            ("Focus", focus_label(focus), focus.is_some()),
        ],
    ];

    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, (name, value, active)) in row.iter().enumerate() {
            let cx = x + column_index as f32 * column;
            push_text(
                frame,
                Id::of("sc-name").nth((row_index * 2 + column_index) as u64),
                name.to_string(),
                Vec2 { x: cx, y },
                column,
                text::LABEL,
                palette.text,
                Align::Left,
                reveal,
            );
            push_text(
                frame,
                Id::of("sc-value").nth((row_index * 2 + column_index) as u64),
                value.clone(),
                Vec2 {
                    x: cx,
                    y: y + text::LABEL.size * 1.45,
                },
                column,
                text::CAPTION,
                if *active {
                    palette.text_soft
                } else {
                    palette.text_faint
                },
                Align::Left,
                reveal,
            );
        }
        y += space::WIDE + space::TIGHT;
    }

    // Volume and brightness, the two continuous controls.
    for (index, (name, value)) in [("Volume", system.volume), ("Brightness", system.brightness)]
        .iter()
        .enumerate()
    {
        push_text(
            frame,
            Id::of("sc-slider-name").nth(index as u64),
            name.to_string(),
            Vec2 { x, y },
            inner,
            text::LABEL,
            palette.text,
            Align::Left,
            reveal,
        );
        y += text::LABEL.size * 1.6;
        slider(
            frame,
            Id::of("sc-slider").nth(index as u64),
            Vec2 { x, y },
            inner,
            *value,
            palette,
            reveal,
        );
        y += space::ROOM + space::SNUG;
    }
}

fn slider(
    frame: &mut Frame,
    id: Id,
    at: Vec2,
    width: f32,
    value: f32,
    palette: &Palette,
    opacity: f32,
) {
    let height = 4.0;
    let value = value.clamp(0.0, 1.0);

    frame.push(
        Item::new(
            id,
            Layer::Overlay,
            Primitive::Fill(Fill {
                rect: Rect::from_min_size(
                    at,
                    Vec2 {
                        x: width,
                        y: height,
                    },
                ),
                radius: height * 0.5,
                squircle: 4.0,
                color: palette.text_faint.alpha(0.22),
            }),
        )
        .opacity(opacity)
        .z(1),
    );

    frame.push(
        Item::new(
            id.nth(1),
            Layer::Overlay,
            Primitive::Fill(Fill {
                rect: Rect::from_min_size(
                    at,
                    Vec2 {
                        x: width * value,
                        y: height,
                    },
                ),
                radius: height * 0.5,
                squircle: 4.0,
                color: palette.text_soft,
            }),
        )
        .opacity(opacity)
        .z(2),
    );
}

#[allow(clippy::too_many_arguments)]
fn push_text(
    frame: &mut Frame,
    id: Id,
    label: String,
    at: Vec2,
    width: f32,
    style: detends_paint::TextStyle,
    color: Color,
    align: Align,
    opacity: f32,
) {
    frame.push(
        Item::new(
            id,
            Layer::Overlay,
            Primitive::Text(Text {
                text: label.into(),
                rect: Rect::from_min_size(
                    at,
                    Vec2 {
                        x: width,
                        y: style.size * 1.7,
                    },
                ),
                size: style.size,
                weight: style.weight,
                tracking: style.tracking,
                line_height: style.line_height,
                color,
                align,
            }),
        )
        .opacity(opacity)
        .z(3),
    );
}

fn connection_label(system: &System) -> String {
    use crate::system::Network;
    match (system.airplane, system.network) {
        (true, _) => "Off".into(),
        (_, Network::Wifi { .. }) => "Connected".into(),
        (_, Network::Wired) => "Wired".into(),
        (_, Network::Offline) => "Not connected".into(),
    }
}

fn on_off(state: bool) -> String {
    if state {
        "On".into()
    } else {
        "Off".into()
    }
}

fn focus_label(focus: Option<&Focus>) -> String {
    match focus {
        Some(f) => f.name.clone().unwrap_or_else(|| "On".into()),
        None => "Off".into(),
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect {
        center: Vec2 {
            x: lerp(a.center.x, b.center.x, t),
            y: lerp(a.center.y, b.center.y, t),
        },
        half: Vec2 {
            x: lerp(a.half.x, b.half.x, t),
            y: lerp(a.half.y, b.half.y, t),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;
    use detends_time::Clock;

    fn cluster() -> Rect {
        Rect::from_min_size(vec2(1250.0, 20.0), vec2(220.0, 34.0))
    }

    fn render(progress: f32, system: &System, focus: Option<&Focus>) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let time = Clock::frozen_at(2026, 9, 17, 17, 14, "UTC").unwrap().now();
        draw(
            &mut frame,
            &Palette::dark(),
            system,
            focus,
            &time,
            cluster(),
            vec2(1512.0, 982.0),
            progress,
        );
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
    fn closed_it_draws_nothing_at_all() {
        assert!(render(0.0, &System::default(), None).items.is_empty());
    }

    #[test]
    fn it_grows_out_of_the_cluster() {
        // Rule 10: the panel is the cluster expanding, so at the very start of
        // the transition its geometry *is* the cluster's.
        let frame = render(0.02, &System::default(), None);
        let pane = frame.items[0].primitive.bounds();
        assert!(
            (pane.width() - cluster().width()).abs() < 12.0,
            "started at {:?}",
            pane.size()
        );

        let open = render(1.0, &System::default(), None).items[0]
            .primitive
            .bounds();
        assert!((open.width() - WIDTH).abs() < 0.5);
        assert!(open.width() > cluster().width() * 1.4, "did not grow");
    }

    #[test]
    fn it_stays_anchored_to_the_upper_right() {
        // It must not drift to the middle of the screen on the way open.
        for progress in [0.1_f32, 0.5, 1.0] {
            let pane = render(progress, &System::default(), None).items[0]
                .primitive
                .bounds();
            assert!(
                pane.max().x <= 1512.0 - space::ROOM + 1.0,
                "escaped the right edge"
            );
            assert!(pane.center.x > 1512.0 * 0.6, "drifted left at {progress}");
        }
    }

    #[test]
    fn the_contents_wait_until_there_is_room_for_them() {
        // Otherwise the text appears crushed inside a panel still opening.
        assert_eq!(
            render(0.2, &System::default(), None).items.len(),
            1,
            "text arrived too early"
        );
        assert!(render(1.0, &System::default(), None).items.len() > 10);
    }

    #[test]
    fn it_shows_everything_the_specification_asks_for() {
        let s = strings(&render(1.0, &System::default(), None));
        for expected in [
            "5:14 PM",
            "87%",
            "Thursday, September 17",
            "Wi-Fi",
            "Airplane",
            "Bluetooth",
            "Focus",
            "Volume",
            "Brightness",
        ] {
            assert!(
                s.iter().any(|t| t == expected),
                "missing {expected:?} in {s:?}"
            );
        }
    }

    #[test]
    fn it_holds_no_actions_only_states() {
        // Rule 3 and 4, checked literally: momentary actions must not appear.
        let s = strings(&render(1.0, &System::default(), None))
            .join(" ")
            .to_lowercase();
        for forbidden in ["screenshot", "record", "capture", "screen recording"] {
            assert!(!s.contains(forbidden), "System Center offers {forbidden:?}");
        }
    }

    #[test]
    fn airplane_mode_is_reflected_across_every_row() {
        let mut system = System {
            bluetooth: Some("AirPods"),
            ..Default::default()
        };
        system.set_airplane(true);

        let s = strings(&render(1.0, &system, None));
        let joined = s.join(" | ");
        assert!(joined.contains("Off"), "airplane state not shown: {joined}");
        assert!(!joined.contains("Connected"), "still claims a connection");
    }

    #[test]
    fn a_named_focus_session_is_named_here() {
        let focus = Focus::begin(0.0, Some("Physics homework".into()), Some(1500.0));
        let s = strings(&render(1.0, &System::default(), Some(&focus)));
        assert!(s.iter().any(|t| t == "Physics homework"), "got {s:?}");
    }

    #[test]
    fn the_sliders_reflect_their_values() {
        let system = System {
            volume: 0.25,
            brightness: 0.9,
            ..Default::default()
        };
        let frame = render(1.0, &system, None);

        let width_of = |id: Id| {
            frame
                .items
                .iter()
                .find(|i| i.id == id)
                .map(|i| i.primitive.bounds().width())
        };
        let volume = width_of(Id::of("sc-slider").nth(0).nth(1)).expect("volume fill");
        let brightness = width_of(Id::of("sc-slider").nth(1).nth(1)).expect("brightness fill");
        assert!(
            brightness > volume * 2.0,
            "volume {volume}, brightness {brightness}"
        );
    }

    #[test]
    fn it_never_hangs_off_the_bottom_of_a_short_screen() {
        let place = resting_place(cluster(), vec2(1512.0, 420.0));
        assert!(
            place.max().y <= 420.0 - space::ROOM + 1.0,
            "ran off the bottom"
        );
    }
}
