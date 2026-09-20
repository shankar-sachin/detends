//! Notifications.
//!
//! "Notifications should be rare… appear briefly, use small glass surfaces,
//! avoid blocking content, disappear automatically." (§15)
//!
//! And during Focus they are collected silently rather than shown, because
//! Focus exists to not be interrupted (§12). Collected, not discarded — the
//! user finds out what happened when the session ends.

use detends_paint::{
    space, springs, text, Align, Frame, Glass, Icon, IconShape, Id, Item, Layer, Palette, Primitive,
    Rect, Seconds, Spring, Text, Vec2, ICON_STROKE,
};

/// How long one stays on screen.
const DWELL: Seconds = 4.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub detail: String,
    pub icon: IconShape,
}

struct Showing {
    notice: Notice,
    since: Seconds,
    presence: Spring<f32>,
    /// Set once it has been asked to leave, so it is not asked twice.
    leaving: bool,
}

#[derive(Default)]
pub struct Notifications {
    showing: Option<Showing>,
    /// Waiting their turn, or held back by Focus.
    queued: Vec<Notice>,
    /// Arrived during Focus. Kept so the user can find out afterwards.
    collected: Vec<Notice>,
}

impl Notifications {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raise a notice.
    ///
    /// During Focus it is collected instead of shown. Nothing is dropped.
    pub fn post(&mut self, notice: Notice, focused: bool) {
        if focused {
            self.collected.push(notice);
            return;
        }
        self.queued.push(notice);
    }

    /// What arrived during Focus.
    pub fn collected(&self) -> &[Notice] {
        &self.collected
    }

    /// Hand back what was collected, emptying the list.
    pub fn take_collected(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.collected)
    }

    pub fn is_showing(&self) -> bool {
        self.showing.is_some()
    }

    pub fn update(&mut self, now: Seconds) {
        // Promote the next one when nothing is on screen.
        if self.showing.is_none() {
            if self.queued.is_empty() {
                return;
            }
            let notice = self.queued.remove(0);
            let mut presence = Spring::new(springs::SETTLE, 0.0);
            presence.target(now, 1.0);
            self.showing = Some(Showing {
                notice,
                since: now,
                presence,
                leaving: false,
            });
            return;
        }

        let Some(showing) = self.showing.as_mut() else {
            return;
        };

        // Ask it to leave once it has had its time.
        if !showing.leaving && now - showing.since >= DWELL {
            showing.presence.target(now, 0.0);
            showing.leaving = true;
        }

        // And drop it once it has gone, so it stops being drawn at all.
        if showing.leaving && showing.presence.at_rest(now) {
            self.showing = None;
        }
    }

    /// Dismiss whatever is on screen early.
    pub fn dismiss(&mut self, now: Seconds) {
        if let Some(showing) = self.showing.as_mut() {
            if !showing.leaving {
                showing.presence.target(now, 0.0);
                showing.leaving = true;
            }
        }
    }

    pub fn animating(&self, now: Seconds) -> bool {
        !self.queued.is_empty()
            || self
                .showing
                .as_ref()
                .is_some_and(|s| !s.presence.at_rest(now) || !s.leaving)
    }

    pub fn draw(&self, frame: &mut Frame, palette: &Palette, now: Seconds) {
        let Some(showing) = self.showing.as_ref() else {
            return;
        };
        let presence = showing.presence.value(now).clamp(0.0, 1.0);
        if presence <= 0.001 {
            return;
        }

        let width = 320.0_f32.min(frame.size.x - space::WIDE * 2.0);
        let height = 78.0;

        // Bottom centre: out of the way of both the content and the status
        // cluster, and nowhere near where anyone is reading.
        let rect = Rect::from_center_size(
            Vec2 {
                x: frame.size.x * 0.5,
                // Rises into place rather than appearing.
                y: frame.size.y - space::WIDE - height * 0.5 + (1.0 - presence) * 18.0,
            },
            Vec2 { x: width, y: height },
        );

        frame.push(
            Item::new(
                Id::of("notice"),
                Layer::Overlay,
                Primitive::Glass(Glass {
                    rect,
                    radius: 22.0,
                    squircle: 5.0,
                    thickness: 12.0,
                    bevel: 20.0,
                    ior: 1.47,
                    dispersion: 0.016,
                    frost: 0.72,
                    tint: palette.glass,
                    rim: 0.9,
                }),
            )
            .opacity(presence),
        );

        let glyph = 24.0;
        frame.push(
            Item::new(
                Id::of("notice-icon"),
                Layer::Overlay,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(
                        Vec2 { x: rect.min().x + space::ROOM + glyph * 0.5, y: rect.center.y },
                        Vec2::splat(glyph),
                    ),
                    shape: showing.notice.icon,
                    stroke: ICON_STROKE,
                    color: palette.text_soft,
                    rim: 0.5,
                }),
            )
            .opacity(presence)
            .z(1),
        );

        let left = rect.min().x + space::ROOM * 2.0 + glyph;
        let text_width = rect.max().x - left - space::ROOM;

        frame.push(
            Item::new(
                Id::of("notice-title"),
                Layer::Overlay,
                Primitive::Text(Text {
                    text: showing.notice.title.clone().into(),
                    rect: Rect::from_min_size(
                        Vec2 { x: left, y: rect.center.y - space::ROOM },
                        Vec2 { x: text_width, y: text::HEADING.size * 1.6 },
                    ),
                    size: text::HEADING.size,
                    weight: text::HEADING.weight,
                    tracking: text::HEADING.tracking,
                    line_height: text::HEADING.line_height,
                    color: palette.text,
                    align: Align::Left,
                }),
            )
            .opacity(presence)
            .z(2),
        );

        frame.push(
            Item::new(
                Id::of("notice-detail"),
                Layer::Overlay,
                Primitive::Text(Text {
                    text: showing.notice.detail.clone().into(),
                    rect: Rect::from_min_size(
                        Vec2 { x: left, y: rect.center.y + space::TIGHT },
                        Vec2 { x: text_width, y: text::CAPTION.size * 2.0 },
                    ),
                    size: text::CAPTION.size,
                    weight: text::CAPTION.weight,
                    tracking: text::CAPTION.tracking,
                    line_height: text::CAPTION.line_height,
                    color: palette.text_faint,
                    align: Align::Left,
                }),
            )
            .opacity(presence)
            .z(2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn notice(title: &str) -> Notice {
        Notice {
            title: title.into(),
            detail: "Timer finished".into(),
            icon: IconShape::Timer,
        }
    }

    fn render(notifications: &Notifications, now: Seconds) -> Frame {
        let mut frame = Frame::new(vec2(1512.0, 982.0), 2.0);
        notifications.draw(&mut frame, &Palette::dark(), now);
        frame
    }

    #[test]
    fn a_notice_appears_and_then_leaves_on_its_own() {
        // §15: brief, and gone without being dismissed.
        let mut n = Notifications::new();
        n.post(notice("Bread"), false);

        n.update(0.0);
        assert!(n.is_showing());
        assert!(!render(&n, 0.5).items.is_empty());

        n.update(DWELL + 0.1);
        n.update(DWELL + 2.0);
        assert!(!n.is_showing(), "it never left");
        assert!(render(&n, DWELL + 2.0).items.is_empty());
    }

    #[test]
    fn during_focus_it_is_collected_rather_than_shown() {
        // §12: Focus is not interrupted. But nothing is thrown away.
        let mut n = Notifications::new();
        n.post(notice("Bread"), true);
        n.update(0.0);

        assert!(!n.is_showing(), "Focus was interrupted");
        assert_eq!(n.collected().len(), 1);
        assert!(render(&n, 0.0).items.is_empty());
    }

    #[test]
    fn what_was_collected_can_be_handed_back_afterwards() {
        let mut n = Notifications::new();
        n.post(notice("Bread"), true);
        n.post(notice("Alarm"), true);

        let held = n.take_collected();
        assert_eq!(held.len(), 2);
        assert!(n.collected().is_empty(), "they were handed back twice");
    }

    #[test]
    fn notices_queue_rather_than_stacking_up_on_screen() {
        // Rule 2: one thing owns attention. Three at once is a notification
        // centre, which §15 explicitly does not want.
        let mut n = Notifications::new();
        n.post(notice("One"), false);
        n.post(notice("Two"), false);
        n.post(notice("Three"), false);

        n.update(0.0);
        let frame = render(&n, 0.5);
        let panes = frame
            .items
            .iter()
            .filter(|i| matches!(i.primitive, Primitive::Glass(_)))
            .count();
        assert_eq!(panes, 1, "more than one notice on screen");
    }

    #[test]
    fn the_queue_drains_one_at_a_time() {
        let mut n = Notifications::new();
        n.post(notice("One"), false);
        n.post(notice("Two"), false);

        n.update(0.0);
        assert!(n.is_showing());

        // First leaves, second arrives.
        n.update(DWELL + 0.1);
        n.update(DWELL + 2.0);
        n.update(DWELL + 2.1);
        assert!(n.is_showing(), "the second notice never appeared");
    }

    #[test]
    fn it_can_be_dismissed_early() {
        let mut n = Notifications::new();
        n.post(notice("Bread"), false);
        n.update(0.0);

        n.dismiss(0.5);
        n.update(2.5);
        assert!(!n.is_showing());
    }

    #[test]
    fn it_sits_at_the_bottom_where_it_blocks_nothing() {
        // §15: avoid blocking content. The clock and the cluster are both in
        // the upper half.
        let mut n = Notifications::new();
        n.post(notice("Bread"), false);
        n.update(0.0);

        let frame = render(&n, 1.0);
        let pane = frame.items[0].primitive.bounds();
        assert!(pane.center.y > 982.0 * 0.75, "it is over the content");
        assert!(pane.max().y < 982.0, "it ran off the bottom");
    }

    #[test]
    fn nothing_animates_once_the_queue_is_empty() {
        let mut n = Notifications::new();
        assert!(!n.animating(0.0));

        n.post(notice("Bread"), false);
        assert!(n.animating(0.0));

        n.update(0.0);
        n.update(DWELL + 0.1);
        n.update(DWELL + 3.0);
        assert!(!n.animating(DWELL + 3.0), "still asking for frames");
    }
}
