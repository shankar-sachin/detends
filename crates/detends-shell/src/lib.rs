//! The détends shell.
#![forbid(unsafe_code)]

pub mod boot;
pub mod center;
pub mod content;
pub mod input;
pub mod mode;
pub mod search;
pub mod shell;
pub mod status;
pub mod system;

pub use boot::{Boot, Mark, Phase};
pub use input::{Event, Key, Modifiers, MouseButton};
pub use mode::{Mode, Navigator};
pub use search::{Command, Power, Search, Setting};
pub use shell::{Brand, EnvironmentState, Output, Shell};
pub use system::{Battery, Focus, Network, System, FOCUS_DURATIONS};
