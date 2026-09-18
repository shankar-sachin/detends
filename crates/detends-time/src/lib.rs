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

pub mod alarm;
pub mod clock;
pub mod schedule;
pub mod stopwatch;
pub mod store;
pub mod timer;

pub use alarm::{Alarm, AlarmId, Repeat};
pub use clock::{Clock, TimeOfDay};
pub use schedule::{Fired, Schedule, WorldClock};
pub use stopwatch::Stopwatch;
pub use timer::{format_duration, Timer, TimerId, TimerState};
