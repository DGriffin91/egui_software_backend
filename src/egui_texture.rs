use alloc::vec::Vec;
use egui::{Color32, Vec2, vec2};

use crate::{ColorFieldOrder, color::swizzle_rgba_bgra};

pub(crate) struct EguiTexture {
    pub(crate) data: Vec<[u8; 4]>,
    pub(crate) width: usize,
    #[allow(dead_code)]
    pub(crate) height: usize,
    pub(crate) fsize: Vec2,
}

impl EguiTexture {
    pub(crate) fn new(
        field_order: ColorFieldOrder,
        size: [usize; 2],
        pixels: &[Color32],
    ) -> EguiTexture {
        let data = pixels
            .iter()
            .map(|p| match field_order {
                ColorFieldOrder::RGBA => p.to_array(),
                ColorFieldOrder::BGRA => swizzle_rgba_bgra(p.to_array()),
            })
            .collect::<Vec<_>>();
        EguiTexture {
            data,
            width: size[0],
            height: size[1],
            fsize: vec2(size[0] as f32, size[1] as f32),
        }
    }

    pub(crate) fn sample_nearest(&self, uv: Vec2) -> [u8; 4] {
        let ss_x = ((uv.x * self.fsize.x) as i32)
            .max(0)
            .min(self.width as i32 - 1);
        let ss_y = ((uv.y * self.fsize.y) as i32)
            .max(0)
            .min(self.height as i32 - 1);
        self.data[ss_x as usize + ss_y as usize * self.width]
    }
}
