//! What a frame costs on the CPU.
//!
//! The GPU side needs a real present loop to measure, but the shell's own work
//! is measurable here — and it is the part that would silently grow as modes
//! gain content.

use detends_paint::vec2;
use detends_shell::{App, Shell};
use std::time::Instant;

#[test]
fn building_a_frame_is_far_cheaper_than_a_frame_budget() {
    let mut shell = Shell::new(0.0, vec2(1512.0, 982.0), 2.0);

    // Get past boot.
    let mut t = 0.0;
    for _ in 0..1000 {
        t += 1.0 / 120.0;
        shell.tick(t);
    }

    // Steady state, cycling modes so transitions are included.
    let frames = 2000;
    let start = Instant::now();
    for n in 0..frames {
        if n % 200 == 0 {
            shell.open(t, App::ALL[(n / 200) % App::ALL.len()]);
        }
        t += 1.0 / 120.0;
        let _ = shell.tick(t);
    }
    let per_frame = start.elapsed().as_secs_f64() / frames as f64;

    println!("  shell frame build: {:.1} µs", per_frame * 1_000_000.0);

    // A 120Hz budget is 8.3ms for everything. The shell should be a rounding
    // error inside it — this is a smoke alarm, not a benchmark.
    assert!(
        per_frame < 0.001,
        "building a frame takes {:.3} ms, which is a large slice of 8.3",
        per_frame * 1000.0
    );
}
