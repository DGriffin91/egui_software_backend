mod tests {
    use std::path::Path;

    use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};
    use image::{ImageBuffer, Rgba};

    use egui_kittest::{Harness, HarnessBuilder};

    const PIXELS_PER_POINT: f32 = 1.0; // TODO test with multiple 
    const ALLOW_RASTER_OPT: bool = true; // TODO test with/without
    const CONVERT_TRIS_TO_RECTS: bool = true; // TODO test with/without

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
        harness.set_pixels_per_point(PIXELS_PER_POINT);
        harness.run();
        let gpu_render_image = harness.render().unwrap();

        // --- Render on CPU
        let egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::RGBA)
            .with_allow_raster_opt(ALLOW_RASTER_OPT)
            .with_convert_tris_to_rects(CONVERT_TRIS_TO_RECTS);
        let mut harness = HarnessBuilder::default()
            .renderer(egui_software_render)
            .with_pixels_per_point(PIXELS_PER_POINT)
            .build(app());
        harness.run();
        let cpu_render_image = harness.render().unwrap();

        let _ = std::fs::create_dir("tests/tmp/");
        gpu_render_image
            .save("tests/tmp/gpu_render_image.png")
            .unwrap();
        cpu_render_image
            .save("tests/tmp/cpu_render_image.png")
            .unwrap();

        dify(
            &gpu_render_image,
            &cpu_render_image,
            0.6, // egui's default is 0.6
            1,   // egui's default is 0
            "tests/tmp/render_diff.png",
        );
    }

    fn dify<P: AsRef<Path>>(
        gpu_render_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        cpu_render_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        threshold: f32,
        failed_pixel_count_threshold: i32,
        path: P,
    ) {
        if let Some((num_wrong_pixels, diff_image)) = dify::diff::get_results(
            gpu_render_image.clone(),
            cpu_render_image.clone(),
            threshold,
            true,
            None,
            &None,
            &None,
        ) {
            let _ = diff_image.save(path);
            if num_wrong_pixels > failed_pixel_count_threshold {
                panic!(
                    "num_wrong_pixels {} > failed_pixel_count_threshold {}",
                    num_wrong_pixels, failed_pixel_count_threshold
                );
            }
        }
    }
}
