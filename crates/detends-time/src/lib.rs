//! Time for détends.
//!
//! One environment, not five applications (§5). Clock, World Clock, Alarms,
//! Timers and Stopwatch share a single domain so that a timer started from
//! Search, a Focus session and an alarm are all the same kind of thing to the
//! system.
//!
//! Deliberately free of any rendering or platform dependency: everything here
//! is testable without a GPU, a display, or waiting in real time.

#![forbid(unsafe_code)]

pub mod clock;

pub use clock::{Clock, TimeOfDay};
