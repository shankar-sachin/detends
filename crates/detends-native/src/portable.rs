//! Pure-Rust implementations of every kernel.
//!
//! These serve two purposes, and the second is the more important one.
//!
//! They are the fallback wherever the assembly does not apply — a machine that
//! is not aarch64 still runs détends, just without the hand-written paths.
//!
//! And they are the **oracle**. Hand-written assembly is worth having only if
//! you can show it computes the same thing as something obviously correct, so
//! every kernel is checked against the version here, on ordinary input and on
//! the awkward cases: empty buffers, lengths that do not divide by the vector
//! width, values past full scale, denormals.

/// `dst[i] += src[i] * gain`
pub fn mix_gain(dst: &mut [f32], src: &[f32], gain: f32) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d += s * gain;
    }
}

/// Float samples to the 16-bit integers a device wants, saturating.
pub fn f32_to_i16(dst: &mut [i16], src: &[f32]) {
    for (d, s) in dst.iter_mut().zip(src) {
        // Saturate rather than wrap: a sample past full scale must clip, not
        // reappear at the opposite polarity.
        let scaled = s * 32767.0;
        *d = if scaled >= 32767.0 {
            32767
        } else if scaled <= -32768.0 {
            -32768
        } else {
            scaled as i16
        };
    }
}

/// Loudest absolute sample, and the sum of squares.
pub fn analyse(src: &[f32]) -> (f32, f32) {
    let mut peak = 0.0_f32;
    let mut energy = 0.0_f32;
    for s in src {
        peak = peak.max(s.abs());
        energy += s * s;
    }
    (peak, energy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_accumulates_rather_than_replacing() {
        let mut dst = vec![1.0, 2.0, 3.0];
        mix_gain(&mut dst, &[1.0, 1.0, 1.0], 0.5);
        assert_eq!(dst, vec![1.5, 2.5, 3.5]);
    }

    #[test]
    fn conversion_saturates_at_both_ends() {
        let mut dst = vec![0_i16; 4];
        f32_to_i16(&mut dst, &[0.0, 1.0, 2.0, -2.0]);
        assert_eq!(dst, vec![0, 32767, 32767, -32768]);
    }

    #[test]
    fn analysis_finds_the_peak_regardless_of_sign() {
        let (peak, energy) = analyse(&[0.5, -0.9, 0.2]);
        assert!((peak - 0.9).abs() < 1e-6);
        assert!((energy - (0.25 + 0.81 + 0.04)).abs() < 1e-5);
    }

    #[test]
    fn empty_input_is_not_a_special_case_anywhere() {
        mix_gain(&mut [], &[], 1.0);
        f32_to_i16(&mut [], &[]);
        assert_eq!(analyse(&[]), (0.0, 0.0));
    }
}
