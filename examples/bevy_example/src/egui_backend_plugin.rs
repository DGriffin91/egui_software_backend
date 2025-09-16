pub struct EguiSoftwareRenderPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiContext, EguiPostUpdateSet, EguiRenderOutput};
use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};

use crate::softbuffer_plugin::{FrameSurface, clear, present};

impl Plugin for EguiSoftwareRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EguiSoftwareRenderResource>()
            .add_systems(
                PostUpdate,
                egui_render
                    .after(clear)
                    .before(present)
                    .in_set(EguiPostUpdateSet::ProcessOutput),
            );
    }
}

#[derive(Resource, Deref, DerefMut)]
struct EguiSoftwareRenderResource(EguiSoftwareRender);

impl Default for EguiSoftwareRenderResource {
    fn default() -> Self {
        EguiSoftwareRenderResource(EguiSoftwareRender::new(ColorFieldOrder::BGRA))
    }
}

fn egui_render(
    mut contexts: Query<(&mut EguiContext, &mut EguiRenderOutput)>,
    mut surface: NonSendMut<FrameSurface>,
    mut egui_software_render: ResMut<EguiSoftwareRenderResource>,
) {
    let Some(mut frame_buffer) = surface.buffer() else {
        return;
    };
    let mut buffer = frame_buffer.as_mut();
    for (mut context, render_output) in contexts.iter_mut() {
        let pixels_per_point = context.get_mut().pixels_per_point();
        egui_software_render.render(
            buffer.width,
            buffer.height,
            &render_output.paint_jobs,
            &render_output.textures_delta,
            pixels_per_point,
        );
        egui_software_render.blit_canvas_to_buffer(&mut buffer);
    }
}
