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

fn main() {
    App::new()
        .init_non_send_resource::<DemoApp>()
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .add_plugins(
            DefaultPlugins
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        backends: None,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: WindowResolution::new(1920.0, 1080.0)
                            .with_scale_factor_override(1.0),
                        present_mode: PresentMode::AutoNoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            SoftBufferPlugin,
            EguiSoftwareRenderPlugin,
            EguiPlugin::default(),
            bevy::diagnostic::LogDiagnosticsPlugin::default(),
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
        ))
        .add_systems(Startup, setup_camera_system)
        .add_systems(EguiPrimaryContextPass, ui_example_system)
        .run();
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
