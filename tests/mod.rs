use image::{DynamicImage, ImageBuffer, Rgba};
use nv_flip::FlipImageRgb8;

#[cfg(test)]
mod tests {
    use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};

    use crate::*;
    use egui_kittest::{Harness, HarnessBuilder};

    #[test]
    pub fn compare_software_render_with_gpu() {
        fn app() -> impl FnMut(&egui::Context) {
            let mut egui_demo = egui_demo_lib::DemoWindows::default();
            move |ctx: &egui::Context| {
                egui_demo.ui(&ctx);

                //egui::CentralPanel::default().show(ctx, |ui| {
                //    #[allow(const_item_mutation)]
                //    ui.color_edit_button_srgba(&mut egui::Color32::TRANSPARENT);
                //    ui.end_row();
                //});
            }
        }

        // --- Render on GPU
        let mut harness = Harness::new(app());
        harness.run();
        let gpu_render_image = harness.render().unwrap();

        // --- Render on CPU
        let egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::RGBA);
        let mut harness = HarnessBuilder::default()
            .renderer(egui_software_render)
            .build(app());
        harness.run();
        let cpu_render_image = harness.render().unwrap();

        // Compare with FLIP
        let size = harness.ctx.screen_rect();
        let width = size.width() as usize;
        let height = size.height() as usize;

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
