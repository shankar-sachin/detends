//! Runs the détends shell as a desktop application.
//!
//! This is the seam. Everything platform-specific lives here: winit, the event
//! loop, the frame clock, and the translation from winit's events into the
//! shell's own vocabulary. A Wayland compositor replaces this crate and nothing
//! else — the shell and the renderer never learn which one is driving them.

mod clock;
mod input;

use clock::Clock;
use detends_paint::{vec2, Appearance};
use detends_render::{Environment, Renderer};
use detends_shell::{Brand, Shell};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

struct Options {
    fullscreen: bool,
    /// Render frames to this path and exit.
    capture: Option<String>,
    /// Moments to capture, in seconds. More than one writes `path-1.png`,
    /// `path-2.png`, … from a single run.
    capture_at: Vec<f64>,
    /// Which mode a capture should be showing.
    mode: Option<String>,
    /// A surface to open before capturing: "search" or "center".
    open: Option<String>,
    /// Text to type into Search before capturing.
    query: Option<String>,
    /// Start a Focus session before capturing.
    focus: bool,
    /// Turn on Airplane Mode before capturing.
    airplane: bool,
    /// Report the frame rate periodically.
    fps: bool,
    /// Render the whole icon set as a sheet instead of the shell.
    icons: bool,
    /// Which of Clock's utilities to open, and some state to show in it.
    section: Option<String>,
    /// Which Files destination to show.
    place: Option<String>,
}

fn options() -> Options {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    Options {
        // Fullscreen by default: the shell owns the graphical experience (§2).
        // `--windowed` exists because iterating on a shader inside a fullscreen
        // window is miserable.
        fullscreen: !flag("--windowed"),
        capture: value("--capture"),
        capture_at: value("--capture-at")
            .map(|v| v.split(',').filter_map(|p| p.trim().parse().ok()).collect())
            .filter(|v: &Vec<f64>| !v.is_empty())
            .unwrap_or_else(|| vec![2.5]),
        mode: value("--mode"),
        open: value("--open"),
        query: value("--query"),
        focus: flag("--focus"),
        airplane: flag("--airplane"),
        fps: flag("--fps"),
        icons: flag("--icons"),
        section: value("--section"),
        place: value("--place"),
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = EventLoop::new().expect("could not start the event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(options());
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("détends stopped: {e}");
    }
}

struct App {
    options: Options,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    shell: Option<Shell>,
    clock: Clock,
    modifiers: input::Modifiers,
    pointer: (f32, f32),
    /// When the frame rate was last reported.
    last_report: f64,
    /// Whether the window has been revealed yet.
    shown: bool,
}

impl App {
    fn new(options: Options) -> Self {
        Self {
            options,
            window: None,
            renderer: None,
            shell: None,
            clock: Clock::new(),
            modifiers: input::Modifiers::default(),
            pointer: (0.0, 0.0),
            last_report: 0.0,
            shown: false,
        }
    }

    fn logical_size(window: &Window) -> (f32, f32) {
        let physical = window.inner_size();
        let scale = window.scale_factor() as f32;
        (
            physical.width as f32 / scale.max(0.1),
            physical.height as f32 / scale.max(0.1),
        )
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attributes = Window::default_attributes()
            .with_title("détends")
            .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0));

        // Stay hidden until there is something to show. wgpu initialisation,
        // font loading and the blue-noise tile take long enough that an
        // immediately-visible window means an empty frame on screen first, and
        // the boot sequence appears not to happen at all.
        attributes = attributes.with_visible(false);

        if self.options.capture.is_some() {
            // A capture still needs a surface for the swapchain, but it should
            // never appear, take focus, or disturb what is on screen. Without
            // this a run of captures flashes a window per frame and fights the
            // compositor for the display.
            attributes = attributes
                .with_visible(false)
                .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0));
        }

        if self.options.fullscreen && self.options.capture.is_none() {
            // Borderless rather than exclusive: exclusive fullscreen on macOS
            // triggers the Spaces animation and takes over the display mode,
            // both of which are the opposite of a calm handover from boot.
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("could not create a window"),
        );

        let physical = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            physical.width.max(1),
            physical.height.max(1),
        ))
        .expect("could not start the renderer");

        let (w, h) = Self::logical_size(&window);
        let now = self.clock.now();
        let mut shell = Shell::new(now, vec2(w, h), window.scale_factor() as f32);
        shell.set_appearance(Appearance::Automatic, prefers_dark(&window));

        // Timers and alarms set before quitting are still set afterwards.
        // Captures are deliberately excluded: a screenshot should not adopt —
        // or overwrite — whatever the real system has running.
        if self.options.capture.is_none() {
            if let Some(path) = detends_time::store::default_path() {
                shell.load_schedule(path);
            }
        }

        // The vault behind Files (§9). A capture gets a vault of its own under
        // the temporary directory, for the same reason it gets no schedule: a
        // screenshot must never show — or disturb — the user's own files.
        let vault_root = if self.options.capture.is_some() {
            let root = std::env::temp_dir().join("detends-capture-vault");
            seed_capture_vault(&root);
            Some(root)
        } else {
            detends_fs::place::default_root()
        };
        if let Some(root) = vault_root {
            shell.open_vault(root);
        }

        // The logo. Boot falls back to a typographic mark if either file is
        // missing, so a broken asset costs the artwork rather than the boot.
        let mut renderer = renderer;
        match (
            load_brand_image(&mut renderer, "mark-short", MARK_SHORT),
            load_brand_image(&mut renderer, "mark-full", MARK_FULL),
        ) {
            (Some((short, short_aspect)), Some((full, full_aspect))) => {
                shell.set_brand(Brand {
                    short,
                    short_aspect,
                    full,
                    full_aspect,
                });
            }
            _ => log::warn!("brand artwork unavailable; booting with the wordmark"),
        }

        self.renderer = Some(renderer);
        self.shell = Some(shell);
        self.window = Some(window.clone());

        if let Some(path) = self.options.capture.clone() {
            if self.options.icons {
                self.capture_icons(&path);
                event_loop.exit();
                return;
            }
            self.capture(&path);
            event_loop.exit();
            return;
        }

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(renderer), Some(shell)) = (
            self.window.as_ref(),
            self.renderer.as_mut(),
            self.shell.as_mut(),
        ) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                shell.save_schedule();
                event_loop.exit()
            }

            WindowEvent::Resized(size) => {
                renderer.resize(size.width, size.height);
                let (w, h) = Self::logical_size(window);
                shell.resize(vec2(w, h), window.scale_factor() as f32);
                window.request_redraw();
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let (w, h) = Self::logical_size(window);
                shell.resize(vec2(w, h), scale_factor as f32);
            }

            WindowEvent::ModifiersChanged(m) => {
                let state = m.state();
                self.modifiers = input::Modifiers {
                    sys: state.super_key(),
                    shift: state.shift_key(),
                    alt: state.alt_key(),
                    control: state.control_key(),
                };
            }

            WindowEvent::CursorMoved { position, .. } => {
                let scale = window.scale_factor() as f32;
                self.pointer = (position.x as f32 / scale, position.y as f32 / scale);
                let now = self.clock.now();
                shell.input(
                    now,
                    &input::Event::PointerMoved {
                        x: self.pointer.0,
                        y: self.pointer.1,
                    },
                );
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let key = input::from_winit(&event.logical_key);
                let pressed = event.state == ElementState::Pressed;

                // Escape belongs to the shell, where it backs out one step and
                // lands Home. Quitting is Cmd+Q or closing the window — an
                // Escape that exits the system can never dismiss anything, and
                // makes every surface a trap.
                let now = self.clock.now();
                let modifiers = self.modifiers;
                shell.input(
                    now,
                    &if pressed {
                        input::Event::KeyDown { key, modifiers }
                    } else {
                        input::Event::KeyUp { key, modifiers }
                    },
                );

                // Power is asked for by the shell and carried out here: the
                // shell has no business knowing how a machine is turned off,
                // and the host has no business knowing how it was asked (§19).
                if let Some(power) = shell.take_power_request() {
                    Self::act_on_power(power, shell, event_loop);
                    return;
                }

                window.request_redraw();
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = input::button_from_winit(button) else {
                    return;
                };
                let (x, y) = self.pointer;
                let now = self.clock.now();
                shell.input(
                    now,
                    &if state == ElementState::Pressed {
                        input::Event::PointerDown { x, y, button }
                    } else {
                        input::Event::PointerUp { x, y, button }
                    },
                );
                window.request_redraw();
            }

            WindowEvent::ThemeChanged(theme) => {
                let now = self.clock.now();
                shell.input(
                    now,
                    &input::Event::AppearanceChanged {
                        prefers_dark: theme == winit::window::Theme::Dark,
                    },
                );
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                let now = self.clock.tick();
                let first_frame = !self.shown;
                let out = shell.tick(now);
                let animating = out.frame.animating;
                let env = environment(out.environment, now);

                // Tell the platform a present is imminent, so it can schedule
                // the frame rather than discovering it.
                window.pre_present_notify();
                renderer.draw(out.frame, env);

                // Reveal only once a real frame has been presented, so the
                // first thing on screen is the mark rather than nothing.
                if first_frame {
                    self.shown = true;
                    window.set_visible(true);
                }

                // Perceived performance is a feature (§21), so it should be
                // measurable without a profiler attached.
                if self.options.fps && animating && now - self.last_report > 1.0 {
                    self.last_report = now;
                    log::info!(
                        "{:.0} fps  ({:.2} ms/frame)",
                        self.clock.fps(),
                        1000.0 / self.clock.fps()
                    );
                }

                if animating {
                    // With Fifo, acquiring the next drawable blocks until
                    // vsync, so this paces itself against the display rather
                    // than spinning.
                    window.request_redraw();
                } else {
                    // Nothing is moving. Stop drawing entirely rather than
                    // burning a core to redraw a static screen (§21).
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }

            _ => {}
        }
    }
}

impl App {
    fn capture(&mut self, path: &str) {
        let (Some(renderer), Some(shell)) = (self.renderer.as_mut(), self.shell.as_mut()) else {
            return;
        };

        // Step the sequence forward at a fixed rate so a capture is
        // reproducible rather than dependent on how fast this machine is, and
        // take every requested moment in one pass — one process, one GPU
        // context, however many frames.
        let mut moments: Vec<f64> = self.options.capture_at.clone();
        moments.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let multiple = moments.len() > 1;

        let step = 1.0 / 120.0;
        let mut t = 0.0;

        // Run boot to completion first, then switch, so a capture of a mode is
        // of the settled mode rather than of it arriving.
        if let Some(name) = self.options.mode.clone() {
            if let Some(mode) = detends_shell::Mode::matching(&name) {
                while t < 6.0 {
                    t += step;
                    shell.tick(t);
                }
                shell.go(t, mode);
            } else {
                log::warn!("no mode called {name:?}");
            }
        } else {
            // Always let boot finish, so a capture is of the settled shell.
            while t < 6.0 {
                t += step;
                shell.tick(t);
            }
        }

        if self.options.airplane {
            shell.set_airplane(t, true);
        }
        if self.options.focus {
            shell.begin_focus(t, Some("Physics homework".into()), Some(42.0 * 60.0 + 18.0));
        }

        if let Some(section) = self.options.section.clone() {
            use detends_shell::Section;
            let chosen = match section.to_lowercase().as_str() {
                "timers" | "timer" => Some(Section::Timers),
                "alarms" | "alarm" => Some(Section::Alarms),
                "stopwatch" => Some(Section::Stopwatch),
                "world" => Some(Section::World),
                _ => None,
            };
            if let Some(chosen) = chosen {
                shell.populate_clock_for_capture(t, chosen);
            } else {
                log::warn!("no clock section called {section:?}");
            }
        }

        if let Some(place) = self.options.place.clone() {
            use detends_fs::Place;
            let chosen = match place.to_lowercase().as_str() {
                "recents" | "recent" => Some(Place::Recents),
                "studio" => Some(Place::Studio),
                "downloads" | "download" => Some(Place::Downloads),
                "screenshots" | "screenshot" => Some(Place::Screenshots),
                "deleted" | "trash" => Some(Place::Deleted),
                _ => None,
            };
            match chosen {
                Some(chosen) => shell.show_place(t, chosen),
                None => log::warn!("no Files destination called {place:?}"),
            }
        }

        match self.options.open.as_deref() {
            Some("search") => {
                shell.open_search(t);
                if let Some(query) = self.options.query.clone() {
                    shell.type_into_search(&query);
                }
            }
            Some("center") => shell.open_center(t),
            Some(other) => log::warn!("nothing called {other:?} can be opened"),
            None => {}
        }

        for (index, moment) in moments.iter().enumerate() {
            let deadline = t + *moment;
            while t < deadline {
                t += step;
                shell.tick(t);
            }

            let out = shell.tick(t);
            let env = environment(out.environment, t);
            let (w, h) = renderer.size();
            let pixels = renderer.capture(out.frame, env);

            let target = if multiple {
                match path.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem}-{}.{ext}", index + 1),
                    None => format!("{path}-{}", index + 1),
                }
            } else {
                path.to_string()
            };

            match write_png(&target, w, h, &pixels) {
                Ok(()) => log::info!("captured {w}×{h} at t={t:.2}s to {target}"),
                Err(e) => log::error!("could not write {target}: {e}"),
            }
        }
    }
}

impl App {
    /// Carry out a power request (§19).
    ///
    /// Only the two that end the session are real here; Lock and Sleep need a
    /// compositor and a session manager that détends does not have yet, so they
    /// say so rather than pretending. Whatever happens, Clock's state is
    /// written out first — an alarm set before a restart is still set after.
    fn act_on_power(
        power: detends_shell::Power,
        shell: &mut Shell,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        use detends_shell::Power;

        shell.save_schedule();

        match power {
            Power::ShutDown | Power::Restart => {
                log::info!("détends: {power:?}");
                event_loop.exit();
            }
            Power::Lock | Power::Sleep => {
                log::warn!("{power:?} needs the compositor — Milestone 8");
            }
        }
    }

    /// Render every icon at several sizes, so the set can be judged as a set.
    ///
    /// An icon that works at 96px and falls apart at 24 is not finished, and
    /// the only way to know is to look at them together.
    fn capture_icons(&mut self, path: &str) {
        use detends_paint::{
            text, Align, Color, Frame, Icon, IconShape, Id, Item, Layer, Palette, Primitive, Rect,
            Text, Vec2, ICON_STROKE,
        };

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let (pw, ph) = renderer.size();
        let scale = 2.0_f32;
        let size = Vec2 { x: pw as f32 / scale, y: ph as f32 / scale };

        let palette = Palette::dark();
        let mut frame = Frame::new(size, scale);

        // Three sizes across, every shape down: small enough to test legibility,
        // large enough to test the curves.
        let sizes = [26.0_f32, 48.0, 96.0];
        let columns = 5.0_f32;
        let cell = Vec2 { x: size.x / columns, y: 132.0 };
        let top = 70.0;

        for (index, shape) in IconShape::ALL.iter().enumerate() {
            let col = (index % 5) as f32;
            let row = (index / 5) as f32;
            let origin = Vec2 {
                x: cell.x * col + cell.x * 0.5,
                y: top + row * cell.y,
            };

            let mut x = origin.x - 74.0;
            for side in sizes {
                frame.push(Item::new(
                    Id::of("icon").nth(index as u64 * 8 + side as u64),
                    Layer::Content,
                    Primitive::Icon(Icon {
                        rect: Rect::from_center_size(
                            Vec2 { x: x + side * 0.5, y: origin.y },
                            Vec2::splat(side),
                        ),
                        shape: *shape,
                        stroke: ICON_STROKE,
                        color: palette.text,
                        rim: 0.55,
                    }),
                ));
                x += side + 16.0;
            }

            frame.push(Item::new(
                Id::of("icon-label").nth(index as u64),
                Layer::Content,
                Primitive::Text(Text {
                    text: shape.name().to_uppercase().into(),
                    rect: Rect::from_center_size(
                        Vec2 { x: origin.x, y: origin.y + 62.0 },
                        Vec2 { x: cell.x, y: 24.0 },
                    ),
                    size: text::CAPTION.size,
                    weight: text::CAPTION.weight,
                    tracking: text::CAPTION.tracking * 2.0,
                    line_height: text::CAPTION.line_height,
                    color: palette.text_faint,
                    align: Align::Center,
                }),
            ));
        }

        frame.sort();

        let env = Environment {
            time: 0.0,
            focus: 0.0,
            near: palette.ground_far,
            far: palette.ground,
            glow_warm: palette.glow_warm,
            glow_cool: palette.glow_cool,
            glass_intensity: 1.0,
            glass_transparency: 1.0,
            presence: 1.0,
            fade: 1.0,
        };
        let _ = Color::WHITE;

        let pixels = renderer.capture(&frame, env);
        match write_png(path, pw, ph, &pixels) {
            Ok(()) => log::info!("icon sheet written to {path}"),
            Err(e) => log::error!("could not write {path}: {e}"),
        }
    }
}

fn environment(state: detends_shell::EnvironmentState, now: f64) -> Environment {
    Environment {
        time: now as f32,
        focus: state.focus,
        near: state.near,
        far: state.far,
        glow_warm: state.glow_warm,
        glow_cool: state.glow_cool,
        glass_intensity: state.glass_intensity,
        glass_transparency: state.glass_transparency,
        presence: state.presence,
        fade: state.fade,
    }
}

/// The two marks, compiled in so the shell has no runtime asset dependency.
const MARK_SHORT: &[u8] = include_bytes!("../../../assets/brand/mark-short.png");
const MARK_FULL: &[u8] = include_bytes!("../../../assets/brand/mark-full.png");

/// Decode a PNG and hand it to the renderer, returning its id and aspect ratio.
fn load_brand_image(
    renderer: &mut Renderer,
    label: &str,
    bytes: &[u8],
) -> Option<(detends_paint::TextureId, f32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;

    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        log::warn!("{label} is not 8-bit RGBA; skipping");
        return None;
    }

    buffer.truncate(info.buffer_size());
    let id = renderer.load_image(label, &buffer, info.width, info.height);
    Some((id, info.width as f32 / info.height.max(1) as f32))
}

fn write_png(path: &str, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

/// The platform's light/dark preference.
fn prefers_dark(window: &Window) -> bool {
    matches!(window.theme(), Some(winit::window::Theme::Dark) | None)
}

/// Fill the capture vault with the same kind of placeholder content the other
/// modes draw (§22: "Use placeholder content where necessary").
///
/// A screenshot of an empty Files says nothing about the layout, and pointing a
/// capture at the real vault would put the user's own filenames into any image
/// that got shared. So captures get their own vault, rebuilt each time.
fn seed_capture_vault(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);

    // One of each Studio type, so the three document icons are all visible,
    // plus a folder and an ordinary file for contrast.
    let files: [(&str, &str); 7] = [
        ("Studio/Physics Homework.dpg", "page"),
        ("Studio/Michaelmas Term.dek", "deck"),
        ("Studio/Lab Results.dgr", "grid"),
        ("Studio/Term/Week 1.dpg", "page"),
        ("Downloads/Resonance.pdf", "pdf"),
        ("Downloads/inter.zip", "zip"),
        ("Screenshots/Screenshot 2026-09-17 at 17.14.02.png", "png"),
    ];

    for (path, body) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, body.as_bytes());
    }
}
