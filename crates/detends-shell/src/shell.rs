//! The shell proper.
//!
//! Owns the whole graphical experience: boot, the five modes, the status
//! cluster, Focus. Produces one [`Output`] per tick and knows nothing about how
//! it gets drawn.

use crate::app::App;
use crate::boot::{Boot, Mark, Phase};
use crate::dock;
use crate::window::{Press, Windows};
use crate::browser::{self, Browser};
use crate::center;
use crate::input::{Event, Key, MouseButton};
use crate::clockface::{self, ClockFace, Section};
use crate::notify::{Notice, Notifications};
use crate::nowplaying;
use crate::search::{self, Command, Power, Search};
use crate::status;
use crate::system::{Focus, System, FOCUS_DURATIONS};
use crate::wallpaper;
use detends_paint::{
    space, springs, text, Align, Appearance, Color, Frame, GlassSettings, Id, Item, Layer,
    MotionPreference, Palette, Primitive, Rect, Seconds, Spring, TextureId, Vec2,
};
use detends_paint::IconShape;
use detends_fs::{Place, Vault};
use detends_music::{LocalProvider, Music};
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
    /// The two lights the field is lit by (§17). The shell owns the palette,
    /// so the shell decides what the room is lit with.
    pub glow_warm: Color,
    pub glow_cool: Color,
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
    /// Every open window, back to front.
    windows: Windows,
    /// Which dock slot the pointer is over, if any.
    dock_hover: Option<App>,
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
    clock_face: ClockFace,
    browser: Browser,
    /// Music, running its provider on its own thread.
    ///
    /// `None` until a provider is chosen, so the shell still builds frames —
    /// and still tests — without starting a thread or touching the network.
    music: Option<Music>,
    /// The vault behind Files. `None` until a root is given, so the shell
    /// still runs — and still tests — without touching a disk.
    vault: Option<Vault>,
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
    /// The slider being dragged, if any. Held so a drag keeps working once the
    /// pointer has left the track — releasing outside a control should not
    /// silently stop the thing you are still holding.
    dragging: Option<center::Control>,
    /// A volume the host should apply to the machine, set when a drag ends.
    ///
    /// On release rather than per-frame: applying a device volume sixty times
    /// a second while a finger moves is an expensive way to arrive at the same
    /// number, and on most platforms it means spawning something.
    pending_volume: Option<f32>,
    /// Where the cluster was last drawn, so System Center can grow from it.
    cluster_bounds: Rect,
    /// Set when a power command is chosen; the host acts on it.
    pending_power: Option<Power>,
}

impl Shell {
    pub fn new(now: Seconds, size: Vec2, scale_factor: f32) -> Self {
        Self {
            boot: Boot::new(now),
            windows: Windows::new(),
            dock_hover: None,
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
            clock_face: ClockFace::new(),
            browser: Browser::new(),
            music: None,
            vault: None,
            schedule: Schedule::new(),
            notifications: Notifications::new(),
            zone: TimeZone::system(),
            store: None,
            hush: Spring::new(springs::HUSH, 0.0),
            search: Search::default(),
            search_presence: Spring::new(springs::SETTLE, 0.0),
            center_open: false,
            center_presence: Spring::new(springs::SETTLE, 0.0),
            dragging: None,
            pending_volume: None,
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

    pub fn vault(&self) -> Option<&Vault> {
        self.vault.as_ref()
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    pub fn browser_mut(&mut self) -> &mut Browser {
        &mut self.browser
    }

    /// Give Files a vault to show.
    ///
    /// Separate from `new` for the same reason Clock's store is: the shell
    /// should be constructible, drivable and testable without a filesystem
    /// underneath it, and the host decides where the vault lives.
    /// Point Files at one destination. Used by `--place` when capturing, and
    /// by anything else that needs to put Files somewhere specific.
    pub fn show_place(&mut self, now: Seconds, place: Place) {
        if let Some(vault) = self.vault.take() {
            self.browser.select_place(now, place, &vault);
            self.vault = Some(vault);
        }
    }

    /// Give Music a provider to play through.
    ///
    /// The shell never names one: the host decides whether that is Spotify or
    /// the local player, and Music cannot tell the difference (§4).
    pub fn set_music(&mut self, music: Music) {
        self.music = Some(music);
    }

    /// Start Music on the provider that needs no account.
    ///
    /// What a first run gets, so Music is a working mode rather than an error
    /// message before anything is connected.
    pub fn use_local_music(&mut self) {
        self.music = Some(Music::start(Box::new(LocalProvider::placeholder())));
    }

    pub fn music(&self) -> Option<&Music> {
        self.music.as_ref()
    }

    pub fn open_vault(&mut self, root: std::path::PathBuf) {
        let vault = Vault::open(root);
        self.browser.refresh(&vault);
        self.vault = Some(vault);
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
        self.open(now, App::Clock);
    }

    /// Start a timer, and go where it can be watched.
    pub fn start_timer(&mut self, now: Seconds, duration: Seconds, name: Option<String>) {
        let stamp = self.clock.now().zoned().timestamp();
        self.schedule.start_timer(stamp, duration, name);
        self.clock_face.select(now, Section::Timers);
        self.open(now, App::Clock);
        self.save_schedule();
    }

    /// The app in front, if anything is open.
    pub fn focused_app(&self) -> Option<App> {
        self.windows.focused_app()
    }

    pub fn windows(&self) -> &Windows {
        &self.windows
    }

    /// Open an app, or bring it forward if it is already running.
    pub fn open(&mut self, now: Seconds, app: App) {
        self.close_search(now);
        self.windows.open_app(app, self.size, now);
    }

    /// Whether nothing is open — the wallpaper and the dock, and that is all.
    pub fn showing_desktop(&self) -> bool {
        self.windows.visible().count() == 0
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

    /// A volume the machine should be set to, taken once.
    pub fn take_volume_request(&mut self) -> Option<f32> {
        self.pending_volume.take()
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
            Command::Open(app) => self.open(now, app),
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
            // Escape backs out one step: first any temporary surface, then the
            // mode, landing at Home. It never leaves the system — quitting is
            // the window's own business.
            //
            // It is the FIRST arm on purpose. Clock, Files and Home each claim
            // bare keys while they are in view, and an arm that matches `key`
            // with a `_ => {}` fallback silently eats everything it does not
            // name. A global gesture has to be matched before any mode gets the
            // chance to swallow it, or the mode becomes a room with no door.
            Event::KeyDown {
                key: Key::Escape, ..
            } => {
                if self.browser.cancel() {
                    // A name being typed is the innermost thing open.
                } else if self.search.is_open() || self.center_open {
                    self.close_search(now);
                    self.close_center(now);
                } else {
                    self.hide_focused(now);
                }
            }

            // Bare digits open the app in that dock slot.
            //
            // The dock is the thing you reach for, so the keyboard shortcut
            // names a dock position rather than an app — 1 is always whatever
            // is leftmost, which is what your hand learns.
            Event::KeyDown { key: Key::Mode(index), modifiers }
                if modifiers.none()
                    && !self.search.is_open()
                    && !self.browser.is_editing() =>
            {
                if let Some(app) = App::ALL.get(*index as usize - 1).copied() {
                    self.close_center(now);
                    self.open(now, app);
                }
            }

            // Clock's own keyboard: the four utilities.
            Event::KeyDown { key, modifiers }
                if modifiers.none()
                    && self.windows.focused_app() == Some(App::Clock)
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
                    Key::Enter if self.clock_face.section() == Section::Stopwatch => {
                        let stamp = self.clock.now().zoned().timestamp();
                        self.schedule.stopwatch.lap(stamp);
                    }
                    _ => {}
                }
            }

            // Music's own keyboard (§4). Space is play/pause wherever you are
            // in Music, because that is the one control everybody reaches for.
            Event::KeyDown { key, modifiers }
                if modifiers.none()
                    && self.windows.focused_app() == Some(App::Spotify)
                    && !self.search.is_open() =>
            {
                use detends_music::Command;
                let command = match key {
                    Key::Space | Key::Enter => Some(Command::PlayPause),
                    Key::Right => Some(Command::Next),
                    Key::Left => Some(Command::Previous),
                    Key::Character(c) => match c.to_ascii_lowercase() {
                        's' => Some(Command::ToggleShuffle),
                        'r' => Some(Command::CycleRepeat),
                        _ => None,
                    },
                    _ => None,
                };
                if let (Some(command), Some(music)) = (command, self.music.as_mut()) {
                    music.send(command);
                }
            }

            // Files' own keyboard (§9). Bare keys, because Files is a place
            // you are in rather than a window you have focused.
            Event::KeyDown { key, modifiers }
                if self.windows.focused_app() == Some(App::Files)
                    && !self.search.is_open()
                    && (modifiers.none() || modifiers.only_sys()) =>
            {
                self.files_key(now, key.clone(), modifiers.only_sys());
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
                // Super+1…6: the dock, always reachable.
                Key::Mode(index) => {
                    if let Some(app) = App::ALL.get(*index as usize - 1).copied() {
                        self.close_search(now);
                        self.close_center(now);
                        self.open(now, app);
                    }
                }
                // Super+W closes the window in front, Super+M puts it away,
                // and Super+` walks the stack. The three a windowing system
                // cannot do without.
                Key::Character(c) if c.eq_ignore_ascii_case(&'w') => {
                    self.windows.close_focused(now);
                }
                Key::Character(c) if c.eq_ignore_ascii_case(&'m') => {
                    self.hide_focused(now);
                }
                Key::Character(c) if *c == '`' => {
                    self.windows.cycle(now);
                }
                // Super+Q: leave. Escape deliberately never quits — it backs
                // out, and a surface you cannot dismiss without exiting the
                // system is a trap — but that only works if there is some
                // other way out. Fullscreen has no close button, so this is
                // it (§19).
                Key::Character(c) if c.eq_ignore_ascii_case(&'q') => {
                    self.pending_power = Some(Power::ShutDown);
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
                    let panel = center::resting_place(self.cluster_bounds, self.size);
                    if panel.contains(at) {
                        self.press_control(now, panel, at);
                    } else {
                        // Clicking away dismisses it, as a temporary surface
                        // should.
                        self.close_center(now);
                    }
                } else if let Some(app) = dock::hit(self.size, at) {
                    self.open(now, app);
                } else {
                    self.press_window(now, at);
                }
            }

            // A slider being held follows the pointer anywhere, including
            // outside the panel — letting go is what ends a drag, not leaving
            // the track.
            Event::PointerMoved { x, y } if self.dragging.is_some() => {
                let panel = center::resting_place(self.cluster_bounds, self.size);
                if let Some(control) = self.dragging {
                    self.drag_control(panel, control, Vec2 { x: *x, y: *y });
                }
            }

            Event::PointerUp { .. } => {
                if self.dragging == Some(center::Control::Volume) {
                    self.pending_volume = Some(self.system.volume);
                }
                self.dragging = None;
                self.windows.release();
            }

            // A window being dragged follows the pointer; otherwise the dock
            // lights up whatever is under it.
            Event::PointerMoved { x, y } => {
                let at = Vec2 { x: *x, y: *y };
                if self.windows.is_dragging() {
                    self.windows.drag_to(at, self.size);
                } else {
                    self.dock_hover = dock::hit(self.size, at);
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

    /// A press that was not on the dock or a temporary surface.
    ///
    /// Windows first, and only then the app inside one: focusing, dragging and
    /// closing belong to the window manager, and an app never sees the press
    /// that raised it. Otherwise clicking a background window to bring it
    /// forward would also press whatever happened to be under the pointer.
    fn press_window(&mut self, now: Seconds, at: Vec2) {
        match self.windows.press(at, now) {
            Press::Nothing => {}
            Press::Close(id) => self.windows.close(id, now),
            Press::Minimize(id) => self.windows.minimize(id, now),
            Press::Drag(_) => {}
            Press::Content(id, at) => {
                let Some(window) = self.windows.all().iter().find(|w| w.id == id).cloned() else {
                    return;
                };
                // A press that brought a window forward does not also reach
                // the app: raising is its own gesture.
                if Some(id) != self.windows.focused().map(|w| w.id) {
                    return;
                }
                self.press_in_app(now, &window, at);
            }
        }
    }

    /// Hand a press to whatever the window holds.
    fn press_in_app(&mut self, now: Seconds, window: &crate::window::Window, at: Vec2) {
        let area = window.content().inset(space::ROOM);

        match window.app {
            App::Clock => {
                if let Some(section) = clockface::hit(area, at) {
                    self.clock_face.select(now, section);
                }
            }
            App::Spotify => {
                if let Some(hit) = nowplaying::hit(area, at) {
                    let command = nowplaying::command_for(hit, area, at);
                    if let Some(music) = self.music.as_mut() {
                        music.send(command);
                    }
                }
            }
            App::Files => {
                if let Some(place) = browser::hit(area, at) {
                    if let Some(vault) = self.vault.take() {
                        self.browser.select_place(now, place, &vault);
                        self.vault = Some(vault);
                    }
                }
            }
            _ => {}
        }
    }

    /// Act on a press inside System Center (§10).
    ///
    /// Every control here is a persistent state, so a press is a state change
    /// and nothing else — there is no confirmation, no dialog and nothing to
    /// undo, because turning Wi-Fi back on *is* the undo.
    fn press_control(&mut self, now: Seconds, panel: Rect, at: Vec2) {
        let Some(control) = center::hit(panel, at) else {
            return;
        };

        match control {
            center::Control::Wifi => {
                let on = self.system.wifi_on();
                self.system.set_wifi(!on);
            }
            center::Control::Airplane => {
                let on = self.system.airplane;
                self.set_airplane(now, !on);
            }
            center::Control::Bluetooth => {
                let on = self.system.bluetooth.is_some();
                self.system.set_bluetooth(!on);
            }
            // Focus is a session rather than a switch: starting one from here
            // takes the default duration, and pressing again ends it (§12).
            center::Control::Focus => {
                if self.focus.is_some() {
                    self.end_focus(now);
                } else {
                    // The first offered length, which the specification puts
                    // at 25 minutes.
                    self.begin_focus(now, None, FOCUS_DURATIONS[0].1);
                }
            }
            center::Control::Volume | center::Control::Brightness => {
                self.dragging = Some(control);
                self.drag_control(panel, control, at);
            }
        }
    }

    /// Move a continuous control to wherever the pointer is.
    fn drag_control(&mut self, panel: Rect, control: center::Control, at: Vec2) {
        let Some(value) = center::slider_value(panel, control, at) else {
            return;
        };
        match control {
            center::Control::Volume => self.system.volume = value,
            center::Control::Brightness => self.system.brightness = value,
            _ => {}
        }
    }

    /// Close whatever is in front, or put the last one away.
    ///
    /// What Escape does when there is nothing temporary left to dismiss. It
    /// minimises rather than closes: Escape has to stay safe to press, and a
    /// key that quietly destroys the window you were working in is not.
    pub fn hide_focused(&mut self, now: Seconds) -> bool {
        let Some(id) = self.windows.focused().map(|w| w.id) else {
            return false;
        };
        self.windows.minimize(id, now);
        true
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
            glow_warm: self.palette.glow_warm,
            glow_cool: self.palette.glow_cool,
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
            || !self.hush.at_rest(now)
            || !self.search_presence.at_rest(now)
            || !self.center_presence.at_rest(now)
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

        // Underneath everything, including the mode: the mark is the surface
        // the workspace sits on rather than something drawn onto it.
        let presence = if self.boot.is_done() {
            1.0
        } else {
            self.boot.workspace_presence(now)
        };
        wallpaper::draw(
            &mut self.frame,
            &self.palette,
            self.brand,
            self.size,
            now,
            hush,
            presence,
        );

        // Focus makes détends quieter: the chrome recedes, the content does
        // not (§12). Never the reverse.
        let chrome = 1.0 - hush * 0.55;

        self.draw_windows(now, &time);

        dock::draw(
            &mut self.frame,
            &self.palette,
            &self.windows,
            self.size,
            self.dock_hover,
            now,
            chrome,
        );

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

    /// How much room an app gets when it is the only thing on screen.
    ///
    /// Kept clear of the status cluster above and the dock below, so a window
    /// opened at full size never sits under either.
    #[allow(dead_code)]
    fn workspace_area(&self) -> Rect {
        let dock = dock::bounds(self.size);
        let area = Rect::from_min_size(Vec2::ZERO, self.size).inset(space::VAST * 0.5);
        let bottom = (dock.min().y - space::ROOM).min(area.max().y);
        Rect::from_min_size(
            area.min(),
            Vec2 {
                x: area.width(),
                y: (bottom - area.min().y).max(1.0),
            },
        )
    }

    fn draw_windows(&mut self, now: Seconds, time: &TimeOfDay) {
        // While a temporary surface is up, everything behind it recedes (rule
        // 2) — dimmer and very slightly smaller — so the search field replaces
        // what is behind it as the thing being looked at rather than competing
        // with it.
        let surface = self
            .search_presence
            .value(now)
            .max(self.center_presence.value(now))
            .clamp(0.0, 1.0);
        let recede = 1.0 - surface * 0.55;

        let windows: Vec<crate::window::Window> = self.windows.visible().cloned().collect();
        let focused = self.windows.focused().map(|w| w.id);

        for window in windows {
            // Only the focused window is at full strength. Attention still
            // belongs to one thing, which is the part of the five-modes idea
            // worth keeping now that there are several windows.
            let front = Some(window.id) == focused;
            let opacity = recede * if front { 1.0 } else { 0.62 };
            if opacity <= 0.004 {
                continue;
            }

            self.draw_window(&window, front, opacity, now, time);
        }
    }

    /// One window: its glass, its bar, its lights, and whatever it holds.
    fn draw_window(
        &mut self,
        window: &crate::window::Window,
        front: bool,
        opacity: f32,
        now: Seconds,
        time: &TimeOfDay,
    ) {
        use detends_paint::{Fill, Icon, IconShape, Text, ICON_STROKE};

        frame_glass(&mut self.frame, &self.palette, window.rect, front, opacity);

        // The two lights, top right. Colour is the whole signal here, so these
        // are the one place in détends that uses a hue for its own sake —
        // everybody already knows what the red one does.
        for (index, (rect, colour)) in [
            (window.minimize_button(), Color::hex(0xE0B341)),
            (window.close_button(), Color::hex(0xE05A47)),
        ]
        .iter()
        .enumerate()
        {
            self.frame.push(
                Item::new(
                    Id::of("window-light").nth(window.id.0 * 4 + index as u64),
                    Layer::Content,
                    Primitive::Fill(Fill {
                        rect: Rect::from_center_size(rect.center, Vec2::splat(12.0)),
                        radius: 6.0,
                        squircle: 2.0,
                        // Dimmed on an unfocused window, so only the window you
                        // are in advertises what can be done to it.
                        color: if front { *colour } else { colour.fade(0.35) },
                    }),
                )
                .opacity(opacity)
                .z(6),
            );
        }

        let bar = window.titlebar();
        self.frame.push(
            Item::new(
                Id::of("window-icon").nth(window.id.0),
                Layer::Content,
                Primitive::Icon(Icon {
                    rect: Rect::from_center_size(
                        Vec2 { x: bar.min().x + 20.0, y: bar.center.y },
                        Vec2::splat(15.0),
                    ),
                    shape: window.app.icon(),
                    stroke: ICON_STROKE,
                    color: self.palette.text_faint,
                    rim: 0.2,
                }),
            )
            .opacity(opacity)
            .z(6),
        );

        self.frame.push(
            Item::new(
                Id::of("window-title").nth(window.id.0),
                Layer::Content,
                Primitive::Text(Text {
                    text: window.app.name().into(),
                    // Left-aligned text starts at the rect's left edge, so the
                    // rect is centred half its width to the right of where the
                    // title should begin.
                    rect: Rect::from_min_size(
                        Vec2 {
                            x: bar.min().x + 38.0,
                            y: bar.center.y - text::LABEL.size,
                        },
                        Vec2 {
                            x: (bar.width() - 110.0).max(20.0),
                            y: text::LABEL.size * 2.0,
                        },
                    ),
                    size: text::LABEL.size,
                    weight: text::LABEL.weight,
                    tracking: text::LABEL.tracking,
                    line_height: text::LABEL.line_height,
                    color: if front { self.palette.text } else { self.palette.text_faint },
                    align: Align::Left,
                }),
            )
            .opacity(opacity)
            .z(6),
        );

        let area = window.content().inset(space::ROOM);

        match window.app {
            App::Clock => {
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
                    1.0,
                );
            }
            App::Spotify if self.music.is_some() => {
                let music = self.music.as_mut().expect("just checked");
                let state = music.state().clone();
                nowplaying::draw(&mut self.frame, &self.palette, area, &state, now, opacity, 1.0);
            }
            App::Files if self.vault.is_some() => {
                let vault = self.vault.as_ref().expect("just checked");
                self.browser
                    .draw(&mut self.frame, &self.palette, area, vault, now, opacity, 1.0);
            }
            // Not built yet. The window still opens and says what it will be,
            // rather than the app being hidden until it works — a system that
            // looks finished and is not is worse than one visibly partway.
            other => {
                let _ = IconShape::Music;
                self.frame.push(
                    Item::new(
                        Id::of("window-empty").nth(window.id.0),
                        Layer::Content,
                        Primitive::Text(Text {
                            text: match other {
                                App::Surf => "Surf has no engine yet".into(),
                                App::Spotify => "Music is not connected".into(),
                                App::Mail => "Mail is not built yet".into(),
                                App::Studio => "Studio is not built yet".into(),
                                _ => "Not built yet".into(),
                            },
                            rect: Rect::from_center_size(
                                area.center,
                                Vec2 { x: area.width(), y: text::HEADING.size * 2.0 },
                            ),
                            size: text::HEADING.size,
                            weight: text::HEADING.weight,
                            tracking: text::HEADING.tracking,
                            line_height: text::HEADING.line_height,
                            color: self.palette.text_faint,
                            align: Align::Center,
                        }),
                    )
                    .opacity(opacity)
                    .z(5),
                );
            }
        }
    }

    /// Files' keyboard, in one place (§9).
    ///
    /// Two states: typing a name, and not. While a name is being typed every
    /// letter is a letter — that is why this is a single function rather than
    /// guards spread across the event match, where a shortcut could quietly
    /// steal a keystroke out of a text field.
    fn files_key(&mut self, now: Seconds, key: Key, sys: bool) {
        let Some(mut vault) = self.vault.take() else {
            return;
        };

        if self.browser.is_editing() {
            let action = match key {
                Key::Enter => self.browser.commit(&vault),
                Key::Backspace => {
                    self.browser.backspace();
                    browser::Action::None
                }
                Key::Space => {
                    self.browser.type_char(' ');
                    browser::Action::None
                }
                Key::Character(c) => {
                    self.browser.type_char(c);
                    browser::Action::None
                }
                // Digits reach the shell as `Mode`, because bare 1–5 navigate
                // (§3). In a text field they are simply digits — "Week 1" has
                // to be a name a person can type.
                Key::Mode(digit) => {
                    self.browser.type_char((b'0' + digit) as char);
                    browser::Action::None
                }
                _ => browser::Action::None,
            };
            self.vault = Some(vault);
            self.act_on(action);
            return;
        }

        // Super+Backspace deletes, which is the one destructive key here and
        // so the only one that wants a modifier.
        let action = match (key, sys) {
            (Key::Backspace, true) => {
                let stamp = self.clock.now().zoned().timestamp();
                self.browser.delete(&mut vault, stamp)
            }
            (Key::Backspace, false) => {
                self.browser.up(&vault);
                browser::Action::None
            }
            (Key::Left, false) => {
                self.browser.step_place(now, -1, &vault);
                browser::Action::None
            }
            (Key::Right, false) => {
                self.browser.step_place(now, 1, &vault);
                browser::Action::None
            }
            (Key::Up, false) => {
                self.browser.step(-1);
                browser::Action::None
            }
            (Key::Down, false) => {
                self.browser.step(1);
                browser::Action::None
            }
            (Key::Enter, false) => self.browser.open(&vault),
            (Key::Character(c), false) => match c.to_ascii_lowercase() {
                'n' => {
                    self.browser.begin_new_folder();
                    browser::Action::None
                }
                // In Recently Deleted the same key means "put it back", which
                // is the only thing anyone wants there (§9).
                'r' => {
                    if self.browser.place() == Place::Deleted {
                        self.browser.restore(&mut vault)
                    } else {
                        self.browser.begin_rename();
                        browser::Action::None
                    }
                }
                'd' => self.browser.duplicate(&vault),
                's' => {
                    self.browser.cycle_sort(&mut vault);
                    browser::Action::None
                }
                _ => browser::Action::None,
            },
            _ => browser::Action::None,
        };

        self.vault = Some(vault);
        self.act_on(action);
    }

    /// Carry out whatever Files asked for.
    fn act_on(&mut self, action: browser::Action) {
        match action {
            browser::Action::None => {}
            browser::Action::Say(notice) => {
                let focused = self.focus.is_some();
                self.notifications.post(notice, focused);
            }
            // §9: opening a `.dpg` belongs to Studio. Studio cannot edit
            // anything yet (Milestone 4), so détends goes there and says what
            // it was asked to open rather than pretending to have opened it.
            browser::Action::OpenDocument(path, kind) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let focused = self.focus.is_some();
                self.notifications.post(
                    Notice {
                        title: name,
                        detail: format!("Studio {} — not yet editable", kind.name()),
                        icon: IconShape::Studio,
                    },
                    focused,
                );
            }
        }
    }

}


/// A window's pane.
///
/// Two rules are doing work here, and both come from how glass composites.
///
/// The pane sits in [`Layer::Environment`], not `Content`, because the app
/// inside it draws its own glass — Spotify's artwork panel, for one — and two
/// glass surfaces that overlap cannot be composited in a single pass. Putting
/// the window one layer down means the app's glass refracts *the window*, and
/// the window refracts the wallpaper, which is also what they physically are.
///
/// And only the focused window is glass at all. Two overlapping windows would
/// be two overlapping glass surfaces in the same layer, which has the same
/// problem and no layer left to solve it with. Behind the front one a window
/// becomes a flat translucent panel — cheaper, and it says plainly which
/// window you are in.
fn frame_glass(frame: &mut Frame, palette: &Palette, rect: Rect, front: bool, opacity: f32) {
    use detends_paint::{Fill, Glass};

    let id = Id::of("window-glass").nth(rect.min().x as u64 ^ ((rect.min().y as u64) << 20));

    if front {
        frame.push(
            Item::new(
                id,
                Layer::Environment,
                Primitive::Glass(Glass {
                    rect,
                    radius: 18.0,
                    squircle: 5.0,
                    thickness: 16.0,
                    bevel: 26.0,
                    ior: 1.48,
                    dispersion: 0.018,
                    frost: 0.78,
                    tint: palette.glass,
                    rim: 0.95,
                }),
            )
            .opacity(opacity)
            .z(4),
        );
    } else {
        frame.push(
            Item::new(
                id,
                Layer::Environment,
                Primitive::Fill(Fill {
                    rect,
                    radius: 18.0,
                    squircle: 5.0,
                    color: palette.ground.fade(0.72),
                }),
            )
            .opacity(opacity)
            .z(4),
        );
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

        // Boot lands on an empty desktop: the wallpaper and the dock. Nothing
        // is opened for you, which is the point of a dock.
        assert!(shell.showing_desktop());
        assert!(
            !s.iter().any(|t| t == "HOME" || t == "DÉTENDS"),
            "the desktop should not label itself"
        );
    }

    #[test]
    fn a_bare_number_goes_to_its_place() {
        // No modifier, so nothing in the window system can intercept it.
        let (mut shell, mut t) = booted();
        for mode in App::ALL {
            t += 1.0;
            let _ = mode;
            press(&mut shell, t, Key::Mode(1));
            assert_eq!(shell.focused_app(), Some(App::ALL[0]), "slot 1 did nothing");
        }
    }

    #[test]
    fn a_bare_number_typed_into_search_is_text() {
        // A digit navigates — except while Search has the keyboard, where it
        // has to be a character or the field would be unusable.
        let (mut shell, t) = booted();
        shell.open(t, App::Files);
        shell.open_search(t);
        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Mode(5),
                modifiers: Modifiers::default(),
            },
        );
        assert!(shell.search_is_open(), "the field closed");
        assert_eq!(shell.focused_app(), Some(App::Files), "a digit navigated while typing");
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
        assert!(shell.showing_desktop(), "a keystroke landed during boot");
    }

    #[test]
    fn the_leaving_mode_stops_being_drawn_once_it_has_gone() {
        let (mut shell, t) = booted();
        shell.open(t, App::Spotify);
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
        shell.open(t, App::Mail);
        shell.begin_focus(t, None, None);
        assert_eq!(shell.focused_app(), Some(App::Mail));
    }

    #[test]
    fn layers_never_overlap_within_themselves() {
        let (mut shell, mut t) = booted();
        for mode in App::ALL {
            shell.open(t, mode);
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
        assert_eq!(shell.focused_app(), Some(App::Files));
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
        shell.open(t, App::Files);
        shell.open_search(t);
        type_into_search(&mut shell, t, "music");
        press(&mut shell, t, Key::Escape);

        assert!(!shell.search_is_open());
        assert_eq!(shell.focused_app(), Some(App::Files), "Escape should not have navigated");
    }

    /// Open System Center and return where its panel is.
    fn with_center_open(shell: &mut Shell, t: Seconds) -> Rect {
        shell.open_center(t);
        // The panel is placed from the cluster, which is only known once a
        // frame has been built.
        shell.tick(t);
        center::resting_place(shell.cluster_bounds, shell.size)
    }

    fn click(shell: &mut Shell, t: Seconds, at: Vec2) {
        shell.input(
            t,
            &Event::PointerDown { x: at.x, y: at.y, button: MouseButton::Left },
        );
        shell.input(
            t,
            &Event::PointerUp { x: at.x, y: at.y, button: MouseButton::Left },
        );
    }

    #[test]
    fn the_system_center_toggles_actually_change_the_system() {
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);
        let layout = center::layout(panel);

        let target = |control: center::Control| {
            layout
                .toggles
                .iter()
                .find(|(c, _)| *c == control)
                .map(|(_, r)| r.center)
                .expect("control has a target")
        };

        // Wi-Fi off, then on again.
        assert!(shell.system.wifi_on());
        click(&mut shell, t, target(center::Control::Wifi));
        assert!(!shell.system.wifi_on(), "Wi-Fi did not turn off");
        click(&mut shell, t, target(center::Control::Wifi));
        assert!(shell.system.wifi_on(), "Wi-Fi did not come back");

        // Bluetooth names what it is connected to rather than saying "On".
        click(&mut shell, t, target(center::Control::Bluetooth));
        assert!(shell.system.bluetooth.is_some());

        // Airplane settles every radio at once (§11).
        click(&mut shell, t, target(center::Control::Airplane));
        assert!(shell.system.airplane);
        assert!(!shell.system.wifi_on(), "Airplane left Wi-Fi on");
        assert!(shell.system.bluetooth.is_none(), "Airplane left Bluetooth on");
    }

    #[test]
    fn asking_for_a_radio_leaves_airplane_mode_rather_than_refusing() {
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);
        let layout = center::layout(panel);
        let wifi = layout.toggles.iter().find(|(c, _)| *c == center::Control::Wifi).unwrap().1;

        shell.set_airplane(t, true);
        click(&mut shell, t, wifi.center);

        assert!(!shell.system.airplane, "it should have left Airplane Mode");
        assert!(shell.system.wifi_on());
    }

    #[test]
    fn focus_can_be_started_and_ended_from_system_center() {
        // Focus is a state, not a mode (§12) — which is why it belongs here.
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);
        let layout = center::layout(panel);
        let focus = layout.toggles.iter().find(|(c, _)| *c == center::Control::Focus).unwrap().1;

        assert!(shell.focus.is_none());
        click(&mut shell, t, focus.center);
        assert!(shell.focus.is_some(), "Focus did not begin");
        click(&mut shell, t, focus.center);
        assert!(shell.focus.is_none(), "Focus did not end");
    }

    #[test]
    fn the_sliders_can_be_dragged() {
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);
        let (_, track) = center::layout(panel).sliders[0];

        // Press near the left end, then drag to the right end.
        shell.input(
            t,
            &Event::PointerDown {
                x: track.min().x + 2.0,
                y: track.center.y,
                button: MouseButton::Left,
            },
        );
        assert!(shell.system.volume < 0.05, "the press should have set it");

        shell.input(t, &Event::PointerMoved { x: track.max().x, y: track.center.y });
        assert!(shell.system.volume > 0.95, "the drag did not follow");
    }

    #[test]
    fn a_drag_keeps_working_outside_the_panel_and_stops_on_release() {
        // Letting go ends a drag; leaving the track does not. Otherwise a
        // slider drops the moment your hand moves slightly off it.
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);
        let (_, track) = center::layout(panel).sliders[1];

        shell.input(
            t,
            &Event::PointerDown {
                x: track.center.x,
                y: track.center.y,
                button: MouseButton::Left,
            },
        );
        shell.input(t, &Event::PointerMoved { x: track.min().x - 400.0, y: 900.0 });
        assert!(shell.system.brightness < 0.05, "the drag stopped at the edge");

        shell.input(
            t,
            &Event::PointerUp { x: 0.0, y: 900.0, button: MouseButton::Left },
        );
        shell.input(t, &Event::PointerMoved { x: track.max().x, y: track.center.y });
        assert!(shell.system.brightness < 0.05, "it kept dragging after release");
    }

    #[test]
    fn clicking_the_panel_background_does_not_dismiss_it() {
        // Only clicking *away* dismisses. A press that misses a control inside
        // the panel should do nothing at all.
        let (mut shell, t) = booted();
        let panel = with_center_open(&mut shell, t);

        click(&mut shell, t, Vec2 { x: panel.center.x, y: panel.min().y + 2.0 });
        assert!(shell.system_center_is_open(), "it closed under its own contents");

        click(&mut shell, t, Vec2 { x: 40.0, y: 900.0 });
        assert!(!shell.system_center_is_open(), "clicking away should dismiss");
    }

    #[test]
    fn super_q_asks_to_quit_and_escape_never_does() {
        // The pair matters: Escape must always be safe to press, which is only
        // true because there is a separate, deliberate way out.
        let (mut shell, t) = booted();
        shell.open(t, App::Clock);

        press(&mut shell, t, Key::Escape);
        assert!(shell.take_power_request().is_none(), "Escape quit the system");

        shell.input(
            t,
            &Event::KeyDown {
                key: Key::Character('q'),
                modifiers: Modifiers { sys: true, ..Default::default() },
            },
        );
        assert_eq!(shell.take_power_request(), Some(Power::ShutDown));
        assert!(shell.take_power_request().is_none(), "it should be taken once");
    }

    #[test]
    fn digits_typed_into_a_files_name_are_text_not_mode_switches() {
        // The same rule Search follows: 1-5 navigate, but never out of a field
        // someone is typing a name into. "Week 1" must be a nameable folder.
        let (mut shell, t) = booted();

        let root = std::env::temp_dir().join(format!(
            "detends-shell-digits-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        shell.open_vault(root.clone());
        shell.open(t, App::Files);

        shell.show_place(t, Place::Studio);
        shell.browser_mut().begin_new_folder();
        for c in "Week ".chars() {
            press(&mut shell, t, Key::Character(c));
        }
        press(&mut shell, t, Key::Mode(1));

        assert_eq!(shell.focused_app(), Some(App::Files), "a digit navigated away");

        press(&mut shell, t, Key::Enter);
        let names: Vec<String> = shell
            .browser()
            .entries()
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(names, ["Week 1"], "the digit should have been typed");

        let _ = std::fs::remove_dir_all(&root);
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
        assert_eq!(shell.focused_app(), Some(App::Clock), "a timer belongs in Clock");
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

        shell.open(t, App::Mail);
        assert_eq!(shell.focused_app(), Some(App::Mail));

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
        shell.open(t, App::Clock);
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
    fn opening_a_surface_makes_the_shell_animate_again() {
        let (mut shell, t) = booted();
        shell.tick(t + 5.0);
        shell.open_search(t + 5.0);
        assert!(shell.tick(t + 5.01).frame.animating);
    }

    #[test]
    fn a_window_recedes_behind_a_temporary_surface() {
        // Rule 2 survives the move to windows: with Search up, what is behind
        // it steps back rather than competing.
        let (mut shell, t) = booted();
        shell.open(t, App::Clock);
        shell.tick(t + 2.0);

        let opacity_of = |shell: &mut Shell, at: Seconds| {
            shell
                .tick(at)
                .frame
                .items
                .iter()
                .find(|i| i.id == detends_paint::Id::of("clock-time"))
                .map(|i| i.opacity)
                .expect("the clock face")
        };

        let bright = opacity_of(&mut shell, t + 2.0);
        shell.open_search(t + 2.0);
        let dim = opacity_of(&mut shell, t + 3.0);
        assert!(dim < bright, "the window did not recede: {dim} vs {bright}");

        shell.close_search(t + 3.0);
        let back = opacity_of(&mut shell, t + 6.0);
        assert!(back > dim, "it did not come back");
    }

    #[test]
    fn the_focused_window_is_brighter_than_the_one_behind_it() {
        // What replaced "one mode at a time": attention still belongs to one
        // window, and the others say so by receding.
        let (mut shell, t) = booted();
        shell.open(t, App::Clock);
        shell.open(t, App::Files);
        let frame = shell.tick(t + 1.0).frame;

        let glass: Vec<f32> = frame
            .items
            .iter()
            .filter(|i| i.layer == detends_paint::Layer::Environment)
            .filter(|i| !matches!(i.primitive, Primitive::Image(_)))
            .map(|i| i.opacity)
            .collect();

        assert_eq!(glass.len(), 2, "expected two window panes, got {glass:?}");
        assert!(
            glass.iter().any(|o| *o > 0.9) && glass.iter().any(|o| *o < 0.9),
            "both windows are drawn at the same strength: {glass:?}"
        );
    }

    #[test]
    fn the_dock_offers_every_app_and_opening_one_shows_it() {
        let (mut shell, t) = booted();
        assert!(shell.showing_desktop());

        for app in App::ALL {
            let at = crate::dock::slots(vec2(1512.0, 982.0))
                .iter()
                .find(|(a, _)| *a == app)
                .map(|(_, r)| r.center)
                .expect("a dock slot");

            shell.input(
                t,
                &Event::PointerDown { x: at.x, y: at.y, button: MouseButton::Left },
            );
            assert_eq!(shell.focused_app(), Some(app), "{app:?} did not open");
        }

        assert!(!shell.showing_desktop());
    }
}
