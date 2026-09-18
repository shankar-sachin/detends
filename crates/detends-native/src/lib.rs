//! CPU kernels for détends, in C, C++ and aarch64 assembly.
//!
//! Three kinds of work live here, chosen because each is genuinely better off
//! outside Rust rather than because native code is interesting:
//!
//! - **Audio mixing and conversion**, in hand-written NEON. This runs in a
//!   real-time callback with a hard deadline: miss it and the user hears a
//!   click, every time. It is per-sample arithmetic over contiguous memory,
//!   which is precisely what SIMD is for.
//! - **Resampling and synthesis**, in C and C++. Not vectorised — the work is
//!   branchy and the structure matters more than the throughput — but naturally
//!   expressed as plain numerical code.
//! - **The blue-noise kernel**, in C. Scattered, wrapping access that does not
//!   vectorise usefully; the honest optimisation is a tight scalar loop.
//!
//! Every kernel with an assembly implementation also has a pure-Rust one in
//! [`portable`], which is both the fallback elsewhere and the oracle the tests
//! compare against.
//!
//! # What the assembly is actually worth
//!
//! Measured on Apple Silicon by `tests/speed.rs`, against the portable path:
//!
//! | kernel | speedup | why |
//! |---|---|---|
//! | `analyse` | **2.5×** | Reordering floating-point additions changes the result, so LLVM will not vectorise a reduction on its own. |
//! | `f32_to_i16` | **2.0×** | The saturating narrow has no portable spelling, so the safe version becomes a branch per sample. |
//! | `mix_gain` | 1.0× | LLVM already auto-vectorises it perfectly. No gain. |
//!
//! That last row is kept deliberately. Two of three kernels earn their place
//! and one does not, and a benchmark that only reported the flattering numbers
//! would be worth nothing.

// `deny` rather than `forbid`, because `forbid` cannot be lifted anywhere —
// and this crate needs exactly one place where it can be. Every other module
// is held to the same standard as the rest of détends.
#![deny(unsafe_code)]

pub mod portable;

/// The boundary to the native kernels, and the only `unsafe` in détends.
#[allow(unsafe_code)]
mod ffi;

pub use ffi::{chime, grain_energy, resample_cubic};

/// Whether the hand-written paths are in use.
pub const fn accelerated() -> bool {
    cfg!(detends_neon)
}

/// A short name for what is running, for logs and the about screen.
pub const fn backend() -> &'static str {
    if cfg!(detends_neon) {
        "aarch64 NEON"
    } else {
        "portable"
    }
}

/// `dst[i] += src[i] * gain`, over the overlapping prefix.
pub fn mix_gain(dst: &mut [f32], src: &[f32], gain: f32) {
    #[cfg(detends_neon)]
    {
        ffi::mix_gain(dst, src, gain);
    }
    #[cfg(not(detends_neon))]
    {
        let n = dst.len().min(src.len());
        portable::mix_gain(&mut dst[..n], &src[..n], gain);
    }
}

/// Float samples to the 16-bit integers a device wants, saturating.
pub fn f32_to_i16(dst: &mut [i16], src: &[f32]) {
    #[cfg(detends_neon)]
    {
        ffi::f32_to_i16(dst, src);
    }
    #[cfg(not(detends_neon))]
    {
        let n = dst.len().min(src.len());
        portable::f32_to_i16(&mut dst[..n], &src[..n]);
    }
}

/// Loudest absolute sample, and the sum of squares.
pub fn analyse(src: &[f32]) -> (f32, f32) {
    #[cfg(detends_neon)]
    {
        ffi::analyse(src)
    }
    #[cfg(not(detends_neon))]
    {
        portable::analyse(src)
    }
}

/// Root-mean-square level of a buffer.
pub fn rms(src: &[f32]) -> f32 {
    if src.is_empty() {
        return 0.0;
    }
    (analyse(src).1 / src.len() as f32).sqrt()
}
