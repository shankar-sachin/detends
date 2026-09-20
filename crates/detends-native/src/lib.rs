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
//! | `deinterleave_stereo` | **2.6×** | LD2 splits the two streams inside the load-store unit. A compiler will not produce it from a strided loop — it emits a scalar load per sample — so the structure *is* the optimisation. |
//! | `analyse` | **2.4×** | Reordering floating-point additions changes the result, so LLVM will not vectorise a reduction on its own. |
//! | `f32_to_i16` | **2.0×** | The saturating narrow has no portable spelling, so the safe version becomes a branch per sample. |
//! | `interleave_stereo` | 1.3× | ST2, in the other direction. Real, but far smaller than its twin: a store has nowhere to stall. |
//! | `i16_to_f32` | 1.0× | Expected to win, and does not. Widening and scaling is exactly the shape LLVM auto-vectorises well. |
//! | `mix_gain` | 1.0× | LLVM already auto-vectorises it perfectly. No gain. |
//!
//! Run `cargo test -p detends-native --release --test speed -- --nocapture`
//! for the current figures on the machine in front of you.
//!
//! The last two rows are kept deliberately, and `i16_to_f32` is the honest one:
//! it was written *expecting* a win by symmetry with `f32_to_i16`, and measured
//! at parity. The asymmetry has a cause — saturation is what makes the forward
//! direction hard for a compiler, and there is nothing to saturate coming back.
//! Four of six kernels earn their place. A benchmark that quietly dropped the
//! two that did not would be worth nothing.

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

/// Sixteen-bit integers to float samples.
///
/// The inverse of [`f32_to_i16`], and the path audio takes on the way *in*.
pub fn i16_to_f32(dst: &mut [f32], src: &[i16]) {
    #[cfg(detends_neon)]
    {
        ffi::i16_to_f32(dst, src);
    }
    #[cfg(not(detends_neon))]
    {
        let n = dst.len().min(src.len());
        portable::i16_to_f32(&mut dst[..n], &src[..n]);
    }
}

/// Interleaved stereo into two planar buffers.
pub fn deinterleave_stereo(left: &mut [f32], right: &mut [f32], src: &[f32]) {
    #[cfg(detends_neon)]
    {
        ffi::deinterleave_stereo(left, right, src);
    }
    #[cfg(not(detends_neon))]
    {
        portable::deinterleave_stereo(left, right, src);
    }
}

/// Two planar buffers into interleaved stereo.
pub fn interleave_stereo(dst: &mut [f32], left: &[f32], right: &[f32]) {
    #[cfg(detends_neon)]
    {
        ffi::interleave_stereo(dst, left, right);
    }
    #[cfg(not(detends_neon))]
    {
        portable::interleave_stereo(dst, left, right);
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
