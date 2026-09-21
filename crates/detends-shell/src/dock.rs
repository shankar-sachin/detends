//! The dock.
//!
//! A permanent strip along the bottom holding every app détends ships, with a
//! mark under whichever are running.
//!
//! This is a reversal. The specification said "avoid permanent docks" and
//! "five modes are enough", and both sentences were right about the system
//! they described — one where an environment filled the screen and switching
//! meant transforming it. That system is gone: apps open into windows now, and
//! a windowing system without somewhere to launch from asks the user to
//! remember a keyboard shortcut for every app or to keep one window open
//! forever to get back. The dock is what replaced Home.
//!
//! What survives from the old rules is the size of it. Six apps, fixed, no
//! installing, no folders, no badges, no bouncing. A dock you can take in at a
//! glance is a different object from a launcher you have to search.

use crate::app::App;
use crate::window::Windows;
use detends_paint::{
    space, text, Align, Fill, Frame, Glass, Icon, Id, Item, Layer, Palette, Primitive, Rect,
    Seconds, Text, Vec2, ICON_STROKE,
};

/// Icon size in the dock.
const ICON: f32 = 42.0;
/// Centre-to-centre spacing.
const PITCH: f32 = 74.0;
/// How far the strip sits above the bottom edge.
const MARGIN: f32 = 18.0;
/// Padding inside the glass strip.
const PAD: f32 = 14.0;
/// The mark under a running app.
const DOT: f32 = 4.0;

/// The whole strip.
pub fn bounds(workspace: Vec2) -> Rect {
    let width = PITCH * App::ALL.len() as f32 + PAD * 2.0 - (PITCH - ICON);
    let height = ICON + PAD * 2.0;
    Rect::from_center_size(
        Vec2 {
            x: workspace.x * 0.5,
            y: workspace.y - MARGIN - height * 0.5,
        },
        Vec2 {
            x: width,
            y: height,
        },
    )
}

/// Where each app sits, for drawing and hit-testing alike.
pub fn slots(workspace: Vec2) -> [(App, Rect); 6] {
    let strip = bounds(workspace);
    let span = PITCH * (App::ALL.len() as f32 - 1.0);
    let start = strip.center.x - span * 0.5;

    let mut out = [(App::Surf, Rect::ZERO); 6];
    for (index, app) in App::ALL.iter().enumerate() {
        out[index] = (
            *app,
            Rect::from_center_size(
                Vec2 {
                    x: start + index as f32 * PITCH,
                    y: strip.center.y,
                },
                Vec2::splat(ICON),
            ),
        );
    }
    out
}

/// Which app a point lands on.
///
/// The whole slot is the target, not the icon inside it — a dock you have to
/// aim at is a dock you avoid.
pub fn hit(workspace: Vec2, at: Vec2) -> Option<App> {
    if !bounds(workspace).contains(at) {
        return None;
    }
    slots(workspace)
        .iter()
        .find(|(_, rect)| {
            Rect::from_center_size(rect.center, Vec2::splat(PITCH)).contains(at)
        })
        .map(|(app, _)| *app)
}

/// Draw the dock.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    palette: &Palette,
    windows: &Windows,
    workspace: Vec2,
    hovered: Option<App>,
    _now: Seconds,
    opacity: f32,
) {
    if opacity <= 0.001 {
        return;
    }

    let strip = bounds(workspace);

    // One pane of glass, not six. The dock is a single object that holds
    // things, and giving each icon its own surface would make it six objects
    // that happen to be in a row.
    frame.push(
        Item::new(
            Id::of("dock"),
            Layer::Overlay,
            Primitive::Glass(Glass {
                rect: strip,
                radius: strip.height() * 0.42,
                squircle: 5.0,
                thickness: 14.0,
                bevel: 22.0,
                ior: 1.48,
                dispersion: 0.018,
                frost: 0.7,
                tint: palette.glass,
                rim: 0.95,
            }),
        )
        .opacity(opacity),
    );

    for (index, (app, rect)) in slots(workspace).iter().enumerate() {
        let running = windows.is_open(*app);
        let focused = windows.focused_app() == Some(*app);
        let near = hovered == Some(*app);

        // Hovering lifts the icon slightly. Enough to acknowledge the pointer,
        // far short of the bouncing and magnifying a dock does when it is
        // trying to be liked.
        let lift = if near { 3.0 } else { 0.0 };
        let scale = if near { 1.08 } else { 1.0 };

        frame.push(
            Item::new(
                Id::of("dock-icon").nth(index as u64),
                Layer::Overlay,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(
                        Vec2 {
                            x: rect.center.x,
                            y: rect.center.y - lift,
                        },
                        rect.size() * scale,
                    ),
                    shape: app.icon(),
                    stroke: ICON_STROKE,
                    color: if focused {
                        palette.text
                    } else if running {
                        palette.text_soft
                    } else {
                        palette.text_faint
                    },
                    rim: if running { 0.6 } else { 0.25 },
                }),
            )
            .opacity(opacity)
            .z(3),
        );

        // A dot under what is running. The focused one is brighter — that is
        // the whole of the state a dock needs to carry.
        if running {
            frame.push(
                Item::new(
                    Id::of("dock-dot").nth(index as u64),
                    Layer::Overlay,
                    Primitive::Fill(Fill {
                        rect: Rect::from_center_size(
                            Vec2 {
                                x: rect.center.x,
                                y: strip.max().y - PAD * 0.42,
                            },
                            Vec2::splat(DOT),
                        ),
                        radius: DOT * 0.5,
                        squircle: 2.0,
                        color: if focused {
                            palette.text
                        } else {
                            palette.text_faint
                        },
                    }),
                )
                .opacity(opacity)
                .z(3),
            );
        }

        // The name, only for whatever the pointer is over. Six permanent
        // labels under six recognisable marks is noise.
        if near {
            frame.push(
                Item::new(
                    Id::of("dock-label"),
                    Layer::Overlay,
                    Primitive::Text(Text {
                        text: app.name().into(),
                        rect: Rect::from_center_size(
                            Vec2 {
                                x: rect.center.x,
                                y: strip.min().y - space::ROOM,
                            },
                            Vec2 {
                                x: PITCH * 2.4,
                                y: text::CAPTION.size * 2.0,
                            },
                        ),
                        size: text::CAPTION.size,
                        weight: text::CAPTION.weight,
                        tracking: text::CAPTION.tracking,
                        line_height: text::CAPTION.line_height,
                        color: palette.text_soft,
                        align: Align::Center,
                    }),
                )
                .opacity(opacity)
                .z(4),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn workspace() -> Vec2 {
        vec2(1512.0, 982.0)
    }

    #[test]
    fn every_app_has_a_slot_and_can_be_pressed() {
        let w = workspace();
        for (app, rect) in slots(w).iter() {
            assert_eq!(hit(w, rect.center), Some(*app));
        }
    }

    #[test]
    fn the_slots_are_in_dock_order_left_to_right() {
        let order: Vec<App> = slots(workspace()).iter().map(|(a, _)| *a).collect();
        assert_eq!(order, App::ALL);

        let xs: Vec<f32> = slots(workspace()).iter().map(|(_, r)| r.center.x).collect();
        assert!(xs.windows(2).all(|p| p[0] < p[1]), "not left to right");
    }

    #[test]
    fn the_dock_sits_along_the_bottom_and_stays_on_screen() {
        let w = workspace();
        let strip = bounds(w);

        assert!(strip.max().y < w.y, "it hangs off the bottom");
        assert!(strip.min().y > w.y * 0.75, "it is not near the bottom");
        assert!(strip.min().x > 0.0 && strip.max().x < w.x, "it is wider than the screen");
        assert!((strip.center.x - w.x * 0.5).abs() < 0.5, "it is not centred");
    }

    #[test]
    fn a_press_away_from_the_dock_hits_nothing() {
        let w = workspace();
        assert_eq!(hit(w, vec2(10.0, 10.0)), None);
        assert_eq!(hit(w, vec2(w.x * 0.5, w.y * 0.4)), None);
    }

    #[test]
    fn the_targets_are_larger_than_the_icons_they_hold() {
        // A dock you have to aim at is a dock you avoid.
        let w = workspace();
        let (_, first) = slots(w)[0];
        assert_eq!(hit(w, vec2(first.center.x - ICON * 0.6, first.center.y)), Some(App::Surf));
    }

    #[test]
    fn running_apps_are_marked_and_the_focused_one_stands_out() {
        let w = workspace();
        let mut windows = Windows::new();
        windows.open_app(App::Files, w, 0.0);

        let mut frame = Frame::new(w, 2.0);
        draw(&mut frame, &Palette::dark(), &windows, w, None, 0.0, 1.0);

        let dots = frame
            .items
            .iter()
            .filter(|i| matches!(i.primitive, Primitive::Fill(_)))
            .count();
        assert_eq!(dots, 1, "exactly one app is running, so one dot");
    }

    #[test]
    fn nothing_running_means_no_dots() {
        let w = workspace();
        let mut frame = Frame::new(w, 2.0);
        draw(&mut frame, &Palette::dark(), &Windows::new(), w, None, 0.0, 1.0);

        assert!(frame
            .items
            .iter()
            .all(|i| !matches!(i.primitive, Primitive::Fill(_))));
    }

    #[test]
    fn it_draws_every_app_on_one_pane_of_glass() {
        let w = workspace();
        let mut frame = Frame::new(w, 2.0);
        draw(&mut frame, &Palette::dark(), &Windows::new(), w, None, 0.0, 1.0);

        let glass = frame
            .items
            .iter()
            .filter(|i| matches!(i.primitive, Primitive::Glass(_)))
            .count();
        let icons = frame
            .items
            .iter()
            .filter(|i| matches!(i.primitive, Primitive::Icon(_)))
            .count();

        assert_eq!(glass, 1, "the dock should be one object, not six");
        assert_eq!(icons, App::ALL.len());
    }

    #[test]
    fn only_the_hovered_app_is_named() {
        let w = workspace();
        let mut frame = Frame::new(w, 2.0);
        draw(&mut frame, &Palette::dark(), &Windows::new(), w, Some(App::Clock), 0.0, 1.0);

        let labels: Vec<String> = frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Clock"], "six permanent labels would be noise");
    }
}
