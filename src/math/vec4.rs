// This exists just to avoid bringing in glam or similar since egui doesn't.
// Might be able to use ecolor::rgba::Rgba with some additions.
// Based on emath Vec2

use core::fmt;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// `vec4(x, y, z, w) == Vec4::new(x, y, z, w)`
#[inline(always)]
pub const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4 { x, y, z, w }
}

// ----------------------------------------------------------------------------
// Compatibility and convenience conversions to and from [f32; 4]:

impl From<[f32; 4]> for Vec4 {
    #[inline(always)]
    fn from(v: [f32; 4]) -> Self {
        Self {
            x: v[0],
            y: v[1],
            z: v[2],
            w: v[3],
        }
    }
}

impl From<&[f32; 4]> for Vec4 {
    #[inline(always)]
    fn from(v: &[f32; 4]) -> Self {
        Self {
            x: v[0],
            y: v[1],
            z: v[2],
            w: v[3],
        }
    }
}

impl From<Vec4> for [f32; 4] {
    #[inline(always)]
    fn from(v: Vec4) -> Self {
        [v.x, v.y, v.z, v.w]
    }
}

impl From<&Vec4> for [f32; 4] {
    #[inline(always)]
    fn from(v: &Vec4) -> Self {
        [v.x, v.y, v.z, v.w]
    }
}

// ----------------------------------------------------------------------------
// Compatibility and convenience conversions to and from (f32, f32, f32, f32):

impl From<(f32, f32, f32, f32)> for Vec4 {
    #[inline(always)]
    fn from(v: (f32, f32, f32, f32)) -> Self {
        Self {
            x: v.0,
            y: v.1,
            z: v.2,
            w: v.3,
        }
    }
}

impl From<&(f32, f32, f32, f32)> for Vec4 {
    #[inline(always)]
    fn from(v: &(f32, f32, f32, f32)) -> Self {
        Self {
            x: v.0,
            y: v.1,
            z: v.2,
            w: v.3,
        }
    }
}

impl From<Vec4> for (f32, f32, f32, f32) {
    #[inline(always)]
    fn from(v: Vec4) -> Self {
        (v.x, v.y, v.z, v.w)
    }
}

impl From<&Vec4> for (f32, f32, f32, f32) {
    #[inline(always)]
    fn from(v: &Vec4) -> Self {
        (v.x, v.y, v.z, v.w)
    }
}

// ----------------------------------------------------------------------------

impl Vec4 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
        w: 1.0,
    };
    pub const INFINITY: Self = Self::splat(f32::INFINITY);
    pub const NAN: Self = Self::splat(f32::NAN);

    #[inline(always)]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Set `x`, `y`, `z`, `w` to the same value.
    #[inline(always)]
    pub const fn splat(v: f32) -> Self {
        Self {
            x: v,
            y: v,
            z: v,
            w: v,
        }
    }

    /// Safe normalize: returns zero if input is zero.
    #[must_use]
    #[inline(always)]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len <= 0.0 { self } else { self / len }
    }

    /// Checks if `self` has length `1.0` up to a precision of `1e-6`.
    #[inline(always)]
    pub fn is_normalized(self) -> bool {
        (self.length_sq() - 1.0).abs() < 2e-6
    }

    #[inline(always)]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    #[inline(always)]
    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    #[must_use]
    #[inline(always)]
    pub fn floor(self) -> Self {
        vec4(
            self.x.floor(),
            self.y.floor(),
            self.z.floor(),
            self.w.floor(),
        )
    }

    #[must_use]
    #[inline(always)]
    pub fn round(self) -> Self {
        vec4(
            self.x.round(),
            self.y.round(),
            self.z.round(),
            self.w.round(),
        )
    }

    #[must_use]
    #[inline(always)]
    pub fn ceil(self) -> Self {
        vec4(self.x.ceil(), self.y.ceil(), self.z.ceil(), self.w.ceil())
    }

    #[must_use]
    #[inline]
    pub fn abs(self) -> Self {
        vec4(self.x.abs(), self.y.abs(), self.z.abs(), self.w.abs())
    }

    /// True if all members are also finite.
    #[inline(always)]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    /// True if any member is NaN.
    #[inline(always)]
    pub fn any_nan(self) -> bool {
        self.x.is_nan() || self.y.is_nan() || self.z.is_nan() || self.w.is_nan()
    }

    #[must_use]
    #[inline]
    pub fn min(self, other: Self) -> Self {
        vec4(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
            self.w.min(other.w),
        )
    }

    #[must_use]
    #[inline]
    pub fn max(self, other: Self) -> Self {
        vec4(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
            self.w.max(other.w),
        )
    }

    /// The dot-product of two vectors.
    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }

    /// Returns the minimum of all elements.
    #[must_use]
    #[inline(always)]
    pub fn min_elem(self) -> f32 {
        self.x.min(self.y).min(self.z).min(self.w)
    }

    /// Returns the maximum of all elements.
    #[inline(always)]
    #[must_use]
    pub fn max_elem(self) -> f32 {
        self.x.max(self.y).max(self.z).max(self.w)
    }

    #[must_use]
    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
            z: self.z.clamp(min.z, max.z),
            w: self.w.clamp(min.w, max.w),
        }
    }
}

impl core::ops::Index<usize> for Vec4 {
    type Output = f32;

    #[inline(always)]
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("Vec4 index out of bounds: {index}"),
        }
    }
}

impl core::ops::IndexMut<usize> for Vec4 {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("Vec4 index out of bounds: {index}"),
        }
    }
}

impl Eq for Vec4 {}

impl Neg for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        vec4(-self.x, -self.y, -self.z, -self.w)
    }
}

impl AddAssign for Vec4 {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        *self = Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        };
    }
}

impl SubAssign for Vec4 {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        *self = Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
            w: self.w - rhs.w,
        };
    }
}

impl Add for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl Add<Vec4> for &Vec4 {
    type Output = Vec4;

    #[inline(always)]
    fn add(self, rhs: Vec4) -> Vec4 {
        Vec4 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl Add<&Vec4> for Vec4 {
    type Output = Vec4;

    #[inline(always)]
    fn add(self, rhs: &Vec4) -> Vec4 {
        Vec4 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl Sub for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
            w: self.w - rhs.w,
        }
    }
}

/// Element-wise multiplication
impl Mul<Self> for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, vec: Self) -> Self {
        Self {
            x: self.x * vec.x,
            y: self.y * vec.y,
            z: self.z * vec.z,
            w: self.w * vec.w,
        }
    }
}

/// Element-wise division
impl Div<Self> for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
            z: self.z / rhs.z,
            w: self.w / rhs.w,
        }
    }
}

impl MulAssign<f32> for Vec4 {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
        self.w *= rhs;
    }
}

impl DivAssign<f32> for Vec4 {
    #[inline(always)]
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
        self.z /= rhs;
        self.w /= rhs;
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, factor: f32) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
            w: self.w * factor,
        }
    }
}

impl Mul<f32> for &Vec4 {
    type Output = Vec4;

    #[inline(always)]
    fn mul(self, factor: f32) -> Vec4 {
        Vec4 {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
            w: self.w * factor,
        }
    }
}

impl Add<f32> for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn add(self, factor: f32) -> Self {
        Self {
            x: self.x + factor,
            y: self.y + factor,
            z: self.z + factor,
            w: self.w + factor,
        }
    }
}

impl Add<f32> for &Vec4 {
    type Output = Vec4;

    #[inline(always)]
    fn add(self, factor: f32) -> Vec4 {
        Vec4 {
            x: self.x + factor,
            y: self.y + factor,
            z: self.z + factor,
            w: self.w + factor,
        }
    }
}

impl Mul<Vec4> for f32 {
    type Output = Vec4;

    #[inline(always)]
    fn mul(self, vec: Vec4) -> Vec4 {
        Vec4 {
            x: self * vec.x,
            y: self * vec.y,
            z: self * vec.z,
            w: self * vec.w,
        }
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;

    #[inline(always)]
    fn div(self, factor: f32) -> Self {
        Self {
            x: self.x / factor,
            y: self.y / factor,
            z: self.z / factor,
            w: self.w / factor,
        }
    }
}

impl fmt::Debug for Vec4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(precision) = f.precision() {
            write!(
                f,
                "[{1:.0$} {2:.0$} {3:.0$} {4:.0$}]",
                precision, self.x, self.y, self.z, self.w
            )
        } else {
            write!(
                f,
                "[{:.1} {:.1} {:.1} {:.1}]",
                self.x, self.y, self.z, self.w
            )
        }
    }
}

impl fmt::Display for Vec4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        self.x.fmt(f)?;
        f.write_str(" ")?;
        self.y.fmt(f)?;
        f.write_str(" ")?;
        self.z.fmt(f)?;
        f.write_str(" ")?;
        self.w.fmt(f)?;
        f.write_str("]")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! almost_eq {
        ($left: expr, $right: expr) => {
            let left = $left;
            let right = $right;
            assert!((left - right).abs() < 1e-6, "{} != {}", left, right);
        };
    }

    #[test]
    fn test_vec4() {
        let mut assignment = vec4(1.0, 2.0, 3.0, 4.0);
        assignment += vec4(3.0, 4.0, 5.0, 6.0);
        assert_eq!(assignment, vec4(4.0, 6.0, 8.0, 10.0));

        let mut assignment = vec4(4.0, 6.0, 8.0, 10.0);
        assignment -= vec4(1.0, 2.0, 3.0, 4.0);
        assert_eq!(assignment, vec4(3.0, 4.0, 5.0, 6.0));

        let mut assignment = vec4(1.0, 2.0, 3.0, 4.0);
        assignment *= 2.0;
        assert_eq!(assignment, vec4(2.0, 4.0, 6.0, 8.0));

        let mut assignment = vec4(2.0, 4.0, 6.0, 8.0);
        assignment /= 2.0;
        assert_eq!(assignment, vec4(1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn test_vec4_normalized() {
        fn generate_spiral(n: usize, start: Vec4, end: Vec4) -> impl Iterator<Item = Vec4> {
            let angle_step = 2.0 * core::f32::consts::PI / n as f32;
            let radius_step = (end.length() - start.length()) / n as f32;

            (0..n).map(move |i| {
                let angle = i as f32 * angle_step;
                let radius = start.length() + i as f32 * radius_step;
                let x = radius * angle.cos();
                let y = radius * angle.sin();
                vec4(x, y, x, y)
            })
        }

        for v in generate_spiral(40, Vec4::splat(0.1), Vec4::splat(2.0)) {
            let vn = v.normalized();
            almost_eq!(vn.length(), 1.0);
            assert!(vn.is_normalized());
        }
    }
}
