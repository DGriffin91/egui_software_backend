#![cfg(feature = "test_render")]
mod tests {

    use egui::accesskit::Role;
    use egui::{Vec2, vec2};
    use egui_kittest::kittest::Queryable;
    use egui_software_backend::test_render::{EguiSoftwareTestRender, EguiSoftwareTestRenderMode};
    use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender, SoftwareRenderCaching};
    use image::{ImageBuffer, Rgba};

    use egui_kittest::{Harness, HarnessBuilder, TestRenderer};

    const RESOLUTION: Vec2 = vec2(1280.0, 720.0);

    #[test]
    // Tests many configurations of the cpu software render backend against the GPU implementation.
    // Outputs PNG files with diffs when pixels didn't match and will panic above a certain threshold:
    // (1px for 1.0 px_per_point, 7px for 1.5 px_per_point).
    // Currently have some pixels that don't match perfectly due to slight rounding when px_per_point is not 1.0
    pub fn compare_software_render_with_gpu() {
        let _ = std::fs::create_dir("tests/tmp/");

        // egui's failed_px_count_thresold default is 0
        for (px_per_point, failed_px_count_thresold) in [(1.0, 8), (1.5, 15)] {
            // --- Render on GPU
            let gpu_render_images = harness_run(
                app(),
                egui_kittest::LazyRenderer::default(),
                px_per_point,
                "tests/tmp/gpu_px_per_point",
            );

            for caching_mode in [
                SoftwareRenderCaching::Direct,
                SoftwareRenderCaching::BlendTiled,
                SoftwareRenderCaching::MeshTiled,
                SoftwareRenderCaching::Mesh,
            ] {
                for buffering_mode in [
                    EguiSoftwareTestRenderMode::AlwaysNewZeroed,
                    EguiSoftwareTestRenderMode::SimpleBuffering,
                    EguiSoftwareTestRenderMode::DoubleBuffering,
                    EguiSoftwareTestRenderMode::TripleBuffeing,
                ] {
                    for allow_raster_opt in [false, true] {
                        for convert_tris_to_rects in [false, true] {
                            test_cpu_render(
                                px_per_point,
                                failed_px_count_thresold,
                                &gpu_render_images,
                                caching_mode,
                                buffering_mode,
                                allow_raster_opt,
                                convert_tris_to_rects,
                            );
                        }
                    }
                }
            }
        }
    }

    fn test_cpu_render(
        px_per_point: f32,
        failed_px_count_thresold: i32,
        gpu_render_images: &Vec<ImageBuffer<Rgba<u8>, Vec<u8>>>,
        caching_mode: SoftwareRenderCaching,
        buffering_mode: EguiSoftwareTestRenderMode,
        allow_raster_opt: bool,
        convert_tris_to_rects: bool,
    ) {
        // --- Render on CPU
        let egui_software_render = EguiSoftwareRender::new(ColorFieldOrder::Rgba)
            .with_allow_raster_opt(allow_raster_opt)
            .with_convert_tris_to_rects(convert_tris_to_rects)
            .with_caching(caching_mode);
        let egui_software_render =
            EguiSoftwareTestRender::new(buffering_mode, egui_software_render);

        let name = format!(
            "px_per_pt {}, {:?}, {:?}, raster_opt {}, tris_to_rects {}",
            px_per_point, caching_mode, buffering_mode, allow_raster_opt, convert_tris_to_rects,
        );
        let cpu_render_images = harness_run(
            app(),
            egui_software_render,
            px_per_point,
            &format!("tests/tmp/cpu_{name}"),
        );

        assert_eq!(gpu_render_images.len(), cpu_render_images.len());
        for (i, (gpu_render_image, cpu_render_image)) in gpu_render_images
            .iter()
            .zip(cpu_render_images.iter())
            .enumerate()
        {
            if let Some((pixels_failed, diff_image)) = dify(
                &gpu_render_image,
                &cpu_render_image,
                0.6, // egui's default is 0.6
            ) {
                if pixels_failed > failed_px_count_thresold {
                    diff_image
                        .save(format!("tests/tmp/cpu_{name}_frame{i}_diff - FAIL.png"))
                        .unwrap();
                    panic!("pixels_failed {pixels_failed}: {name}")
                } else {
                    diff_image
                        .save(format!("tests/tmp/cpu_{name}_frame{i}_diff.png"))
                        .unwrap();
                }
            } else {
                println!("excellent match, no dify diff: {name}")
            };
        }
    }

    fn app() -> impl FnMut(&egui::Context) {
        let mut egui_demo = egui_demo_lib::DemoWindows::default();
        let mut checked = false;
        move |ctx: &egui::Context| {
            if true {
                egui_demo.ui(&ctx);
            } else {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.checkbox(&mut checked, "Checkbox");
                    if ui.button("✨ Misc Demos").clicked() {
                        checked = true;
                    }
                    if checked {
                        egui::Window::new("Color Test")
                            .current_pos((100.0, 100.0))
                            .show(ctx, |ui| {
                                ui.label("hello");
                            });
                        egui::Window::new("!Checked Test")
                            .current_pos((200.0, 100.0))
                            .show(ctx, |ui| {
                                ui.label("hi there");
                            });
                    } else {
                        egui::Window::new("!Checked Test")
                            .current_pos((150.0, 100.0))
                            .show(ctx, |ui| {
                                ui.label("hi there");
                            });
                    }
                });
            }
        }
    }

    fn harness_run(
        app: impl FnMut(&egui::Context),
        renderer: impl TestRenderer + 'static,
        px_per_point: f32,
        save_path_prefix: &str,
    ) -> Vec<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        let mut ret = Vec::new();
        let mut counter = 0;
        let mut run_and_render = |harness: &mut Harness<'_>| {
            harness.run();
            let gpu_render_image = harness.render().unwrap();
            gpu_render_image
                .save(format!(
                    "{save_path_prefix}{px_per_point}_frame{counter}.png"
                ))
                .unwrap();
            ret.push(gpu_render_image);
            counter += 1;
        };
        let mut harness = HarnessBuilder::default()
            .with_size(RESOLUTION)
            .with_pixels_per_point(px_per_point)
            .renderer(renderer)
            .build(app);
        run_and_render(&mut harness);

        let checkbox = harness.get_by_role_and_label(Role::Button, "✨ Misc Demos");
        checkbox.click();

        run_and_render(&mut harness);

        //let checkbox = harness.get_by_role_and_label(Role::Button, "✨ Misc Demos");
        //checkbox.click();
        run_and_render(&mut harness);

        harness.set_size(RESOLUTION * 1.25);

        run_and_render(&mut harness);

        harness.set_size(RESOLUTION);

        run_and_render(&mut harness);

        ret
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
