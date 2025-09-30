use crate::raster::bary::SingleStepper;
use strength_reduce::StrengthReducedU64;

/// Returns Some((start, end)) for the current row in the triangle. The end points are defined within the aabb of the
/// triangle so add ss_min.x to each to get the screen space coordinate. Returns None if there is no span intersecting
/// this row.
pub fn calc_row_span(
    stepper: &SingleStepper,
    max_cols: i64,
    step_rcp: &[StrengthReducedU64; 3],
) -> Option<(i64, i64)> {
    let mut start = 0;
    let mut end = u64::MAX;
    let w = &stepper.sp_weight;
    let sx = [
        stepper.step[0].step.x,
        stepper.step[1].step.x,
        stepper.step[2].step.x,
    ];

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

pub fn step_rcp(stepper: &SingleStepper) -> [StrengthReducedU64; 3] {
    // max(1) is fine here since the element will already be skipped if step.x == 0
    [
        StrengthReducedU64::new(stepper.step[0].step.x.abs().max(1) as u64),
        StrengthReducedU64::new(stepper.step[1].step.x.abs().max(1) as u64),
        StrengthReducedU64::new(stepper.step[2].step.x.abs().max(1) as u64),
    ]
}
