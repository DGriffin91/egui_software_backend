# CPU software render backend for [egui](https://github.com/emilk/egui)

![demo](demo.png)

```rs
let ctx = egui::Context::default();
let mut demo = egui_demo_lib::DemoWindows::default();
let mut sw_render = EguiSoftwareRender::new(ColorFieldOrder::Bgra);

let out = ctx.run(raw_input, |ctx| {
    demo.ui(ctx);
});

let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);

sw_render.render(buffer, &primitives, &out.textures_delta, out.pixels_per_point);
```

## winit quickstart
```rust
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
    fn update(&mut self, ctx: &egui::Context, _backend: &mut SoftwareBackend) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello World!");
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
```

## Other examples
- bevy + softbuffer see examples/bevy_example folder