use std::collections::HashMap;

use egui::{Pos2, Vec2, epaint::Vertex, vec2};

use crate::{
    BufferMutRef, EguiTexture,
    color::{egui_blend_u8, u8x4_to_vec4, unorm_mult4x4, vec4_to_u8x4_no_clamp},
    math::vec4::Vec4,
    raster::{
        bary::stepper_from_ss_tri_backface_cull,
        rect::{draw_solid_rect, draw_textured_rect},
        span::{calc_row_span, step_rcp},
    },
};

pub fn draw_egui_mesh<const SUBPIX_BITS: i32>(
    textures: &HashMap<egui::TextureId, EguiTexture>,
    buffer: &mut BufferMutRef,
    clip_rect: &egui::Rect,
    mesh: &egui::Mesh,
    vert_offset: Vec2,
    allow_raster_opt: bool,
    convert_tris_to_rects: bool,
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
            draw_tri::<SUBPIX_BITS, true, true, true>(buffer, texture, &draw);
            i += 3;
            continue;
        }

        let vert_uvs_vary = !(draw.uv[0] == draw.uv[1] && draw.uv[0] == draw.uv[2]);
        let vert_col_vary = !(color0_u8x4 == color1_u8x4 && color0_u8x4 == color2_u8x4);
        let mut alpha_blend = true;

        if !vert_uvs_vary {
            draw.const_tex_color_u8x4 = texture.sample_bilinear(draw.uv[0]);
            draw.const_tex_color = u8x4_to_vec4(&draw.const_tex_color_u8x4);
        }

        if !vert_col_vary {
            draw.const_vert_color = draw.colors[0];
            draw.const_vert_color_u8x4 = vec4_to_u8x4_no_clamp(&draw.const_vert_color);
        }

        if !vert_uvs_vary && !vert_col_vary {
            let const_tri_color = draw.const_vert_color * draw.const_tex_color;
            draw.const_tri_color_u8x4 = vec4_to_u8x4_no_clamp(&const_tri_color);
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

        let mut tri2_uvs_match = false;
        let mut tri2_colors_match = false;
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
                        tri2_uvs_match = tri[0].uv == tri2[0].uv
                            && tri[0].uv == tri2[1].uv
                            && tri[0].uv == tri2[2].uv;
                    }

                    if !vert_col_vary {
                        tri2_colors_match = tri[0].color == tri2[0].color
                            && tri[0].color == tri2[1].color
                            && tri[0].color == tri2[2].color;
                    }
                } else {
                    found_rect = false;
                }
            }
        }

        let rect = found_rect
            && ((!vert_uvs_vary && !vert_col_vary && tri2_uvs_match && tri2_colors_match)
                || (!vert_col_vary && tri2_colors_match));

        if rect {
            if alpha_blend {
                if vert_uvs_vary {
                    if vert_col_vary {
                        draw_rect::<SUBPIX_BITS, true, true, true>(buffer, texture, &draw);
                    } else {
                        draw_rect::<SUBPIX_BITS, false, true, true>(buffer, texture, &draw);
                    }
                } else {
                    if vert_col_vary {
                        draw_rect::<SUBPIX_BITS, true, false, true>(buffer, texture, &draw);
                    } else {
                        draw_rect::<SUBPIX_BITS, false, false, true>(buffer, texture, &draw);
                    }
                }
            } else {
                if vert_uvs_vary {
                    if vert_col_vary {
                        draw_rect::<SUBPIX_BITS, true, true, false>(buffer, texture, &draw);
                    } else {
                        draw_rect::<SUBPIX_BITS, false, true, false>(buffer, texture, &draw);
                    }
                } else {
                    if vert_col_vary {
                        draw_rect::<SUBPIX_BITS, true, false, false>(buffer, texture, &draw);
                    } else {
                        draw_rect::<SUBPIX_BITS, false, false, false>(buffer, texture, &draw);
                    }
                }
            }
            i += 6;
        } else {
            if alpha_blend {
                if vert_uvs_vary {
                    if vert_col_vary {
                        draw_tri::<SUBPIX_BITS, true, true, true>(buffer, texture, &draw);
                    } else {
                        draw_tri::<SUBPIX_BITS, false, true, true>(buffer, texture, &draw);
                    }
                } else {
                    if vert_col_vary {
                        draw_tri::<SUBPIX_BITS, true, false, true>(buffer, texture, &draw);
                    } else {
                        draw_tri::<SUBPIX_BITS, false, false, true>(buffer, texture, &draw);
                    }
                }
            } else {
                if vert_uvs_vary {
                    if vert_col_vary {
                        draw_tri::<SUBPIX_BITS, true, true, false>(buffer, texture, &draw);
                    } else {
                        draw_tri::<SUBPIX_BITS, false, true, false>(buffer, texture, &draw);
                    }
                } else {
                    if vert_col_vary {
                        draw_tri::<SUBPIX_BITS, true, false, false>(buffer, texture, &draw);
                    } else {
                        draw_tri::<SUBPIX_BITS, false, false, false>(buffer, texture, &draw);
                    }
                }
            }
            i += 3;
        }
    }
}

struct DrawInfo {
    clip_bounds: [i32; 4],
    colors: [Vec4; 3],
    ss_tri: [Vec2; 3],
    uv: [Vec2; 3],
    tri_min: Vec2,
    tri_max: Vec2,
    const_tex_color: Vec4,
    const_tex_color_u8x4: [u8; 4],
    const_vert_color: Vec4,
    const_vert_color_u8x4: [u8; 4],
    const_tri_color_u8x4: [u8; 4],
}

impl DrawInfo {
    fn new(
        clip_bounds: [i32; 4],
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

fn draw_tri<
    const SUBPIX_BITS: i32,
    const VERT_COL_VARY: bool,
    const VERT_UVS_VARY: bool,
    const ALPHA_BLEND: bool,
>(
    buffer: &mut BufferMutRef,
    texture: &EguiTexture,
    draw: &DrawInfo,
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(draw.clip_bounds, &draw.ss_tri)
    else {
        return;
    };

    let step_rcp = step_rcp(&stepper);

    let mut c_stepper = if VERT_COL_VARY {
        stepper.attr(&draw.colors, sp_inv_area)
    } else {
        Default::default()
    };

    let mut uv_stepper = if VERT_UVS_VARY {
        stepper.attr(&draw.uv, sp_inv_area)
    } else {
        Default::default()
    };

    let max_cols = (ss_max.x - ss_min.x) + 1;

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        if VERT_COL_VARY {
            c_stepper.row_start();
        }
        if VERT_UVS_VARY {
            uv_stepper.row_start();
        }

        if let Some((start, end)) = calc_row_span(&stepper, max_cols, &step_rcp) {
            if VERT_COL_VARY {
                c_stepper.attr += c_stepper.step_x * start as f32;
            }
            if VERT_UVS_VARY {
                uv_stepper.attr += uv_stepper.step_x * start as f32;
            }
            let ss_start = ss_min.x + start;
            let ss_end = ss_min.x + end;

            for ss_x in ss_start..ss_end {
                let src = if VERT_UVS_VARY || VERT_COL_VARY {
                    let tex_color = if VERT_UVS_VARY {
                        texture.sample_bilinear(uv_stepper.attr)
                    } else {
                        draw.const_tex_color_u8x4
                    };
                    let vert_color = if VERT_COL_VARY {
                        vec4_to_u8x4_no_clamp(&c_stepper.attr)
                    } else {
                        draw.const_vert_color_u8x4
                    };
                    unorm_mult4x4(vert_color, tex_color)
                } else {
                    draw.const_tri_color_u8x4
                };
                let pixel = buffer.get_mut(ss_x as usize, ss_y as usize);
                *pixel = if ALPHA_BLEND {
                    egui_blend_u8(src, *pixel)
                } else {
                    src
                };
                if VERT_COL_VARY {
                    c_stepper.col_step();
                }
                if VERT_UVS_VARY {
                    uv_stepper.col_step();
                }
            }
        };

        stepper.row_step();
        if VERT_COL_VARY {
            c_stepper.row_step();
        }
        if VERT_UVS_VARY {
            uv_stepper.row_step();
        }
    }
}

fn draw_rect<
    const SUBPIX_BITS: i32,
    const VERT_COL_VARY: bool,
    const VERT_UVS_VARY: bool,
    const ALPHA_BLEND: bool,
>(
    buffer: &mut BufferMutRef,
    texture: &EguiTexture,
    draw: &DrawInfo,
) {
    if !VERT_UVS_VARY && !VERT_COL_VARY {
        draw_solid_rect(
            buffer,
            draw.const_tri_color_u8x4,
            &draw.clip_bounds,
            draw.tri_min,
            draw.tri_max,
            ALPHA_BLEND,
        );
    } else {
        draw_textured_rect(
            buffer,
            draw.const_vert_color_u8x4,
            texture,
            &draw.clip_bounds,
            draw.tri_min,
            draw.tri_max,
            &draw.uv,
        );
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
