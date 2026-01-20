# CPU software render backend for [egui](https://github.com/emilk/egui)
![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg) [![Crates.io](https://img.shields.io/crates/v/egui_software_backend.svg)](https://crates.io/crates/egui_software_backend)
[![Docs](https://docs.rs/egui_software_backend/badge.svg)](https://docs.rs/egui_software_backend/latest/egui_software_backend/)

![demo](demo.png)

```rs
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
let buffer = &mut vec![[0u8; 4]; 512 * 512];
let mut buffer_ref = BufferMutRef::new(buffer, 512, 512);
let ctx = egui::Context::default();
let mut demo = egui_demo_lib::DemoWindows::default();
let mut sw_render = EguiSoftwareRender::new(ColorFieldOrder::Bgra);

let out = ctx.run(egui::RawInput::default(), |ctx| {
    demo.ui(ctx);
});

let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);

sw_render.render(
    &mut buffer_ref,
    &primitives,
    &out.textures_delta,
    out.pixels_per_point,
);
```

## winit quickstart
```rust
use egui::vec2;
use egui_software_backend::{SoftwareBackend, SoftwareBackendAppConfiguration};

struct EguiApp {}

impl EguiApp {
    fn new(context: egui::Context) -> Self {
        egui_extras::install_image_loaders(&context);
        EguiApp {}
    }
}

impl egui_software_backend::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _backend: &mut SoftwareBackend) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello World!");
        });
    }
}

fn main() {
    let settings = SoftwareBackendAppConfiguration::new()
        .inner_size(Some(vec2(500.0, 300.0)))
        .title(Some("Simple example".to_string()));

    egui_software_backend::run_app_with_software_backend(settings, EguiApp::new)
        //Can fail if winit fails to create the window
        .expect("Failed to run app")
}
```

## Other examples
- bevy + softbuffer see examples/bevy_example folder
