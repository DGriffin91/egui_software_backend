use alloc::{string::String, vec, vec::Vec};
use egui::TexturesDelta;
use egui_kittest::TestRenderer;
use image::ImageBuffer;

use crate::{BufferMutRef, BufferState, BufferStates, EguiSoftwareRender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiSoftwareTestRenderMode {
    AlwaysBlit,
    AlwaysBlend,
    SimpleBuffering,
    DoubleBuffering,
    TripleBuffeing,
}

pub struct EguiSoftwareTestRender {
    mode: EguiSoftwareTestRenderMode,
    buffer_states: BufferStates,
    buffer1: Vec<[u8; 4]>,
    buffer2: Vec<[u8; 4]>,
    buffer3: Vec<[u8; 4]>,
    counter: usize,
    renderer: EguiSoftwareRender,
}

impl EguiSoftwareTestRender {
    pub fn new(mode: EguiSoftwareTestRenderMode, renderer: EguiSoftwareRender) -> Self {
        Self {
            mode,
            buffer_states: BufferStates::new(),
            buffer1: Vec::new(),
            buffer2: Vec::new(),
            buffer3: Vec::new(),
            counter: 0,
            renderer,
        }
    }
}

impl TestRenderer for EguiSoftwareTestRender {
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
        let len = crate::as_usize(width * height);
        let age = match self.mode {
            EguiSoftwareTestRenderMode::SimpleBuffering if self.counter >= 1 => 1,
            EguiSoftwareTestRenderMode::DoubleBuffering if self.counter >= 2 => 2,
            EguiSoftwareTestRenderMode::TripleBuffeing if self.counter >= 3 => 3,
            _ => 0,
        };
        let buffer_state = match self.mode {
            EguiSoftwareTestRenderMode::AlwaysBlit => BufferState::AlwaysBlit,
            EguiSoftwareTestRenderMode::AlwaysBlend => BufferState::AlwaysBlend,
            _ => self.buffer_states.next(age, len),
        };

        let buffer = match buffer_state {
            BufferState::AlwaysBlit
            | BufferState::AlwaysBlend
            | BufferState::Buffer1Zeroed
            | BufferState::Buffer1Incremental => &mut self.buffer1,
            BufferState::Buffer2Zeroed | BufferState::Buffer2Incremental => &mut self.buffer2,
            BufferState::Buffer3Zeroed | BufferState::Buffer3Incremental => &mut self.buffer3,
        };
        if buffer.len() != len {
            assert!(buffer_state.is_new_zeroed());
            *buffer = vec![[0u8; 4]; len];
        } else if buffer_state.is_new_zeroed() {
            buffer.fill(Default::default());
        }
        let mut buffer_ref = BufferMutRef::new(buffer, width, height);
        self.counter += 1;
        self.renderer.render(
            &mut buffer_ref,
            buffer_state,
            paint_jobs,
            &output.textures_delta,
            output.pixels_per_point,
        );

        Ok(ImageBuffer::<image::Rgba<u8>, Vec<_>>::from_raw(
            width,
            height,
            buffer.iter().flatten().cloned().collect::<Vec<_>>(),
        )
        .unwrap())
    }
}
