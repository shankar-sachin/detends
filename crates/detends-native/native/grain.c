/* The blue-noise energy kernel.
 *
 * Generating the dither tile means repeatedly asking "would swapping these two
 * pixels make the pattern less clumpy?", and the answer is a Gaussian-weighted
 * sum over each pixel's neighbourhood. That question gets asked a few hundred
 * thousand times at startup, and it is almost all of the cost.
 *
 * In C rather than assembly on purpose: the access pattern wraps around both
 * axes and strides across rows, so the loads scatter. Vectorising it would mean
 * gathering, and NEON has no gather worth using — the win would be small and
 * the code would be much harder to be sure of. The honest optimisation here is
 * a tight scalar loop the compiler can unroll, which is what this is.
 */

#include "detends.h"

float detends_grain_energy(const float *values, int width, int height, int index,
                           const float *kernel, int radius) {
    if (!values || !kernel || width <= 0 || height <= 0 || radius < 0) {
        return 0.0f;
    }

    const int span = 2 * radius + 1;
    const int x = index % width;
    const int y = index / width;
    const float here = values[index];

    float sum = 0.0f;
    for (int dy = -radius; dy <= radius; ++dy) {
        /* Wrap, so the tile is seamless when repeated across the screen. */
        int ny = y + dy;
        ny = (ny % height + height) % height;

        for (int dx = -radius; dx <= radius; ++dx) {
            if (dx == 0 && dy == 0) {
                continue;
            }
            int nx = x + dx;
            nx = (nx % width + width) % width;

            const float weight = kernel[(dy + radius) * span + (dx + radius)];
            /* Similar values close together are exactly what we penalise. */
            const float similarity = 1.0f - (here - values[ny * width + nx] >= 0.0f
                                                 ? here - values[ny * width + nx]
                                                 : values[ny * width + nx] - here);
            sum += weight * similarity * similarity;
        }
    }
    return sum;
}
