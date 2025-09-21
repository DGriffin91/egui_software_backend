use egui::Vec2;
use strength_reduce::StrengthReducedU64;

use crate::{
    math::vec4::Vec4,
    raster::bary::{AttributeStepper, SingleStepper, stepper_from_ss_tri_backface_cull},
};

/// ss for screen space (unit is screen pixel)
pub fn raster_tri_span<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[egui::Vec2; 3],
    // ss_start, ss_end, ss_y
    mut span: impl FnMut(usize, usize, usize),
) {
    let Some((ss_min, ss_max, _sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    let step_rcp = step_rcp(&stepper);

    let max_cols = (ss_max.x - ss_min.x) + 1;

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();

        if let Some((start, end)) = calc_row_span(&stepper, max_cols, &step_rcp) {
            let ss_start = ss_min.x + start;
            let ss_end = ss_min.x + end;
            span(ss_start as usize, ss_end as usize, ss_y as usize);
        };

        stepper.row_step();
    }
}

/// ss for screen space (unit is screen pixel)
pub fn raster_tri_with_colors_span<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[Vec2; 3],
    colors: &[Vec4; 3],
    // ss_start, ss_end, ss_y, AttributeStepper<Vec4>
    mut span: impl FnMut(usize, usize, usize, &mut AttributeStepper<Vec4>),
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    let step_rcp = step_rcp(&stepper);

    let mut c_stepper = stepper.attr(colors, sp_inv_area);

    let max_cols = (ss_max.x - ss_min.x) + 1;

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        c_stepper.row_start();

        if let Some((start, end)) = calc_row_span(&stepper, max_cols, &step_rcp) {
            c_stepper.attr += c_stepper.step_x * start as f32;
            let ss_start = ss_min.x + start;
            let ss_end = ss_min.x + end;
            span(
                ss_start as usize,
                ss_end as usize,
                ss_y as usize,
                &mut c_stepper,
            );
        };

        stepper.row_step();
        c_stepper.row_step();
    }
}

/// Returns Some((start, end)) for the current row in the triangle. The end points are defined within the aabb of the
/// triangle so add ss_min.x to each to get the screen space coordinate. Returns None if there is no span intersecting
/// this row.
fn calc_row_span(
    stepper: &SingleStepper,
    max_cols: i64,
    step_rcp: &[StrengthReducedU64; 3],
) -> Option<(i64, i64)> {
    let mut start = 0;
    let mut end = u64::MAX;
    let w = [
        stepper.w0 + stepper.bias0,
        stepper.w1 + stepper.bias1,
        stepper.w2 + stepper.bias2,
    ];
    let sx = [stepper.e12.step.x, stepper.e20.step.x, stepper.e01.step.x];

    for i in 0..3 {
        let w = w[i];
        let sx = sx[i];
        let step_rcp = step_rcp[i];
        if w < 0 {
            if sx > 0 {
                // unoptimized: start = start.max(div_ceil(-w, sx));
                start = start.max(div_ceil_sr(-w as u64, step_rcp));
            } else {
                return None;
            }
        } else if sx < 0 {
            // unoptimized: end = end.min(div_floor(w, -sx) + 1);
            // sx < 0 and stepper.step_x_rcp is already abs so no need to negate
            // w >= 0 due to if condition above so cast to u64 is fine
            end = end.min((w as u64 / step_rcp) + 1);
        }
    }

    end = end.clamp(0, max_cols as u64);

    if start >= end {
        return None;
    }
    Some((start as i64, end as i64))
}

// based on https://github.com/rust-lang/rust/blob/e4b521903b3b1a671e26a70b9475bcff385767e5/library/core/src/num/int_macros.rs#L3238
#[inline(always)]
pub fn div_ceil_sr(lhs: u64, rhs: StrengthReducedU64) -> u64 {
    let (d, r) = StrengthReducedU64::div_rem(lhs, rhs);

    // When remainder is non-zero we have a.div_ceil(b) == 1 + a.div_floor(b),
    // so we can re-use the algorithm from div_floor, just adding 1.
    let correction = 1 + ((lhs ^ rhs.get()) >> (u64::BITS - 1));
    if r != 0 { d + correction } else { d }
}

fn step_rcp(stepper: &SingleStepper) -> [StrengthReducedU64; 3] {
    // max(1) is fine here since the element will already be skipped if step.x == 0
    [
        StrengthReducedU64::new(stepper.e12.step.x.abs().max(1) as u64),
        StrengthReducedU64::new(stepper.e20.step.x.abs().max(1) as u64),
        StrengthReducedU64::new(stepper.e01.step.x.abs().max(1) as u64),
    ]
}
