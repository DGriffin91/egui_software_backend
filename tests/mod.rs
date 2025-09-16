use bytemuck::cast_slice;
use image::{DynamicImage, ImageBuffer, Rgba};
use nv_flip::FlipImageRgb8;

#[cfg(test)]
mod tests {
    use egui_software_backend::{BufferMutRef, EguiSoftwareRender};

    use crate::*;
    use egui_kittest::Harness;

    #[test]
    pub fn compare_software_render_with_gpu() {
        let app = |ctx: &egui::Context| {
            let mut egui_demo = egui_demo_lib::DemoWindows::default();
            egui_demo.ui(&ctx);
        };

        // -----------------------------------------
        // --- Render on GPU with Harness & wgpu ---
        // -----------------------------------------
        let mut harness = Harness::new(app);
        harness.run();
        let size = harness.ctx.screen_rect();
        let width = size.width() as usize;
        let height = size.height() as usize;
        let gpu_render_image = harness.render().unwrap();

        // --------------------------------------------------
        // --- Render on CPU with software render backend ---
        // --------------------------------------------------

        let ctx = egui::Context::default();
        let mut egui_software_render = EguiSoftwareRender::default();
        let buffer = &mut vec![0u32; width * height];

        for _ in 0..60 {
            let full_output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(harness.ctx.screen_rect()),
                    ..Default::default()
                },
                app,
            );

            let paint_jobs =
                ctx.tessellate(full_output.shapes.clone(), full_output.pixels_per_point);

            buffer.fill(0); // CLEAR
            let mut buffer_ref = BufferMutRef::new(
                bytemuck::cast_slice_mut(&mut buffer[..]),
                width as usize,
                height as usize,
            );

            egui_software_render.render(
                width,
                height,
                &paint_jobs,
                &full_output.textures_delta,
                full_output.pixels_per_point,
                Some(&mut buffer_ref),
                false,
                false,
            );
            //egui_software_render.blit_canvas_to_buffer(&mut buffer_ref);
        }

        let cpu_render_image = bgra_u32_to_image(width, height, buffer.to_vec());

        let (error_map, flip_vis_img) =
            nv_flip(width, height, &gpu_render_image, &cpu_render_image);

        let mut pool = nv_flip::FlipPool::from_image(&error_map);
        println!("FLIP mean: {}", pool.mean());
        println!("percentile: {}", pool.get_percentile(0.5, true));
        println!("min..max: {} .. {}", pool.min_value(), pool.max_value());

        let _ = std::fs::create_dir("tests/tmp/");
        gpu_render_image
            .save("tests/tmp/gpu_render_image.png")
            .unwrap();
        cpu_render_image
            .save("tests/tmp/cpu_render_image.png")
            .unwrap();
        flip_vis_img.save("tests/tmp/nv_flip.png").unwrap();
    }
}

fn nv_flip(
    width: usize,
    height: usize,
    ref_img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    test_img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> (
    nv_flip::FlipImageFloat,
    ImageBuffer<image::Rgb<u8>, Vec<u8>>,
) {
    let ref_img = nv_flip_rgb8(width, height, ref_img);
    let test_img = nv_flip_rgb8(width, height, test_img);

    let error_map = nv_flip::flip(ref_img, test_img, nv_flip::DEFAULT_PIXELS_PER_DEGREE);
    let vis = error_map.apply_color_lut(&nv_flip::magma_lut());
    let vis_img = image::RgbImage::from_raw(vis.width(), vis.height(), vis.to_vec()).unwrap();
    (error_map, vis_img)
}

fn nv_flip_rgb8(
    width: usize,
    height: usize,
    gpu_render_image: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
) -> FlipImageRgb8 {
    FlipImageRgb8::with_data(
        width as u32,
        height as u32,
        &DynamicImage::ImageRgba8(gpu_render_image.clone()).to_rgb8(),
    )
}

fn bgra_u32_to_image(
    width: usize,
    height: usize,
    bgra: Vec<u32>,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    ImageBuffer::<_, _>::from_raw(
        width as u32,
        height as u32,
        cast_slice::<u32, [u8; 4]>(&bgra)
            .iter()
            .map(|p| [p[2], p[1], p[0], p[3]])
            .flatten()
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
