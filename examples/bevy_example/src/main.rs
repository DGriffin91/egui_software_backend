use argh::FromArgs;
use bevy::prelude::*;

use bevy::{
    render::{RenderPlugin, settings::WgpuSettings},
    window::{PresentMode, WindowResolution},
};
use bevy_egui::{
    EguiContext, EguiContexts, EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass,
    EguiRenderOutput,
};
use egui_demo_lib::DemoWindows;
use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};

use crate::softbuffer_plugin::{FrameSurface, SoftBufferPlugin, clear, present};

pub mod softbuffer_plugin;

#[derive(FromArgs)]
/// `bevy` example
struct Args {
    /// render with the standard wgpu render backend rather than with the software renderer.
    #[argh(switch)]
    gpu: bool,

    /// disable raster optimizations. Rasterize everything with triangles, always calculate vertex colors, uvs, use
    /// bilinear everywhere, etc... Things should look the same with this set to true while rendering faster.
    #[argh(switch)]
    no_opt: bool,

    /// disable attempts to optimize by converting suitable triangle pairs into rectangles for faster rendering.
    /// Things should look the same with this set to true while rendering faster.
    #[argh(switch)]
    no_rect: bool,

    /// render directly into buffer without cache. This is much slower and mainly intended for testing.
    #[argh(switch)]
    direct: bool,
}

fn main() {
    let args: Args = argh::from_env();

    let mut default_plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            resolution: WindowResolution::new(1920.0, 1080.0).with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    });

    if !args.gpu {
        default_plugins = default_plugins.set(RenderPlugin {
            render_creation: WgpuSettings {
                backends: None,
                ..default()
            }
            .into(),
            ..default()
        });
    }

    let mut app = App::new();
    app.init_non_send_resource::<DemoApp>()
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .add_plugins((
            default_plugins,
            EguiPlugin::default(),
            bevy::diagnostic::LogDiagnosticsPlugin::default(),
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_systems(Startup, setup_camera_system)
        .add_systems(EguiPrimaryContextPass, ui_example_system);

    if !args.gpu {
        app.add_plugins(SoftBufferPlugin)
            .insert_resource(EguiSoftwareRenderResource(
                EguiSoftwareRender::new(ColorFieldOrder::BGRA)
                    .with_allow_raster_opt(!args.no_opt)
                    .with_convert_tris_to_rects(!args.no_rect)
                    .with_caching(!args.direct),
            ))
            .add_systems(
                PostUpdate,
                egui_render
                    .after(clear)
                    .before(present)
                    .in_set(EguiPostUpdateSet::ProcessOutput),
            );
    }

    app.run();
}

// TODO does this actually need to be non-send?
#[derive(Default)]
pub struct DemoApp(pub DemoWindows);

fn setup_camera_system(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn ui_example_system(mut contexts: EguiContexts, mut demo: NonSendMut<DemoApp>) -> Result {
    egui_extras::install_image_loaders(contexts.ctx_mut()?);
    demo.0.ui(contexts.ctx_mut()?);
    Ok(())
}

#[derive(Resource, Deref, DerefMut)]
struct EguiSoftwareRenderResource(EguiSoftwareRender);

fn egui_render(
    mut contexts: Query<(&mut EguiContext, &mut EguiRenderOutput)>,
    mut surface: NonSendMut<FrameSurface>,
    mut egui_software_render: ResMut<EguiSoftwareRenderResource>,
) {
    let args: Args = argh::from_env();
    let Some(mut frame_buffer) = surface.buffer() else {
        return;
    };
    let mut buffer_ref = frame_buffer.as_mut();
    for (mut context, render_output) in contexts.iter_mut() {
        let pixels_per_point = context.get_mut().pixels_per_point();
        egui_software_render.render(
            &mut buffer_ref,
            &render_output.paint_jobs,
            &render_output.textures_delta,
            pixels_per_point,
        );
    }
}
