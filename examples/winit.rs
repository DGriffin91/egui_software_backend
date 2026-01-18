use egui::Ui;
use egui::Vec2;
use egui::ViewportCommand;
use egui_demo_lib::ColorTest;
use egui_demo_lib::DemoWindows;
use egui_software_backend::SoftwareRenderCaching;
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

struct EguiApp {
    demo: DemoWindows,
    color_test: ColorTest,
    frame_times: Vec<f32>,
}

impl EguiApp {
    fn new(context: egui::Context) -> Self {
        egui_extras::install_image_loaders(&context);
        EguiApp {
            demo: DemoWindows::default(),
            color_test: ColorTest::default(),
            frame_times: Vec::new(),
        }
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |_ui| {
            self.demo.ui(ctx);

            egui::Window::new("Color Test").show(ctx, |ui| {
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    self.color_test.ui(ui);
                });
            });
        });
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |_ui| {
            self.ui(ctx);
        });
    }
}

fn software_backend_ui(backend: &mut SoftwareBackend, ui: &mut Ui) {
    let old = backend.caching();
    let mut new = old;
    egui::ComboBox::from_label("SoftwareRenderCaching")
        .selected_text(format!("{old:?}"))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut new, SoftwareRenderCaching::BlendTiled, "BlendTiled");
            ui.selectable_value(&mut new, SoftwareRenderCaching::MeshTiled, "MeshTiled");
            ui.selectable_value(&mut new, SoftwareRenderCaching::Mesh, "Mesh");
            ui.selectable_value(&mut new, SoftwareRenderCaching::Direct, "Direct");
        });
    if new != old {
        backend.set_caching(new);
    }
}

impl egui_software_backend::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, backend: &mut SoftwareBackend) {
        egui::CentralPanel::default().show(ctx, |_ui| {
            self.ui(ctx);

            #[cfg(feature = "raster_stats")]
            egui::Window::new("Stats").show(ctx, |ui| {
                backend.stats().render(ui);
            });

            egui::Window::new("Software Backend").show(ctx, |ui| software_backend_ui(backend, ui));

            if self.frame_times.len() < 100 {
                self.frame_times
                    .push(backend.last_frame_time().unwrap_or_default().as_secs_f32());
            } else {
                let avg =
                    (self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32) * 1000.0;
                ctx.send_viewport_cmd(ViewportCommand::Title(format!("Frame Time {avg:.2}ms")));
                self.frame_times.clear();
            }
        });
    }
}

fn main() {
    let inner_size = Vec2::new(1600.0, 900.0);

    if std::env::var("USE_EFRAME").unwrap_or_default() == "true" {
        eprintln!("WILL RUN USING EFRAME");
        //eframe for reference.
        let mut native_options = eframe::NativeOptions::default();
        native_options.run_and_return = true;
        native_options.viewport.resizable = Some(false);
        native_options.viewport.title = Some("Viewport Command Tester".to_string());
        native_options.viewport.inner_size = Some(inner_size);
        eframe::run_native(
            "Viewport Command Tester",
            native_options,
            Box::new(|cc| Ok(Box::new(EguiApp::new(cc.egui_ctx.clone())))),
        )
        .expect("Failed to run app");
    } else {
        eprintln!("WILL RUN USING SWR");

        let settings = SoftwareBackendAppConfiguration::new()
            .inner_size(Some(inner_size))
            .caching(egui_software_backend::SoftwareRenderCaching::Mesh)
            .title(Some(String::from("egui software backend")));
        egui_software_backend::run_app_with_software_backend(settings, EguiApp::new)
            //Can fail if winit fails to create the window
            .expect("Failed to run app");
    }
}
