// based on https://github.com/rust-windowing/softbuffer/blob/046de9228d89369151599f3f50dc4b75bd5e522b/examples/utils/winit_app.rs

/// Common boilerplate for setting up a winit application.
use winit::application::ApplicationHandler;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Easily constructable winit application.
pub(crate) struct WinitApp<T, S, Init, InitSurface, Handler> {
    /// Closure to initialize `state`.
    init: Init,

    /// Closure to initialize `surface_state`.
    init_surface: InitSurface,

    /// Closure to run on window events.
    event: Handler,

    /// Contained state.
    state: Option<T>,

    /// Contained surface state.
    surface_state: Option<S>,
}

impl<T, S, Init, InitSurface, Handler> WinitApp<T, S, Init, InitSurface, Handler>
where
    Init: FnMut(&ActiveEventLoop) -> T,
    InitSurface: FnMut(&ActiveEventLoop, &mut T) -> S,
    Handler: FnMut(&mut T, Option<&mut S>, Event<()>, &ActiveEventLoop),
{
    /// Create a new application.
    pub(crate) fn new(init: Init, init_surface: InitSurface, event: Handler) -> Self {
        Self {
            init,
            init_surface,
            event,
            state: None,
            surface_state: None,
        }
    }
}

impl<T, S, Init, InitSurface, Handler> ApplicationHandler
    for WinitApp<T, S, Init, InitSurface, Handler>
where
    Init: FnMut(&ActiveEventLoop) -> T,
    InitSurface: FnMut(&ActiveEventLoop, &mut T) -> S,
    Handler: FnMut(&mut T, Option<&mut S>, Event<()>, &ActiveEventLoop),
{
    fn resumed(&mut self, el: &ActiveEventLoop) {
        debug_assert!(self.state.is_none());
        let mut state = (self.init)(el);
        self.surface_state = Some((self.init_surface)(el, &mut state));
        self.state = Some(state);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        let surface_state = self.surface_state.take();
        debug_assert!(surface_state.is_some());
        drop(surface_state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = self.state.as_mut().unwrap();
        let surface_state = self.surface_state.as_mut();
        (self.event)(
            state,
            surface_state,
            Event::WindowEvent { window_id, event },
            event_loop,
        );
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            (self.event)(
                state,
                self.surface_state.as_mut(),
                Event::AboutToWait,
                event_loop,
            );
        }
    }
}
