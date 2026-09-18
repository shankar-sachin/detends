//! Motion for détends.
//!
//! Animation communicates relationships rather than showing off (rule 10), so
//! this crate is small on purpose: four named springs, one solver, one type.
//!
//! Springs are solved in closed form and sampled at absolute time. See
//! [`solver`] for why that matters — briefly, it is what makes motion identical
//! at any refresh rate, free of accumulated drift, and smoothly interruptible.
//!
//! This crate has no dependencies and defines no geometry. Implement
//! [`Animatable`] for your own types in whichever crate owns them.

#![forbid(unsafe_code)]

mod animatable;
mod params;
mod solver;
mod spring;

pub use animatable::Animatable;
pub use params::{springs, MotionPreference, Params};
pub use solver::{coeffs, Coeffs};
pub use spring::Spring;

/// A monotonic clock reading, in seconds.
///
/// The shell samples this once at the top of each frame and passes the same
/// value everywhere, so every spring in a frame agrees on what time it is.
/// Sampling per-call would let two halves of one transition disagree by a
/// few microseconds — invisible individually, but enough to break the
/// illusion that separately-animated elements are one moving object.
pub type Seconds = f64;
