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

        let width = (ctx.screen_rect().width() * output.pixels_per_point) as usize;
        let height = (ctx.screen_rect().height() * output.pixels_per_point) as usize;

        let mut buffer = vec![[0u8; 4]; width * height];

        let mut buffer_ref = BufferMutRef::new(&mut buffer, width as usize, height as usize);

        self.render(
            &mut buffer_ref,
            &paint_jobs,
            &output.textures_delta,
            output.pixels_per_point,
        );

        Ok(ImageBuffer::<image::Rgba<u8>, std::vec::Vec<_>>::from_raw(
            width as u32,
            height as u32,
            buffer.iter().flatten().cloned().collect::<Vec<_>>(),
        )
        .unwrap())
    }
}
