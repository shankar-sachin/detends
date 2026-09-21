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
    space, text, Align, Color, Fill, Frame, Glass, Icon, IconShape, Id, Item, Layer, Palette,
    Primitive, Rect, Text, Vec2, ICON_STROKE,
};
use detends_time::TimeOfDay;

pub const PANEL: Id = Id::of("system-center");

/// Panel size, in logical units.
const WIDTH: f32 = 340.0;

/// The lit disc behind a toggle's icon.
const PILL: f32 = 32.0;
/// The mark beside a slider's name.
const SLIDER_ICON: f32 = 16.0;
/// How thick a slider's capsule is.
const TRACK: f32 = 22.0;
// Sized to its contents. The panel held a third of empty glass below the
// brightness slider, which read as something failing to load.
const HEIGHT: f32 = 344.0;

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


/// A control in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Wifi,
    Airplane,
    Bluetooth,
    Focus,
    Volume,
    Brightness,
}

/// Where every control sits inside an open panel.
///
/// One function, used by drawing and by hit-testing alike, so what you see and
/// what you can press can never drift apart. This is the same discipline Home
/// and Clock follow, and it matters more here: a toggle that draws in one place
/// and responds in another is indistinguishable from a broken toggle.
pub struct Layout {
    pub toggles: [(Control, Rect); 4],
    pub sliders: [(Control, Rect); 2],
}

/// The vertical rhythm of the panel body, shared by `draw` and `layout`.
///
/// Returns the y of the first toggle row, and the step between rows.
fn body_origin(rect: Rect) -> (f32, f32, f32, f32) {
    let pad = space::ROOM;
    let x = rect.min().x + pad;
    let inner = rect.width() - pad * 2.0;

    // Time, then date, then the gap before the toggles — the same arithmetic
    // `draw` walks through.
    let mut y = rect.min().y + pad;
    y += text::TITLE.size * 1.35;
    y += space::OPEN + space::SNUG;

    (x, y, inner, space::WIDE + space::TIGHT)
}

pub fn layout(panel: Rect) -> Layout {
    let (x, first_row, inner, row_step) = body_origin(panel);
    let column = inner * 0.5;

    // A toggle's target is the whole cell, not just its label: a two-line
    // control that only responds on the word itself is a control people miss.
    let cell = Vec2 {
        x: column,
        y: row_step * 0.9,
    };

    let order = [
        Control::Wifi,
        Control::Airplane,
        Control::Bluetooth,
        Control::Focus,
    ];
    let mut toggles = [(Control::Wifi, Rect::ZERO); 4];
    for (index, control) in order.iter().enumerate() {
        let (row, col) = (index / 2, index % 2);
        toggles[index] = (
            *control,
            Rect::from_min_size(
                Vec2 {
                    x: x + col as f32 * column,
                    y: first_row + row as f32 * row_step - text::LABEL.size * 0.5,
                },
                cell,
            ),
        );
    }

    let mut y = first_row + 2.0 * row_step;
    let mut sliders = [(Control::Volume, Rect::ZERO); 2];
    for (index, control) in [Control::Volume, Control::Brightness].iter().enumerate() {
        // The label row, then the capsule — the same two steps `draw` takes.
        y += text::LABEL.size + space::SNUG;
        sliders[index] = (
            *control,
            Rect::from_min_size(Vec2 { x, y }, Vec2 { x: inner, y: TRACK }),
        );
        y += TRACK + space::ROOM;
    }

    Layout { toggles, sliders }
}

/// Which control a point lands on, if any.
pub fn hit(panel: Rect, at: Vec2) -> Option<Control> {
    let layout = layout(panel);
    layout
        .sliders
        .iter()
        .chain(layout.toggles.iter())
        .find(|(_, rect)| rect.contains(at))
        .map(|(control, _)| *control)
}

/// Where along a slider a point falls, 0 to 1.
pub fn slider_value(panel: Rect, control: Control, at: Vec2) -> Option<f32> {
    let layout = layout(panel);
    let (_, rect) = layout
        .sliders
        .iter()
        .find(|(c, _)| *c == control)?;
    Some(((at.x - rect.min().x) / rect.width().max(1.0)).clamp(0.0, 1.0))
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
    // `body_origin` derives the toggle rows from exactly this arithmetic; if
    // the two ever disagree, `the_layout_matches_what_is_drawn` fails.

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
    frame.push(
        Item::new(
            Id::of("sc-battery-icon"),
            Layer::Overlay,
            Primitive::Icon(Icon {
                rect: Rect::from_center_size(
                    Vec2 {
                        // "87%" at title size is about fifty units wide and is
                        // right-aligned to the inner edge, so this lands just
                        // to its left with a hair of space.
                        x: x + inner - 72.0,
                        y: y + text::TITLE.size * 0.42,
                    },
                    Vec2::splat(17.0),
                ),
                shape: IconShape::Battery,
                stroke: ICON_STROKE,
                color: if system.battery.low() {
                    palette.accent
                } else {
                    palette.text_faint
                },
                rim: 0.2,
            }),
        )
        .opacity(reveal)
        .z(2),
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
    //
    // Each one is an icon, a name and its state. The icon is what makes a
    // control panel readable at a glance — four rows of words all look alike,
    // and the thing you are reaching for is recognised by its shape long
    // before you have read anything.
    let column = inner * 0.5;
    let rows: [[(IconShape, &str, String, bool); 2]; 2] = [
        [
            (
                IconShape::Wifi,
                "Wi-Fi",
                connection_label(system),
                system.wifi_on(),
            ),
            (
                IconShape::Airplane,
                "Airplane",
                on_off(system.airplane),
                system.airplane,
            ),
        ],
        [
            (
                IconShape::Bluetooth,
                "Bluetooth",
                system.bluetooth.unwrap_or("Off").to_string(),
                system.bluetooth.is_some(),
            ),
            (
                IconShape::Focus,
                "Focus",
                focus_label(focus),
                focus.is_some(),
            ),
        ],
    ];

    let toggles = layout(rect).toggles;

    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, (shape, name, value, active)) in row.iter().enumerate() {
            let index = row_index * 2 + column_index;
            let cx = x + column_index as f32 * column;
            let cell = toggles[index].1;

            // A filled disc when the control is on. This is the one place in
            // détends that draws a container around something, and it earns it:
            // a toggle has to say which of two states it is in, and colour
            // alone does not survive being glanced at.
            let badge = Rect::from_center_size(
                Vec2 { x: cx + PILL * 0.5, y: cell.center.y },
                Vec2::splat(PILL),
            );
            frame.push(
                Item::new(
                    Id::of("sc-badge").nth(index as u64),
                    Layer::Overlay,
                    Primitive::Fill(Fill {
                        rect: badge,
                        radius: PILL * 0.5,
                        squircle: 2.0,
                        color: if *active {
                            palette.accent.fade(0.9)
                        } else {
                            palette.text_faint.fade(0.16)
                        },
                    }),
                )
                .opacity(reveal)
                .z(1),
            );

            frame.push(
                Item::new(
                    Id::of("sc-icon").nth(index as u64),
                    Layer::Overlay,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(badge.center, Vec2::splat(PILL * 0.52)),
                        shape: *shape,
                        stroke: ICON_STROKE,
                        // Dark on the lit badge, light on the unlit one: the
                        // icon stays legible either way rather than
                        // disappearing into whichever it is sitting on.
                        color: if *active {
                            palette.ground
                        } else {
                            palette.text_soft
                        },
                        rim: 0.0,
                    }),
                )
                .opacity(reveal)
                .z(2),
            );

            let text_x = cx + PILL + space::SNUG;
            let text_width = column - PILL - space::SNUG;

            push_text(
                frame,
                Id::of("sc-name").nth(index as u64),
                name.to_string(),
                Vec2 {
                    x: text_x,
                    y: cell.center.y - text::LABEL.size * 0.62,
                },
                text_width,
                text::LABEL,
                palette.text,
                Align::Left,
                reveal,
            );
            push_text(
                frame,
                Id::of("sc-value").nth(index as u64),
                value.clone(),
                Vec2 {
                    x: text_x,
                    y: cell.center.y + text::CAPTION.size * 0.75,
                },
                text_width,
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
    //
    // The mark sits beside its name rather than beside the track, so the label
    // row and the toggle rows above it share one left edge, and the track runs
    // the full width beneath. Putting the icon next to the track instead left
    // the label hanging over the icon and the track starting somewhere else —
    // three left edges in four lines.
    for (index, (shape, name, value)) in [
        (
            if system.volume <= 0.001 {
                IconShape::SpeakerMuted
            } else {
                IconShape::Speaker
            },
            "Volume",
            system.volume,
        ),
        (IconShape::Brightness, "Brightness", system.brightness),
    ]
    .iter()
    .enumerate()
    {
        frame.push(
            Item::new(
                Id::of("sc-slider-icon").nth(index as u64),
                Layer::Overlay,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(
                        Vec2 {
                            x: x + SLIDER_ICON * 0.5,
                            y: y + text::LABEL.size * 0.5,
                        },
                        Vec2::splat(SLIDER_ICON),
                    ),
                    shape: *shape,
                    stroke: ICON_STROKE,
                    color: palette.text_soft,
                    rim: 0.2,
                }),
            )
            .opacity(reveal)
            .z(2),
        );

        push_text(
            frame,
            Id::of("sc-slider-name").nth(index as u64),
            name.to_string(),
            Vec2 {
                x: x + SLIDER_ICON + space::SNUG,
                y,
            },
            inner - SLIDER_ICON - space::SNUG,
            text::LABEL,
            palette.text,
            Align::Left,
            reveal,
        );

        y += text::LABEL.size + space::SNUG;
        slider(
            frame,
            Id::of("sc-slider").nth(index as u64),
            Vec2 { x, y },
            inner,
            *value,
            palette,
            reveal,
        );
        y += TRACK + space::ROOM;
    }
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

/// A continuous control, drawn as a capsule.
///
/// Thicker than a hairline on purpose. A 4px rule is a fine thing to look at
/// and a poor thing to aim at, and at this size the filled portion reads as a
/// quantity rather than as a line that happens to change length.
fn slider(
    frame: &mut Frame,
    id: Id,
    at: Vec2,
    width: f32,
    value: f32,
    palette: &Palette,
    opacity: f32,
) {
    let value = value.clamp(0.0, 1.0);
    let rect = Rect::from_min_size(at, Vec2 { x: width, y: TRACK });
    let radius = TRACK * 0.5;

    frame.push(
        Item::new(
            id,
            Layer::Overlay,
            Primitive::Fill(Fill {
                rect,
                radius,
                squircle: 2.0,
                color: palette.text_faint.alpha(0.18),
            }),
        )
        .opacity(opacity)
        .z(1),
    );

    // The fill is never narrower than the capsule is round: a rounded rect
    // thinner than its own radius collapses into a lens, which at zero reads
    // as a stray dot rather than as an empty control.
    let filled = (width * value).max(TRACK);
    if value > 0.001 {
        frame.push(
            Item::new(
                id.nth(1),
                Layer::Overlay,
                Primitive::Fill(Fill {
                    rect: Rect::from_min_size(at, Vec2 { x: filled, y: TRACK }),
                    radius,
                    squircle: 2.0,
                    color: palette.text_soft,
                }),
            )
            .opacity(opacity)
            .z(2),
        );
    }
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


    
    fn panel() -> Rect {
        resting_place(
            Rect::from_min_size(vec2(1150.0, 24.0), vec2(300.0, 40.0)),
            vec2(1512.0, 982.0),
        )
    }

    #[test]
    fn every_control_has_somewhere_to_be_pressed() {
        let panel = panel();
        let layout = layout(panel);

        for (control, rect) in layout.toggles.iter().chain(layout.sliders.iter()) {
            assert!(rect.width() > 0.0 && rect.height() > 0.0, "{control:?} has no target");
            assert_eq!(hit(panel, rect.center), Some(*control));
        }
    }

    #[test]
    fn the_controls_are_the_states_the_specification_lists() {
        // States only. A screenshot button here would be a rule-4 violation.
        let layout = layout(panel());
        let names: Vec<_> = layout.toggles.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            names,
            [Control::Wifi, Control::Airplane, Control::Bluetooth, Control::Focus]
        );
    }

    #[test]
    fn no_two_controls_overlap() {
        // Overlapping targets mean one control is unreachable.
        let layout = layout(panel());
        let all: Vec<Rect> = layout
            .toggles
            .iter()
            .chain(layout.sliders.iter())
            .map(|(_, r)| *r)
            .collect();

        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                let apart = a.max().x <= b.min().x
                    || b.max().x <= a.min().x
                    || a.max().y <= b.min().y
                    || b.max().y <= a.min().y;
                assert!(apart, "two controls overlap: {a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn everything_stays_inside_the_panel() {
        let panel = panel();
        let layout = layout(panel);
        for (control, rect) in layout.toggles.iter().chain(layout.sliders.iter()) {
            assert!(
                rect.min().x >= panel.min().x && rect.max().x <= panel.max().x,
                "{control:?} runs outside the panel horizontally"
            );
            assert!(
                rect.min().y >= panel.min().y && rect.max().y <= panel.max().y,
                "{control:?} runs outside the panel vertically"
            );
        }
    }

    #[test]
    fn a_point_outside_every_control_lands_on_nothing() {
        let panel = panel();
        assert_eq!(hit(panel, panel.min()), None);
        assert_eq!(hit(panel, vec2(10.0, 900.0)), None);
    }

    #[test]
    fn dragging_a_slider_reads_left_to_right() {
        let panel = panel();
        let (_, rect) = layout(panel).sliders[0];

        assert_eq!(slider_value(panel, Control::Volume, rect.min()), Some(0.0));
        assert_eq!(slider_value(panel, Control::Volume, rect.max()), Some(1.0));

        let middle = slider_value(panel, Control::Volume, rect.center).unwrap();
        assert!((middle - 0.5).abs() < 0.01, "got {middle}");
    }

    #[test]
    fn dragging_past_the_ends_clamps_rather_than_running_away() {
        let panel = panel();
        let (_, rect) = layout(panel).sliders[1];
        let far_left = vec2(rect.min().x - 500.0, rect.center.y);
        let far_right = vec2(rect.max().x + 500.0, rect.center.y);

        assert_eq!(slider_value(panel, Control::Brightness, far_left), Some(0.0));
        assert_eq!(slider_value(panel, Control::Brightness, far_right), Some(1.0));
    }

    #[test]
    fn the_layout_matches_what_is_drawn() {
        // draw() and layout() walk the same arithmetic through `body_origin`.
        // This pins the toggle rows to where the drawn labels actually land.
        let panel = panel();
        let layout = layout(panel);
        let (x, first_row, inner, step) = body_origin(panel);

        assert!((layout.toggles[0].1.min().x - x).abs() < 0.01);
        assert!((layout.toggles[1].1.min().x - (x + inner * 0.5)).abs() < 0.01);
        assert!((layout.toggles[2].1.min().y - layout.toggles[0].1.min().y - step).abs() < 0.01);
        assert!(layout.toggles[0].1.min().y < first_row + 1.0);
    }
}
