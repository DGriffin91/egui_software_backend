use egui::Vec2;
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

struct EguiApp {}

impl EguiApp {
    fn new(context: egui::Context) -> Self {
        egui_extras::install_image_loaders(&context);
        EguiApp {}
    }
}

impl egui_software_backend::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, backend: &mut SoftwareBackend) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let last_frame_time = backend.last_frame_time().unwrap_or_default();

            ui.label("Hello World!");
            ui.label(format!("Frame Time {}ns", last_frame_time.as_nanos()));
            ui.label(format!("Frame Time {}ms", last_frame_time.as_millis()));
        });
    }
}

fn main() {
    let settings = SoftwareBackendAppConfiguration::new()
        .inner_size(Some(Vec2::new(500f32, 300f32)))
        .resizable(Some(false))
        .title(Some("Simple example".to_string()));

    egui_software_backend::run_app_with_software_backend(settings, EguiApp::new)
        //Can fail if winit fails to create the window
        .expect("Failed to run app")
}
