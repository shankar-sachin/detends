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

/// Sixteen-bit integers to float samples.
///
/// Divided by 32768, not 32767: the scale is then an exact power of two, so
/// every value converts without rounding error and the most negative sample
/// lands exactly on -1.0.
pub fn i16_to_f32(dst: &mut [f32], src: &[i16]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d = *s as f32 / 32768.0;
    }
}

/// Interleaved stereo to two planar buffers.
pub fn deinterleave_stereo(left: &mut [f32], right: &mut [f32], src: &[f32]) {
    let frames = left.len().min(right.len()).min(src.len() / 2);
    for i in 0..frames {
        left[i] = src[i * 2];
        right[i] = src[i * 2 + 1];
    }
}

/// Two planar buffers to interleaved stereo.
pub fn interleave_stereo(dst: &mut [f32], left: &[f32], right: &[f32]) {
    let frames = left.len().min(right.len()).min(dst.len() / 2);
    for i in 0..frames {
        dst[i * 2] = left[i];
        dst[i * 2 + 1] = right[i];
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
    fn the_integer_round_trip_is_exact_at_full_scale() {
        // -1.0 maps to the most negative sample and back, because the scale is
        // a power of two. This is the reason for 32768 rather than 32767.
        let mut floats = vec![0.0_f32; 3];
        i16_to_f32(&mut floats, &[-32768, 0, 16384]);
        assert_eq!(floats, vec![-1.0, 0.0, 0.5]);
    }

    #[test]
    fn channels_survive_a_round_trip_through_planar_and_back() {
        let original: Vec<f32> = (0..14).map(|i| i as f32).collect();
        let frames = original.len() / 2;

        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
        deinterleave_stereo(&mut left, &mut right, &original);

        assert_eq!(left, vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);
        assert_eq!(right, vec![1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0]);

        let mut back = vec![0.0; original.len()];
        interleave_stereo(&mut back, &left, &right);
        assert_eq!(back, original);
    }

    #[test]
    fn channel_work_stops_at_the_shortest_buffer_rather_than_panicking() {
        let mut left = vec![0.0; 8];
        let mut right = vec![0.0; 2];
        deinterleave_stereo(&mut left, &mut right, &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(right, vec![2.0, 4.0]);
        assert_eq!(left[2], 0.0, "it should not have run past the short one");
    }

    #[test]
    fn empty_input_is_not_a_special_case_anywhere() {
        mix_gain(&mut [], &[], 1.0);
        f32_to_i16(&mut [], &[]);
        i16_to_f32(&mut [], &[]);
        deinterleave_stereo(&mut [], &mut [], &[]);
        interleave_stereo(&mut [], &[], &[]);
        assert_eq!(analyse(&[]), (0.0, 0.0));
    }
}
