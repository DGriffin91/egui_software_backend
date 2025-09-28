// This exists just to avoid bringing in glam or similar since egui doesn't.
// Based on emath Vec2

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Vec4 {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
    pub(crate) w: f32,
}

#[inline(always)]
pub(crate) const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4 { x, y, z, w }
}

// ----------------------------------------------------------------------------

impl Vec4 {
    pub(crate) const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub(crate) const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
        w: 1.0,
    };

    #[must_use]
    #[inline]
    pub(crate) fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
            z: self.z.clamp(min.z, max.z),
            w: self.w.clamp(min.w, max.w),
        }
    }
}

impl Eq for Vec4 {}

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
