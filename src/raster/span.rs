use crate::raster::bary::stepper_from_ss_tri_backface_cull;

/// ss for screen space (unit is screen pixel)
/// sp for subpixel space (unit fraction of screen pixel)
pub fn raster_tri_span<const SUBPIX_BITS: i32>(
    ss_bounds: [i32; 4],
    ss_tri: &[egui::Vec2; 3],
    // ss_start, ss_end, ss_y
    mut span: impl FnMut(i64, i64, i64),
) {
    let Some((ss_min, ss_max, _sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(ss_bounds, ss_tri)
    else {
        return;
    };

    let max_cols = (ss_max.x - ss_min.x) + 1;

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        let w0 = stepper.w0 + stepper.bias0;
        let w1 = stepper.w1 + stepper.bias1;
        let w2 = stepper.w2 + stepper.bias2;

        let sx0 = stepper.e12.step.x;
        let sx1 = stepper.e20.step.x;
        let sx2 = stepper.e01.step.x;

        let mut start = 0;
        let mut end = i64::MAX;

        if w0 < 0 {
            if sx0 > 0 {
                start = start.max(div_ceil(-w0, sx0));
            } else {
                stepper.row_step();
                continue;
            }
        } else if sx0 < 0 {
            end = end.min(div_floor(w0, -sx0) + 1);
        }

        if w1 < 0 {
            if sx1 > 0 {
                start = start.max(div_ceil(-w1, sx1));
            } else {
                stepper.row_step();
                continue;
            }
        } else if sx1 < 0 {
            end = end.min(div_floor(w1, -sx1) + 1);
        }

        if w2 < 0 {
            if sx2 > 0 {
                start = start.max(div_ceil(-w2, sx2));
            } else {
                stepper.row_step();
                continue;
            }
        } else if sx2 < 0 {
            end = end.min(div_floor(w2, -sx2) + 1);
        }

        end = end.clamp(0, max_cols);

        if start >= end {
            stepper.row_step();
            continue;
        }

        let ss_start = ss_min.x + start;
        let ss_end = ss_min.x + end;

        span(ss_start, ss_end, ss_y);
        stepper.row_step();
    }
}

// --------------------------------------------------------------------------------------------
// div_floor() and div_ceil() are currently unstable. Use the built-in ones once they stabilize
// --------------------------------------------------------------------------------------------

// https://github.com/rust-lang/rust/blob/e4b521903b3b1a671e26a70b9475bcff385767e5/library/core/src/num/int_macros.rs#L3196
#[inline(always)]
pub const fn div_floor(lhs: i64, rhs: i64) -> i64 {
    let d = lhs / rhs;
    let r = lhs % rhs;

    // If the remainder is non-zero, we need to subtract one if the
    // signs of lhs and rhs differ, as this means we rounded upwards
    // instead of downwards. We do this branchlessly by creating a mask
    // which is all-ones iff the signs differ, and 0 otherwise. Then by
    // adding this mask (which corresponds to the signed value -1), we
    // get our correction.
    let correction = (lhs ^ rhs) >> (i64::BITS - 1);
    if r != 0 { d + correction } else { d }
}

// https://github.com/rust-lang/rust/blob/e4b521903b3b1a671e26a70b9475bcff385767e5/library/core/src/num/int_macros.rs#L3238
#[inline(always)]
pub const fn div_ceil(lhs: i64, rhs: i64) -> i64 {
    let d = lhs / rhs;
    let r = lhs % rhs;

    // When remainder is non-zero we have a.div_ceil(b) == 1 + a.div_floor(b),
    // so we can re-use the algorithm from div_floor, just adding 1.
    let correction = 1 + ((lhs ^ rhs) >> (i64::BITS - 1));
    if r != 0 { d + correction } else { d }
}
