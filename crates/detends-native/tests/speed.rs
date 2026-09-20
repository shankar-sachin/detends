//! What the hand-written paths actually buy.
//!
//! Reported rather than asserted. A benchmark that fails the build on a slow
//! machine — or a busy one — is a benchmark that gets deleted, and the number
//! is worth having even when it is disappointing.

use detends_native as native;
use detends_native::portable;
use std::time::Instant;

fn buffer(n: usize) -> Vec<f32> {
    (0..n).map(|i| ((i as f32) * 0.001).sin()).collect()
}

fn time(label: &str, iterations: usize, mut work: impl FnMut()) -> f64 {
    // Warm the caches and let the CPU settle on a clock.
    for _ in 0..iterations / 10 {
        work();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    let per = start.elapsed().as_secs_f64() / iterations as f64;
    println!("    {label:<28} {:>9.2} µs", per * 1e6);
    per
}

#[test]
fn how_much_faster_is_the_assembly() {
    // A quarter of a second of stereo at 48kHz: the kind of block a real-time
    // callback actually deals with, several times a second, forever.
    const FRAMES: usize = 48_000 / 4;
    const SAMPLES: usize = FRAMES * 2;
    const RUNS: usize = 400;

    let src = buffer(SAMPLES);
    println!("\n  backend: {}\n", native::backend());

    // Every result is read back through `black_box`. Without it the optimiser
    // notices the output is never used and deletes the whole call, which
    // reports as an impossibly fast portable path rather than as an error.
    println!("  mix, {SAMPLES} samples");
    let mut dst = vec![0.0_f32; SAMPLES];
    let native_mix = time("native", RUNS, || {
        native::mix_gain(&mut dst, &src, 0.5);
        std::hint::black_box(&dst);
    });
    let mut dst = vec![0.0_f32; SAMPLES];
    let portable_mix = time("portable", RUNS, || {
        portable::mix_gain(&mut dst, &src, 0.5);
        std::hint::black_box(&dst);
    });

    println!("\n  convert to i16, {SAMPLES} samples");
    let mut out = vec![0_i16; SAMPLES];
    let native_convert = time("native", RUNS, || {
        native::f32_to_i16(&mut out, &src);
        std::hint::black_box(&out);
    });
    let mut out = vec![0_i16; SAMPLES];
    let portable_convert = time("portable", RUNS, || {
        portable::f32_to_i16(&mut out, &src);
        std::hint::black_box(&out);
    });

    println!("\n  analyse, {SAMPLES} samples");
    let native_analyse = time("native", RUNS, || {
        std::hint::black_box(native::analyse(&src));
    });
    let portable_analyse = time("portable", RUNS, || {
        std::hint::black_box(portable::analyse(&src));
    });

    println!("\n  decode from i16, {SAMPLES} samples");
    let ints: Vec<i16> = (0..SAMPLES).map(|i| (i as i32 * 977 % 65536 - 32768) as i16).collect();
    let mut out = vec![0.0_f32; SAMPLES];
    let native_decode = time("native", RUNS, || {
        native::i16_to_f32(&mut out, &ints);
        std::hint::black_box(&out);
    });
    let mut out = vec![0.0_f32; SAMPLES];
    let portable_decode = time("portable", RUNS, || {
        portable::i16_to_f32(&mut out, &ints);
        std::hint::black_box(&out);
    });

    // The two the load-store unit does for free. Frames, not samples: the
    // interleaved buffer holds two floats per frame.
    let frames = SAMPLES / 2;
    println!("\n  deinterleave, {frames} frames");
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    let native_split = time("native", RUNS, || {
        native::deinterleave_stereo(&mut left, &mut right, &src);
        std::hint::black_box((&left, &right));
    });
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    let portable_split = time("portable", RUNS, || {
        portable::deinterleave_stereo(&mut left, &mut right, &src);
        std::hint::black_box((&left, &right));
    });

    println!("\n  interleave, {frames} frames");
    let planar_l = vec![0.25_f32; frames];
    let planar_r = vec![-0.25_f32; frames];
    let mut woven = vec![0.0_f32; frames * 2];
    let native_weave = time("native", RUNS, || {
        native::interleave_stereo(&mut woven, &planar_l, &planar_r);
        std::hint::black_box(&woven);
    });
    let mut woven = vec![0.0_f32; frames * 2];
    let portable_weave = time("portable", RUNS, || {
        portable::interleave_stereo(&mut woven, &planar_l, &planar_r);
        std::hint::black_box(&woven);
    });

    println!("\n  speedup");
    for (label, portable_time, native_time) in [
        ("mix", portable_mix, native_mix),
        ("convert", portable_convert, native_convert),
        ("analyse", portable_analyse, native_analyse),
        ("decode from i16", portable_decode, native_decode),
        ("deinterleave (LD2)", portable_split, native_split),
        ("interleave (ST2)", portable_weave, native_weave),
    ] {
        println!("    {label:<28} {:>8.2}×", portable_time / native_time);
    }

    // The deadline is what actually matters: at 48kHz a quarter-second block
    // has 250ms to be ready in, and mixing must be a rounding error inside it.
    let budget = FRAMES as f64 / 48_000.0;
    println!(
        "\n  mixing uses {:.4}% of its deadline\n",
        native_mix / budget * 100.0
    );
    assert!(
        native_mix < budget * 0.05,
        "mixing takes {:.2}% of the audio deadline",
        native_mix / budget * 100.0
    );
}
