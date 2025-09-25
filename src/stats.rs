use std::collections::HashMap;

use egui::{Ui, Vec2b};

#[derive(Default)]
pub struct RasterStats {
    pub tri_width_size_buckets: HashMap<u32, u32>,
    pub tri_height_size_buckets: HashMap<u32, u32>,
    pub tri_vert_col_vary: u32,
    pub tri_vert_uvs_vary: u32,
    pub tri_alpha_blend: u32,
    pub rect_width_size_buckets: HashMap<u32, u32>,
    pub rect_height_size_buckets: HashMap<u32, u32>,
    pub rect_vert_col_vary: u32,
    pub rect_vert_uvs_vary: u32,
    pub rect_alpha_blend: u32,
    pub rects: u32,
    pub tris: u32,
}

fn insert_or_increment(size: u32, map: &mut HashMap<u32, u32>) {
    if let Some(count) = map.get_mut(&size) {
        *count += 1;
    } else {
        map.insert(size, 1);
    }
}

impl RasterStats {
    pub(crate) fn clear(&mut self) {
        *self = RasterStats::default();
    }
    pub(crate) fn tri_add_width(&mut self, width: u32) {
        insert_or_increment(width, &mut self.tri_width_size_buckets);
    }
    pub(crate) fn tri_add_height(&mut self, height: u32) {
        insert_or_increment(height, &mut self.tri_height_size_buckets);
    }
    pub(crate) fn rect_add_width(&mut self, width: u32) {
        insert_or_increment(width, &mut self.rect_width_size_buckets);
    }
    pub(crate) fn rect_add_height(&mut self, height: u32) {
        insert_or_increment(height, &mut self.rect_height_size_buckets);
    }
    pub fn render(&self, ui: &mut Ui) {
        egui::ScrollArea::both()
            .auto_shrink(Vec2b::new(false, false))
            .min_scrolled_width(450.0)
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

                fn collect_and_sort(map: &HashMap<u32, u32>) -> Vec<(u32, u32)> {
                    let mut v: Vec<_> = map.iter().map(|(&k, &v)| (k, v)).collect();
                    v.sort_by_key(|&(_, v)| std::cmp::Reverse(v));
                    v
                }

                let tri_width_bucket = collect_and_sort(&self.tri_width_size_buckets);
                let tri_height_bucket = collect_and_sort(&self.tri_height_size_buckets);
                let rect_width_bucket = collect_and_sort(&self.rect_width_size_buckets);
                let rect_height_bucket = collect_and_sort(&self.rect_height_size_buckets);

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
                    ui.heading(" ");
                    ui.heading("Rects");
                    ui.heading(format!("{}", self.rects));
                    ui.heading("");
                    ui.heading("");
                    ui.end_row();
                    ui.heading("W");
                    ui.heading("Qty");
                    ui.heading("H");
                    ui.heading("Qty");
                    ui.heading(" ");
                    ui.heading("W");
                    ui.heading("Qty");
                    ui.heading("H");
                    ui.heading("Qty");
                    ui.end_row();

                    fn row(ui: &mut Ui, i: usize, v: &Vec<(u32, u32)>) {
                        if let Some((size, count)) = v.get(i) {
                            ui.label(format!("{size}"));
                            ui.label(format!("{count}"));
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
