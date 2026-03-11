use egui::{Context, vec2};
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

struct App {
    dummy_data: String,
    frame_counter: u128,
}

impl egui_software_backend::App for App {
    fn update(&mut self, ctx: &Context, _software_backend: &mut SoftwareBackend) {
        self.frame_counter += 1;
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.frame_counter > 166 {
                self.frame_counter = 166;
            }
            ui.label(format!("pixels_per_point: {}", ctx.pixels_per_point()));
            ui.label(format!("zoom_factor: {}", ctx.zoom_factor()));
            ui.label(format!("style: {:?}", &ctx.style().text_styles));
            ui.label("Frame Counter: 166");
            ui.label(format!("Frame Counter: {}", self.frame_counter));
            ui.text_edit_singleline(&mut self.dummy_data);
            if self.frame_counter > 166 {
                self.frame_counter = 166;
            }
            ui.label(format!("Frame Counter: {}", self.frame_counter));
            ui.label("Frame Counter: 166");
        });
    }
}

fn main() {
    #[allow(unused)]
    let mut conf = SoftwareBackendAppConfiguration::default().inner_size(Some(vec2(400.0, 220.0)));
    //conf.caching = false;

    egui_software_backend::run_app_with_software_backend(conf, |_ctx| App {
        dummy_data: "".to_string(),
        frame_counter: 0,
    })
    .unwrap()
}
