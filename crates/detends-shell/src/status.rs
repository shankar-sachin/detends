//! The status cluster.
//!
//! "The upper-right corner contains a minimal persistent status cluster… It
//! should expose only information useful at a glance: Focus · Network · Volume
//! · Battery · Time." (§10)
//!
//! This is the only permanently visible interface in détends, so every element
//! in it has to earn a place that nothing else gets. It is set in small type on
//! a barely-there pane — enough to stay legible over any content, not enough to
//! read as a bar.

use crate::system::{Focus, System};
use detends_paint::{
    space, text, Align, Color, Frame, Glass, Id, Item, Layer, Palette, Primitive, Rect, Text, Vec2,
};
use detends_time::TimeOfDay;

pub const CLUSTER: Id = Id::of("status-cluster");
const PANE: Id = Id::of("status-cluster-pane");

/// Distance from the top and right edges.
const INSET: f32 = space::ROOM;

/// Laid-out geometry, returned so System Center can grow from exactly where the
/// cluster is — the shared element that makes the panel feel like an expansion
/// of the thing you clicked rather than a new window (rule 10).
pub struct Cluster {
    pub bounds: Rect,
}

/// Draw the cluster, returning where it ended up.
///
/// `opacity` fades the whole thing — used during Focus, which makes détends
/// quieter rather than louder (§12).
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    system: &System,
    focus: Option<&Focus>,
    now: f64,
    time: &TimeOfDay,
    opacity: f32,
) -> Cluster {
    let style = text::LABEL;

    // Focus first, then the radios, then volume, battery, time. The order is
    // the specification's, and it runs from "state you chose" to "state the
    // machine is in".
    let mut segments: Vec<String> = Vec::with_capacity(5);
    if let Some(focus) = focus {
        segments.push(format!("◎ {}", focus.readout(now)));
    }
    for indicator in system.indicators() {
        segments.push(indicator.to_string());
    }
    segments.push(format!("{}%", system.battery.percent()));
    segments.push(time.short());

    let label = segments.join("   ");

    // Sized from the text rather than a fixed width, so the cluster contracts
    // under Airplane Mode instead of leaving a gap where the radios were.
    let width = label.chars().count() as f32 * style.size * 0.56 + space::ROOM * 2.0;
    let height = style.size + space::STEP * 1.5;

    let bounds = Rect::from_min_size(
        Vec2 {
            x: frame.size.x - INSET - width,
            y: INSET,
        },
        Vec2 {
            x: width,
            y: height,
        },
    );

    frame.push(
        Item::new(
            PANE,
            Layer::Content,
            Primitive::Glass(Glass {
                rect: bounds,
                radius: height * 0.5,
                squircle: 4.0,
                // Thin, because it is small: a thick slab at this size would be
                // all rim and no body.
                thickness: 6.0,
                bevel: 9.0,
                ior: 1.46,
                dispersion: 0.012,
                frost: 0.7,
                tint: palette.glass,
                rim: 0.55,
            }),
        )
        .opacity(opacity),
    );

    frame.push(
        Item::new(
            CLUSTER,
            Layer::Content,
            Primitive::Text(Text {
                text: label.into(),
                rect: bounds,
                size: style.size,
                weight: style.weight,
                tracking: style.tracking,
                // Centre the single line inside the pane.
                line_height: height / style.size,
                color: palette.text_soft,
                align: Align::Center,
            }),
        )
        .opacity(opacity)
        .z(1),
    );

    Cluster { bounds }
}

/// Where the current mode is named — small, top left, easily ignored.
///
/// There is no dock and no taskbar (§16), so this is the whole of the
/// permanent navigation affordance: it says where you are, and Super+1…5 and
/// Search are how you go elsewhere.
pub fn draw_mode_label(frame: &mut Frame, palette: &Palette, name: &str, opacity: f32) {
    let style = text::CAPTION;
    frame.push(
        Item::new(
            Id::of("mode-label"),
            Layer::Content,
            Primitive::Text(Text {
                text: name.to_uppercase().into(),
                rect: Rect::from_min_size(
                    Vec2 {
                        x: INSET + space::STEP,
                        y: INSET + space::STEP,
                    },
                    Vec2 {
                        x: 240.0,
                        y: style.size * 2.0,
                    },
                ),
                size: style.size,
                weight: style.weight,
                tracking: style.tracking * 2.5,
                line_height: style.line_height,
                color: palette.text_faint,
                align: Align::Left,
            }),
        )
        .opacity(opacity),
    );
}

/// Colour for the battery segment when it wants noticing.
pub fn battery_tint(palette: &Palette, system: &System) -> Color {
    if system.battery.low() {
        palette.accent
    } else {
        palette.text_soft
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::Network;
    use detends_paint::vec2;
    use detends_time::Clock;

    fn clock_at(h: i8, m: i8) -> Clock {
        Clock::frozen_at(2026, 9, 17, h, m, "UTC").expect("a valid instant")
    }

    fn cluster_text(frame: &Frame) -> String {
        frame
            .items
            .iter()
            .find_map(|i| match (&i.primitive, i.id == CLUSTER) {
                (Primitive::Text(t), true) => Some(t.text.to_string()),
                _ => None,
            })
            .expect("the cluster should have a label")
    }

    fn render(system: &System, focus: Option<&Focus>) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        let time = clock_at(17, 14).now();
        draw(&mut frame, &Palette::dark(), system, focus, 0.0, &time, 1.0);
        frame
    }

    #[test]
    fn it_shows_the_documented_information_and_no_more() {
        let frame = render(&System::default(), None);
        let label = cluster_text(&frame);

        assert!(label.contains("Wi-Fi"));
        assert!(label.contains("87%"));
        assert!(label.contains("5:14"));
        // Nothing the specification does not ask for.
        assert!(
            !label.contains("Bluetooth"),
            "no paired device, so no indicator"
        );
    }

    #[test]
    fn it_sits_in_the_upper_right() {
        let frame = render(&System::default(), None);
        let bounds = frame.items[0].primitive.bounds();
        assert!(
            bounds.max().x <= 1512.0 - INSET + 0.5,
            "not against the right edge"
        );
        assert!(bounds.min().y >= INSET - 0.5, "not against the top edge");
        assert!(bounds.center.x > 1512.0 * 0.6, "should be on the right");
        assert!(bounds.center.y < 982.0 * 0.2, "should be at the top");
    }

    #[test]
    fn airplane_mode_makes_it_narrower_not_wider() {
        // §11: the cluster simplifies itself rather than showing a row of
        // disconnected states. Narrower is the observable consequence.
        let mut system = System {
            bluetooth: Some("AirPods"),
            ..Default::default()
        };
        let wide = render(&system, None).items[0].primitive.bounds().width();

        system.set_airplane(true);
        let narrow = render(&system, None).items[0].primitive.bounds().width();

        assert!(
            narrow < wide,
            "airplane cluster is {narrow}, normal is {wide}"
        );
        assert!(cluster_text(&render(&system, None)).contains("✈︎"));
    }

    #[test]
    fn focus_adds_a_quiet_readout() {
        // §12 shows "◎ 42:18" sitting before the other indicators.
        let focus = Focus::begin(
            0.0,
            Some("Physics homework".into()),
            Some(42.0 * 60.0 + 18.0),
        );
        let label = cluster_text(&render(&System::default(), Some(&focus)));

        assert!(label.starts_with("◎ 42:18"), "got {label:?}");
        // Focus does not push anything else out.
        assert!(label.contains("5:14"));
    }

    #[test]
    fn a_paired_device_appears_only_when_there_is_one() {
        let mut system = System::default();
        assert!(!cluster_text(&render(&system, None)).contains("Bluetooth"));
        system.bluetooth = Some("AirPods");
        assert!(cluster_text(&render(&system, None)).contains("Bluetooth"));
    }

    #[test]
    fn it_is_a_single_pane_with_a_single_label() {
        // Rule 1, content beats chrome: the one permanent element in détends
        // must stay one pane and one line, never a bar of separate widgets.
        let frame = render(&System::default(), None);
        assert_eq!(
            frame.items.len(),
            2,
            "cluster should be one pane plus one label"
        );
        assert!(matches!(frame.items[0].primitive, Primitive::Glass(_)));
        assert!(matches!(frame.items[1].primitive, Primitive::Text(_)));
    }

    #[test]
    fn a_low_battery_is_the_only_thing_that_changes_colour() {
        let palette = Palette::dark();
        let mut system = System::default();
        assert_eq!(battery_tint(&palette, &system).r, palette.text_soft.r);

        system.battery.level = 0.08;
        assert_eq!(battery_tint(&palette, &system).r, palette.accent.r);
    }

    #[test]
    fn going_offline_unexpectedly_still_says_so() {
        let system = System {
            network: Network::Offline,
            ..Default::default()
        };
        assert!(cluster_text(&render(&system, None)).contains("Offline"));
    }
}
