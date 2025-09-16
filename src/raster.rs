use egui::Vec2;

// Unfortunately using a closure was slower.

// Based on https://fgiesen.wordpress.com/2013/02/17/optimizing-sw-occlusion-culling-index/
// Particularly:
// https://fgiesen.wordpress.com/2013/02/08/triangle-rasterization-in-practice/
// https://fgiesen.wordpress.com/2013/02/10/optimizing-the-basic-rasterizer/
// https://fgiesen.wordpress.com/2013/02/11/depth-buffers-done-quick-part/
// https://fgiesen.wordpress.com/2013/02/16/depth-buffers-done-quick-part-2/

pub trait PixelRaster {
    const NEED_BARY: bool;
    fn raster(&mut self, x: i64, y: i64, w0: i64, w1: i64, inv_area: f32);
}

pub struct WithBary<F>(pub F);
impl<F> PixelRaster for WithBary<F>
where
    F: FnMut(i64, i64, f32, f32),
{
    const NEED_BARY: bool = true;

    #[inline(always)]
    fn raster(&mut self, x: i64, y: i64, w0: i64, w1: i64, inv_area: f32) {
        let b0 = (w0 as f32) * inv_area;
        let b1 = (w1 as f32) * inv_area;
        (self.0)(x, y, b0, b1)
    }
}

pub struct NoBary<F>(pub F);
impl<F> PixelRaster for NoBary<F>
where
    F: FnMut(i64, i64),
{
    const NEED_BARY: bool = false;

    #[inline(always)]
    fn raster(&mut self, x: i64, y: i64, _w0: i64, _w1: i64, _inv_area: f32) {
        (self.0)(x, y)
    }
}

#[inline(always)]
pub fn with_bary<F>(f: F) -> WithBary<F>
where
    F: FnMut(i64, i64, f32, f32),
{
    WithBary(f)
}

#[inline(always)]
pub fn no_bary<F>(f: F) -> NoBary<F>
where
    F: FnMut(i64, i64),
{
    NoBary(f)
}

#[inline(always)]
pub fn raster_tri_no_depth_no_backface_cull<R, const SUBPIX_BITS: i32>(
    bounds: [i32; 4],
    scr_tri: [Vec2; 3],
    mut raster: R,
) where
    R: PixelRaster,
{
    let subpix_bits = SUBPIX_BITS as u32;
    let subpix: i64 = 1 << subpix_bits;
    let subpix_half: i64 = subpix >> 1;
    let fsubpix = subpix as f32;

    let bounds = [
        bounds[0] as i64,
        bounds[1] as i64,
        bounds[2] as i64,
        bounds[3] as i64,
    ];

    let scr0 = vec2_to_ivec2(scr_tri[0] * fsubpix);
    let scr1 = vec2_to_ivec2(scr_tri[1] * fsubpix);
    let scr2 = vec2_to_ivec2(scr_tri[2] * fsubpix);

    let area = orient2d_hp(&scr0, &scr1, &scr2);
    if area == 0 {
        return;
    }

    let tri_min_x = scr0[0].min(scr1[0]).min(scr2[0]);
    let tri_min_y = scr0[1].min(scr1[1]).min(scr2[1]);
    let tri_max_x = scr0[0].max(scr1[0]).max(scr2[0]);
    let tri_max_y = scr0[1].max(scr1[1]).max(scr2[1]);

    let min_x = ((tri_min_x - subpix_half) >> subpix_bits).clamp(bounds[0], bounds[2] - 1);
    let min_y = ((tri_min_y - subpix_half) >> subpix_bits).clamp(bounds[1], bounds[3] - 1);
    let max_x = ((tri_max_x + subpix_half) >> subpix_bits).clamp(bounds[0], bounds[2] - 1);
    let max_y = ((tri_max_y + subpix_half) >> subpix_bits).clamp(bounds[1], bounds[3] - 1);

    let sizex = max_x - min_x;
    let sizey = max_y - min_y;
    if sizex <= 0 || sizey <= 0 {
        return;
    }

    let p = [min_x, min_y];
    let inv_area: f32 = if R::NEED_BARY {
        1.0 / (area as f32)
    } else {
        0.0
    };

    let mut stepper = SingleStepper::new(&scr0, &scr1, &scr2, &p, subpix);

    if area >= 0 {
        for y in min_y..=max_y {
            stepper.row_start();
            for x in min_x..=max_x {
                if stepper.inside_tri_pos_area() {
                    raster.raster(x, y, stepper.w0, stepper.w1, inv_area);
                }
                stepper.col_step();
            }
            stepper.row_step();
        }
    } else {
        for y in min_y..=max_y {
            stepper.row_start();
            for x in min_x..=max_x {
                if stepper.inside_tri_neg_area() {
                    raster.raster(x, y, stepper.w0, stepper.w1, inv_area);
                }
                stepper.col_step();
            }
            stepper.row_step();
        }
    }
}

#[inline(always)]
pub fn is_top_left(a: &[i64; 2], b: &[i64; 2]) -> bool {
    let dy = b[1] - a[1];
    (dy > 0) || (dy == 0 && (b[0] - a[0]) < 0)
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
    pub fn new(v0: &[i64; 2], v1: &[i64; 2], v2: &[i64; 2], p: &[i64; 2], subpix: i64) -> Self {
        SingleStepper {
            e12: SingleStep::new(v1, v2, p, subpix),
            e20: SingleStep::new(v2, v0, p, subpix),
            e01: SingleStep::new(v0, v1, p, subpix),
            w0: 0,
            w1: 0,
            w2: 0,
            bias0: if is_top_left(v1, v2) { 0 } else { -1 },
            bias1: if is_top_left(v2, v0) { 0 } else { -1 },
            bias2: if is_top_left(v0, v1) { 0 } else { -1 },
        }
    }

    #[inline(always)]
    pub fn inside_tri_pos_area(&self) -> bool {
        let m = ((self.w0 + self.bias0) as u64)
            | ((self.w1 + self.bias1) as u64)
            | ((self.w2 + self.bias2) as u64);
        (m & 0x8000_0000_0000_0000) == 0
    }

    #[inline(always)]
    pub fn inside_tri_neg_area(&self) -> bool {
        let m = ((self.w0 + self.bias0) as u64)
            & ((self.w1 + self.bias1) as u64)
            & ((self.w2 + self.bias2) as u64);
        (m & 0x8000_0000_0000_0000) != 0
    }

    #[inline(always)]
    pub fn row_step(&mut self) {
        self.e12.row += self.e12.one_step_y;
        self.e20.row += self.e20.one_step_y;
        self.e01.row += self.e01.one_step_y;
    }

    #[inline(always)]
    pub fn col_step(&mut self) {
        self.w0 += self.e12.one_step_x;
        self.w1 += self.e20.one_step_x;
        self.w2 += self.e01.one_step_x;
    }

    #[inline(always)]
    pub fn row_start(&mut self) {
        self.w0 = self.e12.row;
        self.w1 = self.e20.row;
        self.w2 = self.e01.row;
    }
}

pub struct SingleStep {
    pub one_step_x: i64,
    pub one_step_y: i64,
    pub row: i64,
}

impl SingleStep {
    pub const STEP_XSIZE: i64 = 1;
    pub const STEP_YSIZE: i64 = 1;

    #[inline(always)]
    fn block_centers(p: &[i64; 2], subpix: i64) -> (i64, i64) {
        let subpix_half = subpix >> 1;
        let base_x = p[0] * subpix + subpix_half;
        let base_y = p[1] * subpix + subpix_half;
        (base_x, base_y)
    }

    #[inline(always)]
    pub fn new(v0: &[i64; 2], v1: &[i64; 2], p: &[i64; 2], subpix: i64) -> Self {
        let a = v0[1] - v1[1];
        let b = v1[0] - v0[0];
        let c = (v0[0]) * (v1[1]) - (v0[1]) * (v1[0]);

        let step_x = (a * Self::STEP_XSIZE).saturating_mul(subpix);
        let step_y = (b * Self::STEP_YSIZE).saturating_mul(subpix);

        let one_step_x = step_x;
        let one_step_y = step_y;

        let (x, y) = Self::block_centers(p, subpix);

        let edge = a * x + b * y + c;

        Self {
            one_step_x,
            one_step_y,
            row: edge,
        }
    }
}

#[inline(always)]
pub fn orient2d_hp(a: &[i64; 2], b: &[i64; 2], c: &[i64; 2]) -> i64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

pub fn vec2_to_ivec2(v: egui::Vec2) -> [i64; 2] {
    [v.x as i64, v.y as i64]
}
