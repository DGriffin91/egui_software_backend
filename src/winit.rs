use crate::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use core::fmt::{Display, Formatter};
use core::num::NonZeroU32;
use core::time::Duration;
use egui::Context;
use softbuffer::SoftBufferError;
use std::boxed::Box;
use std::error::Error;
use std::format;
use std::rc::Rc;
use std::string::String;
use std::string::ToString;
use std::time::Instant;
use std::vec::Vec;
use winit::application::ApplicationHandler;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::{Icon, Window, WindowId};

#[derive(Debug)]
pub enum SoftwareBackendAppError {
    SoftBuffer {
        soft_buffer_error: Box<dyn Error>,
        function: &'static str,
    },
    EventLoop(Box<dyn Error>),
    /// The event loop has errored in addition to an error from the software renderer
    SuppressedEventLoop {
        event_loop_error: Box<dyn Error>,
        suppressed: Box<SoftwareBackendAppError>,
    },

    /// Error when calling winit create_window
    CreateWindowOs(Box<dyn Error>),
}

impl Display for SoftwareBackendAppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            SoftwareBackendAppError::SoftBuffer { function, .. } => {
                f.write_str("error calling ")?;
                f.write_str(function)
            }
            SoftwareBackendAppError::EventLoop(_) => f.write_str("winit event loop has errored"),
            SoftwareBackendAppError::SuppressedEventLoop { .. } => {
                f.write_str("software renderer and winit event loop have both errored")
            }
            SoftwareBackendAppError::CreateWindowOs(_) => {
                f.write_str("os error calling winit::create_window")
            }
        }
    }
}

impl Error for SoftwareBackendAppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SoftwareBackendAppError::SuppressedEventLoop { suppressed, .. } => {
                Some(suppressed as &dyn Error)
            }
            _ => None,
        }
    }
}

impl SoftwareBackendAppError {
    fn soft_buffer(
        function: &'static str,
    ) -> impl FnOnce(SoftBufferError) -> SoftwareBackendAppError {
        move |error| Self::SoftBuffer {
            soft_buffer_error: Box::new(error),
            function,
        }
    }
}

/// Easily constructable winit application.
struct WinitApp<EguiApp: App, Init, InitSurface, Handler> {
    /// Closure to initialize `state`.
    init: Init,

    init_surface: InitSurface,

    /// Closure to run on window events.
    event: Handler,

    /// Contained state.
    state: Option<Rc<Window>>,

    /// Contained surface state.
    surface_state: Option<WinitSurfaceState<EguiApp>>,

    error: Option<SoftwareBackendAppError>,
}

impl<EguiApp: App, Init, InitSurface, Handler> WinitApp<EguiApp, Init, InitSurface, Handler>
where
    Init: FnMut(&ActiveEventLoop) -> Result<Rc<Window>, SoftwareBackendAppError>,
    InitSurface: FnMut(
        &ActiveEventLoop,
        &mut Rc<Window>,
    ) -> Result<WinitSurfaceState<EguiApp>, SoftwareBackendAppError>,
    Handler: FnMut(
        &mut Rc<Window>,
        Option<&mut WinitSurfaceState<EguiApp>>,
        Event<()>,
        &ActiveEventLoop,
    ) -> Result<(), SoftwareBackendAppError>,
{
    /// Create a new application.
    pub(crate) fn new(init: Init, init_surface: InitSurface, event: Handler) -> Self {
        Self {
            init,
            init_surface,
            event,
            state: None,
            surface_state: None,
            error: None,
        }
    }
}

impl<EguiApp: App, Init, InitSurface, Handler> ApplicationHandler
    for WinitApp<EguiApp, Init, InitSurface, Handler>
where
    Init: FnMut(&ActiveEventLoop) -> Result<Rc<Window>, SoftwareBackendAppError>,
    InitSurface: FnMut(
        &ActiveEventLoop,
        &mut Rc<Window>,
    ) -> Result<WinitSurfaceState<EguiApp>, SoftwareBackendAppError>,
    Handler: FnMut(
        &mut Rc<Window>,
        Option<&mut WinitSurfaceState<EguiApp>>,
        Event<()>,
        &ActiveEventLoop,
    ) -> Result<(), SoftwareBackendAppError>,
{
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if el.exiting() {
            return;
        }
        if let Some(state) = self.state.as_mut() {
            if self.surface_state.is_some() {
                return;
            }

            match (self.init_surface)(el, state) {
                Ok(ss) => self.surface_state = Some(ss),
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                }
            }
        } else {
            debug_assert!(self.surface_state.is_none());
            let state = match (self.init)(el) {
                Ok(state) => state,
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                    return;
                }
            };
            let state = self.state.insert(state);

            match (self.init_surface)(el, state) {
                Ok(ss) => self.surface_state = Some(ss),
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                }
            };
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if event_loop.exiting() {
            return;
        }

        let Some(state) = self.state.as_mut() else {
            return;
        };

        let surface_state = self.surface_state.as_mut();

        if let Err(e) = (self.event)(
            state,
            surface_state,
            Event::WindowEvent { window_id, event },
            event_loop,
        ) {
            self.error = Some(e);
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }

        if let Some(state) = self.state.as_mut() {
            if let Err(e) = (self.event)(
                state,
                self.surface_state.as_mut(),
                Event::AboutToWait,
                event_loop,
            ) {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface_state.take();
    }
}

struct WinitSurfaceState<T: App> {
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    egui_ctx: Context,
    egui_app: T,
    egui_winit: egui_winit::State,
}

pub trait App {
    fn update(&mut self, ctx: &Context);

    fn on_exit(&mut self, _ctx: &Context) {}
}

#[derive(Debug, Clone)]
pub struct SoftwareBackendAppConfiguration {
    width: f64,
    height: f64,
    title: Option<String>,
    icon: Option<Icon>,
    show_render_time_in_title: bool,
    allow_raster_opt: bool,
    convert_tris_to_rects: bool,
    caching: bool,
    resizeable: bool,
}

impl SoftwareBackendAppConfiguration {
    pub const fn new() -> Self {
        Self {
            //CGA
            width: 320.0,
            height: 200.0,

            //Recommended defaults
            title: None,
            icon: None,
            show_render_time_in_title: false,
            allow_raster_opt: true,
            convert_tris_to_rects: true,
            caching: true,
            resizeable: true,
        }
    }

    pub const fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    pub fn title(mut self, title: impl ToString) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Set the icon to the given rgba data.
    pub fn icon(mut self, icon_rgba: Vec<u8>, width: u32, height: u32) -> Self {
        self.icon = Icon::from_rgba(icon_rgba, width, height).ok();
        self
    }

    pub fn no_icon(mut self) -> Self {
        self.icon = None;
        self
    }

    pub const fn show_render_time_in_title(mut self, show_render_time_in_title: bool) -> Self {
        self.show_render_time_in_title = show_render_time_in_title;
        self
    }

    pub const fn allow_raster_opt(mut self, allow_raster_opt: bool) -> Self {
        self.allow_raster_opt = allow_raster_opt;
        self
    }

    pub const fn convert_tris_to_rects(mut self, convert_tris_to_rects: bool) -> Self {
        self.convert_tris_to_rects = convert_tris_to_rects;
        self
    }

    pub const fn caching(mut self, caching: bool) -> Self {
        self.caching = caching;
        self
    }

    pub const fn resizeable(mut self, resizeable: bool) -> Self {
        self.resizeable = resizeable;
        self
    }
}

impl Default for SoftwareBackendAppConfiguration {
    fn default() -> Self {
        Self::new()
    }
}

const ONE_PIXEL: NonZeroU32 = NonZeroU32::new(1).unwrap();

pub fn run_app_with_software_backend<T: App>(
    settings: SoftwareBackendAppConfiguration,
    mut egui_app_factory: impl FnMut(Context) -> T,
) -> Result<(), SoftwareBackendAppError> {
    let mut egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::Bgra)
        .with_allow_raster_opt(settings.allow_raster_opt)
        .with_convert_tris_to_rects(settings.convert_tris_to_rects)
        .with_caching(settings.caching);

    let show_fps = settings.show_render_time_in_title;

    let event_loop: EventLoop<()> =
        EventLoop::new().map_err(|e| SoftwareBackendAppError::EventLoop(Box::new(e)))?;

    let softbuffer_context = softbuffer::Context::new(event_loop.owned_display_handle()).map_err(
        SoftwareBackendAppError::soft_buffer("softbuffer::Context::new"),
    )?;

    let mut last_update = Instant::now();
    let mut frame_count: u32 = 0;

    let mut app =
        WinitApp::new(
            |elwt: &ActiveEventLoop| {
                let window = elwt.create_window(
                    Window::default_attributes()
                        .with_inner_size(winit::dpi::LogicalSize::new(
                            settings.width,
                            settings.height,
                        ))
                        .with_title(
                            settings
                                .title
                                .clone()
                                .unwrap_or_else(|| "egui software backend".to_string()),
                        )
                        .with_window_icon(settings.icon.clone())
                        .with_resizable(settings.resizeable),
                );

                window
                    .map_err(|ose| SoftwareBackendAppError::CreateWindowOs(Box::new(ose)))
                    .map(Rc::new)
            },
            move |_elwt, window: &mut Rc<Window>| {
                let surface = softbuffer::Surface::new(&softbuffer_context, window.clone())
                    .map_err(SoftwareBackendAppError::soft_buffer(
                        "softbuffer::Surface::new",
                    ))?;
                let egui_ctx = Context::default();
                let egui_winit = egui_winit::State::new(
                    egui_ctx.clone(),
                    egui::ViewportId::ROOT,
                    &window,
                    Some(window.scale_factor() as f32),
                    None,
                    None,
                );

                let egui_app = egui_app_factory(egui_ctx.clone());

                Ok(WinitSurfaceState {
                    surface,
                    egui_app,
                    egui_ctx,
                    egui_winit,
                })
            },
            |window: &mut Rc<Window>,
             app: Option<&mut WinitSurfaceState<T>>,
             event: Event<()>,
             elwt: &ActiveEventLoop| {
                elwt.set_control_flow(ControlFlow::Wait);
                let Some(app) = app else {
                    return Ok(());
                };

                let Event::WindowEvent {
                    window_id,
                    event: window_event,
                } = event
                else {
                    return Ok(());
                };

                if window_id != window.id() {
                    return Ok(());
                }

                let response = app.egui_winit.on_window_event(window, &window_event);

                if response.repaint {
                    // Redraw when egui says it's necessary (e.g., mouse move, key press):
                    window.request_redraw();
                }

                match window_event {
                    WindowEvent::RedrawRequested => {
                        let size = window.inner_size();
                        let width = NonZeroU32::new(size.width).unwrap_or(ONE_PIXEL);
                        let height = NonZeroU32::new(size.height).unwrap_or(ONE_PIXEL);

                        app.surface.resize(width, height).map_err(
                            SoftwareBackendAppError::soft_buffer("softbuffer::Surface::resize"),
                        )?;

                        let raw_input = app.egui_winit.take_egui_input(window);

                        let full_output = app.egui_ctx.run(raw_input, |ctx| {
                            app.egui_app.update(ctx);
                        });

                        let clipped_primitives = app
                            .egui_ctx
                            .tessellate(full_output.shapes, full_output.pixels_per_point);

                        let mut buffer = app.surface.buffer_mut().map_err(
                            SoftwareBackendAppError::soft_buffer("softbuffer::Surface::buffer_mut"),
                        )?;
                        buffer.fill(0); // CLEAR

                        let buffer_ref = &mut BufferMutRef::new(
                            bytemuck::cast_slice_mut(&mut buffer),
                            width.get() as usize,
                            height.get() as usize,
                        );

                        egui_software_render.render(
                            buffer_ref,
                            &clipped_primitives,
                            &full_output.textures_delta,
                            full_output.pixels_per_point,
                        );

                        buffer
                            .present()
                            .map_err(SoftwareBackendAppError::soft_buffer(
                                "softbuffer::Buffer::present",
                            ))?;

                        if show_fps {
                            frame_count += 1;

                            let now = Instant::now();
                            if now.duration_since(last_update) >= Duration::from_secs(1) {
                                let fps = frame_count as f64
                                    / now.duration_since(last_update).as_secs_f64();
                                window.set_title(&format!(
                                    "egui software backend - {:.2}ms",
                                    1000.0 / fps
                                ));
                                frame_count = 0;
                                last_update = now;
                            }
                        }
                    }

                    WindowEvent::CloseRequested => {
                        app.egui_app.on_exit(&app.egui_ctx);
                        elwt.exit();
                    }
                    _ => {}
                }

                Ok(())
            },
        );

    if let Err(event_loop_error) = event_loop.run_app(&mut app) {
        if let Some(app_err) = app.error.take() {
            return Err(SoftwareBackendAppError::SuppressedEventLoop {
                event_loop_error: Box::new(event_loop_error),
                suppressed: Box::new(app_err),
            });
        }

        return Err(SoftwareBackendAppError::EventLoop(Box::new(
            event_loop_error,
        )));
    }

    app.error.take().map(Err).unwrap_or(Ok(()))
}
