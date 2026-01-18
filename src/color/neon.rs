#![allow(unsafe_code)]

use core::arch::aarch64::*;

use crate::SelectedImpl;

#[derive(Clone, Copy)]
pub(crate) struct NeonImpl(());

impl NeonImpl {
    /// `std::arch::is_aarch64_feature_detected!("neon")` MUST be true
    pub(crate) unsafe fn new() -> Self {
        Self(())
    }
}

impl SelectedImpl for NeonImpl {
    #[inline]
    fn egui_blend_u8_slice(self, src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
        unsafe { egui_blend_u8_slice(src, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_one_src_tinted_fn(
        self,
        src: [u8; 4],
        tint_fn: impl FnMut() -> [u8; 4],
        dst: &mut [[u8; 4]],
    ) {
        unsafe { egui_blend_u8_slice_one_src_tinted_fn(src, tint_fn, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_tinted(self, src: &[[u8; 4]], tint: [u8; 4], dst: &mut [[u8; 4]]) {
        unsafe { egui_blend_u8_slice_tinted(src, tint, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_one_src(self, src: [u8; 4], dst: &mut [[u8; 4]]) {
        unsafe { egui_blend_u8_slice_one_src(src, dst) }
    }

    #[inline]
    fn egui_blend_u8(self, src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
        unsafe { egui_blend_u8(src, dst) }
    }

    #[inline]
    fn unorm_mult4x4(self, a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
        unsafe { unorm_mult4x4(a, b) }
    }
}

/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[inline]
#[target_feature(enable = "neon")]
fn egui_blend_u8(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let a = src[3];
    if a == 255 {
        return src;
    }

    let alpha_compl = vdupq_n_u16((0xFFu16) ^ (a as u16));
    let e1 = vdupq_n_u16(0x0080);

    let dst8 = vreinterpret_u8_u32(vdup_n_u32(u32::from_le_bytes(dst)));
    let mut dst = vmovl_u8(dst8);

    // dst * alpha_compl + 0x0080
    dst = vaddq_u16(vmulq_u16(dst, alpha_compl), e1);

    // ((x >> 8) + x) >> 8
    dst = vaddq_u16(dst, vshrq_n_u16(dst, 8));
    dst = vshrq_n_u16(dst, 8);

    // Pack to back to u8
    let dst = vqmovn_u16(dst);

    let src32x2 = vdup_n_u32(u32::from_le_bytes(src));
    let src8 = vreinterpret_u8_u32(src32x2);

    // saturating add
    let out8 = vqadd_u8(dst, src8);

    let out32 = vget_lane_u32(vreinterpret_u32_u8(out8), 0);
    out32.to_le_bytes()
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "neon")]
pub fn egui_blend_u8_slice_one_src(src: [u8; 4], dst: &mut [[u8; 4]]) {
    let n = dst.len();
    if n == 0 {
        return;
    }

    let a = src[3];

    let src32 = u32::from_le_bytes(src);

    let alpha = src[3] as i32;
    let alpha_compl = 0xFF ^ alpha;

    let e1 = vdupq_n_u16(0x0080);

    // (255 - a) in all u16 lanes
    let simd_alpha_compl = vdupq_n_u16(alpha_compl as u16);

    let mut i = 0;
    if a == 0 {
        let src8_q = vreinterpretq_u8_u32(vdupq_n_u32(src32));
        // Only need to saturating_add src to dst. We can do 4 at a time in this case.
        while i + 3 < n {
            let p = unsafe { dst.as_mut_ptr().add(i) } as *mut u8;
            let d128 = unsafe { vld1q_u8(p) };
            let out = vqaddq_u8(d128, src8_q);
            unsafe { vst1q_u8(p, out) };
            i += 4;
        }
    } else {
        let src8 = vreinterpret_u8_u32(vdup_n_u32(src32));
        while i + 1 < n {
            let dst_p = unsafe { dst.as_mut_ptr().add(i) } as *mut u8;
            // Load two dst pixels
            let d8 = unsafe { vld1_u8(dst_p) };
            // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
            let dst16 = vmovl_u8(d8);

            // dst * alpha_compl + 0x0080008000800080
            let res16 = vaddq_u16(vmulq_u16(dst16, simd_alpha_compl), e1);

            // ((x >> 8) + x) >> 8
            let res16 = vshrq_n_u16(vaddq_u16(res16, vshrq_n_u16(res16, 8)), 8);
            let mut dst8 = vqmovn_u16(res16);

            // dst.saturating_add(src)
            dst8 = vqadd_u8(dst8, src8);

            unsafe { vst1_u8(dst_p, dst8) };
            i += 2;
        }
    }

    while i < n {
        dst[i] = egui_blend_u8(src, dst[i]);
        i += 1;
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i], dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "neon")]
fn egui_blend_u8_slice(src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
    assert_eq!(src.len(), dst.len());

    let n = dst.len();
    if n == 0 {
        return;
    }

    let mut i = 0;
    while i + 1 < n {
        let src_p = unsafe { src.as_ptr().add(i) } as *mut u8;
        // Load two src pixels
        let src8 = unsafe { vld1_u8(src_p) };
        // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
        let src16 = vmovl_u8(src8);

        // Load two dst pixels
        let dst_p = unsafe { dst.as_mut_ptr().add(i) } as *mut u8;
        // Load two dst pixels
        let d8 = unsafe { vld1_u8(dst_p) };
        // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
        let dst16 = vmovl_u8(d8);

        let dst8 = egui_blend_two_u16x4(src8, src16, dst16);

        unsafe { vst1_u8(dst_p, dst8) };
        i += 2;
    }

    if i < n {
        dst[i] = egui_blend_u8(src[i], dst[i]);
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i] * vert, dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "neon")]
fn egui_blend_u8_slice_tinted(src: &[[u8; 4]], tint: [u8; 4], dst: &mut [[u8; 4]]) {
    assert_eq!(src.len(), dst.len());
    let n = dst.len();
    if n == 0 {
        return;
    }

    let e1 = vdupq_n_u16(0x0080);

    let tint32 = u32::from_le_bytes(tint);
    let tint8 = vreinterpret_u8_u32(vdup_n_u32(tint32));
    let tint16 = vmovl_u8(tint8);

    let mut i = 0usize;
    while i + 1 < n {
        let src_p = unsafe { src.as_ptr().add(i) } as *mut u8;
        // Load two src pixels
        let src8 = unsafe { vld1_u8(src_p) };
        // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
        let src16 = vmovl_u8(src8);

        // Load two dst pixels
        let dst_p = unsafe { dst.as_mut_ptr().add(i) } as *mut u8;
        let d8 = unsafe { vld1_u8(dst_p) };
        // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
        let dst16 = vmovl_u8(d8);

        // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
        let mut t = vaddq_u16(vmulq_u16(src16, tint16), e1);
        t = vaddq_u16(t, vshrq_n_u16(t, 8));
        let src_tinted16 = vshrq_n_u16(t, 8);
        let src_tinted8 = vqmovn_u16(src_tinted16);

        let dst8 = egui_blend_two_u16x4(src_tinted8, src_tinted16, dst16);

        unsafe { vst1_u8(dst_p, dst8) };
        i += 2;
    }

    // Tail: handle the last pixel (if any) in scalar
    if i < n {
        dst[i] = egui_blend_u8(unorm_mult4x4(src[i], tint), dst[i]);
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src * tint_fn(), dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "neon")]
fn egui_blend_u8_slice_one_src_tinted_fn(
    src: [u8; 4],
    mut tint_fn: impl FnMut() -> [u8; 4],
    dst: &mut [[u8; 4]],
) {
    let n = dst.len();
    if n == 0 {
        return;
    }

    let src32 = u32::from_le_bytes(src);
    let src8 = vreinterpret_u8_u32(vdup_n_u32(src32));
    let src16 = vmovl_u8(src8);

    let e1 = vdupq_n_u16(0x0080);

    let mut i = 0usize;
    while i + 1 < n {
        // Load two tint values

        let mut t32x2 = vdup_n_u32(0);
        t32x2 = vset_lane_u32(u32::from_le_bytes(tint_fn()), t32x2, 0);
        t32x2 = vset_lane_u32(u32::from_le_bytes(tint_fn()), t32x2, 1);
        let tint8 = vreinterpret_u8_u32(t32x2);
        let tint16 = vmovl_u8(tint8);

        // Load two dst pixels
        let dst_p = unsafe { dst.as_mut_ptr().add(i) } as *mut u8;
        let d8 = unsafe { vld1_u8(dst_p) };
        // [0,0,0,0,rg,ba,rg,ba] -> [r,g,b,a,r,g,b,a]
        let dst16 = vmovl_u8(d8);

        // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
        let mut t = vaddq_u16(vmulq_u16(src16, tint16), e1);
        t = vaddq_u16(t, vshrq_n_u16(t, 8));
        let src_tinted16 = vshrq_n_u16(t, 8);
        let src_tinted8 = vqmovn_u16(src_tinted16);

        let dst8 = egui_blend_two_u16x4(src_tinted8, src_tinted16, dst16);

        unsafe { vst1_u8(dst_p, dst8) };
        i += 2;
    }

    // Tail: handle the last pixel (if any) in scalar
    if i < n {
        dst[i] = egui_blend_u8(unorm_mult4x4(src, tint_fn()), dst[i]);
    }
}

#[inline]
#[target_feature(enable = "neon")]
fn unorm_mult4x4(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    let e1 = vdupq_n_u16(0x0080);
    let a = vmovl_u8(vreinterpret_u8_u32(vdup_n_u32(u32::from_le_bytes(a))));
    let b = vmovl_u8(vreinterpret_u8_u32(vdup_n_u32(u32::from_le_bytes(b))));

    // a * b + 0x0080
    let mut dst = vaddq_u16(vmulq_u16(a, b), e1);

    // ((a >> 8) + a) >> 8
    dst = vaddq_u16(dst, vshrq_n_u16(dst, 8));
    dst = vshrq_n_u16(dst, 8);

    // Pack to back to u8
    let dst = vqmovn_u16(dst);

    vget_lane_u32(vreinterpret_u32_u8(dst), 0).to_le_bytes() // Return first element of dst
}

#[inline]
/// src8 is should have two 8 bit per channel rgba samples stored in the low bits
/// src16 is should have two 16 bit per channel rgba samples
/// dst16 is should have two 16 bit per channel rgba samples
#[target_feature(enable = "neon")]
fn egui_blend_two_u16x4(src8: uint8x8_t, src16: uint16x8_t, dst16: uint16x8_t) -> uint8x8_t {
    let ones = vdupq_n_u16(0x00FF);
    let e1 = vdupq_n_u16(0x0080);

    // Broadcast alpha within each pixel's 4 lanes
    let a_lo4 = vdup_n_u16(vgetq_lane_u16(src16, 3));
    let a_hi4 = vdup_n_u16(vgetq_lane_u16(src16, 7));
    let a_broadcast = vcombine_u16(a_lo4, a_hi4);

    // simd_alpha_compl = 255 - A for each lane, per pixel
    let simd_alpha_compl = vsubq_u16(ones, a_broadcast);

    // dst * alpha_compl + 0x0080008000800080
    let dst_term = vmulq_u16(dst16, simd_alpha_compl);
    let res16 = vaddq_u16(dst_term, e1);

    // ((x >> 8) + x) >> 8
    let res16 = vaddq_u16(res16, vshrq_n_u16(res16, 8));
    let res16 = vshrq_n_u16(res16, 8);
    let dst8 = vqmovn_u16(res16);
    // Pack back to u8

    // dst.saturating_add(src)
    vqadd_u8(dst8, src8)
}
