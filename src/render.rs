use std::collections::HashMap;

use bytemuck::cast_slice_mut;
use egui::{Pos2, Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef, EguiTexture,
    color::{
        egui_blend, egui_blend_u8, egui_blend_u8_slice_one_src_sse41, u8x4_to_vec4, unorm_mult4x4,
        vec4_to_u8x4_no_clamp,
    },
    math::vec4::Vec4,
    raster_bary::{bary, raster_tri_with_bary, raster_tri_with_colors, raster_tri_with_uv},
    raster_rect::{draw_solid_rect, draw_textured_rect},
    raster_span::raster_tri_span,
};

pub fn draw_egui_mesh<const SUBPIX_BITS: i32>(
    textures: &HashMap<egui::TextureId, EguiTexture>,
    buffer: &mut BufferMutRef,
    convert_tris_to_rects: bool,
    clip_rect: &egui::Rect,
    mesh: &egui::Mesh,
    vert_offset: Vec2,
    allow_raster_opt: bool,
) {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return;
    }

    let Some(texture) = textures.get(&mesh.texture_id) else {
        return;
    };

    if texture.options.magnification != texture.options.minification {
        todo!(); // Warn? Would need helper lanes to impl?
    }

    let indices = &mesh.indices;
    let vertices = &mesh.vertices;
    let clip_x = clip_rect.min.x as i32;
    let clip_y = clip_rect.min.y as i32;

    let clip_width = (clip_rect.max.x - clip_rect.min.x + 0.5) as i32;
    let clip_height = (clip_rect.max.y - clip_rect.min.y + 0.5) as i32;

    let clip_bounds = [
        clip_x.clamp(0, buffer.width as i32),
        clip_y.clamp(0, buffer.height as i32),
        (clip_x + clip_width).clamp(0, buffer.width as i32),
        (clip_y + clip_height).clamp(0, buffer.height as i32),
    ];

    if clip_bounds[2] - clip_bounds[0] <= 0 || clip_bounds[3] - clip_bounds[1] <= 0 {
        return;
    }

    let mut i = 0;
    // Get texture
    while i < indices.len() {
        let mut const_tex_color = Vec4::ONE;
        let mut const_tex_color_u8x4 = [255; 4];
        let mut const_vert_color = Vec4::ONE;
        let mut const_vert_color_u8x4 = [255; 4];
        let mut const_tri_color_u8x4 = [255; 4];

        let mut tri = [
            vertices[indices[i] as usize],
            vertices[indices[i + 1] as usize],
            vertices[indices[i + 2] as usize],
        ];
        tri[0].pos += vert_offset;
        tri[1].pos += vert_offset;
        tri[2].pos += vert_offset;

        let tri_min = vec2(
            tri[0].pos.x.min(tri[1].pos.x).min(tri[2].pos.x),
            tri[0].pos.y.min(tri[1].pos.y).min(tri[2].pos.y),
        );
        let tri_max = vec2(
            tri[0].pos.x.max(tri[1].pos.x).max(tri[2].pos.x),
            tri[0].pos.y.max(tri[1].pos.y).max(tri[2].pos.y),
        );

        let fsize = tri_max - tri_min;
        if fsize.x <= 0.0 || fsize.y <= 0.0 {
            i += 3;
            continue;
        }

        let color0_u8x4 = tri[0].color.to_array();
        let color1_u8x4 = tri[1].color.to_array();
        let color2_u8x4 = tri[2].color.to_array();

        let colors = [
            u8x4_to_vec4(&color0_u8x4),
            u8x4_to_vec4(&color1_u8x4),
            u8x4_to_vec4(&color2_u8x4),
        ];

        let scr_tri = [
            tri[0].pos.to_vec2(),
            tri[1].pos.to_vec2(),
            tri[2].pos.to_vec2(),
        ];

        let uv = [
            vec2(tri[0].uv.x, tri[0].uv.y),
            vec2(tri[1].uv.x, tri[1].uv.y),
            vec2(tri[2].uv.x, tri[2].uv.y),
        ];

        if !allow_raster_opt {
            raster_tri_with_bary::<SUBPIX_BITS>(clip_bounds, &scr_tri, |x, y, w0, w1, inv_area| {
                let (b0, b1, b2) = bary(w0, w1, inv_area);
                let uv = b0 * uv[0] + b1 * uv[1] + b2 * uv[2];
                let tex_color = u8x4_to_vec4(&texture.sample_bilinear(uv));
                let vert_color = b0 * colors[0] + b1 * colors[1] + b2 * colors[2];
                let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                let dst = u8x4_to_vec4(pixel);
                let src = vert_color * tex_color;
                *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
            });
            i += 3;
            continue;
        }

        let uvs_match = uv[0] == uv[1] && uv[0] == uv[2];
        let colors_match = color0_u8x4 == color1_u8x4 && color0_u8x4 == color2_u8x4;
        let mut requires_alpha_blending = true;

        if uvs_match {
            const_tex_color_u8x4 = texture.sample_bilinear(uv[0]);
            const_tex_color = u8x4_to_vec4(&const_tex_color_u8x4);
        }

        if colors_match {
            const_vert_color = colors[0];
            const_vert_color_u8x4 = vec4_to_u8x4_no_clamp(&const_vert_color);
        }

        if uvs_match && colors_match {
            let const_tri_color = const_vert_color * const_tex_color;
            const_tri_color_u8x4 = vec4_to_u8x4_no_clamp(&const_tri_color);
            if const_tri_color_u8x4[3] == 255 {
                requires_alpha_blending = false;
            }
        }

        if uvs_match
            && !colors_match
            && const_tex_color_u8x4[3] == 255
            && color0_u8x4[3] == 255
            && color1_u8x4[3] == 255
            && color2_u8x4[3] == 255
        {
            requires_alpha_blending = false;
        }

        let mut tri2_uvs_match = false;
        let mut tri2_colors_match = false;
        let find_rects = convert_tris_to_rects && colors_match && i + 6 < indices.len();
        let mut found_rect = false;

        if find_rects {
            let mut tri2 = [
                vertices[indices[i + 3] as usize],
                vertices[indices[i + 4] as usize],
                vertices[indices[i + 5] as usize],
            ];
            tri2[0].pos += vert_offset;
            tri2[1].pos += vert_offset;
            tri2[2].pos += vert_offset;

            found_rect = tri_verts_match_corners(tri_min, tri_max, tri, tri2);

            if found_rect {
                let tri_area = egui_orient2df(&tri[0].pos, &tri[1].pos, &tri[2].pos).abs();
                let rect_area = (tri_max.x - tri_min.x) * (tri_max.y - tri_min.y);
                let areas_match = (tri_area - rect_area).abs() < 0.5;

                if areas_match {
                    if rect_area.abs() < 0.25 {
                        i += 6; // Skip both tris
                        continue; // early out of rects smaller than quarter px
                    }

                    if uvs_match {
                        tri2_uvs_match = tri[0].uv == tri2[0].uv
                            && tri[0].uv == tri2[1].uv
                            && tri[0].uv == tri2[2].uv;
                    }

                    if colors_match {
                        tri2_colors_match = tri[0].color == tri2[0].color
                            && tri[0].color == tri2[1].color
                            && tri[0].color == tri2[2].color;
                    }
                } else {
                    found_rect = false;
                }
            }
        }

        if uvs_match && colors_match {
            if found_rect && tri2_uvs_match && tri2_colors_match {
                draw_solid_rect(
                    buffer,
                    const_tri_color_u8x4,
                    &clip_bounds,
                    tri_min,
                    tri_max,
                    requires_alpha_blending,
                );
                i += 6; // Skip both tris
                continue;
            } else if requires_alpha_blending {
                if is_x86_feature_detected!("sse4.1") {
                    raster_tri_span::<SUBPIX_BITS>(clip_bounds, &scr_tri, |start, end, y| {
                        let row_start = y as usize * buffer.width;
                        let start = row_start + start as usize;
                        let end = row_start + end as usize;
                        let dst = cast_slice_mut(&mut buffer.data[start..end]);
                        // SAFETY: we first check is_x86_feature_detected!("sse4.1") outside the loop
                        unsafe { egui_blend_u8_slice_one_src_sse41(const_tri_color_u8x4, dst) }
                    });
                } else {
                    raster_tri_span::<SUBPIX_BITS>(clip_bounds, &scr_tri, |start, end, y| {
                        let row_start = y as usize * buffer.width;
                        let start = row_start + start as usize;
                        let end = row_start + end as usize;
                        for pixel in &mut buffer.data[start..end] {
                            *pixel = egui_blend_u8(const_tri_color_u8x4, *pixel);
                        }
                    });
                }
            } else {
                raster_tri_span::<SUBPIX_BITS>(clip_bounds, &scr_tri, |start, end, y| {
                    let row_start = y as usize * buffer.width;
                    let start = row_start + start as usize;
                    let end = row_start + end as usize;
                    buffer.data[start..end].fill(const_tri_color_u8x4)
                });
            }
        } else if uvs_match {
            // if uvs match but colors don't match
            if requires_alpha_blending {
                raster_tri_with_colors::<SUBPIX_BITS>(
                    clip_bounds,
                    &scr_tri,
                    &colors,
                    |x, y, vert_color| {
                        let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                        let dst = u8x4_to_vec4(pixel);
                        let src = vert_color * const_tex_color;
                        *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
                    },
                );
            } else {
                raster_tri_with_colors::<SUBPIX_BITS>(
                    clip_bounds,
                    &scr_tri,
                    &colors,
                    |x, y, vert_color| {
                        let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                        let src = vert_color * const_tex_color;
                        *pixel = vec4_to_u8x4_no_clamp(&src);
                    },
                );
            }
        } else if colors_match {
            // if colors match but uvs don't match

            if found_rect && tri2_colors_match {
                draw_textured_rect(
                    buffer,
                    const_vert_color_u8x4,
                    texture,
                    &clip_bounds,
                    tri_min,
                    tri_max,
                    &tri,
                );
                i += 6; // Skip both tris
                continue;
            } else {
                raster_tri_with_uv::<SUBPIX_BITS>(clip_bounds, &scr_tri, &uv, |x, y, uv| {
                    let tex_color = texture.sample_bilinear(uv);
                    let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                    let src = unorm_mult4x4(const_vert_color_u8x4, tex_color);
                    *pixel = egui_blend_u8(src, *pixel);
                });
            }
        } else {
            // Unique colors and uvs, didn't find a rect.
            // This is the standard full version sans shortcuts. Everything could be rendered using just this.
            raster_tri_with_bary::<SUBPIX_BITS>(clip_bounds, &scr_tri, |x, y, w0, w1, inv_area| {
                let (b0, b1, b2) = bary(w0, w1, inv_area);
                let uv = b0 * uv[0] + b1 * uv[1] + b2 * uv[2];
                let tex_color = u8x4_to_vec4(&texture.sample_bilinear(uv));
                let vert_color = b0 * colors[0] + b1 * colors[1] + b2 * colors[2];
                let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                let dst = u8x4_to_vec4(pixel);
                let src = vert_color * tex_color;
                *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
            });
        }

        i += 3;
    }
}

#[inline(always)]
pub fn egui_orient2df(a: &Pos2, b: &Pos2, c: &Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn tri_verts_match_corners(
    tri_min: Vec2,
    tri_max: Vec2,
    tri: [Vertex; 3],
    tri2: [Vertex; 3],
) -> bool {
    #[inline(always)]
    fn close(a: f32, b: f32) -> bool {
        //(a - b).abs() <= 0.1
        a == b
    }

    // https://github.com/emilk/imgui_software_renderer/blob/b5ae63a9e42eccf7db3bf64696761a53424c53dd/src/imgui_sw.cpp#L577
    (close(tri[0].pos.x, tri_min.x) || close(tri[0].pos.x, tri_max.x))
        && (close(tri[0].pos.y, tri_min.y) || close(tri[0].pos.y, tri_max.y))
        && (close(tri[1].pos.x, tri_min.x) || close(tri[1].pos.x, tri_max.x))
        && (close(tri[1].pos.y, tri_min.y) || close(tri[1].pos.y, tri_max.y))
        && (close(tri[2].pos.x, tri_min.x) || close(tri[2].pos.x, tri_max.x))
        && (close(tri[2].pos.y, tri_min.y) || close(tri[2].pos.y, tri_max.y))
        && (close(tri2[0].pos.x, tri_min.x) || close(tri2[0].pos.x, tri_max.x))
        && (close(tri2[0].pos.y, tri_min.y) || close(tri2[0].pos.y, tri_max.y))
        && (close(tri2[1].pos.x, tri_min.x) || close(tri2[1].pos.x, tri_max.x))
        && (close(tri2[1].pos.y, tri_min.y) || close(tri2[1].pos.y, tri_max.y))
        && (close(tri2[2].pos.x, tri_min.x) || close(tri2[2].pos.x, tri_max.x))
        && (close(tri2[2].pos.y, tri_min.y) || close(tri2[2].pos.y, tri_max.y))
}
