use std::collections::HashMap;

use egui::{Pos2, TextureOptions, Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef, EguiTexture,
    egui_texture::{egui_blend, egui_blend_u8, u8x4_to_vec4, unorm_mult4x4, vec4_to_u8x4_no_clamp},
    raster::{no_bary, raster_tri_no_depth_no_backface_cull, with_bary},
    vec4::Vec4,
};

pub fn draw_egui_mesh<const SUBPIX_BITS: i32>(
    textures: &HashMap<egui::TextureId, EguiTexture>,
    target_size: Vec2,
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

    if texture.options
        != (TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification: egui::TextureFilter::Linear,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
            mipmap_mode: None,
        })
    {
        todo!();
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

        let color0 = u8x4_to_vec4(&color0_u8x4);
        let color1 = u8x4_to_vec4(&color1_u8x4);
        let color2 = u8x4_to_vec4(&color2_u8x4);

        let uv_v0 = vec2(tri[0].pos.x, tri[0].pos.y) / target_size;
        let uv_v1 = vec2(tri[1].pos.x, tri[1].pos.y) / target_size;
        let uv_v2 = vec2(tri[2].pos.x, tri[2].pos.y) / target_size;
        let uv_tri = [uv_v0, uv_v1, uv_v2];

        let uv0 = vec2(tri[0].uv.x, tri[0].uv.y);
        let uv1 = vec2(tri[1].uv.x, tri[1].uv.y);
        let uv2 = vec2(tri[2].uv.x, tri[2].uv.y);

        if !allow_raster_opt {
            raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
                clip_bounds,
                target_size,
                uv_tri,
                with_bary(|x, y, b0, b1| {
                    let b2 = 1.0 - b0 - b1;
                    let uv = b0 * uv0 + b1 * uv1 + b2 * uv2;
                    let tex_color = u8x4_to_vec4(&texture.sample_bilinear(uv));
                    let vert_color = b0 * color0 + b1 * color1 + b2 * color2;
                    let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                    let dst = u8x4_to_vec4(pixel);
                    let src = vert_color * tex_color;
                    *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
                }),
            );
            i += 3;
            continue;
        }

        let uvs_match = uv0 == uv1 && uv0 == uv2;
        let colors_match = color0_u8x4 == color1_u8x4 && color0_u8x4 == color2_u8x4;
        let mut requires_alpha_blending = true;

        let mut use_bilinear = false; // TODO perf: specialize based on this 

        if uvs_match {
            const_tex_color_u8x4 = texture.sample_bilinear(uv0);
            const_tex_color = u8x4_to_vec4(&const_tex_color_u8x4);
        } else {
            let uvs_size = (uv0.max(uv1).max(uv2) - uv0.min(uv1).min(uv2)) * texture.fsize;
            use_bilinear = uvs_size != fsize;
        }
        if colors_match {
            const_vert_color = color0;
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
        let find_rects = convert_tris_to_rects && i + 6 < indices.len();
        let find_rects = find_rects && colors_match;
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

            #[inline(always)]
            fn close(a: f32, b: f32) -> bool {
                //(a - b).abs() <= 0.1
                a == b
            }

            // https://github.com/emilk/imgui_software_renderer/blob/b5ae63a9e42eccf7db3bf64696761a53424c53dd/src/imgui_sw.cpp#L577
            found_rect = (close(tri[0].pos.x, tri_min.x) || close(tri[0].pos.x, tri_max.x))
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
                && (close(tri2[2].pos.y, tri_min.y) || close(tri2[2].pos.y, tri_max.y));

            if found_rect {
                tri2_uvs_match =
                    tri[0].uv == tri2[0].uv && tri[0].uv == tri2[1].uv && tri[0].uv == tri2[2].uv;

                tri2_colors_match = tri[0].color == tri2[0].color
                    && tri[0].color == tri2[1].color
                    && tri[0].color == tri2[2].color;
            }
        }

        if uvs_match && colors_match {
            if found_rect
                && tri2_uvs_match
                && tri2_colors_match
                && draw_solid_rect(
                    buffer,
                    const_tri_color_u8x4,
                    &clip_bounds,
                    tri_min,
                    tri_max,
                    &tri,
                    requires_alpha_blending,
                )
            {
                i += 6; // Skip both tris
                continue;
            } else {
                draw_solid_tri::<SUBPIX_BITS>(
                    &target_size,
                    buffer,
                    const_tri_color_u8x4,
                    &clip_bounds,
                    &uv_tri,
                    requires_alpha_blending,
                );
            }
        } else if uvs_match {
            // if uvs match but colors don't match
            if requires_alpha_blending {
                raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
                    clip_bounds,
                    target_size,
                    uv_tri,
                    with_bary(|x, y, b0, b1| {
                        let b2 = 1.0 - b0 - b1;
                        let vert_color = b0 * color0 + b1 * color1 + b2 * color2;
                        let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                        let dst = u8x4_to_vec4(pixel);
                        let src = vert_color * const_tex_color;
                        *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
                    }),
                );
            } else {
                raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
                    clip_bounds,
                    target_size,
                    uv_tri,
                    with_bary(|x, y, b0, b1| {
                        let b2 = 1.0 - b0 - b1;
                        let vert_color = b0 * color0 + b1 * color1 + b2 * color2;
                        let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                        let src = vert_color * const_tex_color;
                        *pixel = vec4_to_u8x4_no_clamp(&src);
                    }),
                );
            }
        } else if colors_match {
            // if colors match but uvs don't match

            if found_rect
                && tri2_colors_match
                && draw_textured_rect(
                    buffer,
                    const_vert_color_u8x4,
                    texture,
                    &clip_bounds,
                    tri_min,
                    tri_max,
                    &tri,
                    use_bilinear,
                )
            {
                i += 6; // Skip both tris
                continue;
            } else {
                raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
                    clip_bounds,
                    target_size,
                    uv_tri,
                    with_bary(|x, y, b0, b1| {
                        let b2 = 1.0 - b0 - b1;
                        let uv = b0 * uv0 + b1 * uv1 + b2 * uv2;
                        let tex_color = u8x4_to_vec4(&texture.sample_bilinear(uv));
                        let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                        let dst = u8x4_to_vec4(pixel);
                        let src = const_vert_color * tex_color;
                        *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
                    }),
                );
            }
        } else {
            // Unique colors and uvs, didn't find a rect.
            // This is the standard full version sans shortcuts. Everything could be rendered using just this.
            raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
                clip_bounds,
                target_size,
                uv_tri,
                with_bary(|x, y, b0, b1| {
                    let b2 = 1.0 - b0 - b1;
                    let uv = b0 * uv0 + b1 * uv1 + b2 * uv2;
                    let tex_color = u8x4_to_vec4(&texture.sample_bilinear(uv));
                    let vert_color = b0 * color0 + b1 * color1 + b2 * color2;
                    let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                    let dst = u8x4_to_vec4(pixel);
                    let src = vert_color * tex_color;
                    *pixel = vec4_to_u8x4_no_clamp(&egui_blend(&src, &dst));
                }),
            );
        }

        i += 3;
    }
}

fn draw_textured_rect(
    buffer: &mut BufferMutRef,
    const_vert_color_u8x4: [u8; 4],
    texture: &EguiTexture,
    clip_bounds: &[i32; 4],
    tri_min: Vec2,
    tri_max: Vec2,
    tri: &[Vertex; 3],
    _use_bilinear: bool,
) -> bool {
    let tri_area = egui_orient2df(&tri[0].pos, &tri[1].pos, &tri[2].pos).abs();
    let rect_area = (tri_max.x - tri_min.x) * (tri_max.y - tri_min.y);
    let areas_match = (tri_area - rect_area).abs() < 0.5;

    if areas_match {
        let min_x = ((tri_min.x + 0.5) as i32).max(clip_bounds[0]);
        let min_y = ((tri_min.y + 0.5) as i32).max(clip_bounds[1]);
        let max_x = ((tri_max.x + 0.5) as i32).min(clip_bounds[2]);
        let max_y = ((tri_max.y + 0.5) as i32).min(clip_bounds[3]);

        let sizex = max_x - min_x;
        let sizey = max_y - min_y;

        if sizex <= 0 || sizey <= 0 {
            return false;
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

        let uv_step = (max_uv - min_uv) / vec2(sizex as f32, sizey as f32);
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

        return true;
    }
    false
}

fn draw_solid_tri<const SUBPIX_BITS: i32>(
    target_size: &Vec2,
    buffer: &mut BufferMutRef,
    const_tri_color_u8x4: [u8; 4],
    clip_bounds: &[i32; 4],
    uv_tri: &[Vec2; 3],
    requires_alpha_blending: bool,
) {
    // TODO is scanline faster when barycentrics are not needed?
    if requires_alpha_blending {
        raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
            *clip_bounds,
            *target_size,
            *uv_tri,
            no_bary(|x, y| {
                let pixel = buffer.get_mut_clamped(x as usize, y as usize);
                let src = const_tri_color_u8x4;
                *pixel = egui_blend_u8(src, *pixel);
            }),
        );
    } else {
        raster_tri_no_depth_no_backface_cull::<_, SUBPIX_BITS>(
            *clip_bounds,
            *target_size,
            *uv_tri,
            no_bary(|x, y| {
                *buffer.get_mut_clamped(x as usize, y as usize) = const_tri_color_u8x4;
            }),
        );
    }
}

fn draw_solid_rect(
    buffer: &mut BufferMutRef,
    const_tri_color_u8x4: [u8; 4],
    clip_bounds: &[i32; 4],
    tri_min: Vec2,
    tri_max: Vec2,
    tri: &[Vertex; 3],
    requires_alpha_blending: bool,
) -> bool {
    let tri_area = egui_orient2df(&tri[0].pos, &tri[1].pos, &tri[2].pos).abs();
    let rect_area = (tri_max.x - tri_min.x) * (tri_max.y - tri_min.y);
    let areas_match = (tri_area - rect_area).abs() < 0.5;

    if areas_match {
        let min_x = ((tri_min.x + 0.5) as i32).max(clip_bounds[0]);
        let min_y = ((tri_min.y + 0.5) as i32).max(clip_bounds[1]);
        let max_x = ((tri_max.x + 0.5) as i32).min(clip_bounds[2]);
        let max_y = ((tri_max.y + 0.5) as i32).min(clip_bounds[3]);

        let sizex = max_x - min_x;
        let sizey = max_y - min_y;

        if sizex <= 0 || sizey <= 0 {
            return false;
        }

        let min_x = min_x as usize;
        let min_y = min_y as usize;
        let max_x = max_x as usize;
        let max_y = max_y as usize;

        if requires_alpha_blending {
            for y in min_y..max_y {
                let buf_y = y * buffer.width;
                for x in min_x..max_x {
                    let pixel = &mut buffer.data[x + buf_y];
                    let src = const_tri_color_u8x4;
                    *pixel = egui_blend_u8(src, *pixel);
                }
            }
        } else {
            for y in min_y..max_y {
                let buf_y = y * buffer.width;
                buffer.data[min_x + buf_y..max_x + buf_y].fill(const_tri_color_u8x4);
            }
        }

        return true;
    }
    false
}

#[inline(always)]
pub fn egui_orient2df(a: &Pos2, b: &Pos2, c: &Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
