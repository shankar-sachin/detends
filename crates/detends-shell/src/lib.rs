//! The détends shell.
#![forbid(unsafe_code)]

pub mod app;
pub mod boot;
pub mod browser;
pub mod center;
pub mod clockface;
pub mod dock;
pub mod input;
pub mod notify;
pub mod nowplaying;
pub mod search;
pub mod settings;
pub mod shell;
pub mod status;
pub mod system;
pub mod window;
pub mod wallpaper;

pub use app::App;
pub use boot::{Boot, Mark, Phase};
pub use window::{Press, Window, WindowId, Windows};
pub use browser::Browser;
pub use input::{Event, Key, Modifiers, MouseButton};
pub use clockface::{ClockFace, Section};
pub use notify::{Notice, Notifications};
pub use search::{Command, Power, Search, Setting};
pub use shell::{Brand, EnvironmentState, Output, Shell};
pub use system::{Battery, Focus, Network, System, FOCUS_DURATIONS};
