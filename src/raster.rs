use egui::Vec2;

// Based on https://fgiesen.wordpress.com/2013/02/17/optimizing-sw-occlusion-culling-index/
// Particularly:
// https://fgiesen.wordpress.com/2013/02/08/triangle-rasterization-in-practice/
// https://fgiesen.wordpress.com/2013/02/10/optimizing-the-basic-rasterizer/
// https://fgiesen.wordpress.com/2013/02/11/depth-buffers-done-quick-part/
// https://fgiesen.wordpress.com/2013/02/16/depth-buffers-done-quick-part-2/

#[inline(always)]
/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri_no_depth_backface_cull<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: [Vec2; 3],
    // ss_x, ss_y, w0, w1, sp_inv_area
    mut raster: impl FnMut(i64, i64, i64, i64, f32),
) {
    let subpix_bits = SUBPIX_BITS as u32;
    let subpix: i64 = 1 << subpix_bits;
    let subpix_half: i64 = subpix >> 1;
    let fsubpix = subpix as f32;

    let bounds = [
        ss_bounds[0] as i64,
        ss_bounds[1] as i64,
        ss_bounds[2] as i64,
        ss_bounds[3] as i64,
    ];

    // sp for subpixel space
    let sp0 = vec2_to_ivec2(ss_tri[0] * fsubpix);
    let sp1 = vec2_to_ivec2(ss_tri[1] * fsubpix);
    let sp2 = vec2_to_ivec2(ss_tri[2] * fsubpix);

    let sp_area = orient2d(&sp0, &sp1, &sp2);
    if sp_area <= 0 {
        return;
    }

    let sp_min_x = sp0[0].min(sp1[0]).min(sp2[0]);
    let sp_min_y = sp0[1].min(sp1[1]).min(sp2[1]);
    let sp_max_x = sp0[0].max(sp1[0]).max(sp2[0]);
    let sp_max_y = sp0[1].max(sp1[1]).max(sp2[1]);

    let ss_min_x = ((sp_min_x - subpix_half) >> subpix_bits).clamp(bounds[0], bounds[2] - 1);
    let ss_min_y = ((sp_min_y - subpix_half) >> subpix_bits).clamp(bounds[1], bounds[3] - 1);
    let ss_max_x = ((sp_max_x + subpix_half) >> subpix_bits).clamp(bounds[0], bounds[2] - 1);
    let ss_max_y = ((sp_max_y + subpix_half) >> subpix_bits).clamp(bounds[1], bounds[3] - 1);

    // The center of the minimum point in sub pixel space
    let sp_min_p = [
        ss_min_x * subpix + subpix_half,
        ss_min_y * subpix + subpix_half,
    ];

    let ss_sizex = ss_max_x - ss_min_x;
    let ss_sizey = ss_max_y - ss_min_y;
    if ss_sizex <= 0 || ss_sizey <= 0 {
        return;
    }

    let sp_inv_area = 1.0 / (sp_area as f32);

    let mut stepper = SingleStepper::new(&sp0, &sp1, &sp2, &sp_min_p, subpix);

    for ss_y in ss_min_y..=ss_max_y {
        stepper.row_start();
        for ss_x in ss_min_x..=ss_max_x {
            if stepper.inside_tri_pos_area() {
                raster(ss_x, ss_y, stepper.w0, stepper.w1, sp_inv_area);
            }
            stepper.col_step();
        }
        stepper.row_step();
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
    pub fn new(
        sp0: &[i64; 2],
        sp1: &[i64; 2],
        sp2: &[i64; 2],
        sp_min_p: &[i64; 2],
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
        self.e12.row += self.e12.step_y;
        self.e20.row += self.e20.step_y;
        self.e01.row += self.e01.step_y;
    }

    #[inline(always)]
    pub fn col_step(&mut self) {
        self.w0 += self.e12.step_x;
        self.w1 += self.e20.step_x;
        self.w2 += self.e01.step_x;
    }

    #[inline(always)]
    pub fn row_start(&mut self) {
        self.w0 = self.e12.row;
        self.w1 = self.e20.row;
        self.w2 = self.e01.row;
    }
}

pub struct SingleStep {
    pub step_x: i64,
    pub step_y: i64,
    pub row: i64,
}

impl SingleStep {
    #[inline(always)]
    pub fn new(sp0: &[i64; 2], sp1: &[i64; 2], sp_min_p: &[i64; 2], subpix: i64) -> Self {
        let a = sp0[1] - sp1[1];
        let b = sp1[0] - sp0[0];
        let c = (sp0[0]) * (sp1[1]) - (sp0[1]) * (sp1[0]);

        Self {
            step_x: a * subpix,
            step_y: b * subpix,
            row: a * sp_min_p[0] + b * sp_min_p[1] + c,
        }
    }
}

#[inline(always)]
pub fn orient2d(a: &[i64; 2], b: &[i64; 2], c: &[i64; 2]) -> i64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

#[inline(always)]
pub fn vec2_to_ivec2(v: egui::Vec2) -> [i64; 2] {
    [v.x as i64, v.y as i64]
}

#[inline(always)]
pub fn bary(w0: i64, w1: i64, inv_area: f32) -> (f32, f32, f32) {
    let b0 = (w0 as f32) * inv_area;
    let b1 = (w1 as f32) * inv_area;
    let b2 = 1.0 - b0 - b1;
    (b0, b1, b2)
}
