//! Points and rectangles, in logical units.
//!
//! Everything in a display list is expressed in *logical* units — the units a
//! designer thinks in. The renderer multiplies by the frame's scale factor.
//! The shell therefore never needs to know about Retina backing scales or
//! Wayland fractional scaling, which is the point.

use detends_motion::Animatable;

/// A point or a size, depending on what is being described.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn min(self, o: Self) -> Self {
        vec2(self.x.min(o.x), self.y.min(o.y))
    }

    pub fn max(self, o: Self) -> Self {
        vec2(self.x.max(o.x), self.y.max(o.y))
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        vec2(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, k: f32) -> Self {
        vec2(self.x * k, self.y * k)
    }
}

impl Animatable for Vec2 {
    fn zero() -> Self {
        Self::ZERO
    }
    fn add(self, rhs: Self) -> Self {
        self + rhs
    }
    fn sub(self, rhs: Self) -> Self {
        self - rhs
    }
    fn scale(self, k: f32) -> Self {
        self * k
    }
    fn magnitude(self) -> f32 {
        self.x.abs().max(self.y.abs())
    }
}

/// An axis-aligned rectangle, stored by centre and half-extent.
///
/// Centre-based rather than corner-based because that is what the signed
/// distance functions in the glass shader want, and because scaling a surface
/// about its own middle — which is what almost every détends transition does —
/// becomes a single multiply instead of a correction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub center: Vec2,
    pub half: Vec2,
}

impl Rect {
    pub const ZERO: Self = Self {
        center: Vec2::ZERO,
        half: Vec2::ZERO,
    };

    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self {
            center,
            half: size * 0.5,
        }
    }

    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self {
            center: min + size * 0.5,
            half: size * 0.5,
        }
    }

    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self {
            center: (min + max) * 0.5,
            half: (max - min) * 0.5,
        }
    }

    pub fn size(&self) -> Vec2 {
        self.half * 2.0
    }
    pub fn min(&self) -> Vec2 {
        self.center - self.half
    }
    pub fn max(&self) -> Vec2 {
        self.center + self.half
    }
    pub fn width(&self) -> f32 {
        self.half.x * 2.0
    }
    pub fn height(&self) -> f32 {
        self.half.y * 2.0
    }

    pub fn contains(&self, p: Vec2) -> bool {
        (p.x - self.center.x).abs() <= self.half.x && (p.y - self.center.y).abs() <= self.half.y
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        (self.center.x - o.center.x).abs() < self.half.x + o.half.x
            && (self.center.y - o.center.y).abs() < self.half.y + o.half.y
    }

    /// Shrink on every side. Negative values grow it.
    pub fn inset(&self, by: f32) -> Self {
        Self {
            center: self.center,
            half: vec2((self.half.x - by).max(0.0), (self.half.y - by).max(0.0)),
        }
    }

    /// Scale about the centre — the basis of every arrive/depart transition.
    pub fn scaled(&self, k: f32) -> Self {
        Self {
            center: self.center,
            half: self.half * k,
        }
    }

    pub fn translated(&self, by: Vec2) -> Self {
        Self {
            center: self.center + by,
            half: self.half,
        }
    }

    /// The smallest rectangle containing both. Used to merge damage regions.
    pub fn union(&self, o: &Rect) -> Self {
        Self::from_min_max(self.min().min(o.min()), self.max().max(o.max()))
    }
}

impl Animatable for Rect {
    fn zero() -> Self {
        Self::ZERO
    }
    fn add(self, rhs: Self) -> Self {
        Self {
            center: self.center + rhs.center,
            half: self.half + rhs.half,
        }
    }
    fn sub(self, rhs: Self) -> Self {
        Self {
            center: self.center - rhs.center,
            half: self.half - rhs.half,
        }
    }
    fn scale(self, k: f32) -> Self {
        Self {
            center: self.center * k,
            half: self.half * k,
        }
    }
    fn magnitude(self) -> f32 {
        self.center.magnitude().max(self.half.magnitude())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_round_trips_through_its_constructors() {
        let r = Rect::from_min_size(vec2(10.0, 20.0), vec2(100.0, 50.0));
        assert_eq!(r.min(), vec2(10.0, 20.0));
        assert_eq!(r.max(), vec2(110.0, 70.0));
        assert_eq!(r.size(), vec2(100.0, 50.0));
        assert_eq!(r.center, vec2(60.0, 45.0));
    }

    #[test]
    fn scaling_happens_about_the_centre() {
        let r = Rect::from_center_size(vec2(100.0, 100.0), vec2(40.0, 40.0));
        let small = r.scaled(0.5);
        assert_eq!(small.center, r.center, "must not drift while scaling");
        assert_eq!(small.size(), vec2(20.0, 20.0));
    }

    #[test]
    fn inset_clamps_rather_than_inverting() {
        let r = Rect::from_center_size(Vec2::ZERO, vec2(10.0, 10.0));
        assert_eq!(r.inset(50.0).size(), Vec2::ZERO);
    }

    #[test]
    fn overlap_and_containment_agree_with_intuition() {
        let a = Rect::from_min_size(Vec2::ZERO, vec2(10.0, 10.0));
        let b = Rect::from_min_size(vec2(5.0, 5.0), vec2(10.0, 10.0));
        let c = Rect::from_min_size(vec2(20.0, 20.0), vec2(5.0, 5.0));
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
        assert!(a.contains(vec2(5.0, 5.0)));
        assert!(!a.contains(vec2(15.0, 5.0)));
    }

    #[test]
    fn rects_animate_as_one_shape() {
        use detends_motion::{springs, Spring};
        let from = Rect::from_min_size(Vec2::ZERO, vec2(10.0, 10.0));
        let to = Rect::from_min_size(vec2(100.0, 100.0), vec2(200.0, 80.0));

        let mut s = Spring::new(springs::SETTLE, from);
        s.target(0.0, to);
        let settled = s.params().settle_estimate_for(200.0) as f64;
        assert_eq!(s.value(settled), to);
        assert!(s.at_rest(settled));
    }
}
