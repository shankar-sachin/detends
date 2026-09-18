/* Peak and energy of a buffer, in one pass.
 *
 * Level metering needs both the loudest sample and the total energy, and
 * reading the buffer twice to get them wastes the memory bandwidth that
 * actually limits this work. One pass computes both: a running lane-wise
 * maximum of |x|, and a running lane-wise sum of x² through FMLA.
 *
 * void detends_analyse(const float *src, size_t n, float *peak, float *energy)
 *   x0 = src, x1 = n, x2 = &peak, x3 = &energy
 */

#include "abi.h"

.text
.globl SYM(detends_analyse)
#if !defined(__APPLE__)
.type SYM(detends_analyse), %function
#endif
.p2align 2
SYM(detends_analyse):
    movi    v16.4s, #0              /* running peak, four lanes */
    movi    v17.4s, #0              /* running sum of squares, four lanes */

    lsr     x4, x1, #2
    cbz     x4, 2f

1:  ld1     {v0.4s}, [x0], #16
    fabs    v1.4s, v0.4s
    fmax    v16.4s, v16.4s, v1.4s
    fmla    v17.4s, v0.4s, v0.4s
    subs    x4, x4, #1
    b.ne    1b

    /* Fold the lanes down *before* touching the tail.
     *
     * Writing to a scalar register zeroes the upper lanes of the vector it
     * aliases — `fmadd s17, ...` silently discards lanes 1 to 3 of v17. Doing
     * the reduction first means the tail only ever accumulates into a value
     * that is already scalar, which is correct and also simpler to read. */
2:  fmaxv   s18, v16.4s
    faddp   v17.4s, v17.4s, v17.4s
    faddp   v17.4s, v17.4s, v17.4s
    fmov    s19, s17

    ands    x4, x1, #3              /* 0-3 samples left over */
    b.eq    3f

4:  ldr     s0, [x0], #4
    fabs    s1, s0
    fmax    s18, s18, s1
    /* FMLA has no scalar single-precision form; the scalar fused
     * multiply-add is FMADD, with the accumulator named explicitly. */
    fmadd   s19, s0, s0, s19
    subs    x4, x4, #1
    b.ne    4b

3:  str     s18, [x2]
    str     s19, [x3]
    ret
