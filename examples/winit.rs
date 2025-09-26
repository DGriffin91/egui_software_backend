// Based on: https://github.com/rust-windowing/softbuffer/blob/046de9228d89369151599f3f50dc4b75bd5e522b/examples/winit.rs

use argh::FromArgs;
use egui_demo_lib::ColorTest;
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::Window;

use crate::winit_app::WinitApp;

#[path = "../examples/utils/winit_app.rs"]
mod winit_app;

#[derive(FromArgs, Copy, Clone)]
/// `bevy` example
struct Args {
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

struct AppState {
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
}

fn main() {
    let args: Args = argh::from_env();

    let mut egui_demo = egui_demo_lib::DemoWindows::default();
    let mut egui_color_test = ColorTest::default();
    let mut egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::BGRA)
        .with_allow_raster_opt(!args.no_opt)
        .with_convert_tris_to_rects(!args.no_rect)
        .with_caching(!args.direct);

    let event_loop: EventLoop<()> = EventLoop::new().unwrap();

    let softbuffer_context = softbuffer::Context::new(event_loop.owned_display_handle()).unwrap();

    let mut last_update = Instant::now();
    let mut frame_count: u32 = 0;

    let mut app = WinitApp::new(
        |elwt: &ActiveEventLoop| {
            let window = elwt.create_window(
                Window::default_attributes()
                    .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 900.0))
                    .with_title("egui software backend"),
            );
            Rc::new(window.unwrap())
        },
        |_elwt, window: &mut Rc<Window>| {
            let surface = softbuffer::Surface::new(&softbuffer_context, window.clone()).unwrap();
            let egui_ctx = egui::Context::default();
            let egui_winit = egui_winit::State::new(
                egui_ctx.clone(),
                egui::ViewportId::ROOT,
                &window,
                Some(window.scale_factor() as f32),
                None,
                None,
            );

            AppState {
                surface,
                egui_ctx,
                egui_winit,
            }
        },
        |window: &mut Rc<Window>,
         app: Option<&mut AppState>,
         event: Event<()>,
         elwt: &ActiveEventLoop| {
            elwt.set_control_flow(ControlFlow::Wait);
            let Some(app) = app else {
                return;
            };

            egui_extras::install_image_loaders(&app.egui_ctx);

            let Event::WindowEvent {
                window_id,
                event: window_event,
            } = event
            else {
                return;
            };

            if window_id != window.id() {
                return;
            }

            let response = app.egui_winit.on_window_event(window, &window_event);

            if response.repaint {
                // Redraw when egui says it's needed (e.g., mouse move, key press):
                window.request_redraw();
            }

            match window_event {
                WindowEvent::RedrawRequested => {
                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width.max(1), size.height.max(1))
                    };
                    app.surface
                        .resize(
                            NonZeroU32::new(width).unwrap(),
                            NonZeroU32::new(height).unwrap(),
                        )
                        .unwrap();

                    let raw_input = app.egui_winit.take_egui_input(window);

                    let full_output = app.egui_ctx.run(raw_input, |ctx| {
                        egui_demo.ui(ctx);

                        egui::Window::new("Color Test").show(ctx, |ui| {
                            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                                egui_color_test.ui(ui);
                            });
                        });

                        #[cfg(feature = "raster_stats")]
                        egui::Window::new("Stats").show(ctx, |ui| {
                            egui_software_render.stats.render(ui);
                        });
                    });

                    let clipped_primitives = app
                        .egui_ctx
                        .tessellate(full_output.shapes, full_output.pixels_per_point);

                    let mut buffer = app.surface.buffer_mut().unwrap();
                    buffer.fill(0); // CLEAR

                    let buffer_ref = &mut BufferMutRef::new(
                        bytemuck::cast_slice_mut(&mut buffer),
                        width as usize,
                        height as usize,
                    );

                    egui_software_render.render(
                        buffer_ref,
                        &clipped_primitives,
                        &full_output.textures_delta,
                        full_output.pixels_per_point,
                    );

                    buffer.present().unwrap();

                    frame_count += 1;
                    let now = Instant::now();
                    if now.duration_since(last_update) >= Duration::from_secs(1) {
                        let fps =
                            frame_count as f64 / now.duration_since(last_update).as_secs_f64();
                        window.set_title(&format!("egui software backend - {:.2}ms", 1000.0 / fps));
                        frame_count = 0;
                        last_update = now;
                    }
                }

                WindowEvent::CloseRequested => {
                    elwt.exit();
                }
                _ => {}
            }
        },
    );

    event_loop.run_app(&mut app).unwrap();
}
