/* Moving between interleaved and planar stereo.
 *
 * A device hands over — and wants back — interleaved frames: L R L R L R. Every
 * piece of processing in between wants planar buffers: all the left, then all
 * the right. So this conversion sits on both ends of the audio path and runs
 * over every sample twice per buffer.
 *
 * This is the clearest case in détends for writing assembly by hand. LD2 and
 * ST2 do the de-interleave and re-interleave *in the load-store unit*, four
 * frames at a time, for the price of an ordinary access. A compiler will not
 * produce them from a strided loop: it sees `src[i * 2]` and emits a scalar
 * load per sample, because proving the stride is uniform and the pointers do
 * not alias is beyond what it is willing to assume. The structure is the whole
 * optimisation, and the structure is exactly what does not survive being
 * written in C.
 *
 * void detends_deinterleave_stereo(float *left, float *right,
 *                                  const float *src, size_t frames)
 *   x0 = left, x1 = right, x2 = src, x3 = frames
 *
 * void detends_interleave_stereo(float *dst, const float *left,
 *                                const float *right, size_t frames)
 *   x0 = dst, x1 = left, x2 = right, x3 = frames
 */

#include "abi.h"

.text

/* ---- interleaved -> planar ------------------------------------------- */

.globl SYM(detends_deinterleave_stereo)
#if !defined(__APPLE__)
.type SYM(detends_deinterleave_stereo), %function
#endif
.p2align 2
SYM(detends_deinterleave_stereo):
    cbz     x3, 3f

    lsr     x4, x3, #2              /* four frames — eight floats — at a time */
    cbz     x4, 2f

    /* LD2 splits the two interleaved streams as it loads: v0 takes the even
     * elements and v1 the odd, which is precisely left and right. */
1:  ld2     {v0.4s, v1.4s}, [x2], #32
    st1     {v0.4s}, [x0], #16
    st1     {v1.4s}, [x1], #16
    subs    x4, x4, #1
    b.ne    1b

2:  ands    x4, x3, #3              /* 0-3 frames left over */
    b.eq    3f

4:  ldr     s0, [x2], #4
    ldr     s1, [x2], #4
    str     s0, [x0], #4
    str     s1, [x1], #4
    subs    x4, x4, #1
    b.ne    4b

3:  ret

/* ---- planar -> interleaved ------------------------------------------- */

.globl SYM(detends_interleave_stereo)
#if !defined(__APPLE__)
.type SYM(detends_interleave_stereo), %function
#endif
.p2align 2
SYM(detends_interleave_stereo):
    cbz     x3, 3f

    lsr     x4, x3, #2
    cbz     x4, 2f

1:  ld1     {v0.4s}, [x1], #16
    ld1     {v1.4s}, [x2], #16
    /* ST2 weaves the two registers back together on the way out. */
    st2     {v0.4s, v1.4s}, [x0], #32
    subs    x4, x4, #1
    b.ne    1b

2:  ands    x4, x3, #3
    b.eq    3f

4:  ldr     s0, [x1], #4
    ldr     s1, [x2], #4
    str     s0, [x0], #4
    str     s1, [x0], #4
    subs    x4, x4, #1
    b.ne    4b

3:  ret
