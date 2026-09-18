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
