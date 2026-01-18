use core::ops::Deref;

use alloc::vec::Vec;

use crate::TILE_SIZE;

#[derive(Debug, Clone, Copy)]
pub struct DirtyRect {
    pub min_x: u32,
    pub min_y: u32,
    pub max_x: u32,
    pub max_y: u32,
}

impl DirtyRect {
    pub const fn new_empty() -> Self {
        Self {
            min_x: 0,
            min_y: 0,
            max_x: 0,
            max_y: 0,
        }
    }

    #[inline]
    pub const fn tiled<const TILE_SIZE: u32>(self) -> Self {
        Self {
            min_x: self.min_x / TILE_SIZE * TILE_SIZE,
            min_y: self.min_y / TILE_SIZE * TILE_SIZE,
            max_x: self.max_x.div_ceil(TILE_SIZE) * TILE_SIZE,
            max_y: self.max_y.div_ceil(TILE_SIZE) * TILE_SIZE,
        }
    }

    #[inline]
    pub const fn width(self) -> u32 {
        self.max_x - self.min_x
    }
    #[inline]
    pub const fn height(self) -> u32 {
        self.max_y - self.min_y
    }

    #[inline]
    pub const fn to_egui_rect(&self) -> egui::Rect {
        egui::Rect {
            min: egui::Pos2 {
                x: self.min_x as f32,
                y: self.min_y as f32,
            },
            max: egui::Pos2 {
                x: self.max_x as f32,
                y: self.max_y as f32,
            },
        }
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.min_x == self.max_x || self.min_y == self.max_y
    }

    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x && self.max_x > other.min_x
    }

    #[inline]
    pub fn intersection(self, other: DirtyRect) -> Self {
        Self {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        }
    }

    #[inline]
    pub fn union(&self, other: DirtyRect) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
}

#[derive(Debug, Default)]
pub struct ComputeTiledDirtyRects {
    minimal_non_overlapping_bboxes: Vec<DirtyRect>,
    pub(crate) bboxes: Vec<DirtyRect>,
    x_intervals: Vec<(u32, u32)>,
    ys: Vec<u32>,
}

impl Deref for ComputeTiledDirtyRects {
    type Target = [DirtyRect];

    fn deref(&self) -> &Self::Target {
        &self.minimal_non_overlapping_bboxes
    }
}

impl ComputeTiledDirtyRects {
    pub fn intersections(&self, other: DirtyRect) -> impl Iterator<Item = DirtyRect> + '_ {
        self.minimal_non_overlapping_bboxes
            .iter()
            .filter(move |bbox| bbox.intersects(other))
            .map(move |bbox| bbox.intersection(other))
    }

    pub fn set_bboxes(&mut self, boxes: impl Iterator<Item = DirtyRect>) {
        fn merge_intervals(intervals: &mut [(u32, u32)], mut f_yield: impl FnMut((u32, u32))) {
            if intervals.is_empty() {
                return;
            }
            intervals.sort_by(|a, b| a.0.cmp(&b.0));
            let mut it = intervals.iter().copied();
            if let Some(mut last) = it.next() {
                for (start, end) in it {
                    if start <= last.1 {
                        last.1 = last.1.max(end);
                    } else {
                        f_yield(last);
                        last = (start, end);
                    }
                }
                f_yield(last);
            }
        }

        self.minimal_non_overlapping_bboxes.clear();
        self.bboxes.clear();
        self.bboxes.extend(boxes.map(|b| b.tiled::<TILE_SIZE>()));
        // Step 1: collect all unique y-coordinates
        self.ys.clear();
        self.ys
            .extend(self.bboxes.iter().flat_map(|b| [b.min_y, b.max_y]));
        self.ys.sort_by(|a, b| a.cmp(b));
        self.ys.dedup();

        // Step 2: iterate over horizontal strips
        for strip in self.ys.windows(2) {
            let min_y = strip[0];
            let max_y = strip[1];

            // Find boxes intersecting this horizontal strip
            self.x_intervals.clear();
            for b in &self.bboxes {
                if b.min_y < max_y && b.max_y > min_y {
                    self.x_intervals.push((b.min_x, b.max_x));
                }
            }

            // Merge overlapping x-intervals
            merge_intervals(&mut self.x_intervals, |(min_x, max_x)| {
                match self.minimal_non_overlapping_bboxes.last_mut() {
                    Some(rect)
                        if rect.min_x == min_x && rect.max_x == max_x && rect.max_y == min_y =>
                    {
                        rect.max_y = max_y;
                    }
                    _ => {
                        self.minimal_non_overlapping_bboxes.push(DirtyRect {
                            min_x,
                            min_y,
                            max_x,
                            max_y,
                        });
                    }
                }
            });
        }
    }
}
