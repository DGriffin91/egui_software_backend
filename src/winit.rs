use crate::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use egui::{
    Context, CursorGrab, IconData, Pos2, SystemTheme, Vec2, ViewportBuilder, ViewportCommand,
    WindowLevel, X11WindowType,
};
use softbuffer::SoftBufferError;
use std::boxed::Box;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::mem;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::rc::Rc;
use std::string::String;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::vec::Vec;
use winit::application::ApplicationHandler;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::{CursorGrabMode, Fullscreen, Icon, Theme, Window, WindowButtons, WindowId};

/// Errors that can occur when using the egui software backend with winit.
#[derive(Debug)]
pub enum SoftwareBackendAppError {
    /// A softbuffer error has occurred.
    /// The softbuffer crate is used to manage the pixel buffer of the window.
    SoftBuffer {
        soft_buffer_error: Box<dyn Error>,
        function: &'static str,
    },

    /// Some event loop error has occurred.
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

// Doing what this suggest would make it impossible for the compiler
// to optimize the layout of this enum, causing state machine transitions to be more expensive!
#[allow(clippy::large_enum_variant)]
/// Winit App State machine, that handles all states of our application.
enum WinitAppStateMachine<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp> {
    /// The app has died, either without or with some sort of error.
    Dead(Option<SoftwareBackendAppError>),

    /// The app is configured, but the window has not been created yet.
    /// Transitions into WindowInitialized (resume).
    Configured(ConfiguredAppState<EguiApp, EguiAppFactory>),

    /// The window has been initialized.
    /// Transitions into Dead (error) or Running (resume).
    WindowInitialized(WindowInitializedAppState<EguiApp, EguiAppFactory>),

    /// The app is running.
    /// Transitions into Dead (quit/error) or WindowInitialized (suspend)
    Running(RunningEguiAppState<EguiApp, EguiAppFactory>),
}

struct ConfiguredAppState<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp> {
    /////////// DANGER ZONE DO NOT CHANGE THE ORDER OF THOSE FIELDS ////////////////////
    // WAYLAND BUG: The wayland clipboard blows up with a segmentation fault if
    // If the fields are dropped in the wrong order. Literally any other platform does not care!
    egui_context: Context,
    softbuffer_context: softbuffer::Context<OwnedDisplayHandle>,

    /////////////////// END OF DANGER ZONE//////////////////////////////////////
    config: SoftwareBackendAppConfiguration,
    software_backend: SoftwareBackend,
    renderer: EguiSoftwareRender,
    egui_app_factory: EguiAppFactory,
}

struct WindowInitializedAppState<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp> {
    /////////// DANGER ZONE DO NOT CHANGE THE ORDER OF THOSE FIELDS ////////////////////
    // WAYLAND BUG: The wayland clipboard blows up with a segmentation fault if
    // If the fields are dropped in the wrong order. Literally any other platform does not care!
    egui_context: Context,
    softbuffer_context: softbuffer::Context<OwnedDisplayHandle>,
    window: Rc<Window>,
    /////////////////// END OF DANGER ZONE//////////////////////////////////////
    config: SoftwareBackendAppConfiguration,
    software_backend: SoftwareBackend,
    renderer: EguiSoftwareRender,
    egui_app_factory: EguiAppFactory,
}

struct RunningEguiAppState<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp> {
    /////////// DANGER ZONE DO NOT CHANGE THE ORDER OF THOSE FIELDS ////////////////////
    // WAYLAND BUG: The wayland clipboard blows up with a segmentation fault if
    // If the fields are dropped in the wrong order. Literally any other platform does not care!
    egui_context: Context,
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    egui_winit: egui_winit::State,
    window: Rc<Window>,
    /////////////////// END OF DANGER ZONE//////////////////////////////////////
    config: SoftwareBackendAppConfiguration,
    software_backend: SoftwareBackend,
    renderer: EguiSoftwareRender,
    egui_app_factory: EguiAppFactory,
    softbuffer_context: softbuffer::Context<OwnedDisplayHandle>,
    egui_app: EguiApp,
    fullscreen: bool,
    visible: bool,
    input_events: Vec<egui::Event>,
}

impl<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp> Default
    for WinitAppStateMachine<EguiApp, EguiAppFactory>
{
    fn default() -> Self {
        Self::Dead(None)
    }
}

impl<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp>
    ConfiguredAppState<EguiApp, EguiAppFactory>
{
    pub(crate) fn create_window(
        mut self,
        elwt: &ActiveEventLoop,
    ) -> Result<WindowInitializedAppState<EguiApp, EguiAppFactory>, SoftwareBackendAppError> {
        // !BUG IN WAYLAND!
        // if resizeable false during window creation you can never make the window resizable again.
        // We always force None before we call into egui_winit and set it on the window object later.
        let resizeable = self
            .config
            .viewport_builder
            .resizable
            .take()
            .unwrap_or(true);

        let window =
            egui_winit::create_window(&self.egui_context, elwt, &self.config.viewport_builder);

        self.config.viewport_builder.resizable = Some(resizeable);

        let window = window
            .map_err(|ose| SoftwareBackendAppError::CreateWindowOs(Box::new(ose)))
            .map(Rc::new)?;

        window.set_resizable(resizeable);

        Ok(WindowInitializedAppState {
            config: self.config,
            software_backend: self.software_backend,
            renderer: self.renderer,
            egui_context: self.egui_context,
            egui_app_factory: self.egui_app_factory,
            softbuffer_context: self.softbuffer_context,
            window,
        })
    }
}

impl<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp>
    WindowInitializedAppState<EguiApp, EguiAppFactory>
{
    pub(crate) fn create_surface(
        mut self,
    ) -> Result<RunningEguiAppState<EguiApp, EguiAppFactory>, SoftwareBackendAppError> {
        let surface = softbuffer::Surface::new(&self.softbuffer_context, self.window.clone())
            .map_err(SoftwareBackendAppError::soft_buffer(
                "softbuffer::Surface::new",
            ))?;

        let egui_winit = egui_winit::State::new(
            self.egui_context.clone(),
            egui::ViewportId::ROOT,
            &self.window,
            Some(self.window.scale_factor() as f32),
            None,
            None,
        );

        let egui_app = (self.egui_app_factory)(self.egui_context.clone());
        let fullscreen = self.config.viewport_builder.fullscreen.unwrap_or_default();
        let visible = self.config.viewport_builder.visible.unwrap_or(true);

        Ok(RunningEguiAppState {
            config: self.config,
            renderer: self.renderer,
            egui_context: self.egui_context,
            egui_app_factory: self.egui_app_factory,
            softbuffer_context: self.softbuffer_context,
            window: self.window,
            surface,
            egui_winit,
            egui_app,
            fullscreen,
            visible,
            input_events: Vec::new(),
            software_backend: self.software_backend,
        })
    }
}

impl<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp>
    WinitAppStateMachine<EguiApp, EguiAppFactory>
{
    /// Create a new application.
    pub(crate) fn new(
        config: SoftwareBackendAppConfiguration,
        renderer: EguiSoftwareRender,
        softbuffer_context: softbuffer::Context<OwnedDisplayHandle>,
        egui_app_factory: EguiAppFactory,
    ) -> Self {
        Self::Configured(ConfiguredAppState {
            config,
            software_backend: SoftwareBackend {
                capture_frame_time: false,
                last_frame_time: None,
            },
            renderer,
            softbuffer_context,
            egui_context: Context::default(),
            egui_app_factory,
        })
    }
}

impl<EguiApp: App, InitSurface: FnMut(Context) -> EguiApp> ApplicationHandler
    for WinitAppStateMachine<EguiApp, InitSurface>
{
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if el.exiting() {
            return;
        }

        match mem::take(self) {
            Self::Configured(state) => match state.create_window(el) {
                Ok(ss) => {
                    *self = Self::WindowInitialized(ss);
                    self.resumed(el);
                }
                Err(e) => {
                    *self = Self::Dead(Some(e));
                    el.exit();
                }
            },
            Self::WindowInitialized(state) => match state.create_surface() {
                Ok(ss) => *self = Self::Running(ss),
                Err(e) => {
                    *self = Self::Dead(Some(e));
                    el.exit();
                }
            },
            Self::Running(state) => {
                *self = Self::Running(state);
                self.suspended(el);
                self.resumed(el);
            }
            Self::Dead(err) => {
                *self = Self::Dead(err);
                el.exit();
            }
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

        match self {
            Self::Configured(_) | Self::WindowInitialized(_) => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Self::Running(state) => {
                if let Err(e) =
                    state.handle_event(Event::WindowEvent { window_id, event }, event_loop)
                {
                    *self = Self::Dead(Some(e));
                    event_loop.exit();
                }
            }
            Self::Dead(_) => {
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }

        match self {
            Self::Configured(_) | Self::WindowInitialized(_) => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Self::Running(state) => {
                if let Err(e) = state.handle_event(Event::AboutToWait, event_loop) {
                    *self = Self::Dead(Some(e));
                    event_loop.exit();
                }
            }
            Self::Dead(_) => {
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }

        match mem::take(self) {
            Self::Dead(e) => {
                *self = Self::Dead(e);
                event_loop.exit();
            }
            Self::Configured(state) => {
                *self = Self::Configured(state);
            }
            Self::WindowInitialized(window) => {
                *self = Self::WindowInitialized(window);
            }
            Self::Running(state) => {
                *self = Self::WindowInitialized(state.suspend());
            }
        }
    }
}

impl<EguiApp: App, EguiAppFactory: FnMut(Context) -> EguiApp>
    RunningEguiAppState<EguiApp, EguiAppFactory>
{
    pub(crate) fn suspend(self) -> WindowInitializedAppState<EguiApp, EguiAppFactory> {
        WindowInitializedAppState {
            config: self.config,
            software_backend: self.software_backend,
            renderer: self.renderer,
            egui_context: self.egui_context,
            egui_app_factory: self.egui_app_factory,
            softbuffer_context: self.softbuffer_context,
            window: self.window,
        }
    }
    pub(crate) fn handle_event(
        &mut self,
        event: Event<()>,
        elwt: &ActiveEventLoop,
    ) -> Result<(), SoftwareBackendAppError> {
        let start = if self.software_backend.capture_frame_time {
            Some(Instant::now())
        } else {
            None
        };

        elwt.set_control_flow(ControlFlow::Wait);

        let Event::WindowEvent {
            window_id,
            event: window_event,
        } = event
        else {
            return Ok(());
        };

        if window_id != self.window.id() {
            return Ok(());
        }

        let response = self
            .egui_winit
            .on_window_event(self.window.deref(), &window_event);

        if response.repaint {
            // Redraw when egui says it's necessary (e.g., mouse move, key press):
            self.window.request_redraw();
        }

        match window_event {
            WindowEvent::RedrawRequested => {
                let size = self.window.inner_size();
                let width = NonZeroU32::new(size.width).unwrap_or(ONE_PIXEL);
                let height = NonZeroU32::new(size.height).unwrap_or(ONE_PIXEL);

                self.surface.resize(width, height).map_err(
                    SoftwareBackendAppError::soft_buffer("softbuffer::Surface::resize"),
                )?;

                let mut raw_input = self.egui_winit.take_egui_input(self.window.deref());

                raw_input
                    .events
                    .extend_from_slice(self.input_events.as_slice());
                self.input_events.clear();

                let full_output = self.egui_context.run(raw_input, |ctx| {
                    self.egui_app.update(ctx, &mut self.software_backend);

                    self.egui_context.viewport(|r| {
                        let mut die = false;
                        for command in &r.commands {
                            match command {
                                ViewportCommand::Close => {
                                    die = true;
                                }
                                ViewportCommand::CancelClose => {
                                    die = false;
                                }
                                ViewportCommand::Title(title) => self.window.set_title(title),
                                ViewportCommand::Transparent(trans) => {
                                    self.window.set_transparent(*trans)
                                }
                                ViewportCommand::Visible(true) => {
                                    self.visible = true;
                                    self.window.set_visible(true);
                                    if self.fullscreen {
                                        self.window
                                            .set_fullscreen(Some(Fullscreen::Borderless(None)));
                                    } else {
                                        self.window.set_fullscreen(None);
                                    }
                                }
                                ViewportCommand::Visible(false) => {
                                    self.visible = false;
                                    // Needed because otherwise fullscreen mode never works again.
                                    self.window.set_fullscreen(None);

                                    // Needed because otherwise cursor grab needs to be manually set to none before it can be enabled again
                                    _ = self.window.set_cursor_grab(CursorGrabMode::None);

                                    self.window.set_visible(false)
                                }
                                ViewportCommand::OuterPosition(state) => {
                                    self.window.set_outer_position(
                                        winit::dpi::LogicalPosition::new(state.x, state.y),
                                    );
                                }
                                ViewportCommand::InnerSize(state) => {
                                    _ = self.window.request_inner_size(
                                        winit::dpi::LogicalSize::new(state.x, state.y),
                                    );
                                }
                                ViewportCommand::MinInnerSize(state) => {
                                    self.window.set_min_inner_size(Some(
                                        winit::dpi::LogicalSize::new(state.x, state.y),
                                    ));
                                }
                                ViewportCommand::MaxInnerSize(state) => {
                                    self.window.set_max_inner_size(Some(
                                        winit::dpi::LogicalSize::new(state.x, state.y),
                                    ));
                                }
                                ViewportCommand::ResizeIncrements(None) => {
                                    self.window
                                        .set_resize_increments::<winit::dpi::LogicalSize<f32>>(
                                            None,
                                        );
                                }
                                ViewportCommand::ResizeIncrements(Some(state)) => {
                                    let size = winit::dpi::LogicalSize::new(state.x, state.y);
                                    self.window.set_resize_increments(Some(size));
                                }
                                ViewportCommand::Resizable(state) => {
                                    self.window.set_resizable(*state);
                                }
                                ViewportCommand::EnableButtons {
                                    close,
                                    maximize,
                                    minimized,
                                } => {
                                    let mut button_state = WindowButtons::empty();
                                    if *close {
                                        button_state.set(WindowButtons::CLOSE, true);
                                    }
                                    if *maximize {
                                        button_state.set(WindowButtons::MAXIMIZE, true);
                                    }
                                    if *minimized {
                                        button_state.set(WindowButtons::MINIMIZE, true);
                                    }

                                    self.window.set_enabled_buttons(button_state);
                                }
                                ViewportCommand::Minimized(state) => {
                                    self.window.set_minimized(*state);
                                }
                                ViewportCommand::Maximized(state) => {
                                    self.window.set_maximized(*state);
                                }
                                ViewportCommand::Fullscreen(false) => {
                                    self.fullscreen = false;
                                    if self.visible {
                                        self.window.set_fullscreen(None)
                                    }
                                }
                                ViewportCommand::Fullscreen(true) => {
                                    self.fullscreen = true;
                                    if self.visible {
                                        self.window
                                            .set_fullscreen(Some(Fullscreen::Borderless(None)))
                                    }
                                }
                                ViewportCommand::Decorations(dec) => {
                                    self.window.set_decorations(*dec)
                                }
                                ViewportCommand::WindowLevel(WindowLevel::Normal) => {
                                    self.window
                                        .set_window_level(winit::window::WindowLevel::Normal);
                                }
                                ViewportCommand::WindowLevel(WindowLevel::AlwaysOnBottom) => {
                                    self.window.set_window_level(
                                        winit::window::WindowLevel::AlwaysOnBottom,
                                    );
                                }
                                ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop) => {
                                    self.window
                                        .set_window_level(winit::window::WindowLevel::AlwaysOnTop);
                                }
                                ViewportCommand::Icon(ico) => {
                                    self.window.set_window_icon(
                                        ico.as_ref()
                                            .map(|i| {
                                                Icon::from_rgba(i.rgba.clone(), i.width, i.height)
                                                    .ok()
                                            })
                                            .unwrap_or(None),
                                    );
                                }
                                ViewportCommand::Focus => self.window.focus_window(),
                                ViewportCommand::SetTheme(SystemTheme::Dark) => {
                                    self.window.set_theme(Some(Theme::Dark))
                                }
                                ViewportCommand::SetTheme(SystemTheme::Light) => {
                                    self.window.set_theme(Some(Theme::Light))
                                }
                                ViewportCommand::SetTheme(SystemTheme::SystemDefault) => {
                                    // Winit has no default...
                                    // we will just use light as that is the default
                                    // of most operating systems.
                                    self.window.set_theme(Some(Theme::Light))
                                }

                                //Wtf, I don't think DXGI, or reading the X11 backbuffer cares about this.
                                ViewportCommand::ContentProtected(x) => {
                                    self.window.set_content_protected(*x);
                                }

                                ViewportCommand::CursorGrab(CursorGrab::Confined) => {
                                    _ = self.window.set_cursor_grab(CursorGrabMode::Confined);
                                }
                                ViewportCommand::CursorGrab(CursorGrab::Locked) => {
                                    _ = self.window.set_cursor_grab(CursorGrabMode::Locked);
                                }
                                ViewportCommand::CursorGrab(CursorGrab::None) => {
                                    _ = self.window.set_cursor_grab(CursorGrabMode::None);
                                }
                                ViewportCommand::CursorVisible(visible) => {
                                    self.window.set_cursor_visible(*visible);
                                }
                                ViewportCommand::RequestCut => {
                                    self.input_events.push(egui::Event::Cut);
                                }
                                ViewportCommand::RequestCopy => {
                                    self.input_events.push(egui::Event::Copy);
                                }
                                ViewportCommand::RequestPaste => {
                                    if let Some(content) = self.egui_winit.clipboard_text() {
                                        self.input_events.push(egui::Event::Paste(content));
                                    }
                                }
                                ViewportCommand::MousePassthrough(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::Screenshot(_) => {
                                    //UNSUPPORTED (YET)
                                }
                                ViewportCommand::BeginResize(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::IMERect(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::IMEAllowed(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::IMEPurpose(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::CursorPosition(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::RequestUserAttention(_) => {
                                    //UNSUPPORTED
                                }
                                ViewportCommand::StartDrag => {
                                    //UNSUPPORTED
                                }
                            }

                            if die {
                                self.window.set_visible(false); //I wish eframe did this.
                                elwt.exit();
                            }
                        }
                    });
                });

                //Makes the clipboard work.
                self.egui_winit
                    .handle_platform_output(self.window.deref(), full_output.platform_output);

                let clipped_primitives = self
                    .egui_context
                    .tessellate(full_output.shapes, full_output.pixels_per_point);

                let mut buffer =
                    self.surface
                        .buffer_mut()
                        .map_err(SoftwareBackendAppError::soft_buffer(
                            "softbuffer::Surface::buffer_mut",
                        ))?;
                buffer.fill(0); // CLEAR

                let buffer_ref = &mut BufferMutRef::new(
                    bytemuck::cast_slice_mut(&mut buffer),
                    width.get() as usize,
                    height.get() as usize,
                );

                self.renderer.render(
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

                self.software_backend.last_frame_time = start.map(|a| a.elapsed());
            }

            WindowEvent::CloseRequested => {
                self.egui_app.on_exit(&self.egui_context);
                elwt.exit();
            }
            _ => {}
        }

        Ok(())
    }
}

/// This struct contains statistics as well as possible interactions with the software renderer.
///
/// # Example
/// ```rust
///  use egui_software_backend::{App, SoftwareBackend};
///
/// struct MyApp {
///
/// }
///
/// impl App for MyApp {
///     fn update(&mut self, ctx: &egui::Context, backend: &mut SoftwareBackend) {
///         backend.set_capture_frame_time(true);
///
///
///        egui::CentralPanel::default().show(ctx, |ui| {
///        ui.label(format!(
///           "Frame Time {}ms",
///            backend.last_frame_time().unwrap_or_default().as_millis()
///         ));
///     });
///    }
/// }
///
/// ```
pub struct SoftwareBackend {
    capture_frame_time: bool,
    last_frame_time: Option<Duration>,
}

impl SoftwareBackend {
    /// Returns true if the frame time for the next frame is captured.
    pub fn is_capture_frame_time(&self) -> bool {
        self.capture_frame_time
    }

    /// Enables or disables capturing the frame time.
    /// Note that once this is called, the value persists until this function is called again.
    /// Calling this with true will not affect the current frame, so once this is called with true,
    /// you will need to wait for 2 more frames until you get a value.
    pub fn set_capture_frame_time(&mut self, capture: bool) {
        self.capture_frame_time = capture;
    }

    /// Returns the rendering duration of the last frame if this information is available.
    /// Returns none otherwise. Note that this information is only captured is `set_capture_frame_time`
    /// is called with true.
    pub fn last_frame_time(&self) -> Option<Duration> {
        self.last_frame_time
    }
}

pub trait App {
    fn update(&mut self, ctx: &Context, software_backend: &mut SoftwareBackend);

    fn on_exit(&mut self, _ctx: &Context) {}
}

#[derive(Debug, Clone)]
pub struct SoftwareBackendAppConfiguration {
    /// The underlying egui viewport builder that is used to create the window with winit.
    pub viewport_builder: ViewportBuilder,

    /// If true: rasterized ClippedPrimitives are cached and rendered to an intermediate tiled canvas. That canvas is
    /// then rendered over the frame buffer. If false ClippedPrimitives are rendered directly to the frame buffer.
    /// Rendering without caching is much slower and primarily intended for testing.
    ///
    /// Default is true!
    pub allow_raster_opt: bool,

    /// If true: attempts to optimize by converting suitable triangle pairs into rectangles for faster rendering.
    ///   Things *should* look the same with this set to `true` while rendering faster.
    ///
    /// Default is true!
    pub convert_tris_to_rects: bool,

    /// If true: rasterized ClippedPrimitives are cached and rendered to an intermediate tiled canvas. That canvas is
    /// then rendered over the frame buffer. If false ClippedPrimitives are rendered directly to the frame buffer.
    /// Rendering without caching is much slower and primarily intended for testing.
    ///
    /// Default is true!
    pub caching: bool,
}

impl SoftwareBackendAppConfiguration {
    /// Creates a new SoftwareBackendAppConfiguration using the default configuration.
    pub const fn new() -> Self {
        //The constructor is not const.
        let vp = ViewportBuilder {
            title: None,
            app_id: None,
            position: None,
            //CGA
            inner_size: Some(Vec2::new(320f32, 200f32)),
            min_inner_size: None,
            max_inner_size: None,
            clamp_size_to_monitor_size: None,
            fullscreen: None,
            maximized: None,
            resizable: None,
            transparent: None,
            decorations: None,
            icon: None,
            active: None,
            visible: None,
            fullsize_content_view: None,
            movable_by_window_background: None,
            title_shown: None,
            titlebar_buttons_shown: None,
            titlebar_shown: None,
            has_shadow: None,
            drag_and_drop: None,
            taskbar: None,
            close_button: None,
            minimize_button: None,
            maximize_button: None,
            window_level: None,
            mouse_passthrough: None,
            window_type: None,
        };

        Self {
            viewport_builder: vp,

            allow_raster_opt: true,
            convert_tris_to_rects: true,
            caching: true,
        }
    }

    /// This sets the egui viewport builder to the given builder. This replaces most settings.
    pub fn viewport_builder(mut self, viewport_builder: ViewportBuilder) -> Self {
        self.viewport_builder = viewport_builder;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn title(mut self, title: Option<String>) -> Self {
        self.viewport_builder.title = title;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn app_id(mut self, app_id: Option<String>) -> Self {
        self.viewport_builder.app_id = app_id;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn position(mut self, position: Option<Pos2>) -> Self {
        self.viewport_builder.position = position;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn inner_size(mut self, inner_size: Option<Vec2>) -> Self {
        self.viewport_builder.inner_size = inner_size;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn min_inner_size(mut self, min_inner_size: Option<Vec2>) -> Self {
        self.viewport_builder.min_inner_size = min_inner_size;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn max_inner_size(mut self, max_inner_size: Option<Vec2>) -> Self {
        self.viewport_builder.max_inner_size = max_inner_size;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn clamp_size_to_monitor_size(
        mut self,
        clamp_size_to_monitor_size: Option<bool>,
    ) -> Self {
        self.viewport_builder.clamp_size_to_monitor_size = clamp_size_to_monitor_size;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn fullscreen(mut self, fullscreen: Option<bool>) -> Self {
        self.viewport_builder.fullscreen = fullscreen;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn maximized(mut self, maximized: Option<bool>) -> Self {
        self.viewport_builder.maximized = maximized;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn resizable(mut self, resizable: Option<bool>) -> Self {
        self.viewport_builder.resizable = resizable;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn transparent(mut self, transparent: Option<bool>) -> Self {
        self.viewport_builder.transparent = transparent;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn decorations(mut self, decorations: Option<bool>) -> Self {
        self.viewport_builder.decorations = decorations;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn icon(mut self, icon: Option<Arc<IconData>>) -> Self {
        self.viewport_builder.icon = icon;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn active(mut self, active: Option<bool>) -> Self {
        self.viewport_builder.active = active;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn visible(mut self, visible: Option<bool>) -> Self {
        self.viewport_builder.visible = visible;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn fullsize_content_view(mut self, fullsize_content_view: Option<bool>) -> Self {
        self.viewport_builder.fullsize_content_view = fullsize_content_view;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn movable_by_window_background(
        mut self,
        movable_by_window_background: Option<bool>,
    ) -> Self {
        self.viewport_builder.movable_by_window_background = movable_by_window_background;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn title_shown(mut self, title_shown: Option<bool>) -> Self {
        self.viewport_builder.title_shown = title_shown;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn titlebar_buttons_shown(mut self, titlebar_buttons_shown: Option<bool>) -> Self {
        self.viewport_builder.titlebar_buttons_shown = titlebar_buttons_shown;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn titlebar_shown(mut self, titlebar_shown: Option<bool>) -> Self {
        self.viewport_builder.titlebar_shown = titlebar_shown;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn has_shadow(mut self, has_shadow: Option<bool>) -> Self {
        self.viewport_builder.has_shadow = has_shadow;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn drag_and_drop(mut self, drag_and_drop: Option<bool>) -> Self {
        self.viewport_builder.has_shadow = drag_and_drop;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub fn taskbar(mut self, taskbar: Option<bool>) -> Self {
        self.viewport_builder.has_shadow = taskbar;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn close_button(mut self, close_button: Option<bool>) -> Self {
        self.viewport_builder.close_button = close_button;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn minimize_button(mut self, minimize_button: Option<bool>) -> Self {
        self.viewport_builder.minimize_button = minimize_button;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn maximize_button(mut self, maximize_button: Option<bool>) -> Self {
        self.viewport_builder.maximize_button = maximize_button;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn window_level(mut self, window_level: Option<WindowLevel>) -> Self {
        self.viewport_builder.window_level = window_level;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn mouse_passthrough(mut self, mouse_passthrough: Option<bool>) -> Self {
        self.viewport_builder.mouse_passthrough = mouse_passthrough;
        self
    }

    /// See egui::viewport::ViewportBuilder. This is a convenience function that sets the field.
    pub const fn window_type(mut self, window_type: Option<X11WindowType>) -> Self {
        self.viewport_builder.window_type = window_type;
        self
    }

    /// If false: Rasterize everything with triangles, always calculate vertex colors, uvs, use bilinear
    ///   everywhere, etc... Things *should* look the same with this set to `true` while rendering faster.
    ///
    /// Default is true!
    pub const fn allow_raster_opt(mut self, allow_raster_opt: bool) -> Self {
        self.allow_raster_opt = allow_raster_opt;
        self
    }

    /// If true: attempts to optimize by converting suitable triangle pairs into rectangles for faster rendering.
    ///   Things *should* look the same with this set to `true` while rendering faster.
    ///
    /// Default is true!
    pub const fn convert_tris_to_rects(mut self, convert_tris_to_rects: bool) -> Self {
        self.convert_tris_to_rects = convert_tris_to_rects;
        self
    }

    /// If true: rasterized ClippedPrimitives are cached and rendered to an intermediate tiled canvas. That canvas is
    /// then rendered over the frame buffer. If false ClippedPrimitives are rendered directly to the frame buffer.
    /// Rendering without caching is much slower and primarily intended for testing.
    ///
    /// Default is true!
    pub const fn caching(mut self, caching: bool) -> Self {
        self.caching = caching;
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
    egui_app_factory: impl FnMut(Context) -> T,
) -> Result<(), SoftwareBackendAppError> {
    let egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::Bgra)
        .with_allow_raster_opt(settings.allow_raster_opt)
        .with_convert_tris_to_rects(settings.convert_tris_to_rects)
        .with_caching(settings.caching);

    let event_loop: EventLoop<()> =
        EventLoop::new().map_err(|e| SoftwareBackendAppError::EventLoop(Box::new(e)))?;

    let softbuffer_context = softbuffer::Context::new(event_loop.owned_display_handle()).map_err(
        SoftwareBackendAppError::soft_buffer("softbuffer::Context::new"),
    )?;

    let mut app = WinitAppStateMachine::new(
        settings.clone(),
        egui_software_render,
        softbuffer_context,
        egui_app_factory,
    );

    if let Err(event_loop_error) = event_loop.run_app(&mut app) {
        if let WinitAppStateMachine::Dead(Some(app_err)) = app {
            return Err(SoftwareBackendAppError::SuppressedEventLoop {
                event_loop_error: Box::new(event_loop_error),
                suppressed: Box::new(app_err),
            });
        }

        return Err(SoftwareBackendAppError::EventLoop(Box::new(
            event_loop_error,
        )));
    }

    if let WinitAppStateMachine::Dead(Some(app_err)) = app {
        return Err(app_err);
    }

    Ok(())
}
