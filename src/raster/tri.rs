use static_dispatch::static_dispatch;

use crate::{
    BufferMutRef,
    color::{egui_blend_u8, unorm_mult4x4, vec4_to_u8x4_no_clamp},
    egui_texture::EguiTexture,
    raster::{
        bary::stepper_from_ss_tri_backface_cull,
        span::{calc_row_span, step_rcp},
    },
    render::DrawInfo,
};

#[static_dispatch]
pub fn draw_tri<const SUBPIX_BITS: i32>(
    buffer: &mut BufferMutRef,
    texture: &EguiTexture,
    draw: &DrawInfo,
    #[dispatch(consts = [true, false])] vert_col_vary: bool,
    #[dispatch(consts = [true, false])] vert_uvs_vary: bool,
    #[dispatch(consts = [true, false])] alpha_blend: bool,
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        stepper_from_ss_tri_backface_cull::<SUBPIX_BITS>(draw.clip_bounds, &draw.ss_tri)
    else {
        return;
    };

    let step_rcp = step_rcp(&stepper);

    let mut c_stepper = if vert_col_vary {
        stepper.attr(&draw.colors, sp_inv_area)
    } else {
        Default::default()
    };

    let mut uv_stepper = if vert_uvs_vary {
        stepper.attr(&draw.uv, sp_inv_area)
    } else {
        Default::default()
    };

    let max_cols = (ss_max.x - ss_min.x) + 1;

    for ss_y in ss_min.y..=ss_max.y {
        stepper.row_start();
        if vert_col_vary {
            c_stepper.row_start();
        }
        if vert_uvs_vary {
            uv_stepper.row_start();
        }

        if let Some((start, end)) = calc_row_span(&stepper, max_cols, &step_rcp) {
            if vert_col_vary {
                c_stepper.attr += c_stepper.step_x * start as f32;
            }
            if vert_uvs_vary {
                uv_stepper.attr += uv_stepper.step_x * start as f32;
            }
            let ss_start = ss_min.x + start;
            let ss_end = ss_min.x + end;

            for ss_x in ss_start..ss_end {
                let src = if vert_uvs_vary || vert_col_vary {
                    let tex_color = if vert_uvs_vary {
                        texture.sample_bilinear(uv_stepper.attr)
                    } else {
                        draw.const_tex_color_u8x4
                    };
                    let vert_color = if vert_col_vary {
                        vec4_to_u8x4_no_clamp(&c_stepper.attr)
                    } else {
                        draw.const_vert_color_u8x4
                    };
                    unorm_mult4x4(vert_color, tex_color)
                } else {
                    draw.const_tri_color_u8x4
                };
                let pixel = buffer.get_mut(ss_x as usize, ss_y as usize);
                *pixel = if alpha_blend {
                    egui_blend_u8(src, *pixel)
                } else {
                    src
                };
                if vert_col_vary {
                    c_stepper.col_step();
                }
                if vert_uvs_vary {
                    uv_stepper.col_step();
                }
            }
        };

        stepper.row_step();
        if vert_col_vary {
            c_stepper.row_step();
        }
        if vert_uvs_vary {
            uv_stepper.row_step();
        }
    }
}
