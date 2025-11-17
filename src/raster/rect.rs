use constify::constify;
use egui::{Vec2, vec2};

use crate::{
    BufferMutRef,
    color::{egui_blend_u8, unorm_mult4x4},
    egui_texture::EguiTexture,
    render::DrawInfo,
};

#[constify]
pub fn draw_rect(
    buffer: &mut BufferMutRef,
    texture: &EguiTexture,
    draw: &DrawInfo,
    #[constify] vert_col_vary: bool,
    #[constify] vert_uvs_vary: bool,
    #[constify] alpha_blend: bool,
    #[constify] simd: bool,
) {
    let const_tri_color_u8x4 = draw.const_tri_color_u8x4;
    let clip_bounds = &draw.clip_bounds;
    let tri_min = draw.tri_min;
    let tri_max = draw.tri_max;
    let min_x = ((tri_min.x + 0.5) as i64).max(clip_bounds[0].x);
    let min_y = ((tri_min.y + 0.5) as i64).max(clip_bounds[0].y);
    let max_x = ((tri_max.x + 0.5) as i64).min(clip_bounds[1].x);
    let max_y = ((tri_max.y + 0.5) as i64).min(clip_bounds[1].y);

    let sizex = max_x - min_x;
    let sizey = max_y - min_y;

    if sizex <= 0 || sizey <= 0 {
        return;
    }

    let min_x = min_x as usize;
    let min_y = min_y as usize;
    let max_x = max_x as usize;
    let max_y = max_y as usize;

    if !vert_uvs_vary && !vert_col_vary {
        for y in min_y..max_y {
            if alpha_blend {
                if simd {
                    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
                    {
                        #[cfg(target_arch = "x86_64")]
                        unsafe {
                            crate::color_sse41::egui_blend_u8_slice_one_src(
                                const_tri_color_u8x4,
                                buffer.get_mut_span(min_x, max_x, y),
                            )
                        }
                        #[cfg(target_arch = "aarch64")]
                        crate::color_neon::egui_blend_u8_slice_one_src(
                            const_tri_color_u8x4,
                            buffer.get_mut_span(min_x, max_x, y),
                        )
                    }
                } else {
                    for pixel in buffer.get_mut_span(min_x, max_x, y) {
                        *pixel = egui_blend_u8(const_tri_color_u8x4, *pixel);
                    }
                }
            } else {
                buffer
                    .get_mut_span(min_x, max_x, y)
                    .fill(const_tri_color_u8x4);
            }
        }
    } else {
        // TODO could another level of constify make this cleaner (const use_nearest_sampling?)
        let mut min_uv = vec2(
            draw.uv[0].x.min(draw.uv[1].x).min(draw.uv[2].x),
            draw.uv[0].y.min(draw.uv[1].y).min(draw.uv[2].y),
        );
        let max_uv = vec2(
            draw.uv[0].x.max(draw.uv[1].x).max(draw.uv[2].x),
            draw.uv[0].y.max(draw.uv[1].y).max(draw.uv[2].y),
        );

        let uv_step = (max_uv - min_uv) / (tri_max - tri_min);
        min_uv += uv_step * (vec2(min_x as f32, min_y as f32) - tri_min).max(Vec2::ZERO); // Offset to account for clip
        min_uv += uv_step * 0.5; // Raster at pixel centers

        let ts_min = min_uv * texture.fsize;
        let ts_max = max_uv * texture.fsize;

        let use_nearest_sampling = {
            let ss_step = uv_step * texture.fsize;
            let dist_from_px_center = (ts_min - ts_min.floor() - vec2(0.5, 0.5)).abs();
            let steps_off_from_1px = (ss_step - Vec2::splat(1.0)).abs();
            let eps = 0.01;
            let steps_are_1px = steps_off_from_1px.x < eps && steps_off_from_1px.y < eps;
            let start_on_texture_px_center =
                dist_from_px_center.x < eps && dist_from_px_center.y < eps;

            steps_are_1px && start_on_texture_px_center
        };

        let no_texture_wrap_or_overflow =
            (ts_max.x as usize) < texture.width && (ts_max.y as usize) < texture.height;

        if use_nearest_sampling && no_texture_wrap_or_overflow {
            // Can just directly blend the texture over the dst buffer, no need to sample with uv
            let min_uv = [ts_min.x as i32, ts_min.y as i32];
            let mut tex_row = min_uv[1];
            for y in min_y..max_y {
                let tex_row_start = tex_row as usize * texture.width;
                let tex_start = tex_row_start + min_uv[0] as usize;
                let tex_end = tex_start + max_x - min_x;

                let dst = &mut buffer.get_mut_span(min_x, max_x, y);
                let src = &texture.data[tex_start..tex_end];

                if simd {
                    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
                    {
                        #[cfg(target_arch = "x86_64")]
                        unsafe {
                            crate::color_sse41::egui_blend_u8_slice_tinted(
                                src,
                                draw.const_vert_color_u8x4,
                                dst,
                            )
                        }
                        #[cfg(target_arch = "aarch64")]
                        crate::color_neon::egui_blend_u8_slice_tinted(
                            src,
                            draw.const_vert_color_u8x4,
                            dst,
                        );
                    }
                } else {
                    for (pixel, tex_color) in dst.iter_mut().zip(src) {
                        *pixel = egui_blend_u8(
                            unorm_mult4x4(draw.const_vert_color_u8x4, *tex_color),
                            *pixel,
                        );
                    }
                }
                tex_row += 1;
            }
        } else {
            // There's overflow or can't use nearest. So we need to do full sample.
            // TODO perf: there could be a situation where we could nearest sample but with wrapping/clipping.
            // We don't have a fast path for that and are falling back to the more general solution below.
            // This would be (use_nearest_sampling && !no_texture_wrap_or_overflow) which at least never occurs in the
            // demo as far as I can tell
            let mut uv = min_uv;
            for y in min_y..max_y {
                uv.x = min_uv.x;
                let buf_y = y * buffer.width;
                for x in min_x..max_x {
                    let tex_color = texture.sample_bilinear(uv);
                    let pixel = &mut buffer.data[x + buf_y];
                    let src = unorm_mult4x4(draw.const_vert_color_u8x4, tex_color);
                    *pixel = egui_blend_u8(src, *pixel);
                    uv.x += uv_step.x;
                }
                uv.y += uv_step.y;
            }
        }
    };
}
