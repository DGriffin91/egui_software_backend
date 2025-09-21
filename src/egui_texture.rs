use egui::{TextureFilter, TextureOptions, Vec2, vec2};

use crate::color::{u8x4_to_vec4, vec4_to_u8x4_no_clamp};

pub struct EguiTexture {
    pub data: Vec<[u8; 4]>,
    /// width - 1
    pub width_extent: i32,
    /// height - 1
    pub height_extent: i32,
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
    pub fsize: Vec2,
    pub options: TextureOptions,
}

impl EguiTexture {
    #[allow(dead_code)]
    pub fn sample_nearest(&self, uv: Vec2) -> [u8; 4] {
        let ss_x = ((uv.x * self.fsize.x) as i32).max(0).min(self.width_extent);
        let ss_y = ((uv.y * self.fsize.y) as i32)
            .max(0)
            .min(self.height_extent);
        self.data[ss_x as usize + ss_y as usize * self.width]
    }

    #[allow(dead_code)]
    pub fn get(&self, uv: [i32; 2]) -> [u8; 4] {
        let ss_x = uv[0].max(0).min(self.width_extent);
        let ss_y = uv[1].max(0).min(self.height_extent);
        self.data[ss_x as usize + ss_y as usize * self.width]
    }

    pub fn sample_bilinear(&self, uv: Vec2) -> [u8; 4] {
        let w = self.fsize.x;
        let h = self.fsize.y;

        #[inline(always)]
        fn mirror(v: f32) -> f32 {
            ((v * 0.5 + 0.5).fract() - 0.5) * 2.0
        }

        let uv = match self.options.wrap_mode {
            egui::TextureWrapMode::ClampToEdge => uv,
            egui::TextureWrapMode::Repeat => vec2(uv.x.fract(), uv.y.fract()),
            egui::TextureWrapMode::MirroredRepeat => vec2(mirror(uv.x), mirror(uv.y)),
        };

        let sx = uv.x * w - 0.5;
        let sy = uv.y * h - 0.5;

        let x0 = sx.floor() as i32;
        let y0 = sy.floor() as i32;
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let fx = sx - x0 as f32;
        let fy = sy - y0 as f32;

        let x0c = x0.max(0).min(self.width_extent);
        let y0c = y0.max(0).min(self.height_extent);
        let x1c = x1.max(0).min(self.width_extent);
        let y1c = y1.max(0).min(self.height_extent);

        let c00 = self.data[(x0c as usize) + (y0c as usize) * self.width];

        if self.options.magnification == TextureFilter::Nearest || (fx == 0.0 && fy == 0.0) {
            // if these are 0 the px at 0,0 will have full influence. Equivalent to nearest sampling.
            return c00;
        }

        let c10 = self.data[(x1c as usize) + (y0c as usize) * self.width];
        let c01 = self.data[(x0c as usize) + (y1c as usize) * self.width];
        let c11 = self.data[(x1c as usize) + (y1c as usize) * self.width];

        let v00 = u8x4_to_vec4(&c00);
        let v10 = u8x4_to_vec4(&c10);
        let v01 = u8x4_to_vec4(&c01);
        let v11 = u8x4_to_vec4(&c11);

        let w00 = (1.0 - fx) * (1.0 - fy);
        let w10 = fx * (1.0 - fy);
        let w01 = (1.0 - fx) * fy;
        let w11 = fx * fy;

        vec4_to_u8x4_no_clamp(&(v00 * w00 + v01 * w01 + v10 * w10 + v11 * w11))
    }
}
