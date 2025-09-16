use egui::{TextureOptions, Vec2};

use crate::vec4::{Vec4, vec4};

pub struct EguiTexture {
    pub data: Vec<[u8; 4]>,
    /// width - 1
    pub width_extent: i32,
    /// height - 1
    pub height_extent: i32,
    pub width: usize,
    pub height: usize,
    pub fsize: Vec2,
    pub options: TextureOptions,
}

impl EguiTexture {
    pub fn sample_nearest(&self, uv: Vec2) -> [u8; 4] {
        let ss_x = ((uv.x * self.fsize.x) as i32).max(0).min(self.width_extent);
        let ss_y = ((uv.y * self.fsize.y) as i32)
            .max(0)
            .min(self.height_extent);
        self.data[ss_x as usize + ss_y as usize * self.width]
    }

    pub fn sample_bilinear(&self, uv: Vec2) -> [u8; 4] {
        let w = self.fsize.x as f32;
        let h = self.fsize.y as f32;

        let sx = uv.x * w - 0.5;
        let sy = uv.y * h - 0.5;

        let x0 = sx.floor() as i32;
        let y0 = sy.floor() as i32;
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let fx = sx - x0 as f32;
        let fy = sy - y0 as f32;

        let x0c = x0.max(0).min(self.width_extent);
        let y0c = y0.max(0).min(self.height_extent);
        let x1c = x1.max(0).min(self.width_extent);
        let y1c = y1.max(0).min(self.height_extent);

        let c00 = self.data[(x0c as usize) + (y0c as usize) * self.width];

        if fx == 0.0 && fy == 0.0 {
            // if these are 0 the px at 0,0 will have full influence. Equivalent to nearest sampling.
            return c00;
        }

        let c10 = self.data[(x1c as usize) + (y0c as usize) * self.width];
        let c01 = self.data[(x0c as usize) + (y1c as usize) * self.width];
        let c11 = self.data[(x1c as usize) + (y1c as usize) * self.width];

        let v00 = u8x4_to_vec4(&c00);
        let v10 = u8x4_to_vec4(&c10);
        let v01 = u8x4_to_vec4(&c01);
        let v11 = u8x4_to_vec4(&c11);

        let w00 = (1.0 - fx) * (1.0 - fy);
        let w10 = fx * (1.0 - fy);
        let w01 = (1.0 - fx) * fy;
        let w11 = fx * fy;

        vec4_to_u8x4_no_clamp(&(v00 * w00 + v01 * w01 + v10 * w10 + v11 * w11))
    }
}

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
    let new = as_color16(u32::from_le_bytes(src));
    let orig = as_color16(u32::from_le_bytes(dst));
    let src_alpha = 0xFF;

    let res16 = new * src_alpha + orig * alpha_compl + 0x0080008000800080;
    let res8 = res16 + ((res16 >> 8) & 0x00FF00FF00FF00FF);

    // transform the result back to 32 bytes
    let res = (res8 >> 8) & 0x00FF00FF00FF00FF;
    let res = (res | (res >> 8)) & 0x0000FFFF0000FFFF;
    let res = res | (res >> 16);
    u32::to_le_bytes((res & 0x00000000FFFFFFFFF) as u32)
}

#[inline(always)]
pub fn egui_blend_u8_old(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    // TODO this subtly changes the shading. See on tables with alternating row backgrounds.
    // if src[3] == 0 {
    //     return dst;
    // }
    if src[3] == 255 {
        return src;
    }

    let mut c = unorm_mult4x1(
        [dst[0] as u32, dst[1] as u32, dst[2] as u32, dst[3] as u32],
        255u32.saturating_sub(src[3] as u32),
    );
    c = [
        c[0] + (src[0] as u32),
        c[1] + (src[1] as u32),
        c[2] + (src[2] as u32),
        c[3] + (src[3] as u32),
    ];
    [
        (c[0].min(255) as u8),
        (c[1].min(255) as u8),
        (c[2].min(255) as u8),
        (c[3].min(255) as u8),
    ]
}

#[inline(always)]
pub fn unorm_mult4x1(a: [u32; 4], b: u32) -> [u32; 4] {
    [
        unorm_mult(a[0], b),
        unorm_mult(a[1], b),
        unorm_mult(a[2], b),
        unorm_mult(a[3], b),
    ]
}

#[inline(always)]
// Jerry R. Van Aken - Alpha Blending with No Division Operations https://arxiv.org/pdf/2202.02864
// Input should be 0..255, is multiplied as if it were 0..1f
pub fn unorm_mult(mut a: u32, mut b: u32) -> u32 {
    b |= b << 8;
    a *= b;
    a += 0x8080;
    return a >> 16;
}

#[inline(always)]
pub fn swizzle_rgba_bgra(a: [u8; 4]) -> [u8; 4] {
    [a[2], a[1], a[0], a[3]]
}
