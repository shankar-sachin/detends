/* Cubic resampling.
 *
 * Audio files arrive at whatever rate they were recorded at; the device runs at
 * whatever rate it was opened at. Something has to reconcile the two, every
 * time, and doing it badly is audible as a dull high end or a metallic edge.
 *
 * Catmull-Rom is the right trade here: it passes exactly through its control
 * points, needs only four of them, and has a continuous first derivative, so it
 * introduces none of the clicks that a linear interpolator produces at every
 * sample boundary. It is also cheap enough to run per sample without thinking
 * about it.
 */

#include "detends.h"

static inline float catmull_rom(float p0, float p1, float p2, float p3, float t) {
    const float t2 = t * t;
    const float t3 = t2 * t;
    return 0.5f * ((2.0f * p1) + (-p0 + p2) * t +
                   (2.0f * p0 - 5.0f * p1 + 4.0f * p2 - p3) * t2 +
                   (-p0 + 3.0f * p1 - 3.0f * p2 + p3) * t3);
}

static inline float tap(const float *src, size_t frames, unsigned channels,
                        long frame, unsigned channel) {
    /* Clamp at the edges rather than wrapping. Wrapping would splice the end of
     * the buffer onto the beginning and produce a click on every block. */
    if (frame < 0) {
        frame = 0;
    }
    if ((size_t)frame >= frames) {
        frame = (long)frames - 1;
    }
    return src[(size_t)frame * channels + channel];
}

size_t detends_resample_cubic(float *dst, size_t dst_capacity, const float *src,
                              size_t src_frames, unsigned channels, double ratio) {
    if (!dst || !src || channels == 0 || src_frames == 0 || ratio <= 0.0) {
        return 0;
    }

    const size_t max_frames = dst_capacity / channels;
    size_t written = 0;
    double position = 0.0;

    while (written < max_frames) {
        const long base = (long)position;
        if ((size_t)base >= src_frames) {
            break;
        }
        const float t = (float)(position - (double)base);

        for (unsigned c = 0; c < channels; ++c) {
            dst[written * channels + c] = catmull_rom(
                tap(src, src_frames, channels, base - 1, c),
                tap(src, src_frames, channels, base, c),
                tap(src, src_frames, channels, base + 1, c),
                tap(src, src_frames, channels, base + 2, c), t);
        }

        written += 1;
        position += ratio;
    }

    return written;
}
