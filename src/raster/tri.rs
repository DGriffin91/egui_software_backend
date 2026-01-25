use constify::constify;

use crate::{
    BufferMutRef,
    color::{SelectedImpl, vec4_to_u8x4},
    egui_texture::EguiTexture,
    raster::{
        bary::SingleStepper,
        span::{calc_row_span, step_rcp},
    },
    render::DrawInfo,
};

#[constify]
pub fn draw_tri<const SUBPIX_BITS: i32>(
    simd_impl: impl SelectedImpl,
    buffer: &mut BufferMutRef,
    texture: &EguiTexture,
    draw: &DrawInfo,
    #[constify] vert_col_vary: bool,
    #[constify] vert_uvs_vary: bool,
    #[constify] alpha_blend: bool,
) {
    let Some((ss_min, ss_max, sp_inv_area, mut stepper)) =
        SingleStepper::from_ss_tri_backface_cull::<SUBPIX_BITS>(draw.clip_bounds, &draw.ss_tri)
    else {
        return;
    };

    let step_rcp = step_rcp(&stepper);

    let mut vert_col_stepper = if vert_col_vary {
        stepper.attr(&draw.colors, sp_inv_area)
    } else {
        Default::default()
    };

    let mut vert_uv_stepper = if vert_uvs_vary {
        stepper.attr(&draw.uv, sp_inv_area)
    } else {
        Default::default()
    };

    let max_cols = ss_max.x - ss_min.x;

    for ss_y in ss_min.y..ss_max.y {
        stepper.row_start();
        if vert_col_vary {
            vert_col_stepper.row_start();
        }
        if vert_uvs_vary {
            vert_uv_stepper.row_start();
        }

        if let Some((start, end)) = calc_row_span(&stepper, max_cols, &step_rcp) {
            if vert_col_vary {
                vert_col_stepper.attr += vert_col_stepper.step_x * start as f32;
            }
            if vert_uvs_vary {
                vert_uv_stepper.attr += vert_uv_stepper.step_x * start as f32;
            }
            let ss_start = (ss_min.x + start) as usize;
            let ss_end = (ss_min.x + end) as usize;

            if alpha_blend && !vert_uvs_vary {
                let dst = buffer.get_mut_span(ss_start, ss_end, ss_y as usize);
                if vert_col_vary {
                    simd_impl.egui_blend_u8_slice_one_src_tinted_fn(
                        draw.const_tex_color_u8x4,
                        || {
                            let v = vec4_to_u8x4(&vert_col_stepper.attr);
                            vert_col_stepper.col_step();
                            v
                        },
                        dst,
                    )
                } else {
                    simd_impl.egui_blend_u8_slice_one_src(draw.const_tri_color_u8x4, dst)
                }
            } else {
                for ss_x in ss_start..ss_end {
                    let src = if vert_uvs_vary || vert_col_vary {
                        let tex_color = if vert_uvs_vary {
                            texture.sample_bilinear(vert_uv_stepper.attr)
                        } else {
                            draw.const_tex_color_u8x4
                        };
                        let vert_color = if vert_col_vary {
                            vec4_to_u8x4(&vert_col_stepper.attr)
                        } else {
                            draw.const_vert_color_u8x4
                        };
                        simd_impl.unorm_mult4x4(vert_color, tex_color)
                    } else {
                        draw.const_tri_color_u8x4
                    };
                    let pixel = buffer.get_mut(ss_x as usize, ss_y as usize);
                    *pixel = if alpha_blend {
                        simd_impl.egui_blend_u8(src, *pixel)
                    } else {
                        src
                    };
                    if vert_col_vary {
                        vert_col_stepper.col_step();
                    }
                    if vert_uvs_vary {
                        vert_uv_stepper.col_step();
                    }
                }
            }
        };

        stepper.row_step();
        if vert_col_vary {
            vert_col_stepper.row_step();
        }
        if vert_uvs_vary {
            vert_uv_stepper.row_step();
        }
    }
}
