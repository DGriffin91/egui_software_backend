// Note: Not all platforms support every feature. Ideally the winit software backend has feature parity here with
// eframe. Test against eframe with USE_EFRAME=true env var.

use eframe::Frame;
use egui::{Context, CursorGrab, SystemTheme, Vec2, ViewportCommand, vec2};
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct EguiApp {
    title_box: String,
    paste_box: String,
    enable_paste_action: bool,
}

impl EguiApp {
    fn new(context: Context) -> Self {
        egui_extras::install_image_loaders(&context);
        EguiApp::default()
    }

    fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::scroll_area::ScrollArea::both().show(ui, |ui| {
                if ui
                    .button("Visible(false) -> Wait 5s -> Visible(true)")
                    .clicked()
                {
                    let ctx = ctx.clone();
                    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(5));
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    });
                }

                if ui.button("Visible(false) + Close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }

                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }

                if ui.button("Fullscreen(true)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                }
                if ui.button("Fullscreen(false)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                }

                if ui
                    .button("Visible(false) -> Fullscreen(true) -> Wait 5s -> Visible(true)")
                    .clicked()
                {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                    let ctx = ctx.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(5));
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    });
                }

                if ui.button("Resizable(false)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
                }

                if ui.button("Resizable(true)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
                }

                if ui.button("InnerSize(800,800)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(800f32, 800f32)));
                }

                if ui.button("InnerSize(700,100)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(700f32, 100f32)));
                }

                ui.separator();
                ui.text_edit_singleline(&mut self.title_box);
                if ui.button("SetTitle").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Title(self.title_box.clone()));
                }
                ui.separator();
                ui.text_edit_multiline(&mut self.paste_box);
                if ui
                    .button("Enable/Disable the 3 hover buttons below")
                    .clicked()
                {
                    self.enable_paste_action = !self.enable_paste_action;
                }

                ui.add_enabled_ui(self.enable_paste_action, |ui| {
                    if ui.button("Copy (On Hover)").hovered() {
                        self.enable_paste_action = false;
                        ctx.send_viewport_cmd(ViewportCommand::RequestCopy);
                    }
                    if ui.button("Paste (On Hover)").hovered() {
                        self.enable_paste_action = false;
                        ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
                    }
                    if ui.button("Cut (On Hover)").hovered() {
                        self.enable_paste_action = false;
                        ctx.send_viewport_cmd(ViewportCommand::RequestCut);
                    }
                });

                ui.separator();

                if ui.button("Decorations(false)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                }

                if ui.button("Decorations(true)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
                }

                if ui.button("CursorGrab(None)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::None));
                }

                if ui.button("CursorGrab(Confined)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::Confined));
                }

                if ui.button("CursorGrab(Locked)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::Locked));
                }

                if ui.button("Minimized(true)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
                if ui.button("Minimized(false)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                }
                if ui.button("Minimized(true) 3s delay").clicked() {
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(3));
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                    });
                }
                if ui.button("Minimized(false) 3s delay").clicked() {
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(3));
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    });
                }

                if ui.button("Maximized(true)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                }
                if ui.button("Maximized(false)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                }
                if ui.button("Maximized(true) 3s delay").clicked() {
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(3));
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                    });
                }
                if ui.button("Maximized(false) 3s delay").clicked() {
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(3));
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                    });
                }

                if ui.button("SetTheme(Dark)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::SetTheme(SystemTheme::Dark));
                }
                if ui.button("SetTheme(Light)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::SetTheme(SystemTheme::Light));
                }
                if ui.button("SetTheme(SystemDefault)").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::SetTheme(SystemTheme::SystemDefault));
                }
                if ui.button("Focus 3s delay").clicked() {
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(3));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    });
                }
            });
        });
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        EguiApp::update(self, ctx);
    }
}

impl egui_software_backend::App for EguiApp {
    fn update(&mut self, ctx: &Context, _backend: &mut SoftwareBackend) {
        EguiApp::update(self, ctx);
    }
}

fn main() {
    let settings = SoftwareBackendAppConfiguration::new()
        .inner_size(Some(Vec2::new(500f32, 300f32)))
        .resizable(Some(false))
        .title(Some("Viewport Command Tester".to_string()));

    if std::env::var("USE_EFRAME").unwrap_or_default() == "true" {
        eprintln!("WILL RUN USING EFRAME");
        //eframe for reference.
        let mut native_options = eframe::NativeOptions::default();
        native_options.run_and_return = true;
        native_options.viewport.resizable = Some(false);
        native_options.viewport.title = Some("Viewport Command Tester".to_string());
        native_options.viewport.inner_size = Some(Vec2::new(300f32, 300f32));
        eframe::run_native(
            "Viewport Command Tester",
            native_options,
            Box::new(|cc| Ok(Box::new(EguiApp::new(cc.egui_ctx.clone())))),
        )
        .expect("Failed to run app");
    } else {
        eprintln!("WILL RUN USING SWR");

        egui_software_backend::run_app_with_software_backend(settings, EguiApp::new)
            //Can fail if winit fails to create the window
            .expect("Failed to run app");
    }

    eprintln!("EVENT LOOP EXIT WITHOUT ERROR. WAITING 5s");
    thread::sleep(Duration::from_secs(5));
    eprintln!("EXITING NOW");
    std::process::exit(0);
}
