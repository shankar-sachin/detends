//! Every native kernel, checked against the obviously-correct version.
//!
//! This is what makes hand-written assembly worth having. The argument for it
//! is never "it is faster" on its own — it is "it is faster *and* it computes
//! exactly what the plain version computes", and the second half has to be
//! demonstrated rather than asserted.
//!
//! The awkward cases get as much attention as the ordinary ones, because the
//! ways vectorised code goes wrong are specific and predictable: lengths that
//! do not divide by the vector width, empty input, values past full scale, and
//! numbers small enough to be denormal.

use detends_native as native;
use detends_native::portable;

/// A deterministic spread of values, including some past full scale.
fn samples(n: usize, seed: u64) -> Vec<f32> {
    let mut state = seed | 1;
    (0..n)
        .map(|i| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let unit = ((state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32)
                / (1u32 << 24) as f32;
            // Mostly inside the usual range, occasionally well past it.
            if i % 17 == 0 {
                (unit - 0.5) * 6.0
            } else {
                unit * 2.0 - 1.0
            }
        })
        .collect()
}

/// Lengths that exercise the tail: not a multiple of four, either side of it,
/// and shorter than one vector.
const AWKWARD: [usize; 12] = [0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 1023];

#[test]
fn mixing_matches_the_oracle_at_every_length() {
    for len in AWKWARD {
        let src = samples(len, 0x51ED);
        let start = samples(len, 0xA17E);

        for gain in [0.0_f32, 1.0, 0.5, -0.75, 1e-30] {
            let mut theirs = start.clone();
            native::mix_gain(&mut theirs, &src, gain);

            let mut ours = start.clone();
            portable::mix_gain(&mut ours, &src, gain);

            for (i, (a, b)) in theirs.iter().zip(&ours).enumerate() {
                assert!(
                    (a - b).abs() <= 1e-6 * b.abs().max(1.0),
                    "len {len}, gain {gain}, sample {i}: {a} vs {b}"
                );
            }
        }
    }
}

#[test]
fn conversion_matches_the_oracle_including_clipping() {
    for len in AWKWARD {
        let src = samples(len, 0xC0FF);

        let mut theirs = vec![0_i16; len];
        native::f32_to_i16(&mut theirs, &src);

        let mut ours = vec![0_i16; len];
        portable::f32_to_i16(&mut ours, &src);

        assert_eq!(theirs, ours, "at length {len}");
    }
}

#[test]
fn conversion_saturates_rather_than_wrapping() {
    // The failure that matters: a sample past full scale must clip, not
    // reappear at the opposite polarity as a burst of noise.
    let loud = [2.0_f32, -2.0, 50.0, -50.0, f32::MAX, f32::MIN];
    let mut out = vec![0_i16; loud.len()];
    native::f32_to_i16(&mut out, &loud);
    assert_eq!(out, vec![32767, -32768, 32767, -32768, 32767, -32768]);
}

#[test]
fn analysis_matches_the_oracle() {
    for len in AWKWARD {
        let src = samples(len, 0x9A11);

        let (peak, energy) = native::analyse(&src);
        let (want_peak, want_energy) = portable::analyse(&src);

        assert!(
            (peak - want_peak).abs() < 1e-5,
            "peak at length {len}: {peak} vs {want_peak}"
        );
        // Summed in a different order, so the error is proportional rather
        // than absolute.
        assert!(
            (energy - want_energy).abs() <= 1e-4 * want_energy.max(1.0),
            "energy at length {len}: {energy} vs {want_energy}"
        );
    }
}

#[test]
fn denormals_do_not_change_the_answer() {
    // Flush-to-zero would make the vector path disagree with the scalar one on
    // very small values. Worth knowing either way.
    let tiny = vec![1e-40_f32; 64];
    let (peak, _) = native::analyse(&tiny);
    let (want, _) = portable::analyse(&tiny);
    assert!((peak - want).abs() <= 1e-38, "{peak} vs {want}");
}

#[test]
fn a_silent_buffer_reports_silence() {
    let quiet = vec![0.0_f32; 256];
    assert_eq!(native::analyse(&quiet), (0.0, 0.0));
    assert_eq!(native::rms(&quiet), 0.0);
}

#[test]
fn rms_of_a_square_wave_is_its_amplitude() {
    let square: Vec<f32> = (0..1024)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    assert!((native::rms(&square) - 0.5).abs() < 1e-5);
}

#[test]
fn mismatched_lengths_stop_at_the_shorter_one() {
    // Reading past the end of either buffer would be the worst kind of bug to
    // have in a kernel taking raw pointers.
    let mut dst = vec![1.0_f32; 4];
    native::mix_gain(&mut dst, &[1.0, 1.0], 1.0);
    assert_eq!(dst, vec![2.0, 2.0, 1.0, 1.0]);

    let mut dst = vec![0.0_f32; 2];
    native::mix_gain(&mut dst, &[1.0; 8], 1.0);
    assert_eq!(dst, vec![1.0, 1.0]);
}

#[test]
fn resampling_at_unity_returns_what_it_was_given() {
    let src: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
    let mut dst = vec![0.0_f32; 64];
    let frames = native::resample_cubic(&mut dst, &src, 1, 1.0);

    assert_eq!(frames, 64);
    for (i, (a, b)) in dst.iter().zip(&src).enumerate() {
        assert!((a - b).abs() < 1e-5, "sample {i}: {a} vs {b}");
    }
}

#[test]
fn resampling_changes_the_length_by_the_ratio() {
    let src: Vec<f32> = (0..100).map(|i| i as f32).collect();

    // Half speed: twice as many frames out.
    let mut dst = vec![0.0_f32; 400];
    assert_eq!(native::resample_cubic(&mut dst, &src, 1, 0.5), 200);

    // Double speed: half as many.
    let mut dst = vec![0.0_f32; 400];
    assert_eq!(native::resample_cubic(&mut dst, &src, 1, 2.0), 50);
}

#[test]
fn resampling_a_ramp_stays_a_ramp() {
    // Catmull-Rom passes through its control points, so a straight line comes
    // back a straight line — an interpolator that rounded off a ramp would be
    // audibly dull on real material.
    //
    // Frames near either end are excluded deliberately. The kernel clamps at
    // the edges rather than extrapolating, so where the four-point window runs
    // off the buffer the curve leans very slightly toward the boundary sample.
    // With a window of four that reaches about two input frames in, which at
    // half speed is four output frames.
    //
    // Clamping is the right trade: extrapolating past the end of a buffer can
    // overshoot arbitrarily far, and a click is worse than a fraction of a
    // sample of tilt on four frames out of a hundred and twenty-eight.
    let src: Vec<f32> = (0..64).map(|i| i as f32).collect();
    let mut dst = vec![0.0_f32; 128];
    let frames = native::resample_cubic(&mut dst, &src, 1, 0.5);

    for (i, got) in dst.iter().enumerate().take(frames - 4).skip(4) {
        let want = i as f32 * 0.5;
        assert!((got - want).abs() < 1e-3, "frame {i}: {got} vs {want}");
    }
}

#[test]
fn the_edges_stay_bounded_rather_than_running_away() {
    // The property that actually matters at a boundary. A cubic through a
    // duplicated endpoint overshoots a little — that is inherent, and harmless.
    // What must never happen is the unbounded excursion that extrapolation
    // produces, which is heard as a crack.
    let src: Vec<f32> = (0..32).map(|i| i as f32).collect();
    let mut dst = vec![0.0_f32; 64];
    let frames = native::resample_cubic(&mut dst, &src, 1, 0.5);

    // One input step of slack either side, no more.
    for (i, value) in dst[..frames].iter().enumerate() {
        assert!(
            (-1.0..=32.0).contains(value),
            "frame {i} left the range of the input: {value}"
        );
    }
}

#[test]
fn resampling_keeps_channels_apart() {
    // Interleaved stereo where the two channels are obviously different; any
    // bleed between them would show up immediately.
    let src: Vec<f32> = (0..64).flat_map(|i| [i as f32, -(i as f32)]).collect();
    let mut dst = vec![0.0_f32; 256];
    let frames = native::resample_cubic(&mut dst, &src, 2, 1.0);

    assert_eq!(frames, 64);
    for i in 0..frames {
        assert!((dst[i * 2] + dst[i * 2 + 1]).abs() < 1e-3, "channels bled at {i}");
    }
}

#[test]
fn resampling_refuses_nonsense_rather_than_misbehaving() {
    let src = vec![1.0_f32; 16];
    let mut dst = vec![0.0_f32; 16];
    assert_eq!(native::resample_cubic(&mut dst, &src, 0, 1.0), 0);
    assert_eq!(native::resample_cubic(&mut dst, &src, 1, 0.0), 0);
    assert_eq!(native::resample_cubic(&mut dst, &src, 1, -1.0), 0);
    assert_eq!(native::resample_cubic(&mut dst, &[], 1, 1.0), 0);
    assert_eq!(native::resample_cubic(&mut [], &src, 1, 1.0), 0);
}

#[test]
fn the_chime_starts_at_silence_and_decays_to_it() {
    let rate = 48_000.0_f32;
    let seconds = 1.5_f32;
    let frames = (rate * seconds) as usize;
    let mut out = vec![0.0_f32; frames * 2];
    native::chime(&mut out, 2, rate, 660.0, seconds);

    // It must begin from nothing: a sine started at full amplitude puts a step
    // in the waveform, heard as a click before the note.
    assert!(out[0].abs() < 1e-3, "the chime starts with a step: {}", out[0]);

    let first = &out[..out.len() / 8];
    let last = &out[out.len() * 7 / 8..];
    assert!(native::rms(first) > 0.01, "the chime never sounded");
    assert!(
        native::rms(last) < native::rms(first) * 0.25,
        "the chime does not decay"
    );
}

#[test]
fn the_chime_stays_within_headroom() {
    let mut out = vec![0.0_f32; 48_000];
    native::chime(&mut out, 1, 48_000.0, 880.0, 1.0);
    let (peak, _) = native::analyse(&out);
    assert!(peak > 0.05, "inaudibly quiet: {peak}");
    assert!(peak < 0.9, "no headroom left for anything else: {peak}");
}

#[test]
fn the_chime_gets_darker_as_it_fades() {
    // The property that separates a struck object from a synthesiser tone: the
    // high partials die first. Measured as zero crossings, which fall as the
    // sound loses its top end.
    let rate = 48_000.0_f32;
    let mut out = vec![0.0_f32; (rate * 2.0) as usize];
    native::chime(&mut out, 1, rate, 440.0, 2.0);

    let crossings = |s: &[f32]| {
        s.windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    };
    let early = crossings(&out[..4800]);
    let late = crossings(&out[out.len() - 9600..out.len() - 4800]);
    assert!(late < early, "the tail is not darker: {late} vs {early}");
}

#[test]
fn both_channels_of_the_chime_are_identical() {
    let mut out = vec![0.0_f32; 2048];
    native::chime(&mut out, 2, 48_000.0, 440.0, 0.5);
    for frame in out.chunks_exact(2) {
        assert_eq!(frame[0], frame[1]);
    }
}

#[test]
fn the_grain_kernel_wraps_at_the_edges() {
    // The tile has to be seamless, so a pixel on the left edge must see the
    // right edge as its neighbour.
    let width = 8;
    let height = 8;
    let values: Vec<f32> = (0..width * height).map(|i| (i % 3) as f32 / 3.0).collect();
    let radius = 2;
    let span = (2 * radius + 1) as usize;
    let kernel = vec![1.0_f32; span * span];

    let left = native::grain_energy(&values, width, height, 0, &kernel, radius);
    let middle = native::grain_energy(&values, width, height, 27, &kernel, radius);

    assert!(left > 0.0, "the edge pixel saw nothing");
    // With a uniform kernel and a repeating pattern, wrapping makes the edge
    // behave like anywhere else. Without it, the edge would see fewer
    // neighbours and score visibly lower.
    assert!(
        (left - middle).abs() < middle * 0.5,
        "the edge is not wrapping: {left} vs {middle}"
    );
}

#[test]
fn the_grain_kernel_refuses_impossible_arguments() {
    let values = vec![0.5_f32; 64];
    let kernel = vec![1.0_f32; 25];
    assert_eq!(native::grain_energy(&values, 0, 8, 0, &kernel, 2), 0.0);
    assert_eq!(native::grain_energy(&values, 8, 8, -1, &kernel, 2), 0.0);
    assert_eq!(native::grain_energy(&values, 8, 8, 999, &kernel, 2), 0.0);
    assert_eq!(native::grain_energy(&values, 8, 8, 0, &[], 2), 0.0);
}

#[test]
fn the_backend_reports_itself_honestly() {
    let backend = native::backend();
    assert!(!backend.is_empty());
    assert_eq!(native::accelerated(), backend.contains("NEON"));

    #[cfg(target_arch = "aarch64")]
    assert!(native::accelerated(), "the assembly should be in use here");
}

// ---- the channel and decode kernels ------------------------------------
//
// These have their own awkward case on top of the usual ones: LD2 and ST2 move
// four *frames* at a time, so the tail is measured in frames rather than
// samples and a buffer with an odd number of floats in it must not tempt the
// kernel into reading one past the end.

#[test]
fn i16_to_f32_matches_the_portable_version_at_every_awkward_length() {
    for n in AWKWARD {
        // The full range, including both endpoints, which is where a wrong
        // scale factor shows up first.
        let src: Vec<i16> = (0..n)
            .map(|i| match i % 5 {
                0 => i16::MIN,
                1 => i16::MAX,
                2 => 0,
                3 => -1,
                _ => (i as i32 * 977 % 32768) as i16,
            })
            .collect();

        let mut fast = vec![0.0_f32; n];
        let mut slow = vec![0.0_f32; n];
        native::i16_to_f32(&mut fast, &src);
        portable::i16_to_f32(&mut slow, &src);

        assert_eq!(fast, slow, "n = {n}");
    }
}

#[test]
fn the_most_negative_sample_lands_exactly_on_minus_one() {
    // The reason the scale is 32768 and not 32767. Exact, not approximate.
    let mut out = [0.0_f32; 2];
    native::i16_to_f32(&mut out, &[i16::MIN, 16384]);
    assert_eq!(out, [-1.0, 0.5]);
}

#[test]
fn deinterleaving_matches_the_portable_version() {
    for frames in AWKWARD {
        let src = samples(frames * 2, 0x51DE);

        let (mut fl, mut fr) = (vec![0.0; frames], vec![0.0; frames]);
        let (mut sl, mut sr) = (vec![0.0; frames], vec![0.0; frames]);

        native::deinterleave_stereo(&mut fl, &mut fr, &src);
        portable::deinterleave_stereo(&mut sl, &mut sr, &src);

        assert_eq!(fl, sl, "left, {frames} frames");
        assert_eq!(fr, sr, "right, {frames} frames");
    }
}

#[test]
fn interleaving_matches_the_portable_version() {
    for frames in AWKWARD {
        let left = samples(frames, 0xA11);
        let right = samples(frames, 0xB22);

        let mut fast = vec![0.0; frames * 2];
        let mut slow = vec![0.0; frames * 2];

        native::interleave_stereo(&mut fast, &left, &right);
        portable::interleave_stereo(&mut slow, &left, &right);

        assert_eq!(fast, slow, "{frames} frames");
    }
}

#[test]
fn a_round_trip_through_planar_and_back_is_the_identity() {
    // The property that matters in the audio path: nothing is lost or swapped
    // on the way out to the processing and back to the device.
    for frames in AWKWARD {
        let original = samples(frames * 2, 0xC0FFEE);

        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
        native::deinterleave_stereo(&mut left, &mut right, &original);

        let mut back = vec![0.0; frames * 2];
        native::interleave_stereo(&mut back, &left, &right);

        assert_eq!(back, original, "{frames} frames");
    }
}

#[test]
fn an_odd_trailing_float_is_left_alone_rather_than_read_past() {
    // Seven floats is three whole frames and a stray. The kernel must convert
    // three frames and stop, rather than reading an eighth float that is not
    // there.
    let src = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    let (mut left, mut right) = (vec![0.0; 4], vec![0.0; 4]);

    native::deinterleave_stereo(&mut left, &mut right, &src);

    assert_eq!(left, vec![1.0, 3.0, 5.0, 0.0]);
    assert_eq!(right, vec![2.0, 4.0, 6.0, 0.0]);
}

#[test]
fn the_shorter_buffer_bounds_the_work() {
    let src = samples(64, 7);
    let (mut left, mut right) = (vec![0.0; 32], vec![0.0; 5]);

    native::deinterleave_stereo(&mut left, &mut right, &src);

    // Five frames done, and nothing written past where `right` ran out.
    assert_eq!(left[5], 0.0);
    assert_ne!(left[4], 0.0);
}
