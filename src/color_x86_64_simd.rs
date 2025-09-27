use std::arch::x86_64::*;

/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_sse41(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let alpha = src[3];
    if alpha == 255 {
        return src;
    }

    let alpha_compl = _mm_set1_epi16(0xFFi16 ^ (alpha as i16));
    let e1 = _mm_set1_epi16(0x0080);
    let mut dst = _mm_cvtsi32_si128(i32::from_le_bytes(dst)); // Load dst into element a
    dst = _mm_cvtepu8_epi16(dst); // [0,0,0,0,0,0,0,rgba] -> [0,0,0,0,r,g,b,a]

    // dst * alpha_compl + 0x0080
    dst = _mm_add_epi16(_mm_mullo_epi16(dst, alpha_compl), e1);

    // ((x >> 8) + x) >> 8
    dst = _mm_add_epi16(dst, _mm_srli_epi16(dst, 8));
    dst = _mm_srli_epi16(dst, 8);

    // Pack to back to u8
    let dst = _mm_packus_epi16(dst, _mm_setzero_si128());

    let src = _mm_cvtsi32_si128(i32::from_le_bytes(src)); // Load src into element a
    let dst = _mm_adds_epu8(dst, src); // dst.saturating_add(src)

    i32::to_le_bytes(_mm_cvtsi128_si32(dst)) // Return first element of dst
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
#[target_feature(enable = "sse4.1")]
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
pub unsafe fn egui_blend_u8_slice_one_src_sse41(src: [u8; 4], dst: &mut [[u8; 4]]) {
    unsafe {
        let n = dst.len();
        if n == 0 {
            return;
        }

        let a = src[3];

        let src32 = u32::from_le_bytes(src);
        let src64 = (src32 as u64) | ((src32 as u64) << 32);
        let src128 = _mm_set1_epi64x(src64 as i64);

        let alpha = src[3] as i32;
        let alpha_compl = 0xFF ^ alpha;

        let e1 = _mm_set1_epi16(0x0080);
        let e2 = _mm_set1_epi16(0x0101);

        // (255 - a) in all u16 lanes
        let simd_alpha_compl = _mm_set1_epi16(alpha_compl as i16);

        let mut i = 0;
        if a == 0 {
            // Only need to saturating_add src to dst. We can do 4 at a time in this case.
            while i + 3 < n {
                let p = dst.as_mut_ptr().add(i) as *mut __m128i;
                let d128 = _mm_loadu_si128(p);
                let out = _mm_adds_epu8(d128, src128);
                _mm_storeu_si128(p, out);
                i += 4;
            }
        } else {
            while i + 1 < n {
                let dst = dst.as_mut_ptr().add(i).cast::<u64>();
                // Load two dst pixels
                let d64 = core::ptr::read_unaligned(dst);
                let d128 = _mm_cvtsi64_si128(d64 as i64);
                // [0,0,0,0,0,0,rgba,rgba] -> [r,g,b,a,r,g,b,a]
                let dst16 = _mm_cvtepu8_epi16(d128);

                // dst * alpha_compl + 0x0080008000800080
                let res16 = _mm_add_epi16(_mm_mullo_epi16(dst16, simd_alpha_compl), e1);

                // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation. (can you see why?)
                let res16 = _mm_mulhi_epu16(res16, e2);
                let mut dst8 = _mm_packus_epi16(res16, res16); // Pack back to u8

                // dst.saturating_add(src)
                dst8 = _mm_adds_epu8(dst8, src128);

                let lo64 = _mm_cvtsi128_si64(dst8) as u64;
                core::ptr::write_unaligned(dst, lo64);
                i += 2;
            }
        }

        while i < n {
            dst[i] = egui_blend_u8_sse41(src, dst[i]);
            i += 1;
        }
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i], dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_slice_sse41(src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
    unsafe {
        assert_eq!(src.len(), dst.len());

        let n = dst.len();
        if n == 0 {
            return;
        }

        let ones = _mm_set1_epi16(0x00FF);
        let e1 = _mm_set1_epi16(0x0080);
        let e2 = _mm_set1_epi16(0x0101);

        let mut i = 0;
        while i + 1 < n {
            // Load two src pixels
            let src = src.as_ptr().add(i).cast::<u64>();
            let src64 = core::ptr::read_unaligned(src);
            // [0,0,rgba,rgba] -> [rgba,rgba,rgba,rgba]
            let src128 = _mm_set1_epi64x(src64 as i64);

            let src_simd = _mm_cvtsi64_si128(src64 as i64);
            // [0,0,0,0,0,0,rgba,rgba] -> [r,g,b,a,r,g,b,a]
            let src_simd = _mm_cvtepu8_epi16(src_simd);

            // Broadcast alpha within each pixel's 4 lanes
            let a_broadcast_lo = _mm_shufflelo_epi16(src_simd, 0b11111111);
            let a_broadcast = _mm_shufflehi_epi16(a_broadcast_lo, 0b11111111);

            // simd_alpha_compl = 255 - A for each lane, per pixel
            let simd_alpha_compl = _mm_sub_epi16(ones, a_broadcast);

            // Load two dst pixels
            let dst = dst.as_mut_ptr().add(i).cast::<u64>();
            let d64 = core::ptr::read_unaligned(dst);
            let d128 = _mm_cvtsi64_si128(d64 as i64);
            let dst16 = _mm_cvtepu8_epi16(d128);

            // dst * alpha_compl + 0x0080008000800080
            let dst_term = _mm_mullo_epi16(dst16, simd_alpha_compl);
            let res16 = _mm_add_epi16(dst_term, e1);

            // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation. (can you see why?)
            let res16 = _mm_mulhi_epu16(res16, e2);
            let mut dst8 = _mm_packus_epi16(res16, res16); // Pack back to u8

            // dst.saturating_add(src)
            dst8 = _mm_adds_epu8(dst8, src128);

            let lo64 = _mm_cvtsi128_si64(dst8) as u64;
            core::ptr::write_unaligned(dst, lo64);
            i += 2;
        }

        if i < n {
            dst[i] = egui_blend_u8_sse41(src[i], dst[i]);
        }
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i] * vert, dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_slice_tinted_sse41(
    src: &[[u8; 4]],
    tint: [u8; 4],
    dst: &mut [[u8; 4]],
) {
    unsafe {
        assert_eq!(src.len(), dst.len());
        let n = dst.len();
        if n == 0 {
            return;
        }

        let ones = _mm_set1_epi16(0x00FF);
        let e1 = _mm_set1_epi16(0x0080);
        let e2 = _mm_set1_epi16(0x0101);

        let t32 = _mm_set1_epi32(i32::from_le_bytes(tint));
        let tint16 = _mm_cvtepu8_epi16(t32);

        let mut i = 0usize;
        while i + 1 < n {
            // Load two src pixels
            let src = src.as_ptr().add(i).cast::<u64>();
            let src64 = core::ptr::read_unaligned(src);

            let src_simd = _mm_cvtsi64_si128(src64 as i64);
            // [0,0,0,0,0,0,rgba,rgba] -> [r,g,b,a,r,g,b,a]
            let src_simd = _mm_cvtepu8_epi16(src_simd);

            // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
            let tint_mul = _mm_mullo_epi16(src_simd, tint16);
            let tint_rounded = _mm_add_epi16(tint_mul, e1);
            let src_tinted16 = _mm_mulhi_epu16(tint_rounded, e2);

            // Broadcast alpha within each pixel's 4 lanes
            let a_broadcast_lo = _mm_shufflelo_epi16(src_tinted16, 0b11111111);
            let a_broadcast = _mm_shufflehi_epi16(a_broadcast_lo, 0b11111111);

            // simd_alpha_compl = 255 - A for each lane, per pixel
            let simd_alpha_compl = _mm_sub_epi16(ones, a_broadcast);

            // Load two dst pixels
            let dst = dst.as_mut_ptr().add(i).cast::<u64>();
            let d64 = core::ptr::read_unaligned(dst);
            let d128 = _mm_cvtsi64_si128(d64 as i64);
            let dst16 = _mm_cvtepu8_epi16(d128);

            // dst * alpha_compl + 0x0080008000800080
            let dst_term = _mm_mullo_epi16(dst16, simd_alpha_compl);
            let res16 = _mm_add_epi16(dst_term, e1);

            // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation. (can you see why?)
            let res16 = _mm_mulhi_epu16(res16, e2);
            let mut dst8 = _mm_packus_epi16(res16, res16); // Pack back to u8

            // dst.saturating_add(src)
            let src_tinted8 = _mm_packus_epi16(src_tinted16, src_tinted16);
            dst8 = _mm_adds_epu8(dst8, src_tinted8);

            let lo64 = _mm_cvtsi128_si64(dst8) as u64;
            core::ptr::write_unaligned(dst, lo64);
            i += 2;
        }

        // Tail: handle the last pixel (if any) in scalar
        if i < n {
            dst[i] = egui_blend_u8_sse41(unorm_mult4x4_sse41(src[i], tint), dst[i]);
        }
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src * tint_fn(), dst[i]) // As unorm
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_slice_one_src_tinted_fn_sse41(
    src: [u8; 4],
    mut tint_fn: impl FnMut() -> [u8; 4],
    dst: &mut [[u8; 4]],
) {
    unsafe {
        let n = dst.len();
        if n == 0 {
            return;
        }

        let src32 = u32::from_le_bytes(src);
        let src64 = (src32 as u64) | ((src32 as u64) << 32);

        let ones = _mm_set1_epi16(0x00FF);
        let e1 = _mm_set1_epi16(0x0080);
        let e2 = _mm_set1_epi16(0x0101);

        let mut i = 0usize;
        while i + 1 < n {
            // Load two tint values
            let tint_a = u32::from_le_bytes(tint_fn()) as i64;
            let tint_b = u32::from_le_bytes(tint_fn()) as i64;
            let tint_simd = _mm_cvtsi64_si128((tint_b << 32) | tint_a);
            let tint16 = _mm_cvtepu8_epi16(tint_simd);

            let src_simd = _mm_cvtsi64_si128(src64 as i64);
            // [0,0,0,0,0,0,rgba,rgba] -> [r,g,b,a,r,g,b,a]
            let src_simd = _mm_cvtepu8_epi16(src_simd);

            // src_tinted = (src16 * vert16 + 128) * 257 >> 16  (rounded /255)
            let tint_mul = _mm_mullo_epi16(src_simd, tint16);
            let tint_rounded = _mm_add_epi16(tint_mul, e1);
            let src_tinted16 = _mm_mulhi_epu16(tint_rounded, e2);

            // Broadcast alpha within each pixel's 4 lanes
            let a_broadcast_lo = _mm_shufflelo_epi16(src_tinted16, 0b11111111);
            let a_broadcast = _mm_shufflehi_epi16(a_broadcast_lo, 0b11111111);

            // simd_alpha_compl = 255 - A for each lane, per pixel
            let simd_alpha_compl = _mm_sub_epi16(ones, a_broadcast);

            // Load two dst pixels
            let dst = dst.as_mut_ptr().add(i).cast::<u64>();
            let d64 = core::ptr::read_unaligned(dst);
            let d128 = _mm_cvtsi64_si128(d64 as i64);
            let dst16 = _mm_cvtepu8_epi16(d128);

            // dst * alpha_compl + 0x0080008000800080
            let dst_term = _mm_mullo_epi16(dst16, simd_alpha_compl);
            let res16 = _mm_add_epi16(dst_term, e1);

            // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation. (can you see why?)
            let res16 = _mm_mulhi_epu16(res16, e2);
            let mut dst8 = _mm_packus_epi16(res16, res16); // Pack back to u8

            // dst.saturating_add(src)
            let src_tinted8 = _mm_packus_epi16(src_tinted16, src_tinted16);
            dst8 = _mm_adds_epu8(dst8, src_tinted8);

            let lo64 = _mm_cvtsi128_si64(dst8) as u64;
            core::ptr::write_unaligned(dst, lo64);
            i += 2;
        }

        // Tail: handle the last pixel (if any) in scalar
        if i < n {
            dst[i] = egui_blend_u8_sse41(unorm_mult4x4_sse41(src, tint_fn()), dst[i]);
        }
    }
}

#[target_feature(enable = "sse4.1")]
pub unsafe fn unorm_mult4x4_sse41(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    let e1 = _mm_set1_epi16(0x0080);
    let a = _mm_cvtepu8_epi16(_mm_cvtsi32_si128(i32::from_le_bytes(a)));
    let b = _mm_cvtepu8_epi16(_mm_cvtsi32_si128(i32::from_le_bytes(b)));

    // a * b + 0x0080
    let mut dst = _mm_add_epi16(_mm_mullo_epi16(a, b), e1);

    // ((a >> 8) + a) >> 8
    dst = _mm_add_epi16(dst, _mm_srli_epi16(dst, 8));
    dst = _mm_srli_epi16(dst, 8);

    // Pack to back to u8
    let dst = _mm_packus_epi16(dst, _mm_setzero_si128());

    i32::to_le_bytes(_mm_cvtsi128_si32(dst)) // Return first element of dst
}
