//! Windows, and the order they stack in.
//!
//! détends used to switch between five environments. It now opens apps into
//! windows you can move, which means it needs the thing every windowing system
//! needs: a stack, a focused thing at the top of it, and a way to drag.
//!
//! It is deliberately *not* a full desktop window manager. There is no
//! minimise, no maximise, no tiling, no snapping and no per-window resize
//! handles — all of which exist to manage dozens of windows, and none of which
//! earn their place when the system ships six apps and expects one or two open.
//! An app opens at the size it asked for, sits where it is put, and closes.
//!
//! One app is focused at a time and it is drawn at full strength; everything
//! behind it recedes. That is the part of the old five-modes idea worth keeping
//! — attention still belongs to one thing.

use crate::app::App;
use detends_paint::{Rect, Seconds, Vec2};

/// Identifies one open window.
///
/// A counter rather than the app itself, because the same app may one day be
/// open twice and the stack has to keep them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// The bar along the top of a window, which is also its handle.
pub const TITLEBAR: f32 = 38.0;

/// How far each new window is offset from the last, so two open windows are
/// visibly two.
const CASCADE: f32 = 34.0;

/// How much of a window must stay on screen.
///
/// Dragging one entirely off the edge would leave no way to get it back, since
/// there is no window menu to recover it from.
const KEEP_VISIBLE: f32 = 90.0;

/// How large the traffic lights are, and how far apart.
const LIGHT: f32 = 13.0;
const LIGHT_GAP: f32 = 20.0;

#[derive(Clone, Debug)]
pub struct Window {
    pub id: WindowId,
    pub app: App,
    pub rect: Rect,
    /// Out of the way but still running.
    ///
    /// A minimised window keeps its place in the stack and its dot in the dock;
    /// it simply is not drawn. There is nowhere for it to shrink *to* — no
    /// window list, no exposé — so the dock is how it comes back, which is
    /// also why the dot has to stay.
    pub minimized: bool,
}

impl Window {
    /// The strip along the top that carries the name and drags the window.
    pub fn titlebar(&self) -> Rect {
        Rect::from_min_size(
            self.rect.min(),
            Vec2 {
                x: self.rect.width(),
                y: TITLEBAR,
            },
        )
    }

    /// Where the app itself draws, below the bar.
    pub fn content(&self) -> Rect {
        let min = self.rect.min();
        Rect::from_min_size(
            Vec2 {
                x: min.x,
                y: min.y + TITLEBAR,
            },
            Vec2 {
                x: self.rect.width(),
                y: (self.rect.height() - TITLEBAR).max(1.0),
            },
        )
    }

    /// The red dot, at the far right.
    ///
    /// Right rather than left: the close is the last thing on the bar in
    /// reading order, and putting the destructive one at the end of the row
    /// rather than the start means a stray press at the window's leading edge
    /// cannot shut it.
    pub fn close_button(&self) -> Rect {
        let bar = self.titlebar();
        Rect::from_center_size(
            Vec2 {
                x: bar.max().x - 20.0,
                y: bar.center.y,
            },
            Vec2::splat(LIGHT + 5.0),
        )
    }

    /// The yellow dot, just inside the red one.
    pub fn minimize_button(&self) -> Rect {
        let close = self.close_button();
        Rect::from_center_size(
            Vec2 {
                x: close.center.x - LIGHT_GAP,
                y: close.center.y,
            },
            Vec2::splat(LIGHT + 5.0),
        )
    }
}

/// Every open window, back to front.
pub struct Windows {
    /// Back to front: the last element is focused and drawn on top.
    open: Vec<Window>,
    next_id: u64,
    /// The window being dragged, and where inside it the pointer took hold.
    dragging: Option<(WindowId, Vec2)>,
    /// When the stack last changed, so arrival can be animated.
    pub changed_at: Seconds,
}

impl Default for Windows {
    fn default() -> Self {
        Self::new()
    }
}

impl Windows {
    pub fn new() -> Self {
        Windows {
            open: Vec::new(),
            next_id: 1,
            dragging: None,
            changed_at: 0.0,
        }
    }

    /// Back to front. The last one is focused.
    pub fn all(&self) -> &[Window] {
        &self.open
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// The focused window, if any.
    ///
    /// Minimised windows are not focusable: the topmost *visible* one is.
    pub fn focused(&self) -> Option<&Window> {
        self.open.iter().rev().find(|w| !w.minimized)
    }

    /// Put a window away without closing it.
    pub fn minimize(&mut self, id: WindowId, now: Seconds) {
        if let Some(window) = self.open.iter_mut().find(|w| w.id == id) {
            window.minimized = true;
        }
        self.dragging = None;
        self.changed_at = now;
    }

    /// Everything currently drawn, back to front.
    pub fn visible(&self) -> impl Iterator<Item = &Window> {
        self.open.iter().filter(|w| !w.minimized)
    }

    pub fn focused_app(&self) -> Option<App> {
        self.focused().map(|w| w.app)
    }

    pub fn is_open(&self, app: App) -> bool {
        self.open.iter().any(|w| w.app == app)
    }

    /// Open an app, or bring it forward if it is already open.
    ///
    /// Launching something already running focuses it rather than opening a
    /// second copy. With six apps and a dock, a second window of the same app
    /// is almost always a misclick.
    pub fn open_app(&mut self, app: App, workspace: Vec2, now: Seconds) -> WindowId {
        if let Some(index) = self.open.iter().position(|w| w.app == app) {
            let id = self.open[index].id;
            // The dock is the only way back from minimised, so launching has
            // to restore rather than merely focus.
            self.open[index].minimized = false;
            self.focus(id, now);
            return id;
        }

        let (fraction_w, fraction_h) = app.preferred_size();
        let size = Vec2 {
            x: workspace.x * fraction_w,
            y: workspace.y * fraction_h,
        };

        // Cascade from the centre, so a second window does not land exactly on
        // the first and look like nothing happened.
        let step = self.open.len() as f32;
        let centre = Vec2 {
            x: workspace.x * 0.5 + step * CASCADE,
            y: workspace.y * 0.47 + step * CASCADE * 0.6,
        };

        let id = WindowId(self.next_id);
        self.next_id += 1;

        let mut window = Window {
            id,
            app,
            rect: Rect::from_center_size(centre, size),
            minimized: false,
        };
        Self::keep_on_screen(&mut window, workspace);

        self.open.push(window);
        self.changed_at = now;
        id
    }

    pub fn close(&mut self, id: WindowId, now: Seconds) {
        self.open.retain(|w| w.id != id);
        self.dragging = None;
        self.changed_at = now;
    }

    pub fn close_focused(&mut self, now: Seconds) -> bool {
        let Some(id) = self.focused().map(|w| w.id) else {
            return false;
        };
        self.close(id, now);
        true
    }

    /// Bring one to the front.
    pub fn focus(&mut self, id: WindowId, now: Seconds) {
        let Some(index) = self.open.iter().position(|w| w.id == id) else {
            return;
        };
        if index + 1 == self.open.len() {
            return; // already focused
        }
        let window = self.open.remove(index);
        self.open.push(window);
        self.changed_at = now;
    }

    /// Move focus to the window behind the focused one, and so on round.
    pub fn cycle(&mut self, now: Seconds) {
        if self.visible().count() < 2 {
            return;
        }
        // Take the back one and put it in front.
        let window = self.open.remove(0);
        self.open.push(window);
        self.changed_at = now;
    }

    /// The topmost visible window under a point.
    pub fn hit(&self, at: Vec2) -> Option<&Window> {
        self.open
            .iter()
            .rev()
            .find(|w| !w.minimized && w.rect.contains(at))
    }

    /// Begin a press. Returns what it landed on.
    pub fn press(&mut self, at: Vec2, now: Seconds) -> Press {
        let Some(window) = self.hit(at).cloned() else {
            return Press::Nothing;
        };

        self.focus(window.id, now);

        if window.close_button().contains(at) {
            return Press::Close(window.id);
        }

        if window.minimize_button().contains(at) {
            return Press::Minimize(window.id);
        }

        if window.titlebar().contains(at) {
            self.dragging = Some((
                window.id,
                Vec2 {
                    x: at.x - window.rect.min().x,
                    y: at.y - window.rect.min().y,
                },
            ));
            return Press::Drag(window.id);
        }

        Press::Content(window.id, at)
    }

    /// Continue a drag, if one is in progress.
    pub fn drag_to(&mut self, at: Vec2, workspace: Vec2) -> bool {
        let Some((id, grab)) = self.dragging else {
            return false;
        };
        let Some(window) = self.open.iter_mut().find(|w| w.id == id) else {
            return false;
        };

        let size = window.rect.size();
        window.rect = Rect::from_min_size(
            Vec2 {
                x: at.x - grab.x,
                y: at.y - grab.y,
            },
            size,
        );
        Self::keep_on_screen(window, workspace);
        true
    }

    pub fn release(&mut self) {
        self.dragging = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Nudge a window back until enough of it is on screen to grab again.
    fn keep_on_screen(window: &mut Window, workspace: Vec2) {
        let size = window.rect.size();
        let min = window.rect.min();

        let x = min
            .x
            .min(workspace.x - KEEP_VISIBLE)
            .max(KEEP_VISIBLE - size.x);
        // Never above the top: the titlebar is the only handle, so a window
        // dragged past the top edge could never be moved again.
        let y = min.y.max(0.0).min(workspace.y - TITLEBAR);

        window.rect = Rect::from_min_size(Vec2 { x, y }, size);
    }
}

/// What a press landed on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Press {
    Nothing,
    Close(WindowId),
    Minimize(WindowId),
    Drag(WindowId),
    /// Inside an app's own area, with the point for it to interpret.
    Content(WindowId, Vec2),
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::vec2;

    fn workspace() -> Vec2 {
        vec2(1512.0, 982.0)
    }

    fn windows() -> Windows {
        Windows::new()
    }

    #[test]
    fn opening_an_app_puts_a_window_on_screen() {
        let mut w = windows();
        assert!(w.is_empty());

        w.open_app(App::Files, workspace(), 0.0);
        assert_eq!(w.all().len(), 1);
        assert_eq!(w.focused_app(), Some(App::Files));
        assert!(w.is_open(App::Files));
    }

    #[test]
    fn opening_something_already_open_focuses_it_rather_than_duplicating() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        w.open_app(App::Clock, workspace(), 0.0);
        assert_eq!(w.focused_app(), Some(App::Clock));

        w.open_app(App::Files, workspace(), 1.0);
        assert_eq!(w.all().len(), 2, "it opened a second copy");
        assert_eq!(w.focused_app(), Some(App::Files));
    }

    #[test]
    fn the_last_window_is_the_focused_one() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let clock = w.open_app(App::Clock, workspace(), 0.0);

        assert_eq!(w.focused().unwrap().id, clock);
        assert_eq!(w.all().last().unwrap().id, clock);
    }

    #[test]
    fn pressing_a_window_behind_brings_it_forward() {
        let mut w = windows();
        let files = w.open_app(App::Files, workspace(), 0.0);
        w.open_app(App::Clock, workspace(), 0.0);

        // Somewhere inside Files but outside Clock: the far left edge.
        let point = {
            let f = w.all().iter().find(|x| x.id == files).unwrap().clone();
            vec2(f.rect.min().x + 4.0, f.rect.center.y)
        };

        w.press(point, 1.0);
        assert_eq!(w.focused_app(), Some(App::Files));
    }

    #[test]
    fn closing_removes_it() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        w.close(id, 1.0);

        assert!(w.is_empty());
        assert!(!w.is_open(App::Files));
        assert!(w.focused().is_none());
    }

    #[test]
    fn the_close_dot_closes_rather_than_drags() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        let dot = w.all()[0].close_button().center;

        assert_eq!(w.press(dot, 1.0), Press::Close(id));
        assert!(!w.is_dragging(), "a close should not begin a drag");
    }

    #[test]
    fn the_titlebar_drags_and_the_content_does_not() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        let window = w.all()[0].clone();

        // Well clear of the close dot.
        let bar = vec2(window.rect.center.x, window.titlebar().center.y);
        assert_eq!(w.press(bar, 1.0), Press::Drag(id));
        assert!(w.is_dragging());

        w.release();
        let inside = window.content().center;
        assert!(matches!(w.press(inside, 1.0), Press::Content(_, _)));
        assert!(!w.is_dragging());
    }

    #[test]
    fn dragging_moves_the_window() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let before = w.all()[0].rect.min();

        let bar = vec2(w.all()[0].rect.center.x, w.all()[0].titlebar().center.y);
        w.press(bar, 1.0);
        w.drag_to(vec2(bar.x + 120.0, bar.y + 60.0), workspace());

        let after = w.all()[0].rect.min();
        assert!((after.x - before.x - 120.0).abs() < 1.0, "x did not move");
        assert!((after.y - before.y - 60.0).abs() < 1.0, "y did not move");
    }

    #[test]
    fn a_window_can_never_be_dragged_fully_off_screen() {
        // The titlebar is the only handle, so losing it loses the window.
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);

        let bar = vec2(w.all()[0].rect.center.x, w.all()[0].titlebar().center.y);
        w.press(bar, 1.0);

        for target in [vec2(-5000.0, -5000.0), vec2(9000.0, 9000.0)] {
            w.drag_to(target, workspace());
            let rect = w.all()[0].rect;
            assert!(rect.min().y >= 0.0, "dragged above the top: {rect:?}");
            assert!(rect.min().y <= workspace().y, "dragged below the bottom");
            assert!(rect.max().x > 0.0, "dragged off the left");
            assert!(rect.min().x < workspace().x, "dragged off the right");
        }
    }

    #[test]
    fn dragging_stops_on_release() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let bar = vec2(w.all()[0].rect.center.x, w.all()[0].titlebar().center.y);

        w.press(bar, 1.0);
        w.release();
        assert!(!w.drag_to(vec2(bar.x + 200.0, bar.y), workspace()));
    }

    #[test]
    fn windows_cascade_so_two_are_visibly_two() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        w.open_app(App::Clock, workspace(), 0.0);

        let a = w.all()[0].rect.min();
        let b = w.all()[1].rect.min();
        assert!((a.x - b.x).abs() > 1.0 || (a.y - b.y).abs() > 1.0, "they landed on each other");
    }

    #[test]
    fn cycling_walks_the_stack_and_comes_back_round() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        w.open_app(App::Clock, workspace(), 0.0);
        w.open_app(App::Mail, workspace(), 0.0);

        fn order(w: &Windows) -> Vec<App> {
            w.all().iter().map(|x| x.app).collect()
        }
        let before = order(&w);

        for _ in 0..3 {
            w.cycle(1.0);
        }
        assert_eq!(order(&w), before, "three cycles of three should return");
    }

    #[test]
    fn cycling_with_nothing_open_is_harmless() {
        let mut w = windows();
        w.cycle(1.0);
        assert!(w.is_empty());
        assert!(!w.close_focused(1.0));
    }

    #[test]
    fn the_content_area_sits_below_the_titlebar() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let window = &w.all()[0];

        assert!(window.content().min().y >= window.titlebar().max().y - 0.01);
        assert!(window.content().height() > 0.0);
        assert!(window.titlebar().height() > 0.0);
    }

    #[test]
    fn a_press_on_empty_space_hits_nothing() {
        let mut w = windows();
        w.open_app(App::Clock, workspace(), 0.0);
        assert_eq!(w.press(vec2(4.0, 970.0), 1.0), Press::Nothing);
    }

    #[test]
    fn apps_get_the_window_sizes_they_asked_for() {
        let mut w = windows();
        w.open_app(App::Clock, workspace(), 0.0);
        w.open_app(App::Surf, workspace(), 0.0);

        let clock = w.all().iter().find(|x| x.app == App::Clock).unwrap();
        let surf = w.all().iter().find(|x| x.app == App::Surf).unwrap();
        assert!(surf.rect.width() > clock.rect.width(), "Clock is not a glance");
    }

    #[test]
    fn the_lights_sit_at_the_top_right_with_close_outermost() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let window = &w.all()[0];

        let close = window.close_button();
        let minimize = window.minimize_button();
        let bar = window.titlebar();

        assert!(close.center.x > minimize.center.x, "close should be outermost");
        assert!(minimize.center.x > bar.center.x, "both belong on the right");
        assert!(close.max().x <= bar.max().x + 0.01, "close hangs off the bar");
        assert!(!close.contains(minimize.center), "the two lights overlap");
    }

    #[test]
    fn minimising_keeps_it_running_but_stops_drawing_it() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);

        w.minimize(id, 1.0);
        assert!(w.is_open(App::Files), "minimising should not close it");
        assert_eq!(w.visible().count(), 0, "it should not be drawn");
        assert!(w.focused().is_none(), "a minimised window is not focused");
    }

    #[test]
    fn the_yellow_light_minimises_and_the_red_one_closes() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);

        let minimize = w.all()[0].minimize_button().center;
        assert_eq!(w.press(minimize, 1.0), Press::Minimize(id));

        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        let close = w.all()[0].close_button().center;
        assert_eq!(w.press(close, 1.0), Press::Close(id));
    }

    #[test]
    fn the_dock_is_how_a_minimised_window_comes_back() {
        // There is no window list and no exposé, so launching has to restore.
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        w.minimize(id, 1.0);

        w.open_app(App::Files, workspace(), 2.0);
        assert_eq!(w.visible().count(), 1, "it did not come back");
        assert_eq!(w.focused_app(), Some(App::Files));
        assert_eq!(w.all().len(), 1, "it opened a second copy instead");
    }

    #[test]
    fn a_minimised_window_cannot_be_clicked_through_to() {
        let mut w = windows();
        let id = w.open_app(App::Files, workspace(), 0.0);
        let inside = w.all()[0].rect.center;
        w.minimize(id, 1.0);

        assert_eq!(w.press(inside, 2.0), Press::Nothing);
    }

    #[test]
    fn focus_falls_through_to_the_window_behind_a_minimised_one() {
        let mut w = windows();
        w.open_app(App::Files, workspace(), 0.0);
        let clock = w.open_app(App::Clock, workspace(), 0.0);

        w.minimize(clock, 1.0);
        assert_eq!(w.focused_app(), Some(App::Files));
    }
}
