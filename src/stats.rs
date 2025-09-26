use std::{collections::HashMap, time::Instant};

use egui::{Ui, Vec2, Vec2b};

pub struct RasterStats {
    // <size, (count, duration in seconds)>
    pub tri_width_buckets: HashMap<u32, (u32, f32)>,
    pub tri_height_buckets: HashMap<u32, (u32, f32)>,
    pub rect_width_buckets: HashMap<u32, (u32, f32)>,
    pub rect_height_buckets: HashMap<u32, (u32, f32)>,
    pub tri_vert_col_vary: u32,
    pub tri_vert_uvs_vary: u32,
    pub tri_alpha_blend: u32,
    pub rect_vert_col_vary: u32,
    pub rect_vert_uvs_vary: u32,
    pub rect_alpha_blend: u32,
    pub rects: u32,
    pub tris: u32,
    pub start: Instant,
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
            start: Instant::now(),
        }
    }
}

fn insert_or_increment(size: u32, elapsed: f32, map: &mut HashMap<u32, (u32, f32)>) {
    if let Some((count, duration)) = map.get_mut(&size) {
        *count += 1;
        *duration += elapsed;
    } else {
        map.insert(size, (1, elapsed));
    }
}

impl RasterStats {
    pub(crate) fn clear(&mut self) {
        *self = RasterStats::default();
    }

    pub(crate) fn start_raster(&mut self) {
        self.start = Instant::now();
    }

    pub(crate) fn finish_rect(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.rects += 1;
        insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            &mut self.rect_width_buckets,
        );
        insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            &mut self.rect_height_buckets,
        );
        self.rect_vert_col_vary += vert_col_vary as u32;
        self.rect_vert_uvs_vary += vert_uvs_vary as u32;
        self.rect_alpha_blend += alpha_blend as u32;
    }

    pub(crate) fn finish_tri(
        &mut self,
        fsize: Vec2,
        vert_uvs_vary: bool,
        vert_col_vary: bool,
        alpha_blend: bool,
    ) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.tris += 1;
        insert_or_increment(
            (fsize.x as u32).max(1),
            elapsed,
            &mut self.tri_width_buckets,
        );
        insert_or_increment(
            (fsize.y as u32).max(1),
            elapsed,
            &mut self.tri_height_buckets,
        );
        self.tri_vert_col_vary += vert_col_vary as u32;
        self.tri_vert_uvs_vary += vert_uvs_vary as u32;
        self.tri_alpha_blend += alpha_blend as u32;
    }

    pub fn render(&self, ui: &mut Ui) {
        egui::ScrollArea::both()
            .auto_shrink(Vec2b::new(false, false))
            .min_scrolled_width(650.0)
            .show(ui, |ui| {
                egui::Grid::new("stats_grid").striped(true).show(ui, |ui| {
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

                fn collect_and_sort(map: &HashMap<u32, (u32, f32)>) -> Vec<(u32, (u32, f32))> {
                    let mut v: Vec<_> = map.iter().map(|(&k, &v)| (k, v)).collect();
                    v.sort_by_key(|&(_, (v, _))| std::cmp::Reverse(v));
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
                    ui.heading("");
                    ui.heading("");
                    ui.heading("");
                    ui.heading("");
                    ui.heading(" ");
                    ui.heading("Rects");
                    ui.heading(format!("{}", self.rects));
                    ui.heading("");
                    ui.heading("");
                    ui.heading("");
                    ui.heading("");
                    ui.end_row();
                    ui.heading("W");
                    ui.heading("Qty");
                    ui.heading("μs");
                    ui.heading("H");
                    ui.heading("Qty");
                    ui.heading("μs");
                    ui.heading(" ");
                    ui.heading("W");
                    ui.heading("Qty");
                    ui.heading("μs");
                    ui.heading("H");
                    ui.heading("Qty");
                    ui.heading("μs");
                    ui.end_row();

                    fn row(ui: &mut Ui, i: usize, v: &Vec<(u32, (u32, f32))>) {
                        if let Some((size, (count, t))) = v.get(i) {
                            ui.label(format!("{size}"));
                            ui.label(format!("{count}"));
                            ui.label(format!("{:.0}", t * 1000000.0)); // Seconds to microseconds
                        } else {
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
