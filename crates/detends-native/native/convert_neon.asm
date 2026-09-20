/* Converting float samples to the 16-bit integers a device wants.
 *
 * Saturating rather than wrapping. A sample above 1.0 has to clip to full
 * scale: wrapping turns a moment of loudness into a burst of white noise at
 * the opposite polarity, which is both audible and unpleasant. FCVTZS
 * saturates into 32 bits and SQXTN saturates down to 16, so the clamp costs
 * nothing extra.
 *
 * void detends_f32_to_i16(int16_t *dst, const float *src, size_t n)
 *   x0 = dst, x1 = src, x2 = n
 */

#include "abi.h"

.text
.globl SYM(detends_f32_to_i16)
#if !defined(__APPLE__)
.type SYM(detends_f32_to_i16), %function
#endif
.p2align 2
SYM(detends_f32_to_i16):
    cbz     x2, 3f

    /* 32767.0f, built in two halves because it has no immediate encoding. */
    mov     w4, #0xFE00
    movk    w4, #0x46FF, lsl #16
    dup     v4.4s, w4

    lsr     x3, x2, #2
    cbz     x3, 2f

1:  ld1     {v0.4s}, [x1], #16
    fmul    v0.4s, v0.4s, v4.4s
    fcvtzs  v0.4s, v0.4s            /* saturating float -> i32 */
    sqxtn   v1.4h, v0.4s            /* saturating narrow  -> i16 */
    st1     {v1.4h}, [x0], #8
    subs    x3, x3, #1
    b.ne    1b

2:  ands    x3, x2, #3
    b.eq    3f

4:  ldr     s0, [x1], #4
    fmul    s0, s0, s4
    fcvtzs  w5, s0
    /* Narrow the scalar tail by hand, with the same saturation. */
    mov     w6, #32767
    cmp     w5, w6
    csel    w5, w6, w5, gt
    mov     w6, #-32768
    cmp     w5, w6
    csel    w5, w6, w5, lt
    strh    w5, [x0], #2
    subs    x3, x3, #1
    b.ne    4b

3:  ret

/* Sixteen-bit integers back to float samples.
 *
 * The other direction: what a file or a decoder hands over, on its way into a
 * mixer that works in float. Divides by 32768 rather than 32767, so that the
 * most negative representable sample maps exactly to -1.0 and the mapping is
 * an exact power of two — every value round-trips through the multiply without
 * error, which is not true if you scale by 32767.
 *
 * void detends_i16_to_f32(float *dst, const int16_t *src, size_t n)
 *   x0 = dst, x1 = src, x2 = n
 */
.globl SYM(detends_i16_to_f32)
#if !defined(__APPLE__)
.type SYM(detends_i16_to_f32), %function
#endif
.p2align 2
SYM(detends_i16_to_f32):
    cbz     x2, 3f

    /* 1/32768 = 2^-15, which is exactly 0x38000000 as a float. One MOVZ. */
    movz    w4, #0x3800, lsl #16
    dup     v4.4s, w4

    lsr     x3, x2, #3              /* eight at a time */
    cbz     x3, 2f

1:  ld1     {v0.8h}, [x1], #16
    /* Widen signed 16 -> 32 in two halves, low then high. */
    sshll   v1.4s, v0.4h, #0
    sshll2  v2.4s, v0.8h, #0
    scvtf   v1.4s, v1.4s
    scvtf   v2.4s, v2.4s
    fmul    v1.4s, v1.4s, v4.4s
    fmul    v2.4s, v2.4s, v4.4s
    st1     {v1.4s, v2.4s}, [x0], #32
    subs    x3, x3, #1
    b.ne    1b

2:  ands    x3, x2, #7              /* 0-7 samples left over */
    b.eq    3f

4:  ldrsh   w5, [x1], #2            /* sign-extending load */
    scvtf   s0, w5
    fmul    s0, s0, s4
    str     s0, [x0], #4
    subs    x3, x3, #1
    b.ne    4b

3:  ret
