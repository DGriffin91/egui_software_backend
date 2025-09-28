use egui::{Pos2, Vec2, ahash::HashMap, vec2};

use crate::{
    BufferMutRef, EguiTexture,
    color::{u8x4_to_vec4, vec4_to_u8x4},
    math::{
        i64vec2::{I64Vec2, i64vec2},
        vec4::Vec4,
    },
    raster::tri::draw_tri,
};

pub(crate) fn draw_egui_mesh<const SUBPIX_BITS: i32>(
    textures: &HashMap<egui::TextureId, EguiTexture>,
    buffer: &mut BufferMutRef,
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
        );

        if !allow_raster_opt {
            draw_tri::<SUBPIX_BITS>(buffer, texture, &draw, true, true, true);
            i += 3;
            continue;
        }

        let vert_uvs_vary = !(draw.uv[0] == draw.uv[1] && draw.uv[0] == draw.uv[2]);
        let vert_col_vary = !(color0_u8x4 == color1_u8x4 && color0_u8x4 == color2_u8x4);
        let mut alpha_blend = true;

        if !vert_uvs_vary {
            draw.const_tex_color_u8x4 = texture.sample_nearest(draw.uv[0]);
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

        draw_tri::<SUBPIX_BITS>(
            buffer,
            texture,
            &draw,
            vert_col_vary,
            vert_uvs_vary,
            alpha_blend,
        );

        i += 3;
    }
}

pub(crate) struct DrawInfo {
    pub(crate) clip_bounds: [I64Vec2; 2],
    pub(crate) colors: [Vec4; 3],
    pub(crate) ss_tri: [Vec2; 3],
    pub(crate) uv: [Vec2; 3],
    pub(crate) const_tex_color: Vec4,
    pub(crate) const_tex_color_u8x4: [u8; 4],
    pub(crate) const_vert_color: Vec4,
    pub(crate) const_vert_color_u8x4: [u8; 4],
    pub(crate) const_tri_color_u8x4: [u8; 4],
}

impl DrawInfo {
    fn new(clip_bounds: [I64Vec2; 2], colors: [Vec4; 3], ss_tri: [Vec2; 3], uv: [Vec2; 3]) -> Self {
        Self {
            clip_bounds,
            colors,
            ss_tri,
            uv,
            const_tex_color: Vec4::ONE,
            const_tex_color_u8x4: [255; 4],
            const_vert_color: Vec4::ONE,
            const_vert_color_u8x4: [255; 4],
            const_tri_color_u8x4: [255; 4],
        }
    }
}

#[inline(always)]
pub(crate) fn egui_orient2df(a: &Pos2, b: &Pos2, c: &Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
