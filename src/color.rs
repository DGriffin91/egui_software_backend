use crate::math::vec4::{Vec4, vec4};

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
#[allow(dead_code)]
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
/// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
#[inline(always)]
pub fn egui_blend_u8(src: [u8; 4], mut dst: [u8; 4]) -> [u8; 4] {
    let a = src[3];
    if a == 255 {
        return src;
    }
    if a != 0 {
        let alpha = a as u64;
        let alpha_compl = 0xFF ^ alpha;
        let dst64 = as_color16(u32::from_le_bytes(dst));

        let res16 = dst64 * alpha_compl + 0x0080008000800080;
        let res8 = res16 + ((res16 >> 8) & 0x00FF00FF00FF00FF);

        // transform the result back to 32 bytes
        let res = (res8 >> 8) & 0x00FF00FF00FF00FF;
        let res = (res | (res >> 8)) & 0x0000FFFF0000FFFF;
        let res = res | (res >> 16);
        dst = u32::to_le_bytes((res & 0x00000000FFFFFFFF) as u32);
    }

    [
        dst[0].saturating_add(src[0]),
        dst[1].saturating_add(src[1]),
        dst[2].saturating_add(src[2]),
        dst[3].saturating_add(src[3]),
    ]
}

// https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
/// colors in egui aren't always actually premultiplied which can result in overflow in this faster version
#[inline(always)]
#[allow(dead_code)]
pub fn egui_blend_u8_fast(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let a = src[3];
    if a == 255 {
        return src;
    }

    let alpha = a as u64;
    let alpha_compl = 0xFF ^ alpha;
    let src = as_color16(u32::from_le_bytes(src));
    let dst = as_color16(u32::from_le_bytes(dst));

    let res16 = src * 0xFF + dst * alpha_compl + 0x0080008000800080;
    let res8 = res16 + ((res16 >> 8) & 0x00FF00FF00FF00FF);

    // transform the result back to 32 bytes
    let res = (res8 >> 8) & 0x00FF00FF00FF00FF;
    let res = (res | (res >> 8)) & 0x0000FFFF0000FFFF;
    let res = res | (res >> 16);
    u32::to_le_bytes((res & 0x00000000FFFFFFFF) as u32)
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
