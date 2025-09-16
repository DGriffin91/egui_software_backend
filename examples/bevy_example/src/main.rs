use argh::FromArgs;
use bevy::prelude::*;

use bevy::{
    render::{RenderPlugin, settings::WgpuSettings},
    window::{PresentMode, WindowResolution},
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use egui_demo_lib::DemoWindows;

use crate::egui_backend_plugin::EguiSoftwareRenderPlugin;
use crate::softbuffer_plugin::SoftBufferPlugin;

pub mod egui_backend_plugin;
pub mod softbuffer_plugin;

#[derive(FromArgs)]
/// `bevy` example
struct Args {
    /// render with the standard wgpu render backend rather than with the software renderer.
    #[argh(switch)]
    gpu: bool,
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
        app.add_plugins((SoftBufferPlugin, EguiSoftwareRenderPlugin));
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
