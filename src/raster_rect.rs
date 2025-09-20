use egui::{Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef,
    egui_texture::{EguiTexture, egui_blend_u8, unorm_mult4x4},
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

pub fn draw_solid_rect(
    buffer: &mut BufferMutRef,
    const_tri_color_u8x4: [u8; 4],
    clip_bounds: &[i32; 4],
    tri_min: Vec2,
    tri_max: Vec2,
    requires_alpha_blending: bool,
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

    if requires_alpha_blending {
        for y in min_y..max_y {
            let row_start = y * buffer.width;
            let start = row_start + min_x;
            let end = row_start + max_x;
            for pixel in &mut buffer.data[start..end] {
                *pixel = egui_blend_u8(const_tri_color_u8x4, *pixel);
            }
        }
    } else {
        for y in min_y..max_y {
            let row_start = y * buffer.width;
            let start = row_start + min_x;
            let end = row_start + max_x;
            buffer.data[start..end].fill(const_tri_color_u8x4);
        }
    }
}
