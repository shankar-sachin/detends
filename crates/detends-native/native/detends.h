/* The native kernels, as Rust sees them. */
#ifndef DETENDS_H
#define DETENDS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Resampling — native/resample.c */
size_t detends_resample_cubic(float *dst, size_t dst_capacity, const float *src,
                              size_t src_frames, unsigned channels, double ratio);

/* Blue-noise energy — native/grain.c */
float detends_grain_energy(const float *values, int width, int height, int index,
                           const float *kernel, int radius);

/* The aarch64 kernels are declared in Rust rather than here: they have no C
 * callers, and a header that promised them on every platform would be a lie on
 * the ones where they are not built. See src/ffi.rs. */

/* Chime synthesis — native/synth.cpp */
void detends_chime(float *out, size_t frames, unsigned channels, float sample_rate,
                   float base_hz, float seconds);

#ifdef __cplusplus
}
#endif

#endif
