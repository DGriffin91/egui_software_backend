use alloc::{string::String, vec, vec::Vec};
use egui::TexturesDelta;
use egui_kittest::TestRenderer;
use image::ImageBuffer;

use crate::{BufferMutRef, EguiSoftwareRenderCanvas};

impl TestRenderer for EguiSoftwareRenderCanvas {
    fn handle_delta(&mut self, delta: &TexturesDelta) {
        self.renderer.inner.set_textures(delta);
        self.renderer.inner.free_textures(delta);
    }

    fn render(
        &mut self,
        ctx: &egui::Context,
        output: &egui::FullOutput,
    ) -> Result<image::RgbaImage, String> {
        let paint_jobs = ctx.tessellate(output.shapes.clone(), output.pixels_per_point);

        let width = (ctx.content_rect().width() * output.pixels_per_point) as u32;
        let height = (ctx.content_rect().height() * output.pixels_per_point) as u32;

        let mut buffer = vec![[0u8; 4]; crate::as_usize(width * height)];

        let mut buffer_ref = BufferMutRef::new(&mut buffer, width, height);

        self.render(
            &mut buffer_ref,
            &paint_jobs,
            &output.textures_delta,
            output.pixels_per_point,
        );

        Ok(ImageBuffer::<image::Rgba<u8>, Vec<_>>::from_raw(
            width as u32,
            height as u32,
            buffer.iter().flatten().cloned().collect::<Vec<_>>(),
        )
        .unwrap())
    }
}
