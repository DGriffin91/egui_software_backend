#![allow(unsafe_code)]

use core::ptr::write_unaligned;
use core::{arch::x86_64::*, ptr::read_unaligned};

use crate::color::SelectedImpl;
use crate::color::sse41::Sse41Impl;

type U8x4x4 = __m128i;
type U16x4x2 = __m128i;
type U16x4x4 = __m256i;

#[derive(Clone, Copy)]
pub(crate) struct Avx2Impl {
    sse41: Sse41Impl,
}

impl Avx2Impl {
    /// `std::arch::is_x86_feature_detected!("avx2")` MUST be true
    /// `std::arch::is_x86_feature_detected!("sse4.1")` MUST be true
    pub(crate) const unsafe fn new() -> Self {
        Self {
            sse41: unsafe { Sse41Impl::new() },
        }
    }
}

impl Avx2Impl {
    // https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
    /// dst[i] = blend(src[i], dst[i]) // As unorm
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    #[target_feature(enable = "avx2")]
    fn egui_blend_u8_slice_avx2(self, src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
        assert_eq!(src.len(), dst.len());

        let n = dst.len();
        if n == 0 {
            return;
        }

        let mut i = 0;
        while i + 3 < n {
            // Load 4 src pixels
            let src_ptr = unsafe { src.as_ptr().add(i) }.cast::<__m128i>();
            let src8: U8x4x4 = unsafe { read_unaligned(src_ptr) };
            let src16: U16x4x4 = x8_zeroextend16(src8);

            // Load 4 dst pixels
            let dst_ptr = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m128i>();
            let dst8: U8x4x4 = unsafe { read_unaligned(dst_ptr) };
            let dst16: U16x4x4 = x8_zeroextend16(dst8);

            let dst8: U8x4x4 = egui_blend_4_u16x4(src8, src16, dst16);

            unsafe { write_unaligned(dst_ptr, dst8) };
            i += 4;
        }

        while i < n {
            dst[i] = self.egui_blend_u8(src[i], dst[i]);
            i += 1;
        }
    }

    // https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
    /// dst[i] = blend(src[i], dst[i]) // As unorm
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    #[target_feature(enable = "avx2")]
    fn egui_blend_u8_slice_one_src_tinted_fn_avx2(
        self,
        src: [u8; 4],
        mut tint_fn: impl FnMut() -> [u8; 4],
        dst: &mut [[u8; 4]],
    ) {
        let n = dst.len();
        if n == 0 {
            return;
        }

        let src8 = _mm_set1_epi32(i32::from_le_bytes(src));
        let src16 = x8_zeroextend16(src8);

        let mut i = 0usize;
        while i + 3 < n {
            // Load 4 tint values
            let tint_a = i32::from_le_bytes(tint_fn());
            let tint_b = i32::from_le_bytes(tint_fn());
            let tint_c = i32::from_le_bytes(tint_fn());
            let tint_d = i32::from_le_bytes(tint_fn());
            let tint8: U8x4x4 = _mm_set_epi32(tint_d, tint_c, tint_b, tint_a);
            let tint16: U16x4x4 = x8_zeroextend16(tint8);

            // Load 4 dst pixels
            let dst_ptr = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m128i>();
            let dst8: U8x4x4 = unsafe { read_unaligned(dst_ptr) };
            let dst16 = x8_zeroextend16(dst8);

            // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
            let src_tinted16 = x16_div_255_approx(a16_times_b16_plus_128(src16, tint16));
            let src_tinted8 = x16_pack8(src_tinted16);

            let dst8 = egui_blend_4_u16x4(src_tinted8, src_tinted16, dst16);

            unsafe { write_unaligned(dst_ptr, dst8) };
            i += 4;
        }

        // Tail: handle the last pixel (if any) in scalar
        while i < n {
            dst[i] = self.egui_blend_u8(self.unorm_mult4x4(src, tint_fn()), dst[i]);
            i += 1;
        }
    }

    #[target_feature(enable = "avx2")]
    fn egui_blend_u8_slice_tinted_avx2(self, src: &[[u8; 4]], tint: [u8; 4], dst: &mut [[u8; 4]]) {
        assert_eq!(src.len(), dst.len());
        let n = dst.len();
        if n == 0 {
            return;
        }

        let tint8: U8x4x4 = _mm_set1_epi32(i32::from_le_bytes(tint));
        let tint16: U16x4x4 = x8_zeroextend16(tint8);

        let mut i = 0usize;
        while i + 3 < n {
            // Load 4 src pixels
            let src = unsafe { src.as_ptr().add(i) }.cast::<__m128i>();
            let src8 = unsafe { read_unaligned(src) };
            let src16 = x8_zeroextend16(src8);

            // Load 4 dst pixels
            let dst = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m128i>();
            let dst8 = unsafe { read_unaligned(dst) };
            let dst16 = x8_zeroextend16(dst8);

            // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
            let src_tinted16 = x16_div_255_approx(a16_times_b16_plus_128(src16, tint16));
            let src_tinted8 = x16_pack8(src_tinted16);

            let dst8 = egui_blend_4_u16x4(src_tinted8, src_tinted16, dst16);

            unsafe { write_unaligned(dst, dst8) };
            i += 4;
        }

        // Tail: handle the last pixel (if any) in scalar
        while i < n {
            dst[i] = self.egui_blend_u8(self.unorm_mult4x4(src[i], tint), dst[i]);
            i += 1;
        }
    }

    #[target_feature(enable = "avx2")]
    fn egui_blend_u8_slice_one_src_avx2(self, src: [u8; 4], dst: &mut [[u8; 4]]) {
        let n = dst.len();
        if n == 0 {
            return;
        }

        let a = src[3];

        let alpha = src[3] as i32;
        let alpha_compl = 0xFF ^ alpha;

        // (255 - a) in all u16 lanes
        let alpha_compl16 = _mm256_set1_epi16(alpha_compl as i16);

        let mut i = 0;
        if a == 0 {
            // Only need to saturating_add src to dst. We can do 8 at a time in this case.
            let src8 = _mm256_set1_epi32(i32::from_le_bytes(src));
            while i + 7 < n {
                let dst_ptr = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m256i>();
                let dst8 = unsafe { read_unaligned(dst_ptr) };
                let dst8 = _mm256_adds_epu8(dst8, src8);
                unsafe { write_unaligned(dst_ptr, dst8) };
                i += 8;
            }
        } else {
            let src8 = _mm_set1_epi32(i32::from_le_bytes(src));
            while i + 3 < n {
                // Load 4 dst pixels
                let dst_ptr = unsafe { dst.as_mut_ptr().add(i) }.cast::<__m128i>();
                let dst8: U8x4x4 = unsafe { read_unaligned(dst_ptr) };
                let dst16 = x8_zeroextend16(dst8);

                // dst * alpha_compl + 0x0080008000800080
                let res16 = a16_times_b16_plus_128(dst16, alpha_compl16);
                let res16 = x16_div_255_approx(res16);
                let dst8 = x16_pack8(res16);

                // dst.saturating_add(src)
                let dst8 = a8_saturatingadd_b8(dst8, src8);

                unsafe { write_unaligned(dst_ptr, dst8) };
                i += 4;
            }
        }

        while i < n {
            dst[i] = self.egui_blend_u8(src, dst[i]);
            i += 1;
        }
    }
}

impl SelectedImpl for Avx2Impl {
    #[inline]
    fn egui_blend_u8_slice(self, src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
        unsafe { self.egui_blend_u8_slice_avx2(src, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_one_src_tinted_fn(
        self,
        src: [u8; 4],
        tint_fn: impl FnMut() -> [u8; 4],
        dst: &mut [[u8; 4]],
    ) {
        unsafe { self.egui_blend_u8_slice_one_src_tinted_fn_avx2(src, tint_fn, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_tinted(self, src: &[[u8; 4]], tint: [u8; 4], dst: &mut [[u8; 4]]) {
        unsafe { self.egui_blend_u8_slice_tinted_avx2(src, tint, dst) }
    }

    #[inline]
    fn egui_blend_u8_slice_one_src(self, src: [u8; 4], dst: &mut [[u8; 4]]) {
        unsafe { self.egui_blend_u8_slice_one_src_avx2(src, dst) }
    }

    #[inline]
    fn egui_blend_u8(self, src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
        self.sse41.egui_blend_u8(src, dst)
    }

    #[inline]
    fn unorm_mult4x4(self, a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
        self.sse41.unorm_mult4x4(a, b)
    }
}

/// src_u8x4x4 is should have four 8 bit per channel rgba samples stored in the low bits
/// src_u16x4x4 is should have four 16 bit per channel rgba samples
/// dst_u16x4x4 is should have four 16 bit per channel rgba samples
#[inline]
#[target_feature(enable = "avx2")]
fn egui_blend_4_u16x4(src8: __m128i, src16: __m256i, dst16: __m256i) -> __m128i {
    let ones_u16x4x4 = _mm256_set1_epi16(0x00FF);

    // Broadcast alpha within each pixel's 4 lanes
    let a_broadcast_lo = _mm256_shufflelo_epi16(src16, 0b11111111);
    let a_broadcast = _mm256_shufflehi_epi16(a_broadcast_lo, 0b11111111);

    // simd_alpha_compl = 255 - A for each lane, per pixel
    let simd_alpha_compl = _mm256_xor_si256(ones_u16x4x4, a_broadcast);

    // dst * alpha_compl + 0x0080008000800080
    let res16 = a16_times_b16_plus_128(dst16, simd_alpha_compl);
    let res16 = x16_div_255_approx(res16);
    let dst8 = x16_pack8(res16);

    a8_saturatingadd_b8(dst8, src8)
}

/// a.saturating_add(b)
#[inline]
#[target_feature(enable = "avx2")]
fn a8_saturatingadd_b8(a: U8x4x4, b: U8x4x4) -> U8x4x4 {
    _mm_adds_epu8(a, b)
}

/// x as u16
#[inline]
#[target_feature(enable = "avx2")]
fn x8_zeroextend16(x: U8x4x4) -> U16x4x4 {
    _mm256_cvtepu8_epi16(x)
}

/// a + b + 128
#[inline]
#[target_feature(enable = "avx2")]
fn a16_times_b16_plus_128(a: U16x4x4, b: U16x4x4) -> U16x4x4 {
    let mul = _mm256_mullo_epi16(a, b);
    _mm256_add_epi16(mul, _mm256_set1_epi16(128))
}

/// Fast approximation of x / 255
/// ((x >> 8) + x) >> x
#[inline]
#[target_feature(enable = "avx2")]
fn x16_div_255_approx(x: U16x4x4) -> U16x4x4 {
    // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation
    //                              1           256     1            257
    // ((x >> 8) + x) >> 8 = (x + x---)/256 = (x--- + x---)/256 = (x-----) = x*257 >> 16
    //                             256          256    256          65536
    _mm256_mulhi_epu16(x, _mm256_set1_epi16(257))
}

// Converts packed 16-bit integers from `x` to packed 8-bit integers using unsigned saturation.
#[inline]
#[target_feature(enable = "avx2")]
fn x16_pack8(x: U16x4x4) -> U8x4x4 {
    let hi: U16x4x2 = _mm256_extracti128_si256(x, 1);
    let lo: U16x4x2 = _mm256_castsi256_si128(x);
    _mm_packus_epi16(lo, hi)
}
