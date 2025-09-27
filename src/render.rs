use std::collections::HashMap;

use egui::{Pos2, Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef, EguiTexture,
    color::{u8x4_to_vec4, vec4_to_u8x4},
    math::{
        i64vec2::{I64Vec2, i64vec2},
        vec4::Vec4,
    },
    raster::{rect::draw_rect, tri::draw_tri},
    sse41,
};

pub fn draw_egui_mesh<const SUBPIX_BITS: i32>(
    textures: &HashMap<egui::TextureId, EguiTexture>,
    buffer: &mut BufferMutRef,
    clip_rect: &egui::Rect,
    mesh: &egui::Mesh,
    vert_offset: Vec2,
    allow_raster_opt: bool,
    convert_tris_to_rects: bool,
    #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
    stats: &mut crate::stats::RasterStats,
) {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return;
    }

    let Some(texture) = textures.get(&mesh.texture_id) else {
        return;
    };

    let indices = &mesh.indices;
    let vertices = &mesh.vertices;

    let clip_bounds = [
        i64vec2(
            ((clip_rect.min.x + 0.5) as i64).clamp(0, buffer.width as i64),
            ((clip_rect.min.y + 0.5) as i64).clamp(0, buffer.height as i64),
        ),
        i64vec2(
            ((clip_rect.max.x + 0.5) as i64).clamp(0, buffer.width as i64),
            ((clip_rect.max.y + 0.5) as i64).clamp(0, buffer.height as i64),
        ),
    ];

    if clip_bounds[1].x - clip_bounds[0].x <= 0 || clip_bounds[1].y - clip_bounds[0].y <= 0 {
        return;
    }

    let mut i = 0;
    // Get texture
    while i < indices.len() {
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

        let mut draw = DrawInfo::new(
            clip_bounds,
            [
                u8x4_to_vec4(&color0_u8x4),
                u8x4_to_vec4(&color1_u8x4),
                u8x4_to_vec4(&color2_u8x4),
            ],
            [
                tri[0].pos.to_vec2(),
                tri[1].pos.to_vec2(),
                tri[2].pos.to_vec2(),
            ],
            [
                vec2(tri[0].uv.x, tri[0].uv.y),
                vec2(tri[1].uv.x, tri[1].uv.y),
                vec2(tri[2].uv.x, tri[2].uv.y),
            ],
            tri_min,
            tri_max,
        );

        if !allow_raster_opt {
            draw_tri::<SUBPIX_BITS>(buffer, texture, &draw, true, true, true, false);
            i += 3;
            continue;
        }

        let mut vert_uvs_vary = !(draw.uv[0] == draw.uv[1] && draw.uv[0] == draw.uv[2]);
        let mut vert_col_vary = !(color0_u8x4 == color1_u8x4 && color0_u8x4 == color2_u8x4);
        let mut alpha_blend = true;

        if !vert_uvs_vary {
            draw.const_tex_color_u8x4 = texture.sample_bilinear(draw.uv[0]);
            draw.const_tex_color = u8x4_to_vec4(&draw.const_tex_color_u8x4);
        }

        if !vert_col_vary {
            draw.const_vert_color = draw.colors[0];
            draw.const_vert_color_u8x4 = color0_u8x4;
        }

        if !vert_uvs_vary && !vert_col_vary {
            let const_tri_color = draw.const_vert_color * draw.const_tex_color;
            draw.const_tri_color_u8x4 = vec4_to_u8x4(&const_tri_color);
            if draw.const_tri_color_u8x4[3] == 255 {
                alpha_blend = false;
            }
        }

        if !vert_uvs_vary
            && vert_col_vary
            && draw.const_tex_color_u8x4[3] == 255
            && color0_u8x4[3] == 255
            && color1_u8x4[3] == 255
            && color2_u8x4[3] == 255
        {
            alpha_blend = false;
        }

        let find_rects = convert_tris_to_rects && !vert_col_vary && i + 6 < indices.len();
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

                    if !vert_uvs_vary {
                        let tri2_uvs_match = tri[0].uv == tri2[0].uv
                            && tri[0].uv == tri2[1].uv
                            && tri[0].uv == tri2[2].uv;
                        vert_uvs_vary = vert_uvs_vary && tri2_uvs_match;
                    }

                    if !vert_col_vary {
                        let tri2_colors_match = tri[0].color == tri2[0].color
                            && tri[0].color == tri2[1].color
                            && tri[0].color == tri2[2].color;
                        vert_col_vary = vert_col_vary && tri2_colors_match;
                    }
                } else {
                    found_rect = false;
                }
            }
        }

        let rect = found_rect && !vert_col_vary; // vert_col_vary not supported by rect render

        #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
        stats.start_raster();
        if rect {
            draw_rect(
                buffer,
                texture,
                &draw,
                vert_col_vary,
                vert_uvs_vary,
                alpha_blend,
                sse41(),
            );

            #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
            stats.finish_rect(fsize, vert_uvs_vary, vert_col_vary, alpha_blend);
            i += 6;
        } else {
            draw_tri::<SUBPIX_BITS>(
                buffer,
                texture,
                &draw,
                vert_col_vary,
                vert_uvs_vary,
                alpha_blend,
                sse41(),
            );

            #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
            stats.finish_tri(fsize, vert_uvs_vary, vert_col_vary, alpha_blend);
            i += 3;
        }
    }
}

pub struct DrawInfo {
    pub clip_bounds: [I64Vec2; 2],
    pub colors: [Vec4; 3],
    pub ss_tri: [Vec2; 3],
    pub uv: [Vec2; 3],
    pub tri_min: Vec2,
    pub tri_max: Vec2,
    pub const_tex_color: Vec4,
    pub const_tex_color_u8x4: [u8; 4],
    pub const_vert_color: Vec4,
    pub const_vert_color_u8x4: [u8; 4],
    pub const_tri_color_u8x4: [u8; 4],
}

impl DrawInfo {
    fn new(
        clip_bounds: [I64Vec2; 2],
        colors: [Vec4; 3],
        ss_tri: [Vec2; 3],
        uv: [Vec2; 3],
        tri_min: Vec2,
        tri_max: Vec2,
    ) -> Self {
        Self {
            clip_bounds,
            colors,
            ss_tri,
            uv,
            tri_min,
            tri_max,
            const_tex_color: Vec4::ONE,
            const_tex_color_u8x4: [255; 4],
            const_vert_color: Vec4::ONE,
            const_vert_color_u8x4: [255; 4],
            const_tri_color_u8x4: [255; 4],
        }
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
