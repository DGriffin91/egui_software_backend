use crate::vec4::{Vec4, vec4};

#[inline(always)]
pub fn vec4_to_u8x4_no_clamp(v: &Vec4) -> [u8; 4] {
    let v = v * 255.0 + 0.5;
    [v.x as u8, v.y as u8, v.z as u8, v.w as u8]
}

#[inline(always)]
pub fn u8x4_to_vec4(v: &[u8; 4]) -> Vec4 {
    vec4(
        v[0] as f32 / 255.0,
        v[1] as f32 / 255.0,
        v[2] as f32 / 255.0,
        v[3] as f32 / 255.0,
    )
}

// https://github.com/emilk/egui/blob/226bdc4c5bbb2230fb829e01b3fcb0460e741b34/crates/egui/src/lib.rs#L162
#[inline(always)]
pub fn egui_blend(src: &Vec4, dst: &Vec4) -> Vec4 {
    dst * (1.0 - src.w) + src
}

/// transforms 4 bytes RGBA into 8 bytes 0R0G0B0A
#[inline(always)]
pub fn as_color16(color: u32) -> u64 {
    let x = color as u64;
    let x = ((x & 0xFFFF0000) << 16) | (x & 0xFFFF);
    ((x & 0x0000FF000000FF00) << 8) | (x & 0x000000FF000000FF)
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
#[inline(always)]
pub fn egui_blend_u8(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    // TODO this subtly changes the shading. See on tables with alternating row backgrounds.
    // if src[3] == 0 {
    //     return dst;
    // }

    if src[3] == 255 {
        return src;
    }

    let alpha = src[3] as u64;
    let alpha_compl = 0xFF ^ alpha;
    let src = as_color16(u32::from_le_bytes(src));
    let dst = as_color16(u32::from_le_bytes(dst));

    let res16 = src * 0xFF + dst * alpha_compl + 0x0080008000800080;
    let res8 = res16 + ((res16 >> 8) & 0x00FF00FF00FF00FF);

    // transform the result back to 32 bytes
    let res = (res8 >> 8) & 0x00FF00FF00FF00FF;
    let res = (res | (res >> 8)) & 0x0000FFFF0000FFFF;
    let res = res | (res >> 16);
    u32::to_le_bytes((res & 0x00000000FFFFFFFFF) as u32)
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_slice_one_src_sse41(src: [u8; 4], dst: &mut [[u8; 4]]) {
    unsafe {
        use std::arch::x86_64 as intr;

        let n = dst.len();
        if n == 0 {
            return;
        }

        let src32 = u32::from_le_bytes(src);
        let src64 = (src32 as u64) | ((src32 as u64) << 32);

        let alpha = src[3] as i32;
        let alpha_compl = 0xFF ^ alpha;

        let ones = intr::_mm_set1_epi16(0x00FF);
        let e1 = intr::_mm_set1_epi16(0x0080);
        let e2 = intr::_mm_set1_epi16(0x0101);

        let src_simd = intr::_mm_cvtsi64_si128(src64 as i64);
        let src_simd = intr::_mm_cvtepu8_epi16(src_simd);

        // (255 - a) in all u16 lanes
        let simd_alpha_compl = intr::_mm_set1_epi16(alpha_compl as i16);

        let mut i = 0;
        while i + 1 < n {
            let dst = dst.as_mut_ptr().add(i).cast::<u64>();
            // Load two dst pixels
            let d64 = core::ptr::read_unaligned(dst);
            let d128 = intr::_mm_cvtsi64_si128(d64 as i64);
            let dst16 = intr::_mm_cvtepu8_epi16(d128);

            // src * 0xFF + dst * alpha_compl + 0x0080008000800080
            let src_term = intr::_mm_mullo_epi16(src_simd, ones);
            let dst_term = intr::_mm_mullo_epi16(dst16, simd_alpha_compl);
            let res16 = intr::_mm_add_epi16(intr::_mm_add_epi16(src_term, dst_term), e1);

            // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation.
            // (can you see why?)
            let res16 = intr::_mm_mulhi_epu16(res16, e2);
            let final_i = intr::_mm_packus_epi16(res16, res16); // RGBA for two pixels

            let lo64 = intr::_mm_cvtsi128_si64(final_i) as u64;
            core::ptr::write_unaligned(dst, lo64);
            i += 2;
        }

        if i < n {
            dst[i] = egui_blend_u8(src, dst[i]);
        }
    }
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// dst[i] = blend(src[i], dst[i]) // As unorm
#[target_feature(enable = "sse4.1")]
pub unsafe fn egui_blend_u8_slice_sse41(src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
    unsafe {
        use std::arch::x86_64 as intr;
        assert_eq!(src.len(), dst.len());

        let n = dst.len();
        if n == 0 {
            return;
        }

        let ones = intr::_mm_set1_epi16(0x00FF);
        let e1 = intr::_mm_set1_epi16(0x0080);
        let e2 = intr::_mm_set1_epi16(0x0101);

        let mut i = 0;
        while i + 1 < n {
            // Load two src pixels
            let src = src.as_ptr().add(i).cast::<u64>();
            let src64 = core::ptr::read_unaligned(src);

            let src_simd = intr::_mm_cvtsi64_si128(src64 as i64);
            let src_simd = intr::_mm_cvtepu8_epi16(src_simd);

            // Broadcast alpha within each pixel's 4 lanes
            let a_broadcast_lo = intr::_mm_shufflelo_epi16(src_simd, 0b11111111);
            let a_broadcast = intr::_mm_shufflehi_epi16(a_broadcast_lo, 0b11111111);

            // simd_alpha_compl = 255 - A for each lane, per pixel
            let simd_alpha_compl = intr::_mm_sub_epi16(ones, a_broadcast);

            // Load two dst pixels
            let dst = dst.as_mut_ptr().add(i).cast::<u64>();
            let d64 = core::ptr::read_unaligned(dst);
            let d128 = intr::_mm_cvtsi64_si128(d64 as i64);
            let dst16 = intr::_mm_cvtepu8_epi16(d128);

            // src * 0xFF + dst * alpha_compl + 0x0080008000800080
            let src_term = intr::_mm_mullo_epi16(src_simd, ones);
            let dst_term = intr::_mm_mullo_epi16(dst16, simd_alpha_compl);
            let res16 = intr::_mm_add_epi16(intr::_mm_add_epi16(src_term, dst_term), e1);

            // This mulhi is equivalent to the ((x >> 8) + x) >> 8 operation.
            // (can you see why?)
            let res16 = intr::_mm_mulhi_epu16(res16, e2);
            let final_i = intr::_mm_packus_epi16(res16, res16); // RGBA for two pixels

            let lo64 = intr::_mm_cvtsi128_si64(final_i) as u64;
            core::ptr::write_unaligned(dst, lo64);
            i += 2;
        }

        if i < n {
            dst[i] = egui_blend_u8(src[i], dst[i]);
        }
    }
}

#[inline(always)]
pub fn swizzle_rgba_bgra(a: [u8; 4]) -> [u8; 4] {
    [a[2], a[1], a[0], a[3]]
}

// TODO perf: optimize
#[inline(always)]
pub fn unorm_mult4x4(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    [
        unorm_mult(a[0] as u32, b[0] as u32) as u8,
        unorm_mult(a[1] as u32, b[1] as u32) as u8,
        unorm_mult(a[2] as u32, b[2] as u32) as u8,
        unorm_mult(a[3] as u32, b[3] as u32) as u8,
    ]
}

#[inline(always)]
// Jerry R. Van Aken - Alpha Blending with No Division Operations https://arxiv.org/pdf/2202.02864
// Input should be 0..255, is multiplied as if it were 0..1f
pub fn unorm_mult(mut a: u32, b: u32) -> u32 {
    a *= b;
    a += 0x80;
    a += a >> 8;
    a >> 8
}
