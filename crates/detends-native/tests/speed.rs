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

    println!("\n  speedup");
    for (label, portable_time, native_time) in [
        ("mix", portable_mix, native_mix),
        ("convert", portable_convert, native_convert),
        ("analyse", portable_analyse, native_analyse),
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
