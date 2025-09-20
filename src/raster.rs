use std::ops::{Add, AddAssign, Mul, Sub};

use egui::Vec2;

use crate::{
    i64vec2::{I64Vec2, i64vec2},
    vec4::Vec4,
};

// https://fgiesen.wordpress.com/2013/02/17/optimizing-sw-occlusion-culling-index/
// https://jtsorlinis.github.io/rendering-tutorial/
// https://www.cs.cornell.edu/courses/cs4620/2011fa/lectures/16rasterizationWeb.pdf

/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri_with_uv<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
    uv: &[Vec2; 3],
    // ss_x, ss_y, uv
    mut raster: impl FnMut(i64, i64, Vec2),
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    let mut uv_stepper = stepper.attr(uv, sp_inv_area);

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        uv_stepper.row_start();
        for ss_x in ss_min.x..=ss_max.x {
            if stepper.inside_tri_pos_area() {
                raster(ss_x, ss_y, uv_stepper.attr);
            }
            stepper.col_step();
            uv_stepper.col_step();
        }
        stepper.row_step();
        uv_stepper.row_step();
    }
}

/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri_with_colors<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
    colors: &[Vec4; 3],
    // ss_x, ss_y, color
    mut raster: impl FnMut(i64, i64, Vec4),
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    let mut uv_stepper = stepper.attr(colors, sp_inv_area);

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        uv_stepper.row_start();
        for ss_x in ss_min.x..=ss_max.x {
            if stepper.inside_tri_pos_area() {
                raster(ss_x, ss_y, uv_stepper.attr);
            }
            stepper.col_step();
            uv_stepper.col_step();
        }
        stepper.row_step();
        uv_stepper.row_step();
    }
}

/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri_with_bary<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
    // ss_x, ss_y, w0, w1, sp_inv_area
    mut raster: impl FnMut(i64, i64, i64, i64, f32),
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        for ss_x in ss_min.x..=ss_max.x {
            if stepper.inside_tri_pos_area() {
                raster(ss_x, ss_y, stepper.w0, stepper.w1, sp_inv_area);
            }
            stepper.col_step();
        }
        stepper.row_step();
    }
}

/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
    // ss_x, ss_y
    mut raster: impl FnMut(i64, i64),
) {
    // TODO is scanline faster when barycentrics are not needed?
    let Some((ss_min, ss_max, _sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        for ss_x in ss_min.x..=ss_max.x {
            if stepper.inside_tri_pos_area() {
                raster(ss_x, ss_y);
            }
            stepper.col_step();
        }
        stepper.row_step();
    }
}

/// returns: ss_min, ss_max, sp_inv_area, stepper
fn stepper_from_ss_tri_backface_cull<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
) -> Option<(I64Vec2, I64Vec2, f32, SingleStepper)> {
    let subpix_bits = SUBPIX_BITS as u32;
    let subpix: i64 = 1 << subpix_bits;
    let subpix_half: i64 = subpix >> 1;
    let fsubpix = subpix as f32;

    let ss_min_bound = i64vec2(ss_bounds[0] as i64, ss_bounds[1] as i64);
    let ss_max_bound = i64vec2(ss_bounds[2] as i64, ss_bounds[3] as i64);

    let sp0 = I64Vec2::from_vec2(ss_tri[0] * fsubpix);
    let sp1 = I64Vec2::from_vec2(ss_tri[1] * fsubpix);
    let sp2 = I64Vec2::from_vec2(ss_tri[2] * fsubpix);

    let sp_area = orient2d(&sp0, &sp1, &sp2);

    if sp_area <= 0 {
        return None;
    }

    let sp_min = sp0.min(sp1).min(sp2);
    let sp_max = sp0.max(sp1).max(sp2);

    let ss_min = ((sp_min - subpix_half) >> subpix_bits)
        .max(ss_min_bound)
        .min(ss_max_bound - 1);
    let ss_max = ((sp_max + subpix_half) >> subpix_bits)
        .max(ss_min_bound)
        .min(ss_max_bound - 1);

    let sp_min_p = ss_min * subpix + subpix_half;
    let ss_size = ss_max - ss_min;

    if ss_size.x <= 0 || ss_size.y <= 0 {
        return None;
    }

    let sp_inv_area = 1.0 / (sp_area as f32);

    let stepper = SingleStepper::new(&sp0, &sp1, &sp2, &sp_min_p, subpix);

    Some((ss_min, ss_max, sp_inv_area, stepper))
}

#[inline(always)]
pub fn is_top_left(a: &I64Vec2, b: &I64Vec2) -> bool {
    let dy = b.y - a.y;
    (dy > 0) || (dy == 0 && (b.x - a.x) < 0)
}

pub struct SingleStepper {
    pub e12: SingleStep,
    pub e20: SingleStep,
    pub e01: SingleStep,
    pub w0: i64,
    pub w1: i64,
    pub w2: i64,
    pub bias0: i64,
    pub bias1: i64,
    pub bias2: i64,
}

impl SingleStepper {
    pub fn new(
        sp0: &I64Vec2,
        sp1: &I64Vec2,
        sp2: &I64Vec2,
        sp_min_p: &I64Vec2,
        subpix: i64,
    ) -> Self {
        SingleStepper {
            e12: SingleStep::new(sp1, sp2, sp_min_p, subpix),
            e20: SingleStep::new(sp2, sp0, sp_min_p, subpix),
            e01: SingleStep::new(sp0, sp1, sp_min_p, subpix),
            w0: 0,
            w1: 0,
            w2: 0,
            bias0: if is_top_left(sp1, sp2) { 0 } else { -1 },
            bias1: if is_top_left(sp2, sp0) { 0 } else { -1 },
            bias2: if is_top_left(sp0, sp1) { 0 } else { -1 },
        }
    }

    #[inline(always)]
    pub fn inside_tri_pos_area(&self) -> bool {
        // None w are negative
        let m = ((self.w0 + self.bias0) as u64)
            | ((self.w1 + self.bias1) as u64)
            | ((self.w2 + self.bias2) as u64);
        (m & 0x8000_0000_0000_0000) == 0
    }

    #[inline(always)]
    pub fn row_step(&mut self) {
        self.e12.row += self.e12.step.y;
        self.e20.row += self.e20.step.y;
        self.e01.row += self.e01.step.y;
    }

    #[inline(always)]
    pub fn col_step(&mut self) {
        self.w0 += self.e12.step.x;
        self.w1 += self.e20.step.x;
        self.w2 += self.e01.step.x;
    }

    #[inline(always)]
    pub fn row_start(&mut self) {
        self.w0 = self.e12.row;
        self.w1 = self.e20.row;
        self.w2 = self.e01.row;
    }

    /// Generate stepper for float attribute (like vertex UVs or vertex colors)
    /// Depends on SingleStepper's initial state. Make sure to run before using SingleStepper::row_step() or
    /// SingleStepper::col_step()
    fn attr<T>(&self, attr: &[T; 3], sp_inv_area: f32) -> AttributeStepper<T>
    where
        T: Copy + Add<Output = T> + Sub<Output = T> + AddAssign + Mul<f32, Output = T>,
    {
        // Get attribute value of top left
        let w0 = self.e12.row;
        let w1 = self.e20.row;
        let (b0, b1, b2) = bary(w0, w1, sp_inv_area);
        let attr_tl = attr[0] * b0 + attr[1] * b1 + attr[2] * b2;

        // Get attribute value of one x step right from top left
        let w0sx = w0 + self.e12.step.x;
        let w1sx = w1 + self.e20.step.x;
        let (b0, b1, b2) = bary(w0sx, w1sx, sp_inv_area);
        let attr_1x = attr[0] * b0 + attr[1] * b1 + attr[2] * b2;

        // Get attribute value of one y step down from top left
        let w0sy = w0 + self.e12.step.y;
        let w1sy = w1 + self.e20.step.y;
        let (b0, b1, b2) = bary(w0sy, w1sy, sp_inv_area);
        let attr_1y = attr[0] * b0 + attr[1] * b1 + attr[2] * b2;

        // Compute deltas
        let step_x = attr_1x - attr_tl;
        let step_y = attr_1y - attr_tl;

        let row = attr_tl;

        AttributeStepper {
            step_x,
            step_y,
            row,
            attr: attr_tl,
        }
    }
}

pub struct SingleStep {
    pub step: I64Vec2,
    pub row: i64,
}

impl SingleStep {
    #[inline(always)]
    pub fn new(sp0: &I64Vec2, sp1: &I64Vec2, sp_min_p: &I64Vec2, subpix: i64) -> Self {
        let a = sp0.y - sp1.y;
        let b = sp1.x - sp0.x;
        let c = (sp0.x) * (sp1.y) - (sp0.y) * (sp1.x);

        Self {
            step: i64vec2(a * subpix, b * subpix),
            row: a * sp_min_p.x + b * sp_min_p.y + c,
        }
    }
}

#[inline(always)]
pub fn orient2d(a: &I64Vec2, b: &I64Vec2, c: &I64Vec2) -> i64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[inline(always)]
pub fn bary(w0: i64, w1: i64, inv_area: f32) -> (f32, f32, f32) {
    let b0 = (w0 as f32) * inv_area;
    let b1 = (w1 as f32) * inv_area;
    let b2 = 1.0 - b0 - b1;
    (b0, b1, b2)
}

struct AttributeStepper<T>
where
    T: Copy + Add<Output = T> + Sub<Output = T> + AddAssign + Mul<f32, Output = T>,
{
    step_x: T,
    step_y: T,
    row: T,
    attr: T,
}

impl<T> AttributeStepper<T>
where
    T: Copy + Add<Output = T> + Sub<Output = T> + AddAssign + Mul<f32, Output = T>,
{
    #[inline(always)]
    pub fn row_step(&mut self) {
        self.row += self.step_y;
    }

    #[inline(always)]
    pub fn col_step(&mut self) {
        self.attr += self.step_x;
    }

    #[inline(always)]
    pub fn row_start(&mut self) {
        self.attr = self.row;
    }
}
