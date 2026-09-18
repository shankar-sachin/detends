//! What a spring is allowed to carry.

/// A value a spring can move through.
///
/// The spring equation is linear and acts on each component independently, so
/// anything that behaves like a small vector space can be animated: a scalar,
/// a point, a rectangle, a colour. One set of solver coefficients drives them
/// all, which is why a mode transition moving a rect and a colour stays
/// perfectly in step — they are literally the same evaluation.
///
/// Implement this for geometry in whichever crate owns it; this crate
/// deliberately defines no geometry of its own.
pub trait Animatable: Copy {
    /// The additive identity — zero velocity, zero displacement.
    fn zero() -> Self;

    fn add(self, rhs: Self) -> Self;
    fn sub(self, rhs: Self) -> Self;
    fn scale(self, k: f32) -> Self;

    /// The largest absolute component, used to decide whether the spring has
    /// visually arrived. A max-norm rather than a euclidean one: it is cheaper,
    /// and "no component is still moving" is exactly the question being asked.
    fn magnitude(self) -> f32;
}

impl Animatable for f32 {
    #[inline]
    fn zero() -> Self {
        0.0
    }
    #[inline]
    fn add(self, rhs: Self) -> Self {
        self + rhs
    }
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        self - rhs
    }
    #[inline]
    fn scale(self, k: f32) -> Self {
        self * k
    }
    #[inline]
    fn magnitude(self) -> f32 {
        self.abs()
    }
}

impl<const N: usize> Animatable for [f32; N] {
    #[inline]
    fn zero() -> Self {
        [0.0; N]
    }
    #[inline]
    fn add(mut self, rhs: Self) -> Self {
        for (a, b) in self.iter_mut().zip(rhs) {
            *a += b;
        }
        self
    }
    #[inline]
    fn sub(mut self, rhs: Self) -> Self {
        for (a, b) in self.iter_mut().zip(rhs) {
            *a -= b;
        }
        self
    }
    #[inline]
    fn scale(mut self, k: f32) -> Self {
        for a in self.iter_mut() {
            *a *= k;
        }
        self
    }
    #[inline]
    fn magnitude(self) -> f32 {
        self.iter().fold(0.0_f32, |acc, c| acc.max(c.abs()))
    }
}
