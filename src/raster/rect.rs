use egui::{Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef,
    color::{egui_blend_u8, unorm_mult4x4},
    egui_texture::EguiTexture,
    sse41,
};

pub fn draw_textured_rect(
    buffer: &mut BufferMutRef,
    const_vert_color_u8x4: [u8; 4],
    texture: &EguiTexture,
    clip_bounds: &[i32; 4],
    tri_min: Vec2,
    tri_max: Vec2,
    tri: &[Vertex; 3],
) {
    let min_x = ((tri_min.x + 0.5) as i32).max(clip_bounds[0]);
    let min_y = ((tri_min.y + 0.5) as i32).max(clip_bounds[1]);
    let max_x = ((tri_max.x + 0.5) as i32).min(clip_bounds[2]);
    let max_y = ((tri_max.y + 0.5) as i32).min(clip_bounds[3]);

    let sizex = max_x - min_x;
    let sizey = max_y - min_y;

    if sizex <= 0 || sizey <= 0 {
        return;
    }

    let min_x = min_x as usize;
    let min_y = min_y as usize;
    let max_x = max_x as usize;
    let max_y = max_y as usize;

    let mut min_uv = vec2(
        tri[0].uv.x.min(tri[1].uv.x).min(tri[2].uv.x),
        tri[0].uv.y.min(tri[1].uv.y).min(tri[2].uv.y),
    );
    let max_uv = vec2(
        tri[0].uv.x.max(tri[1].uv.x).max(tri[2].uv.x),
        tri[0].uv.y.max(tri[1].uv.y).max(tri[2].uv.y),
    );

    let uv_step = (max_uv - min_uv) / (tri_max - tri_min);
    min_uv += uv_step * (vec2(min_x as f32, min_y as f32) - tri_min).max(Vec2::ZERO); // Offset to account for clip
    min_uv += uv_step * 0.5; // Raster at pixel centers

    let ts_min = min_uv * texture.fsize;
    let ts_max = max_uv * texture.fsize;

    let use_nearest_sampling = {
        let ss_step = uv_step * texture.fsize;
        let dist_from_px_center = (ts_min - ts_min.floor() - vec2(0.5, 0.5)).abs();
        let steps_off_from_1px = (ss_step - Vec2::ONE).abs();
        let eps = 0.01;
        let steps_are_1px = steps_off_from_1px.x < eps && steps_off_from_1px.y < eps;
        let start_on_texture_px_center = dist_from_px_center.x < eps && dist_from_px_center.y < eps;

        steps_are_1px && start_on_texture_px_center
    };

    if use_nearest_sampling {
        if sse41() && (ts_max.x as usize) < texture.width && (ts_max.y as usize) < texture.height {
            #[cfg(target_arch = "x86_64")]
            {
                let min_uv = [ts_min.x as i32, ts_min.y as i32];
                let mut tex_row = min_uv[1];
                for y in min_y..max_y {
                    let tex_row_start = tex_row as usize * texture.width;
                    let tex_start = tex_row_start + min_uv[0] as usize;
                    let tex_end = tex_start + max_x - min_x;

                    let dst = &mut buffer.get_mut_span(min_x, max_x, y);
                    let src = &texture.data[tex_start..tex_end];

                    // SAFETY: we first check sse41() outside the loop
                    unsafe {
                        crate::color_x86_64_simd::egui_blend_u8_slice_tinted_sse41(
                            src,
                            const_vert_color_u8x4,
                            dst,
                        )
                    };
                    tex_row += 1;
                }
            }
        } else {
            let min_uv = [ts_min.x as i32, ts_min.y as i32];
            let mut uv = min_uv;
            for y in min_y..max_y {
                uv[0] = min_uv[0];
                let buf_y = y * buffer.width;
                for x in min_x..max_x {
                    let tex_color = texture.get(uv);
                    let pixel = &mut buffer.data[x + buf_y];
                    *pixel = egui_blend_u8(unorm_mult4x4(const_vert_color_u8x4, tex_color), *pixel);
                    uv[0] += 1;
                }
                uv[1] += 1;
            }
        }
    } else {
        let mut uv = min_uv;
        for y in min_y..max_y {
            uv.x = min_uv.x;
            let buf_y = y * buffer.width;
            for x in min_x..max_x {
                let tex_color = texture.sample_bilinear(uv);
                let pixel = &mut buffer.data[x + buf_y];
                let src = unorm_mult4x4(const_vert_color_u8x4, tex_color);
                *pixel = egui_blend_u8(src, *pixel);
                uv.x += uv_step.x;
            }
            uv.y += uv_step.y;
        }
    }
}

pub fn draw_solid_rect(
    buffer: &mut BufferMutRef,
    const_tri_color_u8x4: [u8; 4],
    clip_bounds: &[i32; 4],
    tri_min: Vec2,
    tri_max: Vec2,
    alpha_blend: bool,
) {
    let min_x = ((tri_min.x + 0.5) as i32).max(clip_bounds[0]);
    let min_y = ((tri_min.y + 0.5) as i32).max(clip_bounds[1]);
    let max_x = ((tri_max.x + 0.5) as i32).min(clip_bounds[2]);
    let max_y = ((tri_max.y + 0.5) as i32).min(clip_bounds[3]);

    let sizex = max_x - min_x;
    let sizey = max_y - min_y;

    if sizex <= 0 || sizey <= 0 {
        return;
    }

    let min_x = min_x as usize;
    let min_y = min_y as usize;
    let max_x = max_x as usize;
    let max_y = max_y as usize;

    if alpha_blend {
        if sse41() {
            #[cfg(target_arch = "x86_64")]
            for y in min_y..max_y {
                // SAFETY: we first check sse41() outside the loop
                unsafe {
                    crate::color_x86_64_simd::egui_blend_u8_slice_one_src_sse41(
                        const_tri_color_u8x4,
                        buffer.get_mut_span(min_x, max_x, y),
                    )
                }
            }
        } else {
            for y in min_y..max_y {
                for pixel in buffer.get_mut_span(min_x, max_x, y) {
                    *pixel = egui_blend_u8(const_tri_color_u8x4, *pixel);
                }
            }
        }
    } else {
        for y in min_y..max_y {
            buffer
                .get_mut_span(min_x, max_x, y)
                .fill(const_tri_color_u8x4);
        }
    }
}
