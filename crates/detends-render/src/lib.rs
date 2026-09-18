//! The détends renderer.
#![forbid(unsafe_code)]

pub mod buffers;
pub mod gpu;
pub mod grain;
pub mod instance;
pub mod pipelines;
pub mod renderer;
pub mod targets;
pub mod text;
pub mod textures;

pub use gpu::{Gpu, GpuError, OutputMode, WORKING_FORMAT};
pub use renderer::{Environment, Renderer};
pub use targets::{Target, Targets, PYRAMID_DEPTH};
pub use text::TextStack;
pub use textures::Textures;
