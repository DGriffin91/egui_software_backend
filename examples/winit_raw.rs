// Based on: https://github.com/rust-windowing/softbuffer/blob/046de9228d89369151599f3f50dc4b75bd5e522b/examples/winit.rs

use argh::FromArgs;
use core::num::NonZeroU32;
use egui_demo_lib::ColorTest;
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::rc::Rc;
use std::time::Instant;
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
    let mut egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::Bgra)
        .with_allow_raster_opt(!args.no_opt)
        .with_convert_tris_to_rects(!args.no_rect)
        .with_caching(if args.direct {
            egui_software_backend::SoftwareRenderCaching::Direct
        } else {
            egui_software_backend::SoftwareRenderCaching::BlendTiled
        });

    let event_loop: EventLoop<()> = EventLoop::new().unwrap();

    let softbuffer_context = softbuffer::Context::new(event_loop.owned_display_handle()).unwrap();

    let mut frame_times = Vec::new();
    let mut last_frame_time = Instant::now();

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
                            egui_software_render.display_stats(ui);
                        });
                    });

                    let clipped_primitives = app
                        .egui_ctx
                        .tessellate(full_output.shapes, full_output.pixels_per_point);

                    let mut buffer = app.surface.buffer_mut().unwrap();

                    let buffer_ref = &mut BufferMutRef::new(
                        bytemuck::cast_slice_mut(&mut buffer),
                        width,
                        height,
                    );
                    let redraw_everything_this_frame =
                        egui_software_render.cached_size() != (buffer_ref.width, buffer_ref.height);
                    let dirty_rect = egui_software_render.render(
                        buffer_ref,
                        redraw_everything_this_frame,
                        clipped_primitives,
                        &full_output.textures_delta,
                        full_output.pixels_per_point,
                    );
                    if !dirty_rect.is_empty() {
                        let dirty_rect = softbuffer::Rect {
                            x: dirty_rect.min_x,
                            y: dirty_rect.min_y,
                            width: NonZeroU32::new(dirty_rect.width()).expect("non zero rect"),
                            height: NonZeroU32::new(dirty_rect.height()).expect("non zero rect"),
                        };
                        buffer.present_with_damage(&[dirty_rect]).unwrap();
                    }

                    let now = Instant::now();
                    if frame_times.len() < 100 {
                        frame_times.push(now.duration_since(last_frame_time).as_secs_f32());
                    } else {
                        let avg =
                            (frame_times.iter().sum::<f32>() / frame_times.len() as f32) * 1000.0;
                        window.set_title(&format!("Frame Time {avg:.2}ms"));
                        frame_times.clear();
                    }
                    last_frame_time = now;
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
