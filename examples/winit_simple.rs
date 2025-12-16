use egui_software_backend::SoftwareBackendAppConfiguration;

struct EguiApp {}

impl EguiApp {
    fn new(context: egui::Context) -> Self {
        egui_extras::install_image_loaders(&context);
        EguiApp {}
    }
}

impl egui_software_backend::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello World!");
        });
    }
}

fn main() {
    let settings = SoftwareBackendAppConfiguration::new()
        .width(500.0)
        .height(300.0)
        .resizeable(false)
        .title("Simple example");

    egui_software_backend::run_app_with_software_backend(settings, EguiApp::new)
        //Can fail if winit fails to create the window
        .expect("Failed to run app")
}
