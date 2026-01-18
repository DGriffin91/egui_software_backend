use crate::alloc::string::ToString;
use alloc::format;
use alloc::vec::Vec;
use core::sync::atomic::{self};
use egui::ahash::HashMap;
use egui::mutex::Mutex;
use std::time::Instant;

#[allow(unused_imports)]
use egui::{Ui, Vec2, Vec2b};

#[derive(Clone, Copy)]
pub struct Stat {
    pub count: u32,
    pub time: f32,
    pub sum_area: f32,
}

#[derive(Default)]
pub struct DurationStat {
    elapsed_secs: atomic::AtomicU32, // f32
}

impl DurationStat {
    pub(crate) fn mark(&self, start: Instant) {
        let secs = start.elapsed().as_secs_f32();
        let secs: u32 = secs.to_bits();
        self.elapsed_secs.store(secs, atomic::Ordering::Relaxed);
    }

    pub fn elapsed_secs(&self) -> f32 {
        let secs: u32 = self.elapsed_secs.load(atomic::Ordering::Relaxed);
        f32::from_bits(secs)
    }
}

#[derive(Default)]
pub struct RasterStats {
    /// Key is tri width
    pub tri_width_buckets: HashMap<u32, Stat>,
    /// Key is tri height
    pub tri_height_buckets: HashMap<u32, Stat>,
    /// Key is rect width
    pub rect_width_buckets: HashMap<u32, Stat>,
    /// Key is rect height
    pub rect_height_buckets: HashMap<u32, Stat>,
    /// Count of tris where the vertex colors varied
    pub tri_vert_col_vary: u32,
    /// Count of tris where the vertex uvs varied
    pub tri_vert_uvs_vary: u32,
    /// Count of tris that required alpha blending
    pub tri_alpha_blend: u32,
    /// Count of rects where the vertex colors varied
    pub rect_vert_col_vary: u32,
    /// Count of rects where the vertex uvs varied
    pub rect_vert_uvs_vary: u32,
    /// Count of rects that required alpha blending
    pub rect_alpha_blend: u32,
    /// Total tris drawn
    pub tris: u32,
    /// Total rects drawn
    pub rects: u32,
}

#[derive(Default)]
pub struct RenderStats {
    pub raster: Mutex<RasterStats>,
    pub set_textures: DurationStat,
    pub render_prims_to_cache: DurationStat,
    pub update_dirty_rect: DurationStat,
    pub update_dirty_tiles: DurationStat,
    pub update_dirty_rects: DurationStat,
    pub render_from_meshcache: DurationStat,
    pub render_from_tiledcache: DurationStat,
    pub render_direct: DurationStat,
    pub blit_canvas_to_buffer: DurationStat,
}

#[cfg(not(feature = "rayon"))]
pub(crate) struct RasterStatsStarted<'a> {
    start: Instant,
    stats: egui::mutex::MutexGuard<'a, RasterStats>,
}

#[cfg(not(feature = "rayon"))]
impl<'a> RasterStatsStarted<'a> {
    pub(crate) fn finish_rect(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.stats.rects += 1;
        let tri_area = (fsize.x * fsize.y) * 0.5;
        Self::insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            tri_area,
            &mut self.stats.rect_width_buckets,
        );
        Self::insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            tri_area,
            &mut self.stats.rect_height_buckets,
        );
        self.stats.rect_vert_col_vary += vert_col_vary as u32;
        self.stats.rect_vert_uvs_vary += vert_uvs_vary as u32;
        self.stats.rect_alpha_blend += alpha_blend as u32;
    }

    pub(crate) fn finish_tri(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.stats.tris += 1;
        let rect_area = fsize.x * fsize.y;
        Self::insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            rect_area,
            &mut self.stats.tri_width_buckets,
        );
        Self::insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            rect_area,
            &mut self.stats.tri_height_buckets,
        );
        self.stats.tri_vert_col_vary += vert_col_vary as u32;
        self.stats.tri_vert_uvs_vary += vert_uvs_vary as u32;
        self.stats.tri_alpha_blend += alpha_blend as u32;
    }

    fn insert_or_increment(
        long_side_size: u32,
        elapsed: f32,
        area: f32,
        map: &mut HashMap<u32, Stat>,
    ) {
        if let Some(stat) = map.get_mut(&long_side_size) {
            stat.count += 1;
            stat.time += elapsed;
            stat.sum_area += area;
        } else {
            map.insert(
                long_side_size,
                Stat {
                    count: 1,
                    time: elapsed,
                    sum_area: area,
                },
            );
        }
    }
}

impl RenderStats {
    pub(crate) fn clear(&mut self) {
        *self = RenderStats::default();
    }

    #[cfg(not(feature = "rayon"))]
    pub(crate) fn start_raster(&self) -> RasterStatsStarted<'_> {
        RasterStatsStarted {
            start: Instant::now(),
            stats: self.raster.lock(),
        }
    }

    pub fn render(&self, ui: &mut Ui) {
        egui::ScrollArea::both()
            .auto_shrink(Vec2b::new(false, false))
            .min_scrolled_width(900.0)
            .show(ui, |ui| {
                let raster = self.raster.lock();
                egui::Grid::new("stats_grid").striped(true).show(ui, |ui| {
                    let mut stat = |label: &str, val: &DurationStat| {
                        ui.label(label);
                        ui.label(format!("{:.2}ms", val.elapsed_secs() * 1000.0));
                        ui.end_row();
                    };
                    stat("set_textures", &self.set_textures);
                    stat("render_prims_to_cache", &self.render_prims_to_cache);
                    stat("update_dirty_rect", &self.update_dirty_tiles);
                    stat("update_dirty_tiles", &self.update_dirty_tiles);
                    stat("update_dirty_rects", &self.update_dirty_tiles);
                    stat("render_from_tiledcache", &self.render_from_tiledcache);
                    stat("render_from_meshcache", &self.render_from_meshcache);
                    stat("blit_canvas_to_buffer", &self.blit_canvas_to_buffer);
                    stat("render_direct", &self.render_direct);

                    ui.heading("");
                    ui.heading("Tri");
                    ui.heading("Rect");
                    ui.end_row();
                    let mut stat = |label: &str, val: u32, val2: u32| {
                        ui.label(label);
                        ui.label(val.to_string());
                        ui.label(val2.to_string());
                        ui.end_row();
                    };
                    stat(
                        "Vertex colors vary",
                        raster.tri_vert_col_vary,
                        raster.rect_vert_col_vary,
                    );
                    stat(
                        "Vertex uvs vary",
                        raster.tri_vert_uvs_vary,
                        raster.rect_vert_uvs_vary,
                    );
                    stat(
                        "Requires alpha blend",
                        raster.tri_alpha_blend,
                        raster.rect_alpha_blend,
                    );
                });

                ui.label("");
                ui.end_row();

                fn collect_and_sort(map: &HashMap<u32, Stat>) -> Vec<(u32, Stat)> {
                    let mut v: Vec<_> = map.iter().map(|(&k, &s)| (k, s)).collect();
                    v.sort_by_key(|&(_, s)| std::cmp::Reverse((s.time * 1000000.0) as u32)); // Seconds to microseconds
                    v
                }

                let tri_width_bucket = collect_and_sort(&raster.tri_width_buckets);
                let tri_height_bucket = collect_and_sort(&raster.tri_height_buckets);
                let rect_width_bucket = collect_and_sort(&raster.rect_width_buckets);
                let rect_height_bucket = collect_and_sort(&raster.rect_height_buckets);

                let max_rows = tri_width_bucket
                    .len()
                    .max(tri_height_bucket.len())
                    .max(rect_width_bucket.len())
                    .max(rect_height_bucket.len());

                egui::Grid::new("stats_grid2").striped(true).show(ui, |ui| {
                    ui.heading("Tris");
                    ui.heading(format!("{}", raster.tris));
                    (0..=5).for_each(|_| _ = ui.heading(""));
                    ui.heading(" ");
                    ui.heading("Rects");
                    ui.heading(format!("{}", raster.rects));
                    (0..=5).for_each(|_| _ = ui.heading(""));
                    ui.end_row();

                    let headers = ["W", "Qty", "μs", "area"];

                    headers.iter().for_each(|s| _ = ui.heading(*s));
                    headers.iter().for_each(|s| _ = ui.heading(*s));
                    ui.heading(" ");
                    headers.iter().for_each(|s| _ = ui.heading(*s));
                    headers.iter().for_each(|s| _ = ui.heading(*s));
                    ui.end_row();

                    fn row(ui: &mut Ui, i: usize, v: &[(u32, Stat)]) {
                        if let Some((size, stat)) = v.get(i) {
                            ui.label(format!("{size}"));
                            ui.label(format!("{}", stat.count));
                            ui.label(format!("{:.0}", stat.time * 1000000.0)); // Seconds to microseconds
                            ui.label(format!("{}", stat.sum_area as u32));
                        } else {
                            ui.label("");
                            ui.label("");
                            ui.label("");
                            ui.label("");
                        }
                    }

                    for i in 0..max_rows {
                        row(ui, i, &tri_width_bucket);
                        row(ui, i, &tri_height_bucket);
                        ui.label(" ");
                        row(ui, i, &rect_width_bucket);
                        row(ui, i, &rect_height_bucket);
                        ui.end_row();
                    }
                });
            });
    }
}
