# CPU software render backend for [egui](https://github.com/emilk/egui)

This minimal branch is a simplified implementation for educational purposes. It only depends on egui contains no `unsafe`, and is `no_std` (though egui itself is not currently `no_std`). It's missing many optimizations present in the main branch (sse 4.1, caching, static dispatch macro, rectangle detection/rendering, mutithreading), while retaining others (Detecting constant vertex colors, or uvs. Efficient integer rasterization with space skipping. SWAR SIMD. Integer color blending). It also only supports nearest texture sampling with clamping (which is the mode predominantly used by egui).

For simple interfaces with ~5 windows (like what you first see when running the winit example), it's around 4x slower than the main branch rendering in around 10ms on 1 thread of a 7950x. For very complex interfaces like the demo image below it's around 8x slower than main branch.

![demo](demo.png)

```rs
let ctx = egui::Context::default();
let mut demo = egui_demo_lib::DemoWindows::default();
let mut sw_render = EguiSoftwareRender::new(ColorFieldOrder::BGRA);

let out = ctx.run(raw_input, |ctx| {
    demo.ui(ctx);
});

let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);

sw_render.render(buffer, &primitives, &out.textures_delta, out.pixels_per_point);
```

- winit + softbuffer example: `cargo run --example winit`