use std::num::NonZeroU32;

use bevy::{
    ecs::system::SystemState,
    prelude::*,
    window::{PrimaryWindow, RawHandleWrapper, ThreadLockedRawWindowHandleWrapper, WindowResized},
    winit::WinitWindows,
};

use egui_software_backend::{BufferMutRef, BufferRef};
use softbuffer::{Buffer, Context, Surface};

pub struct SoftBufferPlugin;

impl Plugin for SoftBufferPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, resize)
            .add_systems(Startup, startup)
            .add_systems(PostUpdate, clear.before(present))
            .add_systems(PostUpdate, present);
    }
}

pub fn resize(
    mut events: EventReader<WindowResized>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut buffer: NonSendMut<FrameSurface>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();

    let Ok((_bevy_window_entity, bevy_window)) = windows.single_mut() else {
        return;
    };

    let width = bevy_window.physical_width().max(1);
    let height = bevy_window.physical_height().max(1);

    buffer.width = width;
    buffer.height = height;
    buffer
        .surface
        .resize(
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        )
        .unwrap();
}

pub fn clear(mut buffer: NonSendMut<FrameSurface>, clear_color: Res<ClearColor>) {
    if buffer.width == 0 || buffer.height == 0 {
        return;
    }
    let mut buffer = buffer.surface.buffer_mut().unwrap();
    let clear_color = rgba_to_u32(clear_color.0.to_srgba().to_vec4());
    for b in buffer.iter_mut() {
        *b = clear_color;
    }
}

pub fn present(mut buffer: NonSendMut<FrameSurface>) {
    buffer.surface.buffer_mut().unwrap().present().unwrap();
}

pub struct FrameSurface {
    pub surface: Surface<ThreadLockedRawWindowHandleWrapper, ThreadLockedRawWindowHandleWrapper>,
    pub width: u32,
    pub height: u32,
}

impl FrameSurface {
    pub fn buffer(&mut self) -> Option<FrameBuffer<'_>> {
        self.surface
            .buffer_mut()
            .map(|buffer| FrameBuffer {
                buffer,
                width: self.width as usize,
                height: self.height as usize,
                width_extent: self.width as usize - 1,
                height_extent: self.height as usize - 1,
            })
            .ok()
    }
}

pub struct FrameBuffer<'a> {
    pub buffer: Buffer<'a, ThreadLockedRawWindowHandleWrapper, ThreadLockedRawWindowHandleWrapper>,
    pub width: usize,
    pub height: usize,
    pub width_extent: usize,
    pub height_extent: usize,
}

impl<'a> FrameBuffer<'a> {
    pub fn as_mut(&mut self) -> BufferMutRef {
        BufferMutRef {
            data: bytemuck::cast_slice_mut(&mut self.buffer[..]),
            width: self.width,
            height: self.height,
            width_extent: self.width_extent,
            height_extent: self.height_extent,
        }
    }

    pub fn as_ref(&self) -> BufferRef {
        BufferRef {
            data: bytemuck::cast_slice(&self.buffer[..]),
            width: self.width,
            height: self.height,
            width_extent: self.width_extent,
            height_extent: self.height_extent,
        }
    }
}

pub fn startup(
    world: &mut World,
    // in 0.17 `NonSendMut<WinitWindows>` will be bevy::winit::WINIT_WINDOWS
    // used with `WINIT_WINDOWS.with_borrow(|winit_windows| {`
    params: &mut SystemState<(Query<Entity, With<PrimaryWindow>>, NonSendMut<WinitWindows>)>,
) {
    let (primary_window, winit_windows) = params.get_mut(world);

    let primary_window = primary_window
        .single()
        .expect("Expected PrimaryWindow entity");

    let window = winit_windows
        .get_window(primary_window)
        .expect("Expected winit window matching PrimaryWindow entity");
    let handle = RawHandleWrapper::new(window).unwrap();

    // SAFETY: `Framebuffer` is `!Send`, `!Sync` and threrefore only accessed on the main thread.
    let (raw_display, raw_window) = unsafe { (handle.get_handle(), handle.get_handle()) };

    let mut surface = {
        let context = Context::new(raw_display).unwrap();
        Surface::new(&context, raw_window).unwrap()
    };

    let size = window.inner_size();

    let width = size.width.max(1);
    let height = size.height.max(1);

    surface
        .resize(
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        )
        .unwrap();

    world.insert_non_send_resource(FrameSurface {
        surface,
        width,
        height,
    });
}

#[inline(always)]
pub fn rgba_to_u32(v: Vec4) -> u32 {
    u32::from_le_bytes([
        (v.z.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (v.y.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (v.x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (v.w.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ])
}
