use std::{collections::HashMap, time::Instant};

#[allow(unused_imports)]
use egui::{Ui, Vec2, Vec2b};

#[derive(Clone, Copy)]
pub struct Stat {
    pub count: u32,
    pub time: f32,
    pub sum_area: f32,
}

pub struct RasterStats {
    pub tri_width_buckets: HashMap<u32, Stat>, // Key is tri width
    pub tri_height_buckets: HashMap<u32, Stat>, // Key is tri height
    pub rect_width_buckets: HashMap<u32, Stat>, // Key is rect width
    pub rect_height_buckets: HashMap<u32, Stat>, // Key is rect height
    pub tri_vert_col_vary: u32,                // Count of tris where the vertex colors varied
    pub tri_vert_uvs_vary: u32,                // Count of tris where the vertex uvs varied
    pub tri_alpha_blend: u32,                  // Count of tris that required alpha blending
    pub rect_vert_col_vary: u32,               // Count of rects where the vertex colors varied
    pub rect_vert_uvs_vary: u32,               // Count of rects where the vertex uvs varied
    pub rect_alpha_blend: u32,                 // Count of rects that required alpha blending
    pub tris: u32,                             // Total tris drawn
    pub rects: u32,                            // Total rects drawn
    pub start: Instant,                        // Time just before latest rasterization
    pub set_textures: f32,
    pub update_dirty_tiles: f32,
    pub update_canvas_from_cached: f32,
    pub render_prims_to_cache: f32,
    pub render_direct: f32,
    pub blit_canvas_to_buffer: f32,
}

impl Default for RasterStats {
    fn default() -> Self {
        Self {
            tri_width_buckets: Default::default(),
            tri_height_buckets: Default::default(),
            tri_vert_col_vary: Default::default(),
            tri_vert_uvs_vary: Default::default(),
            tri_alpha_blend: Default::default(),
            rect_width_buckets: Default::default(),
            rect_height_buckets: Default::default(),
            rect_vert_col_vary: Default::default(),
            rect_vert_uvs_vary: Default::default(),
            rect_alpha_blend: Default::default(),
            rects: Default::default(),
            tris: Default::default(),
            set_textures: Default::default(),
            update_dirty_tiles: Default::default(),
            update_canvas_from_cached: Default::default(),
            render_prims_to_cache: Default::default(),
            render_direct: Default::default(),
            blit_canvas_to_buffer: Default::default(),
            start: Instant::now(),
        }
    }
}

#[cfg(not(feature = "rayon"))]
fn insert_or_increment(long_side_size: u32, elapsed: f32, area: f32, map: &mut HashMap<u32, Stat>) {
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

impl RasterStats {
    pub(crate) fn clear(&mut self) {
        *self = RasterStats::default();
    }

    #[cfg(not(feature = "rayon"))]
    pub(crate) fn start_raster(&mut self) {
        self.start = Instant::now();
    }

    #[cfg(not(feature = "rayon"))]
    pub(crate) fn finish_rect(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.rects += 1;
        let tri_area = (fsize.x * fsize.y) * 0.5;
        insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            tri_area,
            &mut self.rect_width_buckets,
        );
        insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            tri_area,
            &mut self.rect_height_buckets,
        );
        self.rect_vert_col_vary += vert_col_vary as u32;
        self.rect_vert_uvs_vary += vert_uvs_vary as u32;
        self.rect_alpha_blend += alpha_blend as u32;
    }

    #[cfg(not(feature = "rayon"))]
    pub(crate) fn finish_tri(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.tris += 1;
        let rect_area = fsize.x * fsize.y;
        insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            rect_area,
            &mut self.tri_width_buckets,
        );
        insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            rect_area,
            &mut self.tri_height_buckets,
        );
        self.tri_vert_col_vary += vert_col_vary as u32;
        self.tri_vert_uvs_vary += vert_uvs_vary as u32;
        self.tri_alpha_blend += alpha_blend as u32;
    }

    pub fn render(&self, ui: &mut Ui) {
        egui::ScrollArea::both()
            .auto_shrink(Vec2b::new(false, false))
            .min_scrolled_width(900.0)
            .show(ui, |ui| {
                egui::Grid::new("stats_grid").striped(true).show(ui, |ui| {
                    let mut stat = |label: &str, val: f32| {
                        ui.label(label);
                        ui.label(format!("{:.2}ms", val * 1000.0));
                        ui.end_row();
                    };
                    stat("set_textures", self.set_textures);
                    stat("render_prims_to_cache", self.render_prims_to_cache);
                    stat("update_dirty_tiles", self.update_dirty_tiles);
                    stat("update_canvas_from_cached", self.update_canvas_from_cached);
                    stat("blit_canvas_to_buffer", self.blit_canvas_to_buffer);
                    stat("render_direct", self.render_direct);

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
                        self.tri_vert_col_vary,
                        self.rect_vert_col_vary,
                    );
                    stat(
                        "Vertex uvs vary",
                        self.tri_vert_uvs_vary,
                        self.rect_vert_uvs_vary,
                    );
                    stat(
                        "Requires alpha blend",
                        self.tri_alpha_blend,
                        self.rect_alpha_blend,
                    );
                });

                ui.label("");
                ui.end_row();

                fn collect_and_sort(map: &HashMap<u32, Stat>) -> Vec<(u32, Stat)> {
                    let mut v: Vec<_> = map.iter().map(|(&k, &s)| (k, s)).collect();
                    v.sort_by_key(|&(_, s)| std::cmp::Reverse((s.time * 1000000.0) as u32)); // Seconds to microseconds
                    v
                }

                let tri_width_bucket = collect_and_sort(&self.tri_width_buckets);
                let tri_height_bucket = collect_and_sort(&self.tri_height_buckets);
                let rect_width_bucket = collect_and_sort(&self.rect_width_buckets);
                let rect_height_bucket = collect_and_sort(&self.rect_height_buckets);

                let max_rows = tri_width_bucket
                    .len()
                    .max(tri_height_bucket.len())
                    .max(rect_width_bucket.len())
                    .max(rect_height_bucket.len());

                egui::Grid::new("stats_grid2").striped(true).show(ui, |ui| {
                    ui.heading("Tris");
                    ui.heading(format!("{}", self.tris));
                    (0..=5).for_each(|_| _ = ui.heading(""));
                    ui.heading(" ");
                    ui.heading("Rects");
                    ui.heading(format!("{}", self.rects));
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
