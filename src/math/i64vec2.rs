#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C, align(16))]
pub struct I64Vec2 {
    pub x: i64,
    pub y: i64,
}

impl I64Vec2 {
    #[inline(always)]
    pub fn from_vec2(v: egui::Vec2) -> Self {
        i64vec2(v.x as i64, v.y as i64)
    }

    #[inline(always)]
    pub fn min(self, v: Self) -> Self {
        i64vec2(self.x.min(v.x), self.y.min(v.y))
    }

    #[inline(always)]
    pub fn max(self, v: Self) -> Self {
        i64vec2(self.x.max(v.x), self.y.max(v.y))
    }
}

impl std::ops::Mul<Self> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        i64vec2(self.x * rhs.x, self.y * rhs.y)
    }
}

impl std::ops::Add<Self> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        i64vec2(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub<Self> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        i64vec2(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<i64> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: i64) -> Self {
        i64vec2(self.x * rhs, self.y * rhs)
    }
}

impl std::ops::Add<i64> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: i64) -> Self {
        i64vec2(self.x + rhs, self.y + rhs)
    }
}

impl std::ops::Sub<i64> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: i64) -> Self {
        i64vec2(self.x - rhs, self.y - rhs)
    }
}

impl std::ops::Shr<u32> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn shr(self, rhs: u32) -> Self {
        i64vec2(self.x >> rhs, self.y >> rhs)
    }
}

impl std::ops::Shl<u32> for I64Vec2 {
    type Output = Self;
    #[inline(always)]
    fn shl(self, rhs: u32) -> Self {
        i64vec2(self.x << rhs, self.y << rhs)
    }
}

#[inline(always)]
pub fn i64vec2(x: i64, y: i64) -> I64Vec2 {
    I64Vec2 { x, y }
}
