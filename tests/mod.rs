mod tests {

    use egui::{Vec2, vec2};
    use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};
    use image::{ImageBuffer, Rgba};

    use egui_kittest::HarnessBuilder;

    const RESOLUTION: Vec2 = vec2(1280.0, 720.0);

    #[test]
    // Tests many configurations of the cpu software render backend against the GPU implementation.
    // Outputs PNG files with diffs when pixels didn't match and will panic above a certain threshold:
    // (1px for 1.0 px_per_point, 7px for 1.5 px_per_point).
    // Currently have some pixels that don't match perfectly due to slight rounding when px_per_point is not 1.0
    pub fn compare_software_render_with_gpu() {
        fn app() -> impl FnMut(&egui::Context) {
            let mut egui_demo = egui_demo_lib::DemoWindows::default();
            move |ctx: &egui::Context| {
                egui_demo.ui(&ctx);

                // egui::CentralPanel::default().show(ctx, |ui| {
                //     #[allow(const_item_mutation)]
                //     ui.color_edit_button_srgba(&mut egui::Color32::TRANSPARENT);
                //     ui.end_row();
                // });
            }
        }

        let _ = std::fs::create_dir("tests/tmp/");

        // egui's failed_px_count_thresold default is 0
        for (px_per_point, failed_px_count_thresold) in [(1.0, 8), (1.5, 15)] {
            // --- Render on GPU
            let mut harness = HarnessBuilder::default()
                .with_size(RESOLUTION)
                .with_pixels_per_point(px_per_point)
                .renderer(egui_kittest::LazyRenderer::default())
                .build(app());
            harness.run();
            let gpu_render_image = harness.render().unwrap();
            gpu_render_image
                .save(format!("tests/tmp/gpu_px_per_point{px_per_point}.png"))
                .unwrap();

            for use_cache in [false, true] {
                for allow_raster_opt in [false, true] {
                    for convert_tris_to_rects in [false, true] {
                        // --- Render on CPU
                        let egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::Rgba)
                            .with_allow_raster_opt(allow_raster_opt)
                            .with_convert_tris_to_rects(convert_tris_to_rects)
                            .with_caching(use_cache);

                        let mut harness = HarnessBuilder::default()
                            .with_size(RESOLUTION)
                            .with_pixels_per_point(px_per_point)
                            .renderer(egui_software_render)
                            .build(app());
                        harness.run();
                        let cpu_render_image = harness.render().unwrap();

                        let name = format!(
                            "px_per_pt {px_per_point}, use_cache {use_cache}, raster_opt {allow_raster_opt}, tris_to_rects {convert_tris_to_rects}"
                        );

                        if let Some((pixels_failed, diff_image)) = dify(
                            &gpu_render_image,
                            &cpu_render_image,
                            0.6, // egui's default is 0.6
                        ) {
                            if pixels_failed > failed_px_count_thresold {
                                diff_image
                                    .save(format!("tests/tmp/diff_{name} - FAIL.png"))
                                    .unwrap();
                                cpu_render_image
                                    .save(format!("tests/tmp/cpu_{name} - FAIL.png"))
                                    .unwrap();
                                panic!("pixels_failed {pixels_failed}: {name}")
                            } else {
                                diff_image
                                    .save(format!("tests/tmp/diff_{name}.png"))
                                    .unwrap();
                                cpu_render_image
                                    .save(format!("tests/tmp/cpu_{name}.png"))
                                    .unwrap();
                            }
                        } else {
                            println!("excellent match, no dify diff: {name}")
                        };
                    }
                }
            }
        }
    }

    // Returning none indicates no diff
    fn dify(
        gpu_render_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        cpu_render_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        threshold: f32,
    ) -> Option<(i32, ImageBuffer<Rgba<u8>, Vec<u8>>)> {
        dify::diff::get_results(
            gpu_render_image.clone(),
            cpu_render_image.clone(),
            threshold,
            true,
            None,
            &None,
            &None,
        )
    }
}
