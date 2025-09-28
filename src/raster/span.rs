use crate::raster::bary::SingleStepper;

/// Returns Some((start, end)) for the current row in the triangle. The end points are defined within the aabb of the
/// triangle so add ss_min.x to each to get the screen space coordinate. Returns None if there is no span intersecting
/// this row.
pub(crate) fn calc_row_span(stepper: &SingleStepper, max_cols: i64) -> Option<(i64, i64)> {
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
        if w < 0 {
            if sx > 0 {
                start = start.max(div_ceil(-w, sx) as u64);
            } else {
                return None;
            }
        } else if sx < 0 {
            end = end.min(div_floor(w, -sx) as u64 + 1);
        }
    }

    end = end.clamp(0, max_cols as u64);

    if start >= end {
        return None;
    }
    Some((start as i64, end as i64))
}

// Not in stable rust yet. So vendored in:
// https://github.com/rust-lang/rust/blob/e4b521903b3b1a671e26a70b9475bcff385767e5/library/core/src/num/int_macros.rs#L3236C1-L3250C10
pub(crate) const fn div_ceil(lhs: i64, rhs: i64) -> i64 {
    let d = lhs / rhs;
    let r = lhs % rhs;

    // When remainder is non-zero we have a.div_ceil(b) == 1 + a.div_floor(b),
    // so we can re-use the algorithm from div_floor, just adding 1.
    let correction = 1 + ((lhs ^ rhs) >> (i64::BITS - 1));
    if r != 0 { d + correction } else { d }
}

// Not in stable rust yet. So vendored in:
// https://github.com/rust-lang/rust/blob/e4b521903b3b1a671e26a70b9475bcff385767e5/library/core/src/num/int_macros.rs#L3196C1-L3212C10
pub(crate) const fn div_floor(lhs: i64, rhs: i64) -> i64 {
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
