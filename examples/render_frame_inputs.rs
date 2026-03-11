use std::{fs::File, io::Read};

use bytemuck::cast_slice;
use egui::{ClippedPrimitive, epaint::Primitive};
use egui_software_backend::FrameInputData;
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use image::{ColorType, save_buffer};
use postcard::from_bytes;

fn main() -> std::io::Result<()> {
    let _ = std::fs::create_dir("frame_outputs/");

    let mut buffer;
    let mut buffer_ref: Option<BufferMutRef> = None;
    let mut width = 0usize;
    let mut height = 0usize;

    let mut sw_render = EguiSoftwareRender::new(ColorFieldOrder::Rgba);
    sw_render.record_frame_input = false;
    let _ = std::fs::create_dir("frame_outputs/");
    for i in 0..=500 {
        let Ok(mut file) = File::open(&format!("./frame_inputs/frame_{i}")) else {
            return Ok(());
        };
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let frame: FrameInputData = from_bytes(&data).unwrap();

        if frame.buffer_width as usize != width || frame.buffer_height as usize != height {
            width = frame.buffer_width as usize;
            height = frame.buffer_height as usize;
            buffer = vec![[0u8; 4]; width * height];
            buffer_ref = Some(BufferMutRef::new(&mut buffer, width, height));
        }

        let primitives = frame
            .paint_jobs
            .iter()
            .map(|(clip_rect, mesh)| ClippedPrimitive {
                clip_rect: clip_rect.clone(),
                primitive: Primitive::Mesh(mesh.clone()),
            })
            .collect::<Vec<_>>();

        let b_ref = buffer_ref.as_mut().unwrap();

        sw_render.render(
            b_ref,
            &primitives,
            &frame.textures_delta,
            frame.pixels_per_point,
        );

        save_buffer(
            format!("frame_outputs/frame_{i}.png"),
            cast_slice(b_ref.data),
            b_ref.width as u32,
            b_ref.height as u32,
            ColorType::Rgba8,
        )
        .unwrap();
    }
    Ok(())
}
