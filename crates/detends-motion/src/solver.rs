//! The closed-form damped-spring solution.
//!
//! Every spring in détends is solved analytically rather than integrated. That
//! choice buys three things which together are most of what makes the system
//! feel physical:
//!
//! 1. **Frame-rate independence.** The result at `t` is identical whether we
//!    sampled once or a hundred times getting there, so 60Hz and 120Hz agree
//!    exactly and a dropped frame costs nothing.
//! 2. **No accumulated error.** Position is a function of absolute time, not a
//!    running sum, so jitter in frame delivery causes sub-pixel error that
//!    never compounds.
//! 3. **Exact retargeting.** Velocity at any instant is available in closed
//!    form, so redirecting a spring mid-flight preserves momentum precisely
//!    instead of approximating it.

use crate::Params;

/// Below this distance from ζ = 1 we use the critical-damping solution.
///
/// The underdamped and overdamped forms both divide by a term that vanishes as
/// ζ → 1 (`ω√(1−ζ²)` and `ω√(ζ²−1)` respectively), so near the boundary they
/// lose precision catastrophically. The critical form is the limit of both and
/// is well-conditioned there.
const CRITICAL_BAND: f32 = 1.0e-3;

/// The solution of the spring ODE at one instant, as four scalars.
///
/// The equation is linear, so for a displacement `u₀ = x₀ − target` and an
/// initial velocity `v₀`:
///
/// ```text
/// x(t) = target + a·u₀ + b·v₀
/// v(t) =          c·u₀ + d·v₀
/// ```
///
/// Because these coefficients don't depend on the *type* being animated, one
/// evaluation drives a float, a point, a rectangle or a colour identically.
#[derive(Clone, Copy, Debug)]
pub struct Coeffs {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

impl Coeffs {
    /// The identity: no time has passed.
    pub const START: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
    };

    /// Fully settled: displacement and velocity are both gone.
    pub const REST: Self = Self {
        a: 0.0,
        b: 0.0,
        c: 0.0,
        d: 0.0,
    };
}

/// Evaluate the spring `dt` seconds after its initial conditions.
///
/// `dt` is clamped at zero; time never runs backwards in a shell.
pub fn coeffs(p: Params, dt: f32) -> Coeffs {
    if dt <= 0.0 {
        return Coeffs::START;
    }

    let Params { omega, zeta } = p;

    if (zeta - 1.0).abs() < CRITICAL_BAND {
        critical(omega, dt)
    } else if zeta < 1.0 {
        underdamped(omega, zeta, dt)
    } else {
        overdamped(omega, zeta, dt)
    }
}

/// ζ < 1 — oscillates inside a decaying envelope.
fn underdamped(omega: f32, zeta: f32, dt: f32) -> Coeffs {
    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
    let decay = (-zeta * omega * dt).exp();
    let (sin, cos) = (omega_d * dt).sin_cos();

    // x(t) = e^(−ζωt)·[ u₀·cos(ω_d t) + ((v₀ + ζω·u₀)/ω_d)·sin(ω_d t) ]
    // v(t) = e^(−ζωt)·[ v₀·cos(ω_d t) − ((ω²·u₀ + ζω·v₀)/ω_d)·sin(ω_d t) ]
    Coeffs {
        a: decay * (cos + (zeta * omega / omega_d) * sin),
        b: decay * (sin / omega_d),
        c: -decay * (omega * omega / omega_d) * sin,
        d: decay * (cos - (zeta * omega / omega_d) * sin),
    }
}

/// ζ = 1 — the fastest approach that never crosses the target.
fn critical(omega: f32, dt: f32) -> Coeffs {
    let decay = (-omega * dt).exp();

    // x(t) = e^(−ωt)·[ u₀ + (v₀ + ω·u₀)·t ]
    // v(t) = e^(−ωt)·[ v₀ − ω·(v₀ + ω·u₀)·t ]
    Coeffs {
        a: decay * (1.0 + omega * dt),
        b: decay * dt,
        c: -decay * omega * omega * dt,
        d: decay * (1.0 - omega * dt),
    }
}

/// ζ > 1 — two real roots, no oscillation, a slow crawl in.
fn overdamped(omega: f32, zeta: f32, dt: f32) -> Coeffs {
    let root = omega * (zeta * zeta - 1.0).sqrt();
    let r1 = -omega * zeta + root; // the slower-decaying root
    let r2 = -omega * zeta - root;
    let inv = 1.0 / (r1 - r2);

    let e1 = (r1 * dt).exp();
    let e2 = (r2 * dt).exp();

    // x(t) = [ (r₁·e^(r₂t) − r₂·e^(r₁t))·u₀ + (e^(r₁t) − e^(r₂t))·v₀ ] / (r₁ − r₂)
    Coeffs {
        a: (r1 * e2 - r2 * e1) * inv,
        b: (e1 - e2) * inv,
        c: r1 * r2 * (e2 - e1) * inv,
        d: (r1 * e1 - r2 * e2) * inv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::springs;

    /// Reference implementation: tiny fixed-step Euler integration.
    ///
    /// Far too slow and imprecise to ship, but with a small enough step it is
    /// an independent check that the closed forms solve the right equation.
    fn integrate(p: Params, u0: f32, v0: f32, dt: f32) -> (f32, f32) {
        let steps = 200_000;
        let h = dt / steps as f32;
        let (mut u, mut v) = (u0, v0);
        for _ in 0..steps {
            let accel = -2.0 * p.zeta * p.omega * v - p.omega * p.omega * u;
            v += accel * h;
            u += v * h;
        }
        (u, v)
    }

    fn assert_matches_integration(p: Params, u0: f32, v0: f32, dt: f32) {
        let c = coeffs(p, dt);
        let (x, v) = (c.a * u0 + c.b * v0, c.c * u0 + c.d * v0);
        let (xi, vi) = integrate(p, u0, v0, dt);
        assert!(
            (x - xi).abs() < 2e-3,
            "position {x} vs integrated {xi} for {p:?} at t={dt}"
        );
        assert!(
            (v - vi).abs() < 2e-2,
            "velocity {v} vs integrated {vi} for {p:?} at t={dt}"
        );
    }

    #[test]
    fn underdamped_matches_numerical_integration() {
        assert_matches_integration(springs::GLIDE, 1.0, 0.0, 0.1);
        assert_matches_integration(springs::GLIDE, 1.0, 0.0, 0.25);
        assert_matches_integration(springs::GLIDE, -0.4, 3.0, 0.15);
    }

    #[test]
    fn critical_matches_numerical_integration() {
        assert_matches_integration(springs::SNAP, 1.0, 0.0, 0.05);
        assert_matches_integration(springs::SETTLE, 1.0, 0.0, 0.2);
        assert_matches_integration(springs::HUSH, 0.7, -2.0, 0.3);
    }

    #[test]
    fn overdamped_matches_numerical_integration() {
        let p = Params {
            omega: 12.0,
            zeta: 2.2,
        };
        assert_matches_integration(p, 1.0, 0.0, 0.1);
        assert_matches_integration(p, 1.0, 0.0, 0.4);
        assert_matches_integration(p, -0.5, 4.0, 0.2);
    }

    #[test]
    fn critical_damping_never_overshoots() {
        // The defining property: released from rest, it approaches the target
        // monotonically and never crosses it.
        let p = springs::SNAP;
        let mut previous = 1.0_f32;
        for step in 1..=400 {
            let t = step as f32 * 0.002;
            let x = coeffs(p, t).a; // u₀ = 1, v₀ = 0
            assert!(x >= -1e-6, "crossed the target at t={t}: {x}");
            assert!(x <= previous + 1e-6, "moved away from the target at t={t}");
            previous = x;
        }
    }

    #[test]
    fn overdamped_never_overshoots_either() {
        let p = Params {
            omega: 10.0,
            zeta: 3.0,
        };
        for step in 1..=400 {
            let x = coeffs(p, step as f32 * 0.005).a;
            assert!(x >= -1e-6, "overdamped spring crossed the target: {x}");
        }
    }

    #[test]
    fn glide_overshoots_a_little_but_not_much() {
        // GLIDE is the one spring allowed overshoot. It should be present
        // enough to feel like mass, small enough never to read as a bounce.
        let mut most_negative = 0.0_f32;
        for step in 1..=600 {
            most_negative = most_negative.min(coeffs(springs::GLIDE, step as f32 * 0.002).a);
        }
        assert!(most_negative < -1e-4, "GLIDE should overshoot at all");
        assert!(
            most_negative > -0.05,
            "GLIDE overshot {most_negative}, too bouncy"
        );
    }

    #[test]
    fn the_critical_band_is_continuous() {
        // Approaching ζ = 1 from either side must not produce a visible
        // discontinuity, otherwise a spring tuned near critical would jump
        // when retuned.
        //
        // Snapping to the critical solution inside the band does introduce a
        // tiny error. The tolerance here is what that error may reach at the
        // band's edge — about 3e-4, or a third of a pixel across a 1000px
        // travel. Invisible, and the price of numerical stability.
        let dt = 0.08;
        let below = coeffs(
            Params {
                omega: 15.0,
                zeta: 1.0 - 2.0 * CRITICAL_BAND,
            },
            dt,
        );
        let at = coeffs(
            Params {
                omega: 15.0,
                zeta: 1.0,
            },
            dt,
        );
        let above = coeffs(
            Params {
                omega: 15.0,
                zeta: 1.0 + 2.0 * CRITICAL_BAND,
            },
            dt,
        );

        assert!((below.a - at.a).abs() < 1e-3, "{} vs {}", below.a, at.a);
        assert!((above.a - at.a).abs() < 1e-3, "{} vs {}", above.a, at.a);
        assert!((below.d - at.d).abs() < 1e-3);
        assert!((above.d - at.d).abs() < 1e-3);
    }

    #[test]
    fn zero_and_negative_time_are_the_identity() {
        for dt in [0.0, -1.0, -0.001] {
            let c = coeffs(springs::GLIDE, dt);
            assert_eq!((c.a, c.b, c.c, c.d), (1.0, 0.0, 0.0, 1.0));
        }
    }

    #[test]
    fn every_named_spring_converges() {
        for p in [
            springs::SNAP,
            springs::GLIDE,
            springs::SETTLE,
            springs::HUSH,
        ] {
            let c = coeffs(p, p.settle_estimate());
            assert!(c.a.abs() < 0.05, "{p:?} had not arrived: a={}", c.a);
            assert!(c.b.abs() < 0.05, "{p:?} had not arrived: b={}", c.b);
        }
    }
}
