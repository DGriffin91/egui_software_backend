//! `blit_dirty_to_buffer` must produce the same frame as `blit_canvas_to_buffer`
//! while touching fewer tiles.
//!
//! No GPU and no `test_render` feature: the two blits are compared against each
//! other rather than against a reference renderer, which is the property that
//! matters — one is an optimisation of the other.
//!
//! The two tests use deliberately different content, because the two failure
//! modes need opposite things to show up:
//!
//! * dropping the per-tile reset only shows where the canvas is *not* opaque,
//!   so `moving_sprite` leaves most of the frame uncovered and moves a shape
//!   across it. Without the reset the shape smears a trail.
//! * blitting OCCUPIED instead of DIRTY only shows where most tiles are
//!   occupied but few are dirty, so `static_background_live_panel` fills the
//!   viewport and changes one small region.

use egui::{Color32, Pos2, Rect, Vec2};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};

const CLEAR: [u8; 4] = [0, 0, 0, 255];
const W: usize = 512;
const H: usize = 384;
const FRAMES: usize = 12;

/// Mostly-empty frame with one opaque square sliding across it.
fn moving_sprite(ctx: &egui::Context, frame: usize) {
    egui::Area::new("sprite".into())
        .fixed_pos(Pos2::ZERO)
        .show(ctx, |ui| {
            let x = 16.0 + frame as f32 * 24.0;
            ui.painter().rect_filled(
                Rect::from_min_size(egui::pos2(x, 120.0), Vec2::splat(48.0)),
                0.0,
                Color32::from_rgb(220, 40, 40),
            );
            ui.allocate_space(ui.available_size());
        });
}

/// A small live panel over a static full-viewport one.
///
/// The panel split is the point. egui merges shapes that share a clip rect and
/// texture into a single mesh, so a background and a widget drawn into the same
/// container become one primitive covering every tile — and touching the widget
/// then dirties all of them. Two `Area`s are not enough; they still merge.
/// Panels get distinct clip rects, which is what lets the dirty set narrow, and
/// is how a real application is laid out anyway.
fn static_background_live_panel(ctx: &egui::Context, frame: usize) {
    egui::TopBottomPanel::top("live").show(ctx, |ui| {
        ui.label(format!("frame {frame}"));
    });
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(24, 26, 30)))
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::ZERO, egui::vec2(W as f32, H as f32)),
                0.0,
                Color32::from_rgb(24, 26, 30),
            );
        });
}

fn raw_input(frame: usize) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(W as f32, H as f32),
        )),
        max_texture_side: Some(8192),
        time: Some(frame as f64 / 60.0),
        predicted_dt: 1.0 / 60.0,
        focused: true,
        ..Default::default()
    }
}

/// Whole-frame blit: the buffer is cleared every frame and every occupied tile
/// is repainted.
fn render_full(content: fn(&egui::Context, usize), frames: usize) -> Vec<[u8; 4]> {
    let ctx = egui::Context::default();
    let mut render = EguiSoftwareRender::new(ColorFieldOrder::Rgba);
    let mut buffer = vec![CLEAR; W * H];
    for i in 0..frames {
        let out = ctx.run(raw_input(i), |c| content(c, i));
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        buffer.fill(CLEAR);
        let mut buf = BufferMutRef::new(&mut buffer, W, H);
        render.render_to_canvas(W, H, &jobs, &out.textures_delta, out.pixels_per_point);
        render.blit_canvas_to_buffer(&mut buf);
    }
    buffer
}

/// Incremental blit: cleared once, then only dirty tiles are repainted.
/// Also returns the per-frame count of tiles painted.
fn render_dirty(content: fn(&egui::Context, usize), frames: usize) -> (Vec<[u8; 4]>, Vec<usize>) {
    let ctx = egui::Context::default();
    let mut render = EguiSoftwareRender::new(ColorFieldOrder::Rgba);
    let mut buffer = vec![CLEAR; W * H];
    let mut painted = Vec::with_capacity(frames);
    for i in 0..frames {
        let out = ctx.run(raw_input(i), |c| content(c, i));
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut buf = BufferMutRef::new(&mut buffer, W, H);
        render.render_to_canvas(W, H, &jobs, &out.textures_delta, out.pixels_per_point);
        painted.push(render.blit_dirty_to_buffer(&mut buf, CLEAR));
    }
    (buffer, painted)
}

fn assert_same(content: fn(&egui::Context, usize), what: &str) {
    let full = render_full(content, FRAMES);
    let (dirty, _) = render_dirty(content, FRAMES);
    let differing = full.iter().zip(&dirty).filter(|(a, b)| a != b).count();
    assert_eq!(
        differing,
        0,
        "{what}: incremental blit diverged on {differing} of {} pixels",
        full.len()
    );
}

#[test]
fn dirty_blit_matches_full_blit_over_transparent_background() {
    // Catches a missing per-tile reset: the sprite would leave a trail.
    assert_same(moving_sprite, "moving sprite");
}

#[test]
fn dirty_blit_matches_full_blit_over_opaque_background() {
    assert_same(static_background_live_panel, "static background");
}

#[test]
fn dirty_blit_skips_tiles_once_the_frame_settles() {
    let tiles_total = W.div_ceil(64) * H.div_ceil(64);
    let (_, painted) = render_dirty(static_background_live_panel, FRAMES);

    // Cold cache: the first frame has to paint everything it covers.
    assert!(painted[0] > 0, "first frame painted nothing");
    // The background covers every tile, so blitting OCCUPIED would repaint all
    // of them forever. Only one small region changes, so DIRTY must be a small
    // fraction — this is the assertion that a regression in the mask trips, and
    // that the equivalence tests above cannot see.
    let settled = painted[FRAMES - 1];
    assert!(
        settled * 4 < tiles_total,
        "settled frame painted {settled} of {tiles_total} tiles; the dirty mask is not narrowing"
    );
}
