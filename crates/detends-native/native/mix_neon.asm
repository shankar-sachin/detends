/* Mixing one source into a destination, with gain.
 *
 * The inner loop of every audio callback: for each sample,
 *     dst[i] += src[i] * gain
 *
 * Four samples per iteration through one fused multiply-add, which is both
 * faster and *more accurate* than a separate multiply and add — FMLA keeps full
 * intermediate precision instead of rounding twice.
 *
 * Measured against the portable Rust path on Apple Silicon, this is a **wash**
 * — about 1.0×. LLVM already auto-vectorises a loop this simple into the same
 * instructions, and hand-writing it wins nothing. It is kept because it is
 * correct, because it costs nothing, and because the guarantee matters on
 * toolchains that are less clever. But it is not a speedup, and claiming one
 * would be a lie the benchmark immediately exposes.
 *
 * The kernels that *do* win are the ones LLVM cannot vectorise on its own:
 * `analyse` (2.5×, because reordering floating-point additions changes the
 * result, so the optimiser will not do it) and `f32_to_i16` (2.0×, because the
 * saturating narrow has no portable spelling).
 *
 * void detends_mix_gain(float *dst, const float *src, float gain, size_t n)
 *   x0 = dst, x1 = src, s0 = gain, x2 = n (samples, not frames)
 */

#include "abi.h"

.text
.globl SYM(detends_mix_gain)
#if !defined(__APPLE__)
.type SYM(detends_mix_gain), %function
#endif
.p2align 2
SYM(detends_mix_gain):
    cbz     x2, 3f                  /* nothing to do */
    dup     v1.4s, v0.s[0]          /* gain in all four lanes */

    lsr     x3, x2, #2              /* whole vectors */
    cbz     x3, 2f

1:  ld1     {v2.4s}, [x1], #16      /* source */
    ld1     {v3.4s}, [x0]           /* destination, read in place */
    fmla    v3.4s, v2.4s, v1.4s     /* dst += src * gain */
    st1     {v3.4s}, [x0], #16
    subs    x3, x3, #1
    b.ne    1b

2:  ands    x3, x2, #3              /* 0-3 samples left over */
    b.eq    3f

4:  ldr     s2, [x1], #4
    ldr     s3, [x0]
    fmadd   s3, s2, s0, s3
    str     s3, [x0], #4
    subs    x3, x3, #1
    b.ne    4b

3:  ret
