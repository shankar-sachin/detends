//! What a détends frame is.
//!
//! Pure data describing a moment on screen: geometry, colour, the design
//! tokens, and the display list that carries them to the renderer. No GPU, no
//! windowing, no platform.
//!
//! The shell builds a [`Frame`] each time something changes; the renderer draws
//! it. Neither knows anything about the other, which is what lets the same
//! shell run as a desktop application today and as a Wayland compositor later.

#![forbid(unsafe_code)]

mod color;
mod display_list;
mod geometry;
mod tokens;

pub use color::{linear_to_srgb, rgba, srgb_to_linear, Color};
pub use display_list::{
    Align, Fill, Frame, Glass, Icon, IconShape, Id, Image, Item, Layer, Primitive, Text, TextureId,
    ICON_STROKE,
};
pub use geometry::{vec2, Rect, Vec2};
pub use tokens::{space, text, Appearance, GlassSettings, Palette, TextStyle};

/// Re-exported so downstream crates animate against the same trait without
/// depending on `detends-motion` directly.
pub use detends_motion::{springs, Animatable, MotionPreference, Params, Seconds, Spring};
