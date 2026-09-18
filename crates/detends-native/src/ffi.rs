//! The boundary to the native kernels.
//!
//! The only `unsafe` in détends. Everything below is a plain C function taking
//! raw pointers and lengths, so the entire safety argument is "the slice
//! lengths handed in are the lengths passed down", which each wrapper below
//! establishes before calling.

#![allow(unsafe_code)]

use std::ffi::c_void;

unsafe extern "C" {
    #[cfg(detends_neon)]
    fn detends_mix_gain(dst: *mut f32, src: *const f32, gain: f32, n: usize);

    #[cfg(detends_neon)]
    fn detends_f32_to_i16(dst: *mut i16, src: *const f32, n: usize);

    #[cfg(detends_neon)]
    fn detends_analyse(src: *const f32, n: usize, peak: *mut f32, energy: *mut f32);

    fn detends_resample_cubic(
        dst: *mut f32,
        dst_capacity: usize,
        src: *const f32,
        src_frames: usize,
        channels: u32,
        ratio: f64,
    ) -> usize;

    fn detends_grain_energy(
        values: *const f32,
        width: i32,
        height: i32,
        index: i32,
        kernel: *const f32,
        radius: i32,
    ) -> f32;

    fn detends_chime(
        out: *mut f32,
        frames: usize,
        channels: u32,
        sample_rate: f32,
        base_hz: f32,
        seconds: f32,
    );
}

/// `dst[i] += src[i] * gain`, over the overlapping prefix.
#[cfg(detends_neon)]
pub fn mix_gain(dst: &mut [f32], src: &[f32], gain: f32) {
    // The shorter of the two, so neither pointer can run past its allocation.
    let n = dst.len().min(src.len());
    if n == 0 {
        return;
    }
    // SAFETY: both pointers are valid for `n` floats, since `n` is the smaller
    // of the two slice lengths. The kernel reads `src` and writes `dst`, which
    // cannot alias because `dst` is an exclusive borrow.
    unsafe { detends_mix_gain(dst.as_mut_ptr(), src.as_ptr(), gain, n) }
}

#[cfg(detends_neon)]
pub fn f32_to_i16(dst: &mut [i16], src: &[f32]) {
    let n = dst.len().min(src.len());
    if n == 0 {
        return;
    }
    // SAFETY: as above — `n` samples are in bounds for both.
    unsafe { detends_f32_to_i16(dst.as_mut_ptr(), src.as_ptr(), n) }
}

#[cfg(detends_neon)]
pub fn analyse(src: &[f32]) -> (f32, f32) {
    let mut peak = 0.0_f32;
    let mut energy = 0.0_f32;
    if src.is_empty() {
        return (peak, energy);
    }
    // SAFETY: `src` is valid for its own length; the two outputs are local.
    unsafe { detends_analyse(src.as_ptr(), src.len(), &mut peak, &mut energy) }
    (peak, energy)
}

/// Resample, returning how many frames were written.
pub fn resample_cubic(
    dst: &mut [f32],
    src: &[f32],
    channels: u32,
    ratio: f64,
) -> usize {
    if channels == 0 || src.is_empty() || dst.is_empty() {
        return 0;
    }
    let src_frames = src.len() / channels as usize;
    if src_frames == 0 {
        return 0;
    }
    // SAFETY: `dst` is valid for its own length and `src` for `src_frames`
    // frames of `channels` samples, which is at most its length.
    unsafe {
        detends_resample_cubic(
            dst.as_mut_ptr(),
            dst.len(),
            src.as_ptr(),
            src_frames,
            channels,
            ratio,
        )
    }
}

/// The Gaussian-weighted clumpiness of one pixel's neighbourhood.
pub fn grain_energy(
    values: &[f32],
    width: i32,
    height: i32,
    index: i32,
    kernel: &[f32],
    radius: i32,
) -> f32 {
    let span = (2 * radius + 1) as usize;
    if width <= 0
        || height <= 0
        || radius < 0
        || values.len() < (width * height) as usize
        || kernel.len() < span * span
        || index < 0
        || index as usize >= values.len()
    {
        return 0.0;
    }
    // SAFETY: the bounds above establish that the kernel reads only within
    // `values` (it wraps its coordinates into `width × height`) and within the
    // `span × span` kernel.
    unsafe {
        detends_grain_energy(
            values.as_ptr(),
            width,
            height,
            index,
            kernel.as_ptr(),
            radius,
        )
    }
}

/// Render a chime into an interleaved buffer.
pub fn chime(out: &mut [f32], channels: u32, sample_rate: f32, base_hz: f32, seconds: f32) {
    if channels == 0 || out.is_empty() {
        return;
    }
    let frames = out.len() / channels as usize;
    if frames == 0 {
        return;
    }
    // SAFETY: `frames * channels` is at most `out.len()` by construction.
    unsafe {
        detends_chime(
            out.as_mut_ptr(),
            frames,
            channels,
            sample_rate,
            base_hz,
            seconds,
        )
    }
}

// Keeps the C linkage referenced on platforms where nothing else uses it.
#[allow(dead_code)]
fn _linkage() -> *const c_void {
    std::ptr::null()
}
