use bytemuck::cast_slice;
use egui::TexturesDelta;
use egui_kittest::TestRenderer;
use image::ImageBuffer;

use crate::{BufferMutRef, EguiSoftwareRender};

impl TestRenderer for EguiSoftwareRender {
    fn handle_delta(&mut self, delta: &TexturesDelta) {
        self.set_textures(delta);
        self.free_textures(delta);
    }

    fn render(
        &mut self,
        ctx: &egui::Context,
        output: &egui::FullOutput,
    ) -> Result<image::RgbaImage, String> {
        let paint_jobs = ctx.tessellate(output.shapes.clone(), output.pixels_per_point);

        let width = ctx.screen_rect().width() as usize;
        let height = ctx.screen_rect().height() as usize;

        let mut buffer = vec![0u32; width * height];

        let mut buffer_ref = BufferMutRef::new(
            bytemuck::cast_slice_mut(&mut buffer),
            width as usize,
            height as usize,
        );

        self.render(
            width,
            height,
            &paint_jobs,
            &output.textures_delta,
            output.pixels_per_point,
            Some(&mut buffer_ref),
            false,
            false,
        );

        Ok(bgra_u32_to_image(width, height, buffer))
    }
}

fn bgra_u32_to_image(
    width: usize,
    height: usize,
    bgra: Vec<u32>,
) -> ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    ImageBuffer::<_, _>::from_raw(
        width as u32,
        height as u32,
        cast_slice::<u32, [u8; 4]>(&bgra)
            .iter()
            .map(|p| [p[2], p[1], p[0], p[3]])
            .flatten()
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
