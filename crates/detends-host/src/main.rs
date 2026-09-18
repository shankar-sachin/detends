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
            WindowEvent::CloseRequested => event_loop.exit(),

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

                // A fullscreen shell must always have a way out.
                if pressed && matches!(key, input::Key::Escape) {
                    event_loop.exit();
                    return;
                }

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
                let out = shell.tick(now);
                let animating = out.frame.animating;
                let env = environment(out.environment, now);

                // Tell the platform a present is imminent, so it can schedule
                // the frame rather than discovering it.
                window.pre_present_notify();
                renderer.draw(out.frame, env);

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

fn environment(state: detends_shell::EnvironmentState, now: f64) -> Environment {
    Environment {
        time: now as f32,
        focus: state.focus,
        near: state.near,
        far: state.far,
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
