use std::sync::atomic::{AtomicBool, Ordering};

use egui::TexturesDelta;
use egui_kittest::TestRenderer;
use image::ImageBuffer;

use crate::{BufferMutRef, EguiSoftwareRender};

// The render mode is not currently stored in the EguiSoftwareRender and can be switched ad-hoc so we need to set it
// externally. When rendering normally with the cache, it's possible to blit separately from render time, this allows
// rendering to happen on a separate thread without require a lock on the render target buffer. There could be a fn that
// did both though to possibly unify the interface. While still optionally allowing blit_canvas_to_buffer separately.
pub struct RenderMode(AtomicBool);

impl RenderMode {
    pub fn direct(&self, direct: bool) {
        self.0.store(direct, Ordering::Release);
    }
    pub fn is_direct(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
pub static TEST_RENDER_MODE: RenderMode = RenderMode(AtomicBool::new(false));

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

        if TEST_RENDER_MODE.is_direct() {
            self.render_direct(
                &mut buffer_ref,
                &paint_jobs,
                &output.textures_delta,
                output.pixels_per_point,
            );
        } else {
            self.render(
                width as usize,
                height as usize,
                &paint_jobs,
                &output.textures_delta,
                output.pixels_per_point,
            );
            self.blit_canvas_to_buffer(&mut buffer_ref);
        }

        Ok(ImageBuffer::<image::Rgba<u8>, std::vec::Vec<_>>::from_raw(
            width as u32,
            height as u32,
            buffer.iter().flatten().cloned().collect::<Vec<_>>(),
        )
        .unwrap())
    }
}
