#![allow(unsafe_code)]

use core::{arch::x86_64::*, ptr::read_unaligned};

use crate::color_sse41::egui_blend_u8;

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i], dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "avx2")]
pub fn egui_blend_u8_slice(src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
    assert_eq!(src.len(), dst.len());

    let n = dst.len();
    if n == 0 {
        return;
    }

    let mut i = 0;
    while i + 3 < n {
        // Load 4 src pixels
        let src_ptr = unsafe { src.as_ptr().add(i) }.cast::<__m128i>();
        let src_u8x4x4 = unsafe { read_unaligned(src_ptr) };
        let src_u16x4x4 = _mm256_cvtepu8_epi16(src_u8x4x4);

        // Load 4 dst pixels
        let dst_ptr = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m128i>();
        let dst_u8x4x4 = unsafe { read_unaligned(dst_ptr) };
        let dst_u16x4x4 = _mm256_cvtepu8_epi16(dst_u8x4x4);

        let dst_u8x4x4 = egui_blend_4_u16x4(src_u8x4x4, src_u16x4x4, dst_u16x4x4);

        unsafe { core::ptr::write_unaligned(dst_ptr, dst_u8x4x4) };
        i += 4;
    }

    while i < n {
        dst[i] = egui_blend_u8(src[i], dst[i]);
        i += 1;
    }
}

#[inline]
/// src_u8x4x4 is should have four 8 bit per channel rgba samples stored in the low bits
/// src_u16x4x4 is should have four 16 bit per channel rgba samples
/// dst_u16x4x4 is should have four 16 bit per channel rgba samples
#[target_feature(enable = "avx2")]
fn egui_blend_4_u16x4(src_u8x4x4: __m128i, src_u16x4x4: __m256i, dst_u16x4x4: __m256i) -> __m128i {
    let ones_u16x4x4 = _mm256_set1_epi16(0x00FF);
    let e1_u16x4x4 = _mm256_set1_epi16(0x0080);
    let e2_u16x4x4 = _mm256_set1_epi16(0x0101);

    // Broadcast alpha within each pixel's 4 lanes
    let a_broadcast_lo = _mm256_shufflelo_epi16(src_u16x4x4, 0b11111111);
    let a_broadcast = _mm256_shufflehi_epi16(a_broadcast_lo, 0b11111111);

    // simd_alpha_compl = 255 - A for each lane, per pixel
    let simd_alpha_compl = _mm256_xor_si256(ones_u16x4x4, a_broadcast);

    // dst * alpha_compl + 0x0080008000800080
    let dst_term = _mm256_mullo_epi16(dst_u16x4x4, simd_alpha_compl);
    let res_u16x4x4 = _mm256_add_epi16(dst_term, e1_u16x4x4);

    // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation
    //                              1           256     1            257
    // ((x >> 8) + x) >> 8 = (x + x---)/256 = (x--- + x---)/256 = (x-----) = x*257 >> 16
    //                             256          256    256          65536
    let res_u16x4x4 = _mm256_mulhi_epu16(res_u16x4x4, e2_u16x4x4);

    // Pack back to u8
    let hi_u16x4x2 = _mm256_extracti128_si256(res_u16x4x4, 1);
    let lo_u16x4x2 = _mm256_castsi256_si128(res_u16x4x4);
    let dst_u8x4x4 = _mm_packus_epi16(lo_u16x4x2, hi_u16x4x2);

    // dst.saturating_add(src)
    _mm_adds_epu8(dst_u8x4x4, src_u8x4x4)
}
