//! The shell proper.
//!
//! Owns the whole graphical experience: boot, the five modes, the status
//! cluster, Focus. Produces one [`Output`] per tick and knows nothing about how
//! it gets drawn.

use crate::boot::{Boot, Mark, Phase};
use crate::center;
use crate::content::{self, Canvas};
use crate::input::{Event, Key, MouseButton};
use crate::clockface::{self, ClockFace, Section};
use crate::home::{self, Home};
use crate::notify::{Notice, Notifications};
use crate::mode::{Destination, Mode, Navigator};
use crate::search::{self, Command, Power, Search};
use crate::status;
use crate::system::{Focus, System};
use detends_paint::{
    space, springs, Appearance, Color, Frame, GlassSettings, MotionPreference, Palette, Rect,
    Seconds, Spring, TextureId, Vec2,
};
use detends_paint::IconShape;
use detends_time::{Clock, Fired, Schedule, TimeOfDay, WorldClock};
use jiff::tz::TimeZone;

/// The two logo marks, once the host has loaded them.
#[derive(Clone, Copy, Debug)]
pub struct Brand {
    pub short: TextureId,
    /// Width / height of the short mark's artwork.
    pub short_aspect: f32,
    pub full: TextureId,
    pub full_aspect: f32,
}

/// What the renderer needs to know about the environment this frame.
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentState {
    pub focus: f32,
    pub near: Color,
    pub far: Color,
    pub glass_intensity: f32,
    pub glass_transparency: f32,
    /// How much of the environment has arrived. Rises during boot.
    pub presence: f32,
    /// Fades *everything* to black, including any mark on top — sleep and
    /// shutdown only.
    pub fade: f32,
}

/// One tick's worth of output.
///
/// The frame and the environment travel together because they describe the same
/// instant and are both derived from state the tick just advanced — handing
/// them back separately invites them to disagree by a frame.
pub struct Output<'a> {
    pub frame: &'a Frame,
    pub environment: EnvironmentState,
}

pub struct Shell {
    boot: Boot,
    navigator: Navigator,
    system: System,
    focus: Option<Focus>,
    clock: Clock,

    palette: Palette,
    appearance: Appearance,
    prefers_dark: bool,
    glass: GlassSettings,
    motion: MotionPreference,

    size: Vec2,
    scale_factor: f32,
    frame: Frame,
    brand: Option<Brand>,
    home: Home,
    clock_face: ClockFace,
    schedule: Schedule,
    notifications: Notifications,
    zone: TimeZone,
    /// Where Clock's state is kept between runs.
    store: Option<std::path::PathBuf>,

    /// How much the interface has quietened for Focus, 0 to 1.
    hush: Spring<f32>,

    search: Search,
    /// 0 closed, 1 fully open.
    search_presence: Spring<f32>,
    center_open: bool,
    center_presence: Spring<f32>,
    /// Where the cluster was last drawn, so System Center can grow from it.
    cluster_bounds: Rect,
    /// Set when a power command is chosen; the host acts on it.
    pending_power: Option<Power>,
}

impl Shell {
    pub fn new(now: Seconds, size: Vec2, scale_factor: f32) -> Self {
        Self {
            boot: Boot::new(now),
            navigator: Navigator::new(Destination::Home),
            system: System::default(),
            focus: None,
            clock: Clock::system(),
            palette: Palette::dark(),
            appearance: Appearance::Dark,
            prefers_dark: true,
            glass: GlassSettings::default(),
            motion: MotionPreference::Full,
            size,
            scale_factor,
            frame: Frame::new(size, scale_factor),
            brand: None,
            home: Home::new(),
            clock_face: ClockFace::new(),
            schedule: Schedule::new(),
            notifications: Notifications::new(),
            zone: TimeZone::system(),
            store: None,
            hush: Spring::new(springs::HUSH, 0.0),
            search: Search::default(),
            search_presence: Spring::new(springs::SETTLE, 0.0),
            center_open: false,
            center_presence: Spring::new(springs::SETTLE, 0.0),
            cluster_bounds: Rect::ZERO,
            pending_power: None,
        }
    }

    pub fn resize(&mut self, size: Vec2, scale_factor: f32) {
        self.size = size;
        self.scale_factor = scale_factor;
        self.apply_brand();
    }

    pub fn set_appearance(&mut self, appearance: Appearance, prefers_dark: bool) {
        self.appearance = appearance;
        self.prefers_dark = prefers_dark;
        self.palette = Palette::for_appearance(appearance, prefers_dark);
    }

    pub fn set_motion_preference(&mut self, motion: MotionPreference) {
        self.motion = motion;
    }

    /// Hand the shell the logo artwork. Boot falls back to a typographic mark
    /// until this arrives, so a missing asset degrades rather than breaks.
    pub fn set_brand(&mut self, brand: Brand) {
        self.brand = Some(brand);
        self.apply_brand();
    }

    /// For tests: pin the wall clock so output is reproducible.
    pub fn set_clock(&mut self, clock: Clock) {
        self.zone = clock.zone().clone();
        self.clock = clock;
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    pub fn schedule_mut(&mut self) -> &mut Schedule {
        &mut self.schedule
    }

    pub fn notifications(&self) -> &Notifications {
        &self.notifications
    }

    /// Read Clock's state back, and let anything that finished while détends
    /// was closed be finished.
    pub fn load_schedule(&mut self, path: std::path::PathBuf) {
        self.schedule = detends_time::store::load(&path);
        self.schedule.reconcile(self.clock.now().zoned().timestamp());

        // A first run has no world clocks; a few cities are better than an
        // empty list that gives no hint of what the section is for.
        if self.schedule.world.is_empty() {
            self.schedule.world = vec![
                WorldClock::new("Europe/London", "London"),
                WorldClock::new("Europe/Paris", "Paris"),
                WorldClock::new("Asia/Tokyo", "Tokyo"),
            ];
        }
        self.store = Some(path);
    }

    /// Write Clock's state out. Called when it changes, not every frame.
    pub fn save_schedule(&self) {
        if let Some(path) = &self.store {
            if let Err(error) = detends_time::store::save(path, &self.schedule) {
                log::warn!("could not save the clock: {error}");
            }
        }
    }

    /// Fill Clock with representative state and open one utility.
    ///
    /// For captures only: a screenshot of an empty Clock shows the layout but
    /// not the thing the layout is for.
    pub fn populate_clock_for_capture(&mut self, now: Seconds, section: Section) {
        use detends_time::Repeat;

        let stamp = self.clock.now().zoned().timestamp();
        self.schedule = Schedule::new();

        self.schedule.start_timer(stamp, 18.0 * 60.0 + 42.0, Some("Deep work".into()));
        self.schedule.start_timer(stamp, 6.0 * 60.0, Some("Bread".into()));
        self.schedule.start_timer(stamp, 45.0 * 60.0, Some("Laundry".into()));

        let weekday = self.schedule.add_alarm(stamp, 7, 30, Repeat::Weekdays);
        if let Some(alarm) = self.schedule.alarm_mut(weekday) {
            alarm.label = Some("Wake".into());
        }
        self.schedule.add_alarm(stamp, 13, 0, Repeat::Daily);
        let off = self.schedule.add_alarm(stamp, 22, 30, Repeat::Weekends);
        if let Some(alarm) = self.schedule.alarm_mut(off) {
            alarm.set_enabled(false, stamp);
        }

        self.schedule.stopwatch.start(stamp - jiff::SignedDuration::from_secs(83));
        self.schedule.stopwatch.lap(stamp - jiff::SignedDuration::from_secs(51));
        self.schedule.stopwatch.lap(stamp - jiff::SignedDuration::from_secs(22));

        self.schedule.world = vec![
            WorldClock::new("Europe/London", "London"),
            WorldClock::new("Europe/Paris", "Paris"),
            WorldClock::new("Asia/Tokyo", "Tokyo"),
            WorldClock::new("America/Los_Angeles", "San Francisco"),
        ];

        self.clock_face.select(now, section);
        self.go(now, Mode::Clock);
    }

    /// Start a timer, and go where it can be watched.
    pub fn start_timer(&mut self, now: Seconds, duration: Seconds, name: Option<String>) {
        let stamp = self.clock.now().zoned().timestamp();
        self.schedule.start_timer(stamp, duration, name);
        self.clock_face.select(now, Section::Timers);
        self.go(now, Mode::Clock);
        self.save_schedule();
    }

    /// Where the workspace is.
    pub fn destination(&self) -> Destination {
        self.navigator.current()
    }

    /// The mode in view, or `None` at Home.
    pub fn mode(&self) -> Option<Mode> {
        self.navigator.current().mode()
    }

    pub fn at_home(&self) -> bool {
        self.navigator.at_home()
    }

    /// Which place Home has selected.
    pub fn home_selection(&self) -> Mode {
        self.home.selected()
    }

    pub fn system(&self) -> &System {
        &self.system
    }

    pub fn focus(&self) -> Option<&Focus> {
        self.focus.as_ref()
    }

    pub fn search_is_open(&self) -> bool {
        self.search.is_open()
    }

    pub fn system_center_is_open(&self) -> bool {
        self.center_open
    }

    /// A power action the user asked for, taken once.
    pub fn take_power_request(&mut self) -> Option<Power> {
        self.pending_power.take()
    }

    pub fn open_search(&mut self, now: Seconds) {
        self.close_center(now);
        self.search.open();
        self.search_presence.target(now, 1.0);
    }

    pub fn close_search(&mut self, now: Seconds) {
        self.search.close();
        self.search_presence.target(now, 0.0);
    }

    /// Type a whole phrase into Search, as if the user had.
    pub fn type_into_search(&mut self, text: &str) {
        for c in text.chars() {
            self.search.push(c);
        }
    }

    /// Toggle Airplane Mode (§11).
    pub fn set_airplane(&mut self, now: Seconds, on: bool) {
        let _ = now;
        self.system.set_airplane(on);
    }

    pub fn open_center(&mut self, now: Seconds) {
        self.close_search(now);
        self.center_open = true;
        self.center_presence.target(now, 1.0);
    }

    pub fn close_center(&mut self, now: Seconds) {
        self.center_open = false;
        self.center_presence.target(now, 0.0);
    }

    /// Run a command from Search.
    ///
    /// "The interface should disappear immediately after completing an action."
    /// (§13) — so every branch here closes the field.
    pub fn run(&mut self, now: Seconds, command: Command) {
        self.close_search(now);
        match command {
            Command::Go(mode) => self.go(now, mode),
            Command::Focus { seconds, name } => self.begin_focus(now, name, seconds),
            // Timers land in Clock, which owns them.
            Command::Timer { seconds, name } => self.start_timer(now, seconds, name),
            Command::Airplane(on) => self.system.set_airplane(on),
            Command::Setting(setting) => {
                // Settings live in System Center, so Search takes you there
                // rather than growing a second place to change them (rule 6).
                let _ = setting;
                self.open_center(now);
            }
            Command::Power(power) => self.pending_power = Some(power),
        }
    }

    /// Size the marks against the screen rather than against their pixel
    /// dimensions, so the same artwork reads correctly on a laptop and on a
    /// 5K display, and so the aspect ratio is never quietly distorted.
    fn apply_brand(&mut self) {
        let Some(brand) = self.brand else { return };

        let short_h = (self.size.y * 0.23).clamp(130.0, 340.0);
        let short = Mark::Artwork {
            texture: brand.short,
            size: Vec2 {
                x: short_h * brand.short_aspect,
                y: short_h,
            },
        };

        let full_w = (self.size.x * 0.44).clamp(340.0, 920.0);
        let full = Mark::Artwork {
            texture: brand.full,
            size: Vec2 {
                x: full_w,
                y: full_w / brand.full_aspect.max(0.01),
            },
        };

        self.boot.set_marks(short, full);
    }

    /// Handle one thing the user did.
    pub fn input(&mut self, now: Seconds, event: &Event) {
        // Boot owns the screen until it is done; swallowing input during it
        // avoids a keystroke landing in a mode that is not visible yet.
        if !self.boot.is_done() {
            return;
        }

        match event {
            // Bare digits, when nothing is being typed into.
            //
            // The five places have to be reachable without a modifier: a
            // combination the window system might claim is not a reliable way
            // to reach the only five destinations a system has.
            Event::KeyDown { key: Key::Mode(index), modifiers }
                if modifiers.none() && !self.search.is_open() =>
            {
                if let Some(mode) = Mode::from_index(*index) {
                    self.close_center(now);
                    self.go(now, mode);
                }
            }

            // Clock's own keyboard: the four utilities.
            Event::KeyDown { key, modifiers }
                if modifiers.none()
                    && self.navigator.current() == Destination::Mode(Mode::Clock)
                    && !self.search.is_open() =>
            {
                match key {
                    Key::Left => self.clock_face.step(now, -1),
                    Key::Right => self.clock_face.step(now, 1),
                    Key::Space => {
                        // Space runs the stopwatch when it is the one open,
                        // which is the only thing Space could sensibly mean
                        // while looking at a stopwatch.
                        if self.clock_face.section() == Section::Stopwatch {
                            let stamp = self.clock.now().zoned().timestamp();
                            self.schedule.stopwatch.toggle(stamp);
                            self.save_schedule();
                        }
                    }
                    Key::Enter => {
                        if self.clock_face.section() == Section::Stopwatch {
                            let stamp = self.clock.now().zoned().timestamp();
                            self.schedule.stopwatch.lap(stamp);
                        }
                    }
                    _ => {}
                }
            }

            // Home's own keyboard.
            Event::KeyDown { key, modifiers }
                if modifiers.none() && self.navigator.at_home() && !self.search.is_open() =>
            {
                match key {
                    Key::Left => self.home.step(now, -1),
                    Key::Right => self.home.step(now, 1),
                    Key::Enter | Key::Space => {
                        let selected = self.home.selected();
                        self.go(now, selected);
                    }
                    _ => {}
                }
            }

            Event::KeyDown { key, modifiers } if modifiers.only_sys() => match key {
                // Super+Space: the one universal surface (§13).
                Key::Space => {
                    if self.search.is_open() {
                        self.close_search(now);
                    } else {
                        self.open_search(now);
                    }
                }
                // Super+1…5: the five places, always reachable (§3).
                Key::Mode(index) => {
                    if let Some(mode) = Mode::from_index(*index) {
                        self.close_search(now);
                        self.close_center(now);
                        self.go(now, mode);
                    }
                }
                _ => {}
            },

            // Everything else typed while Search is open belongs to Search.
            Event::KeyDown { key, modifiers } if self.search.is_open() && modifiers.none() => {
                match key {
                    Key::Escape => self.close_search(now),
                    Key::Backspace => self.search.backspace(),
                    // Nothing matched means nothing happens — and the field
                    // stays open rather than swallowing what was typed.
                    Key::Enter => {
                        if let Some(command) = self.search.command() {
                            self.run(now, command);
                        }
                    }
                    Key::Space => self.search.push(' '),
                    Key::Mode(digit) => self.search.push((b'0' + digit) as char),
                    Key::Character(c) => self.search.push(*c),
                    _ => {}
                }
            }

            // Escape backs out one step: first any temporary surface, then the
            // mode, landing at Home. It never leaves the system — quitting is
            // the window's own business.
            Event::KeyDown {
                key: Key::Escape, ..
            } => {
                if self.search.is_open() || self.center_open {
                    self.close_search(now);
                    self.close_center(now);
                } else {
                    self.go_home(now);
                }
            }

            // Clicking the cluster opens System Center (§10).
            Event::PointerDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                let at = Vec2 { x: *x, y: *y };
                if self.cluster_bounds.contains(at) {
                    if self.center_open {
                        self.close_center(now);
                    } else {
                        self.open_center(now);
                    }
                } else if self.center_open {
                    // Clicking away dismisses it, as a temporary surface should.
                    let panel = center::resting_place(self.cluster_bounds, self.size);
                    if !panel.contains(at) {
                        self.close_center(now);
                    }
                } else if self.navigator.current() == Destination::Mode(Mode::Clock) {
                    let area = self.mode_area();
                    if let Some(section) = clockface::hit(area, at) {
                        self.clock_face.select(now, section);
                    }
                } else if self.navigator.at_home() {
                    // The reason Home exists: the five places can be pressed.
                    if let Some(mode) = home::hit(self.size, at) {
                        self.go(now, mode);
                    }
                }
            }

            // Moving the pointer over Home previews the selection, so the
            // keyboard and the pointer never disagree about where you are.
            Event::PointerMoved { x, y } if self.navigator.at_home() => {
                if let Some(mode) = home::hit(self.size, Vec2 { x: *x, y: *y }) {
                    self.home.select(now, mode);
                }
            }
            Event::AppearanceChanged { prefers_dark } => {
                let appearance = self.appearance;
                self.set_appearance(appearance, *prefers_dark);
            }
            Event::Resized { width, height } => {
                let scale = self.scale_factor;
                self.resize(
                    Vec2 {
                        x: *width,
                        y: *height,
                    },
                    scale,
                );
            }
            _ => {}
        }
    }

    pub fn go(&mut self, now: Seconds, to: impl Into<Destination>) {
        let to = to.into();
        // Leaving Home leaves the selection on where you went, so coming back
        // puts the cursor where you were rather than resetting it.
        if let Some(mode) = to.mode() {
            self.home.select(now, mode);
        }
        self.navigator.go(now, to);
    }

    /// Back to Home.
    pub fn go_home(&mut self, now: Seconds) {
        self.navigator.go(now, Destination::Home);
    }

    /// Begin a Focus session (§12). A global state, never a mode.
    pub fn begin_focus(&mut self, now: Seconds, name: Option<String>, duration: Option<Seconds>) {
        self.focus = Some(Focus::begin(now, name, duration));
        self.hush.target(now, 1.0);
    }

    pub fn end_focus(&mut self, now: Seconds) {
        self.focus = None;
        self.hush.target(now, 0.0);
    }

    fn environment_at(&self, now: Seconds) -> EnvironmentState {
        let presence = if self.boot.is_done() {
            1.0
        } else {
            self.boot.workspace_presence(now)
        };

        EnvironmentState {
            focus: self.hush.value(now).clamp(0.0, 1.0),
            near: self.palette.ground_far,
            far: self.palette.ground,
            glass_intensity: self.glass.intensity,
            glass_transparency: self.glass.transparency,
            presence,
            fade: 1.0,
        }
    }

    /// Build the frame for this instant.
    pub fn tick(&mut self, now: Seconds) -> Output<'_> {
        self.boot.update(now);
        self.navigator.settle(now);

        // Time keeps running whatever is on screen (§5): a timer started in
        // Clock finishes while the user is in Mail, and says so.
        let stamp = self.clock.now().zoned().timestamp();
        let fired = self.schedule.tick(stamp, &self.zone);
        if !fired.is_empty() {
            let focused = self.focus.is_some();
            for event in &fired {
                self.notifications.post(
                    Notice {
                        title: event.title(),
                        detail: event.detail().to_string(),
                        icon: match event {
                            Fired::Timer { .. } => IconShape::Timer,
                            Fired::Alarm { .. } => IconShape::Alarm,
                        },
                    },
                    focused,
                );
            }
            self.save_schedule();
        }
        self.notifications.update(now);

        // A finished session ends itself rather than sitting at zero.
        if self.focus.as_ref().is_some_and(|f| f.finished(now)) {
            self.end_focus(now);
        }

        self.frame.reset(self.size, self.scale_factor);

        let booting = matches!(self.boot.phase(), Phase::Presenting | Phase::Expanding);
        if !booting {
            self.draw_workspace(now);
        }
        self.boot.draw(&mut self.frame, &self.palette, now);

        if self.boot.animating(now)
            || !self.navigator.settled(now)
            || !self.hush.at_rest(now)
            || !self.search_presence.at_rest(now)
            || !self.center_presence.at_rest(now)
            || !self.home.settled(now)
            || !self.clock_face.settled(now)
            || self.notifications.animating(now)
            // A running timer or stopwatch has to redraw about once a second,
            // even when nothing else is moving.
            || self.schedule.is_active()
        {
            self.frame.keep_animating();
        }

        self.frame.sort();

        #[cfg(debug_assertions)]
        if let Err(problem) = self.frame.debug_check_layers() {
            eprintln!("détends: {problem}");
        }

        Output {
            environment: self.environment_at(now),
            frame: &self.frame,
        }
    }

    fn draw_workspace(&mut self, now: Seconds) {
        let time = self.clock.now();
        let hush = self.hush.value(now).clamp(0.0, 1.0);

        // Focus makes détends quieter: the chrome recedes, the content does
        // not (§12). Never the reverse.
        let chrome = 1.0 - hush * 0.55;

        self.draw_modes(now, &time);

        // Home names itself only by the mark; naming a mode you are looking at
        // is useful, naming Home is noise.
        if !self.navigator.at_home() {
            status::draw_mode_label(
                &mut self.frame,
                &self.palette,
                self.navigator.current().name(),
                chrome,
            );
        }

        let cluster = status::draw(
            &mut self.frame,
            &self.palette,
            &self.system,
            self.focus.as_ref(),
            now,
            &time,
            // The cluster stays legible during Focus — it carries the Focus
            // readout itself, so dimming it into illegibility would be absurd.
            (chrome + 0.25).min(1.0),
        );
        self.cluster_bounds = cluster.bounds;

        center::draw(
            &mut self.frame,
            &self.palette,
            &self.system,
            self.focus.as_ref(),
            &time,
            self.cluster_bounds,
            self.size,
            self.center_presence.value(now).clamp(0.0, 1.0),
        );

        search::draw(
            &mut self.frame,
            &self.palette,
            self.search.query(),
            self.size,
            self.search_presence.value(now).clamp(0.0, 1.0),
        );

        self.notifications.draw(&mut self.frame, &self.palette, now);
    }

    /// The area a mode may use, kept clear of the cluster's margins.
    fn mode_area(&self) -> Rect {
        Rect::from_min_size(Vec2::ZERO, self.size).inset(space::VAST * 0.5)
    }

    fn draw_modes(&mut self, now: Seconds, time: &TimeOfDay) {
        let area = self.mode_area();

        // While a temporary surface is up, the workspace recedes — dimmer and
        // very slightly smaller. One thing owns attention at a time (rule 2),
        // and without this the search field competes with whatever is behind
        // it instead of replacing it as the thing being looked at.
        let surface = self
            .search_presence
            .value(now)
            .max(self.center_presence.value(now))
            .clamp(0.0, 1.0);
        let recede_opacity = 1.0 - surface * 0.55;
        let recede_scale = 1.0 - surface * 0.012;

        // The departing destination first, so the arriving one sits over it.
        if let Some(previous) = self.navigator.previous() {
            let (opacity, scale) = self.navigator.outgoing(now);
            let opacity = opacity * recede_opacity;
            if opacity > 0.004 {
                self.draw_destination(previous, area, opacity, scale * recede_scale, now, time);
            }
        }

        let (opacity, scale) = self.navigator.incoming(now);
        let current = self.navigator.current();
        self.draw_destination(
            current,
            area,
            opacity * recede_opacity,
            scale * recede_scale,
            now,
            time,
        );
    }

    fn draw_destination(
        &mut self,
        destination: Destination,
        area: Rect,
        opacity: f32,
        scale: f32,
        now: Seconds,
        time: &TimeOfDay,
    ) {
        match destination {
            Destination::Home => {
                self.home
                    .draw(&mut self.frame, &self.palette, time, now, opacity, scale);
            }
            // Clock is its own environment rather than a placeholder layout.
            Destination::Mode(Mode::Clock) => {
                let stamp = self.clock.now().zoned().timestamp();
                self.clock_face.draw(
                    &mut self.frame,
                    &self.palette,
                    area,
                    time,
                    &self.schedule,
                    stamp,
                    &self.zone,
                    now,
                    opacity,
                    scale,
                );
            }
            Destination::Mode(mode) => {
                let mut canvas = Canvas {
                    frame: &mut self.frame,
                    palette: &self.palette,
                    area,
                    opacity,
                    scale,
                    time,
                };
                content::draw(mode, &mut canvas);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;
    use detends_paint::vec2;

    fn sys() -> Modifiers {
        Modifiers {
            sys: true,
            ..Default::default()
        }
    }

    fn booted() -> (Shell, Seconds) {
        let mut shell = Shell::new(0.0, vec2(1512.0, 982.0), 2.0);
        shell.set_clock(Clock::frozen_at(2026, 9, 17, 17, 14, "UTC").unwrap());
        let mut t = 0.0;
        for _ in 0..4000 {
            t += 1.0 / 120.0;
            shell.tick(t);
            if shell.boot.is_done() {
                break;
            }
        }
        assert!(shell.boot.is_done(), "boot did not finish");
        (shell, t)
    }

    fn texts(frame: &Frame) -> Vec<String> {
        frame
            .items
            .iter()
            .filter_map(|i| match &i.primitive {
                detends_paint::Primitive::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn boot_gives_way_to_a_mode_and_the_cluster() {
        let (mut shell, t) = booted();
        let frame = shell.tick(t + 1.0).frame;
        let s = texts(frame);

        assert!(
            s.iter().any(|t| t.contains("5:14")),
            "the cluster should show the time"
        );

        // Boot lands on Home, which shows the time and the five places rather
        // than dropping the user into a mode with nothing to press.
        assert!(shell.at_home());
        for mode in Mode::ALL {
            assert!(s.iter().any(|t| t == mode.name()), "{mode:?} is not offered");
        }
        assert!(
            !s.iter().any(|t| t == "HOME" || t == "DÉTENDS"),
            "Home should not label itself"
        );
    }

    #[test]
    fn every_place_can_be_pressed_from_home() {
        // The fix for a system that appeared to do nothing: the five places
        // are reachable with a pointer, not only by unannounced keystrokes.
        let (mut shell, t) = booted();
        for (mode, rect) in crate::home::layout(vec2(1512.0, 982.0)) {
            shell.go_home(t);
            shell.tick(t);
            shell.input(
                t,
                &Event::PointerDown {
                    x: rect.center.x,
                    y: rect.center.y,
                    button: MouseButton::Left,
                },
            );
            assert_eq!(shell.mode(), Some(mode), "clicking {mode:?} did nothing");
        }
    }

    #[test]
    fn a_bare_number_goes_to_its_place() {
        // No modifier, so nothing in the window system can intercept it.
        let (mut shell, mut t) = booted();
        for mode in Mode::ALL {
            t += 1.0;
            press(&mut shell, t, Key::Mode(mode.index()));
            assert_eq!(shell.mode(), Some(mode), "{} did nothing", mode.index());
        }
    }

    #[test]
    fn escape_backs_out_to_home_rather_than_quitting() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Studio);
        assert!(!shell.at_home());

        press(&mut shell, t, Key::Escape);
        assert!(shell.at_home(), "Escape should land Home");
    }

    #[test]
    fn escape_dismisses_a_surface_before_leaving_the_mode() {
        // One step back at a time, so Escape never throws away more than the
        // user meant.
        let (mut shell, t) = booted();
        shell.go(t, Mode::Mail);
        shell.open_search(t);

        press(&mut shell, t, Key::Escape);
        assert!(!shell.search_is_open());
        assert_eq!(shell.mode(), Some(Mode::Mail), "it left the mode too");

        press(&mut shell, t, Key::Escape);
        assert!(shell.at_home());
    }

    #[test]
    fn arrow_keys_and_enter_work_at_home() {
        let (mut shell, t) = booted();
        assert_eq!(shell.home_selection(), Mode::Music);

        press(&mut shell, t, Key::Right);
        press(&mut shell, t, Key::Right);
        assert_eq!(shell.home_selection(), Mode::Mail);

        press(&mut shell, t, Key::Enter);
        assert_eq!(shell.mode(), Some(Mode::Mail));
    }

    #[test]
    fn coming_back_home_remembers_where_you_were() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Files);
        shell.go_home(t);
        assert_eq!(shell.home_selection(), Mode::Files);
    }

    #[test]
    fn super_number_goes_to_each_of_the_five_places() {
        let (mut shell, mut t) = booted();
        for mode in Mode::ALL {
            t += 1.0;
            shell.input(
                t,
                &Event::KeyDown {
                    key: Key::Mode(mode.index()),
                    modifiers: sys(),
                },
            );
            assert_eq!(
                shell.mode(),
                Some(mode),
                "Super+{} should go to {mode:?}",
                mode.index()
            );
        }
    }

    #[test]
    fn a_bare_number_typed_into_search_is_text() {
        // A digit navigates — except while Search has the keyboard, where it
        // has to be a character or the field would be unusable.
        let (mut shell, t) = booted();
        shell.go(t, Mode::Files);
        shell.open_search(t);
        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Mode(5),
                modifiers: Modifiers::default(),
            },
        );
        assert!(shell.search_is_open(), "the field closed");
        assert_eq!(shell.mode(), Some(Mode::Files), "a digit navigated while typing");
    }

    #[test]
    fn input_is_ignored_until_boot_finishes() {
        let mut shell = Shell::new(0.0, vec2(1512.0, 982.0), 2.0);
        shell.tick(0.1);
        shell.input(
            0.1,
            &Event::KeyDown {
                key: Key::Mode(1),
                modifiers: sys(),
            },
        );
        assert!(shell.at_home(), "a keystroke landed during boot");
    }

    #[test]
    fn switching_modes_draws_both_for_a_moment() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Music);

        let s = texts(shell.tick(t + 0.06).frame);
        assert!(
            s.iter().any(|x| x == "Resonance"),
            "the arriving mode is missing"
        );
        assert!(
            s.iter().any(|x| x.contains("September")),
            "the leaving mode is missing"
        );
    }

    #[test]
    fn the_leaving_mode_stops_being_drawn_once_it_has_gone() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Music);
        let s = texts(shell.tick(t + 2.0).frame);
        assert!(
            !s.iter().any(|x| x.contains("September")),
            "the old mode is still drawn"
        );
    }

    #[test]
    fn it_stops_animating_once_everything_has_settled() {
        // A calm system must be able to stop drawing entirely (§21).
        let (mut shell, t) = booted();
        assert!(
            !shell.tick(t + 5.0).frame.animating,
            "still animating after settling"
        );
    }

    #[test]
    fn a_mode_switch_makes_it_animate_again() {
        let (mut shell, t) = booted();
        shell.tick(t + 5.0);
        shell.go(t + 5.0, Mode::Files);
        assert!(shell.tick(t + 5.01).frame.animating);
    }

    #[test]
    fn focus_quietens_the_chrome_without_touching_the_content() {
        // §12: Focus should make détends quieter, not announce itself.
        let (mut shell, t) = booted();
        // The mode label only exists inside a mode; Home does not name itself.
        shell.go(t, Mode::Mail);
        shell.tick(t + 2.0);
        let bright = shell
            .tick(t + 2.0)
            .frame
            .items
            .iter()
            .find(|i| i.id == detends_paint::Id::of("mode-label"))
            .map(|i| i.opacity)
            .expect("mode label");

        shell.begin_focus(t + 2.0, Some("Physics homework".into()), Some(25.0 * 60.0));
        let dim = shell
            .tick(t + 4.0)
            .frame
            .items
            .iter()
            .find(|i| i.id == detends_paint::Id::of("mode-label"))
            .map(|i| i.opacity)
            .expect("mode label");

        assert!(dim < bright, "navigation should recede: {dim} vs {bright}");
        assert!(dim > 0.2, "it must stay usable, not vanish");
    }

    #[test]
    fn focus_shows_its_readout_in_the_cluster() {
        let (mut shell, t) = booted();
        shell.begin_focus(t, None, Some(42.0 * 60.0 + 18.0));
        let frame = shell.tick(t).frame;
        let s = texts(frame);
        assert!(s.iter().any(|x| x.starts_with("42:18")), "got {s:?}");

        // The ring beside the readout is drawn, since ◎ is not in the font.
        let has_ring = frame.items.iter().any(|i| {
            matches!(&i.primitive, detends_paint::Primitive::Icon(icon)
                if icon.shape == detends_paint::IconShape::Focus)
        });
        assert!(has_ring, "the focus ring is missing");
    }

    #[test]
    fn a_finished_session_ends_itself() {
        let (mut shell, t) = booted();
        shell.begin_focus(t, None, Some(10.0));
        assert!(shell.focus().is_some());
        shell.tick(t + 11.0);
        assert!(shell.focus().is_none(), "the session should have ended");
    }

    #[test]
    fn focus_is_not_a_mode() {
        // Rule 8, checked structurally: starting Focus must not change where
        // the user is.
        let (mut shell, t) = booted();
        shell.go(t, Mode::Mail);
        shell.begin_focus(t, None, None);
        assert_eq!(shell.mode(), Some(Mode::Mail));
    }

    #[test]
    fn layers_never_overlap_within_themselves() {
        let (mut shell, mut t) = booted();
        for mode in Mode::ALL {
            shell.go(t, mode);
            for _ in 0..30 {
                t += 1.0 / 60.0;
                let frame = shell.tick(t).frame;
                assert!(
                    frame.debug_check_layers().is_ok(),
                    "{mode:?}: {:?}",
                    frame.debug_check_layers()
                );
            }
        }
    }

    #[test]
    fn frames_do_not_allocate_once_the_shell_is_running() {
        let (mut shell, t) = booted();
        shell.tick(t + 1.0);
        let capacity = shell.frame.items.capacity();
        for n in 1..60 {
            shell.tick(t + 1.0 + n as f64 * 0.01);
        }
        assert_eq!(
            shell.frame.items.capacity(),
            capacity,
            "reallocated per frame"
        );
    }

    fn press(shell: &mut Shell, t: Seconds, key: Key) {
        shell.input(
            t,
            &Event::KeyDown {
                key,
                modifiers: Modifiers::default(),
            },
        );
    }

    fn type_into_search(shell: &mut Shell, t: Seconds, text: &str) {
        for c in text.chars() {
            press(
                shell,
                t,
                if c == ' ' {
                    Key::Space
                } else {
                    Key::Character(c)
                },
            );
        }
    }

    #[test]
    fn super_space_opens_and_closes_search() {
        let (mut shell, t) = booted();
        assert!(!shell.search_is_open());

        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Space,
                modifiers: sys(),
            },
        );
        assert!(shell.search_is_open());

        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Space,
                modifiers: sys(),
            },
        );
        assert!(!shell.search_is_open());
    }

    #[test]
    fn search_shows_its_placeholder_then_what_is_typed() {
        let (mut shell, t) = booted();
        shell.open_search(t);
        let s = texts(shell.tick(t + 0.5).frame);
        assert!(s.iter().any(|x| x == "Search détends…"), "got {s:?}");

        type_into_search(&mut shell, t, "timer 20");
        let s = texts(shell.tick(t + 0.5).frame);
        assert!(s.iter().any(|x| x == "timer 20"), "got {s:?}");
        assert!(!s.iter().any(|x| x == "Search détends…"));
    }

    #[test]
    fn a_search_command_runs_and_the_field_disappears() {
        // §13: "The interface should disappear immediately after completing an
        // action."
        let (mut shell, t) = booted();
        shell.open_search(t);
        type_into_search(&mut shell, t, "files");
        press(&mut shell, t, Key::Enter);

        assert!(!shell.search_is_open(), "the field stayed open");
        assert_eq!(shell.mode(), Some(Mode::Files));
    }

    #[test]
    fn a_search_that_matches_nothing_leaves_the_query_alone() {
        // Swallowing an unmatched query would lose what the user typed.
        let (mut shell, t) = booted();
        shell.open_search(t);
        type_into_search(&mut shell, t, "asdfgh");
        press(&mut shell, t, Key::Enter);
        assert!(shell.search_is_open(), "the field should still be open");
    }

    #[test]
    fn escape_closes_search_without_running_anything() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Files);
        shell.open_search(t);
        type_into_search(&mut shell, t, "music");
        press(&mut shell, t, Key::Escape);

        assert!(!shell.search_is_open());
        assert_eq!(shell.mode(), Some(Mode::Files), "Escape should not have navigated");
    }

    #[test]
    fn digits_typed_into_search_are_text_not_mode_switches() {
        // Super+2 goes to Clock; a bare 2 while searching is a character.
        let (mut shell, t) = booted();
        shell.go(t, Mode::Mail);
        shell.open_search(t);
        type_into_search(&mut shell, t, "timer 2");
        press(&mut shell, t, Key::Mode(5));

        assert_eq!(shell.mode(), Some(Mode::Mail), "a digit navigated");
        assert!(shell.search_is_open());
    }

    #[test]
    fn super_number_still_works_while_search_is_open() {
        let (mut shell, t) = booted();
        shell.open_search(t);
        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Mode(1),
                modifiers: sys(),
            },
        );
        assert_eq!(shell.mode(), Some(Mode::Music));
        assert!(
            !shell.search_is_open(),
            "navigating should dismiss the field"
        );
    }

    #[test]
    fn search_can_start_focus_and_a_timer() {
        let (mut shell, t) = booted();
        shell.open_search(t);
        type_into_search(&mut shell, t, "focus 45 minutes");
        press(&mut shell, t, Key::Enter);
        assert!(shell.focus().is_some());

        shell.end_focus(t);
        shell.open_search(t);
        type_into_search(&mut shell, t, "timer 20 minutes");
        press(&mut shell, t, Key::Enter);

        // A timer is a real timer now, not a Focus session wearing its name.
        assert_eq!(shell.mode(), Some(Mode::Clock), "a timer belongs in Clock");
        assert_eq!(shell.schedule().timers.len(), 1);
        assert!((shell.schedule().timers[0].duration - 1200.0).abs() < 1.0);
        assert!(shell.focus().is_none(), "a timer is not a Focus session");
    }

    #[test]
    fn a_timer_finishes_while_the_user_is_somewhere_else() {
        // §5: alarms and timers keep running whatever mode is on screen.
        let (mut shell, t) = booted();
        shell.open_search(t);
        type_into_search(&mut shell, t, "timer 10 seconds");
        press(&mut shell, t, Key::Enter);

        shell.go(t, Mode::Mail);
        assert_eq!(shell.mode(), Some(Mode::Mail));

        // The shell's wall clock is frozen in tests, so advance it past the
        // timer's end and tick.
        shell.set_clock(Clock::frozen_at(2026, 9, 17, 17, 15, "UTC").unwrap());
        shell.tick(t + 1.0);

        assert!(
            shell.notifications().is_showing() || shell.notifications().animating(t + 1.0),
            "the timer finished silently while the user was elsewhere"
        );
    }

    #[test]
    fn a_timer_finishing_during_focus_is_collected_rather_than_shown() {
        // §12: Focus is not interrupted. Nothing is thrown away either.
        let (mut shell, t) = booted();
        shell.start_timer(t, 10.0, Some("Bread".into()));
        shell.begin_focus(t, Some("Physics".into()), None);

        shell.set_clock(Clock::frozen_at(2026, 9, 17, 17, 15, "UTC").unwrap());
        shell.tick(t + 1.0);

        assert!(!shell.notifications().is_showing(), "Focus was interrupted");
        assert_eq!(shell.notifications().collected().len(), 1);
    }

    #[test]
    fn clock_opens_on_its_utilities() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Clock);
        let s = texts(shell.tick(t + 1.0).frame);
        for name in ["Timers", "Alarms", "Stopwatch", "World"] {
            assert!(s.iter().any(|x| x == name), "missing {name} in {s:?}");
        }
    }

    #[test]
    fn a_power_command_is_handed_to_the_host_once() {
        let (mut shell, t) = booted();
        shell.open_search(t);
        type_into_search(&mut shell, t, "restart");
        press(&mut shell, t, Key::Enter);

        assert_eq!(shell.take_power_request(), Some(Power::Restart));
        assert_eq!(shell.take_power_request(), None, "taken twice");
    }

    #[test]
    fn clicking_the_cluster_opens_system_center() {
        let (mut shell, t) = booted();
        let cluster = shell.cluster_bounds;
        assert!(
            cluster.width() > 0.0,
            "the cluster should have been laid out"
        );

        shell.input(
            t,
            &Event::PointerDown {
                x: cluster.center.x,
                y: cluster.center.y,
                button: MouseButton::Left,
            },
        );
        assert!(shell.system_center_is_open());

        // Clicking it again closes it.
        shell.input(
            t,
            &Event::PointerDown {
                x: cluster.center.x,
                y: cluster.center.y,
                button: MouseButton::Left,
            },
        );
        assert!(!shell.system_center_is_open());
    }

    #[test]
    fn clicking_away_dismisses_system_center() {
        let (mut shell, t) = booted();
        shell.open_center(t);
        shell.input(
            t,
            &Event::PointerDown {
                x: 40.0,
                y: 800.0,
                button: MouseButton::Left,
            },
        );
        assert!(!shell.system_center_is_open());
    }

    #[test]
    fn system_center_and_search_are_never_open_together() {
        // Rule 2: one thing owns attention at a time.
        let (mut shell, t) = booted();
        shell.open_center(t);
        shell.open_search(t);
        assert!(shell.search_is_open() && !shell.system_center_is_open());

        shell.open_center(t);
        assert!(shell.system_center_is_open() && !shell.search_is_open());
    }

    #[test]
    fn the_workspace_recedes_behind_a_temporary_surface() {
        // Rule 2: one thing owns attention at a time. With Search up, the mode
        // behind it must step back rather than compete.
        let (mut shell, t) = booted();
        shell.go(t, Mode::Music);
        shell.tick(t + 2.0);

        let opacity_of = |shell: &mut Shell, at: Seconds| {
            shell
                .tick(at)
                .frame
                .items
                .iter()
                .find(|i| i.id == detends_paint::Id::of("music-track"))
                .map(|i| i.opacity)
                .expect("the track title")
        };

        let bright = opacity_of(&mut shell, t + 2.0);
        shell.open_search(t + 2.0);
        let dim = opacity_of(&mut shell, t + 3.0);

        assert!(
            dim < bright * 0.7,
            "the workspace did not recede: {dim} vs {bright}"
        );
        assert!(
            dim > 0.1,
            "it receded into nothing, which is a different mistake"
        );
    }

    #[test]
    fn the_workspace_comes_back_when_the_surface_closes() {
        let (mut shell, t) = booted();
        shell.go(t, Mode::Music);
        shell.open_search(t);
        shell.tick(t + 1.0);
        shell.close_search(t + 1.0);

        let restored = shell
            .tick(t + 3.0)
            .frame
            .items
            .iter()
            .find(|i| i.id == detends_paint::Id::of("music-track"))
            .map(|i| i.opacity)
            .expect("the track title");
        assert!(restored > 0.95, "the workspace stayed dim: {restored}");
    }

    #[test]
    fn opening_a_surface_makes_the_shell_animate_again() {
        let (mut shell, t) = booted();
        shell.tick(t + 5.0);
        shell.open_search(t + 5.0);
        assert!(shell.tick(t + 5.01).frame.animating);
    }

    #[test]
    fn resizing_recentres_the_workspace() {
        let (mut shell, t) = booted();
        shell.resize(vec2(3840.0, 2160.0), 2.0);
        let frame = shell.tick(t + 1.0).frame;
        let clock = frame
            .items
            .iter()
            .find(|i| i.id == detends_paint::Id::of("home-time"))
            .expect("the clock");
        assert!((clock.primitive.bounds().center.x - 1920.0).abs() < 2.0);
    }
}
