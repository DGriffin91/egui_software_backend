use std::{borrow::Cow, collections::HashMap, ops::Range};

use egui::{Color32, Pos2, Vec2, vec2};

use crate::{
    color::{egui_blend_u8, egui_blend_u8_slice_sse41, swizzle_rgba_bgra},
    egui_texture::EguiTexture,
    hash::Hash32,
    render::{draw_egui_mesh, egui_orient2df},
};

pub(crate) mod color;
pub(crate) mod egui_texture;
pub(crate) mod hash;
pub(crate) mod i64vec2;
pub(crate) mod raster_bary;
pub(crate) mod raster_rect;
pub(crate) mod raster_span;
pub(crate) mod render;
#[cfg(feature = "test_render")]
pub mod test_render;
pub(crate) mod vec4;

const TILE_SIZE: usize = 64;

#[derive(Copy, Clone, Default)]
pub enum ColorFieldOrder {
    #[default]
    RGBA,
    BGRA,
}

pub struct EguiSoftwareRender {
    textures: HashMap<egui::TextureId, EguiTexture>,
    cached_primitives: HashMap<u32, CachedPrimitive>,
    px_mesh: egui::Mesh,
    tiles_dim: [usize; 2],
    dirty_tiles: Vec<u8>,
    target_size: Vec2,
    prims_updated_this_frame: usize,
    output_field_order: ColorFieldOrder,
    canvas: Canvas,
    redraw_everything_this_frame: bool,
    convert_tris_to_rects: bool,
    allow_raster_opt: bool,
}

impl EguiSoftwareRender {
    /// # Arguments
    /// * `output_field_order` - egui textures and vertex colors will be swizzled before rendering to match the desired
    ///   output buffer order.
    pub fn new(output_field_order: ColorFieldOrder) -> Self {
        EguiSoftwareRender {
            textures: Default::default(),
            cached_primitives: Default::default(),
            px_mesh: Default::default(),
            tiles_dim: Default::default(),
            dirty_tiles: Default::default(),
            target_size: Default::default(),
            prims_updated_this_frame: Default::default(),
            output_field_order,
            canvas: Default::default(),
            redraw_everything_this_frame: Default::default(),
            convert_tris_to_rects: true,
            allow_raster_opt: true,
        }
    }

    /// If true: attempts to optimize by converting suitable triangle pairs into rectangles for faster rendering.
    ///   Things *should* look the same with this set to `true` while rendering faster.
    pub fn with_convert_tris_to_rects(mut self, set: bool) -> Self {
        self.convert_tris_to_rects = set;
        self
    }

    /// If false: Rasterize everything with triangles, always calculate vertex colors, uvs, use bilinear
    ///   everywhere, etc... Things *should* look the same with this set to `true` while rendering faster.
    pub fn with_allow_raster_opt(mut self, set: bool) -> Self {
        self.allow_raster_opt = set;
        self
    }

    /// Renders the given paint jobs to a buffer.
    ///
    /// # Arguments
    /// * `width` - The width of the output in pixels. Must match final output buffer dimensions.
    /// * `height` - The height of the output in pixels. Must match final output buffer dimensions.
    /// * `paint_jobs` - List of `egui::ClippedPrimitive` from egui to be rendered.
    /// * `textures_delta` - The change in egui textures since last frame
    /// * `pixels_per_point` - The number of physical pixels for each logical point.
    pub fn render_to_canvas(
        &mut self,
        width: usize,
        height: usize,
        paint_jobs: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        // TODO: need to deal with user textures. Either make the fields of EguiUserTextures pub or need to come up with a replacement.

        assert!(width > 0);
        assert!(height > 0);
        assert!(pixels_per_point > 0.0);

        self.redraw_everything_this_frame = self.canvas.resize(width, height);

        if self.redraw_everything_this_frame {
            self.canvas.clear();
            self.cached_primitives.clear();
        }

        for (_hash, prim) in self.cached_primitives.iter_mut() {
            prim.seen_this_frame = false;
        }

        self.target_size = vec2(width as f32, height as f32);
        self.tiles_dim = [width.div_ceil(TILE_SIZE), height.div_ceil(TILE_SIZE)];

        self.set_textures(textures_delta);

        self.render_prims_to_canvas(paint_jobs, pixels_per_point);

        self.update_dirty_tiles();
        self.clear_unused_cached_prims();

        let mut reinit_canvas = self.redraw_everything_this_frame;

        if self.prims_updated_this_frame > 0 {
            // TODO use tiles
            reinit_canvas = true;
        }

        if reinit_canvas {
            self.update_canvas_from_cached();
        }

        self.free_textures(textures_delta);
    }

    /// Render directly into buffer without cache. This is much slower and mainly intended for testing.
    pub fn render_direct(
        &mut self,
        direct_draw_buffer: &mut BufferMutRef,
        paint_jobs: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        self.set_textures(textures_delta);
        self.target_size = vec2(
            direct_draw_buffer.width as f32,
            direct_draw_buffer.height as f32,
        );

        for egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } in paint_jobs.iter()
        {
            let input_mesh = match primitive {
                egui::epaint::Primitive::Mesh(input_mesh) => input_mesh,
                _ => todo!(),
            };

            if input_mesh.vertices.is_empty() || input_mesh.indices.is_empty() {
                continue;
            }

            let clip_rect = egui::Rect {
                min: clip_rect.min * pixels_per_point,
                // TODO not sure why +1.5 is needed here. Occasionally things are cropped out without it.
                max: clip_rect.max * pixels_per_point + egui::Vec2::splat(1.5),
            };

            let mut mesh_min = egui::Vec2::splat(f32::MAX);
            let mut mesh_max = egui::Vec2::splat(-f32::MAX);

            self.prepare_px_mesh(pixels_per_point, input_mesh, &mut mesh_min, &mut mesh_max);

            let mesh_size = mesh_max - mesh_min;
            if mesh_size.x > 8192.0 || mesh_size.y > 8192.0 {
                // TODO it occasionally tries to make giant buffers in the first couple frames initially for some reason.
                continue;
            }
            let render_in_low_precision = mesh_size.x > 4096.0 || mesh_size.y > 4096.0;
            if render_in_low_precision {
                draw_egui_mesh::<2>(
                    &self.textures,
                    direct_draw_buffer,
                    self.convert_tris_to_rects,
                    &clip_rect,
                    &self.px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                );
            } else {
                draw_egui_mesh::<8>(
                    &self.textures,
                    direct_draw_buffer,
                    self.convert_tris_to_rects,
                    &clip_rect,
                    &self.px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                );
            }
        }

        self.free_textures(textures_delta);
    }

    fn prepare_px_mesh(
        &mut self,
        pixels_per_point: f32,
        mesh: &egui::Mesh,
        mesh_min: &mut Vec2,
        mesh_max: &mut Vec2,
    ) {
        // TODO perf: does this reuse the allocations in temp_px_mesh?
        self.px_mesh.indices = mesh.indices.clone();
        self.px_mesh.vertices = mesh.vertices.clone();
        self.px_mesh.texture_id = mesh.texture_id;

        for v in self.px_mesh.vertices.iter_mut() {
            v.pos *= pixels_per_point;

            // This could fix a tiny sub-pixel bias to match gpu rendering due to alias not matching due to things like:
            // https://github.com/emilk/egui/blob/226bdc4c5bbb2230fb829e01b3fcb0460e741b34/crates/egui/src/widgets/color_picker.rs#L28
            // On the GPU the grid jumps around if the pixels_per_point isn't 1.0.
            // I'm not making this adjustment currently as it makes the text a tiny bit softer. Maybe we can just bias
            // the rounding elsewhere.
            // v.pos -= Vec2::splat(0.0001);

            match self.output_field_order {
                ColorFieldOrder::RGBA => (), // egui uses rgba
                ColorFieldOrder::BGRA => {
                    let d = swizzle_rgba_bgra(v.color.to_array());
                    v.color = Color32::from_rgba_premultiplied(d[0], d[1], d[2], d[3]);
                }
            }

            *mesh_min = mesh_min.min(v.pos.to_vec2());
            *mesh_max = mesh_max.max(v.pos.to_vec2());
        }

        // Make all the tris face forward (ccw) to simplify rasterization.
        // TODO perf: could store the area so it's not recomputed later.
        for i in (0..self.px_mesh.indices.len()).step_by(3) {
            let i0 = self.px_mesh.indices[i] as usize;
            let i1 = self.px_mesh.indices[i + 1] as usize;
            let i2 = self.px_mesh.indices[i + 2] as usize;
            let v0 = self.px_mesh.vertices[i0];
            let v1 = self.px_mesh.vertices[i1];
            let v2 = self.px_mesh.vertices[i2];
            let area = egui_orient2df(&v0.pos, &v1.pos, &v2.pos);
            if area < 0.0 {
                self.px_mesh.indices.swap(i + 1, i + 2);
            }
        }
    }

    fn render_prims_to_canvas(
        &mut self,
        paint_jobs: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) {
        for (
            prim_idx,
            egui::ClippedPrimitive {
                clip_rect,
                primitive,
            },
        ) in paint_jobs.iter().enumerate()
        {
            let input_mesh = match primitive {
                egui::epaint::Primitive::Mesh(input_mesh) => input_mesh,
                _ => todo!(),
            };

            if input_mesh.vertices.is_empty() || input_mesh.indices.is_empty() {
                continue;
            }

            let clip_rect = egui::Rect {
                min: clip_rect.min * pixels_per_point,
                // TODO not sure why +1.5 is needed here. Occasionally things are cropped out without it.
                max: clip_rect.max * pixels_per_point + egui::Vec2::splat(1.5),
            };

            let mut mesh_min = egui::Vec2::splat(f32::MAX);
            let mut mesh_max = egui::Vec2::splat(-f32::MAX);

            self.prepare_px_mesh(pixels_per_point, input_mesh, &mut mesh_min, &mut mesh_max);

            let cropped_min = mesh_min.max(clip_rect.min.to_vec2());
            let cropped_max = mesh_max.min(clip_rect.max.to_vec2());
            let clip_rect = egui::Rect {
                min: Pos2::ZERO,
                max: (cropped_max - cropped_min).to_pos2(),
            };

            let hash = {
                let mut hasher = Hash32::new_fnv();

                hasher.hash_wrap(clip_rect.min.x.to_bits());
                hasher.hash_wrap(clip_rect.min.y.to_bits());
                hasher.hash_wrap(clip_rect.max.x.to_bits());
                hasher.hash_wrap(clip_rect.max.y.to_bits());
                hasher.hash_wrap(match self.px_mesh.texture_id {
                    egui::TextureId::Managed(id) => id as u32,
                    egui::TextureId::User(id) => id as u32 + 9358476,
                });
                for ind in &self.px_mesh.indices {
                    let v = self.px_mesh.vertices[*ind as usize];

                    // Tried to do this to avoid full redraws when moving a window but it was resulting in some
                    // meshes to be matches incorrectly in the ui gradient portion of the egui color test:
                    //let pos = v.pos - cropped_min;

                    // It's much faster to not wrap for every field. General ordering should be sufficiently preserved.
                    hasher.hash(v.pos.x.to_bits());
                    hasher.hash(v.pos.y.to_bits());
                    hasher.hash(v.uv.x.to_bits());
                    hasher.hash(v.uv.y.to_bits());
                    hasher.hash(u32::from_le_bytes(v.color.to_array()));
                    hasher.fnv_wrap();
                }
                hasher.hash_wrap(self.px_mesh.indices.len() as u32);
                hasher.finalize()
            };

            if let Some(cached_primitive) = self.cached_primitives.get_mut(&hash) {
                cached_primitive.seen_this_frame = true;
                cached_primitive.z_order = prim_idx;
                cached_primitive.min_x = cropped_min.x as usize;
                cached_primitive.min_y = cropped_min.y as usize;
                cached_primitive.rendered_this_frame = false;
            } else {
                let width = (cropped_max.x - cropped_min.x + 0.5) as usize;
                let height = (cropped_max.y - cropped_min.y + 0.5) as usize;

                if width > 8192 || height > 8192 {
                    // TODO it occasionally tries to make giant buffers in the first couple frames initially for some reason.
                    continue;
                }

                if width == 0 || height == 0 {
                    continue;
                }

                let render_in_low_precision = width > 4096 || height > 4096;

                let mut prim = CachedPrimitive::new(
                    cropped_min.x as usize,
                    cropped_min.y as usize,
                    width,
                    height,
                    prim_idx,
                );
                let mut buffer_ref = BufferMutRef {
                    data: &mut prim.buffer,
                    width,
                    height,
                    width_extent: width - 1,
                    height_extent: height - 1,
                };

                let offset = -vec2(cropped_min.x.floor(), cropped_min.y.floor());

                // TODO perf: straightforward to multi-thread checking hash and render since the render targets are separate.
                if render_in_low_precision {
                    // Seems to not be an issue in direct draw? Seems like a bug.
                    draw_egui_mesh::<2>(
                        &self.textures,
                        &mut buffer_ref,
                        self.convert_tris_to_rects,
                        &clip_rect,
                        &self.px_mesh,
                        offset,
                        self.allow_raster_opt,
                    );
                } else {
                    draw_egui_mesh::<8>(
                        &self.textures,
                        &mut buffer_ref,
                        self.convert_tris_to_rects,
                        &clip_rect,
                        &self.px_mesh,
                        offset,
                        self.allow_raster_opt,
                    );
                }
                self.prims_updated_this_frame += 1;
                prim.update_occupied_tiles(self.tiles_dim[0], self.tiles_dim[1]);
                self.cached_primitives.insert(hash, prim);
            }
        }
    }

    fn update_canvas_from_cached(&mut self) {
        let mut sorted_prim_cache = self.cached_primitives.values().collect::<Vec<_>>();
        sorted_prim_cache.sort_unstable_by_key(|prim| prim.z_order);

        // TODO perf: parallelize
        for tile_idx in self
            .dirty_tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| **t & Self::DIRTY_TILE_MASK != 0)
            .map(|(idx, _)| idx)
        {
            let tile_x = tile_idx % self.tiles_dim[0];
            let tile_y = tile_idx / self.tiles_dim[0];
            let tile_x_start = tile_x * TILE_SIZE;
            let tile_y_start = tile_y * TILE_SIZE;
            let tile_x_end = (tile_x_start + TILE_SIZE).min(self.canvas.width);
            let tile_y_end = (tile_y_start + TILE_SIZE).min(self.canvas.height);

            // clear tile
            for y in tile_y_start..tile_y_end {
                for x in tile_x_start..tile_x_end {
                    let canvas_x = x.min(self.canvas.width_extent);
                    let canvas_y = y.min(self.canvas.height_extent);
                    self.canvas.data[canvas_x + canvas_y * self.canvas.width] = [0; 4];
                }
            }
            let tile_n = [tile_x as u16, tile_y as u16];
            // redraw cached prims on tile
            for prim in &sorted_prim_cache {
                if !prim.occupied_tiles.contains(&tile_n) {
                    continue;
                }

                let mut min_x = prim.min_x;
                let mut min_y = prim.min_y;
                let mut max_x = min_x + prim.width;
                let mut max_y = min_y + prim.height;

                min_x = min_x.max(tile_x_start).min(self.canvas.width);
                min_y = min_y.max(tile_y_start).min(self.canvas.height);
                max_x = max_x.min(tile_x_end).min(self.canvas.width);
                max_y = max_y.min(tile_y_end).min(self.canvas.height);

                let prim_buf = prim.get_buffer_ref();

                if max_x <= min_x || max_y <= min_y {
                    continue;
                }
                let prim_x_min = (min_x - prim.min_x).min(prim_buf.width);
                let prim_x_max = (max_x - prim.min_x).min(prim_buf.width);

                let get_ranges = |y: usize| -> (Range<usize>, Range<usize>) {
                    let canvas_row_start = y.min(self.canvas.height) * self.canvas.width;
                    let canvas_start = canvas_row_start + min_x;
                    let canvas_end = canvas_row_start + max_x;

                    let prim_y = (y - prim.min_y).min(prim_buf.height);
                    let prim_row_start = prim_y * prim_buf.width;
                    let prim_start = prim_row_start + prim_x_min;
                    let prim_end = prim_row_start + prim_x_max;

                    (canvas_start..canvas_end, prim_start..prim_end)
                };

                if is_x86_feature_detected!("sse4.1") {
                    for y in min_y..max_y {
                        let (canvas_slice, prim_slice) = get_ranges(y);
                        let src_row = &prim_buf.data[prim_slice];
                        let dst_row = &mut self.canvas.data[canvas_slice];
                        // SAFETY: we first check is_x86_feature_detected!("sse4.1") outside the loop
                        unsafe { egui_blend_u8_slice_sse41(src_row, dst_row) }
                    }
                } else {
                    for y in min_y..max_y {
                        let (canvas_slice, prim_slice) = get_ranges(y);
                        let src_row = &prim_buf.data[prim_slice];
                        let dst_row = &mut self.canvas.data[canvas_slice];
                        for (pixel, src) in dst_row.iter_mut().zip(src_row) {
                            *pixel = egui_blend_u8(*src, *pixel);
                        }
                    }
                }
            }
        }
    }

    fn clear_unused_cached_prims(&mut self) {
        self.cached_primitives
            .retain(|_hash, prim| prim.seen_this_frame);
    }

    /// Draw canvas alpha over given buffer.
    /// Do not run if rending with Some(direct_draw_buffer) in EguiSoftwareRender::render()
    /// Only writes tile regions that contain pixels that are not fully transparent.
    pub fn blit_canvas_to_buffer(&mut self, buffer: &mut BufferMutRef) {
        // Simple tile-less version
        // buffer.data.iter_mut().zip(self.canvas.iter()).for_each(|(pixel, src)| {
        //     *pixel = egui_blend_u8(*src, *pixel);
        // });

        if self.canvas.data.is_empty() {
            panic!(
                "Canvas not initialized, call EguiSoftwareRender::blit_canvas_to_buffer() only after EguiSoftwareRender::render_to_canvas()"
            )
        }

        let width = self.canvas.width;
        let height = self.canvas.height;
        assert_eq!(self.canvas.data.len(), width * height);
        assert_eq!(buffer.data.len(), width * height);

        let tiles_x = self.tiles_dim[0];

        for (tile_idx, &mask) in self.dirty_tiles.iter().enumerate() {
            if mask & Self::OCCUPIED_TILE_MASK == 0 {
                continue;
            }

            let tile_x = tile_idx % tiles_x;
            let tile_y = tile_idx / tiles_x;

            let x_start = tile_x * TILE_SIZE;
            let y_start = tile_y * TILE_SIZE;
            let x_end = (x_start + TILE_SIZE).min(width);
            let y_end = (y_start + TILE_SIZE).min(height);

            if is_x86_feature_detected!("sse4.1") {
                for y in y_start..y_end {
                    let row_start = y * width;
                    let src_row = &self.canvas.data[row_start + x_start..row_start + x_end];
                    let dst_row = &mut buffer.data[row_start + x_start..row_start + x_end];
                    // SAFETY: we first check is_x86_feature_detected!("sse4.1") outside the loop
                    unsafe { egui_blend_u8_slice_sse41(src_row, dst_row) }
                }
            } else {
                for y in y_start..y_end {
                    let row_start = y * width;
                    let src_row = &self.canvas.data[row_start + x_start..row_start + x_end];
                    let dst_row = &mut buffer.data[row_start + x_start..row_start + x_end];
                    for (dst, &src) in dst_row.iter_mut().zip(src_row.iter()) {
                        *dst = egui_blend_u8(src, *dst);
                    }
                }
            }
        }
    }

    fn free_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        for free in &textures_delta.free {
            self.textures.remove(free);
        }
    }

    const DIRTY_TILE_MASK: u8 = 0b00000001;
    const OCCUPIED_TILE_MASK: u8 = 0b000000010;
    fn update_dirty_tiles(&mut self) {
        self.dirty_tiles
            .resize(self.tiles_dim[0] * self.tiles_dim[1], 0);
        self.dirty_tiles.fill(0);
        for prim in self.cached_primitives.values() {
            for tile in &prim.occupied_tiles {
                let mask =
                    &mut self.dirty_tiles[tile[0] as usize + tile[1] as usize * self.tiles_dim[0]];
                if !prim.seen_this_frame || prim.rendered_this_frame {
                    *mask |= Self::DIRTY_TILE_MASK;
                }
                *mask |= Self::OCCUPIED_TILE_MASK;
            }
        }
    }

    fn set_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        for (id, delta) in &textures_delta.set {
            let pixels = match &delta.image {
                egui::ImageData::Color(image) => {
                    assert_eq!(image.width() * image.height(), image.pixels.len());
                    Cow::Borrowed(&image.pixels)
                }
            };
            let size = delta.image.size();
            if let Some(pos) = delta.pos {
                if let Some(texture) = self.textures.get_mut(id) {
                    for y in 0..size[1] {
                        for x in 0..size[0] {
                            let src_pos = x + y * size[0];
                            let dest_pos = (x + pos[0]) + (y + pos[1]) * texture.width;
                            texture.data[dest_pos] = match self.output_field_order {
                                ColorFieldOrder::RGBA => pixels[src_pos].to_array(),
                                ColorFieldOrder::BGRA => {
                                    swizzle_rgba_bgra(pixels[src_pos].to_array())
                                }
                            };
                        }
                    }
                }
            } else {
                self.textures.insert(
                    *id,
                    EguiTexture {
                        data: pixels
                            .iter()
                            .map(|p| match self.output_field_order {
                                ColorFieldOrder::RGBA => p.to_array(),
                                ColorFieldOrder::BGRA => swizzle_rgba_bgra(p.to_array()),
                            })
                            .collect::<Vec<_>>(),
                        width_extent: size[0] as i32 - 1,
                        height_extent: size[1] as i32 - 1,
                        width: size[0],
                        height: size[1],
                        fsize: vec2(size[0] as f32, size[1] as f32),
                        options: delta.options,
                    },
                );
            }
        }
    }
}

#[derive(Default)]
struct Canvas {
    data: Vec<[u8; 4]>,
    width: usize,
    height: usize,
    width_extent: usize,
    height_extent: usize,
}

impl Canvas {
    fn clear(&mut self) {
        self.data.iter_mut().for_each(|p| *p = [0; 4]);
    }

    /// returns true if wasn't already the given size
    fn resize(&mut self, width: usize, height: usize) -> bool {
        if width != self.width || height != self.height {
            self.data.resize(width * height, [0; 4]);
            self.width = width;
            self.height = height;
            self.width_extent = width - 1;
            self.height_extent = height - 1;
            true
        } else {
            false
        }
    }
}

pub struct CachedPrimitive {
    buffer: Vec<[u8; 4]>,
    min_x: usize,
    min_y: usize,
    width: usize,
    height: usize,
    z_order: usize,
    seen_this_frame: bool,
    rendered_this_frame: bool,
    occupied_tiles: Vec<[u16; 2]>,
}

impl CachedPrimitive {
    fn get_buffer_ref(&self) -> BufferRef {
        BufferRef {
            data: &self.buffer,
            width: self.width,
            height: self.height,
            width_extent: self.width - 1,
            height_extent: self.height - 1,
        }
    }

    fn new(min_x: usize, min_y: usize, width: usize, height: usize, z_order: usize) -> Self {
        CachedPrimitive {
            buffer: vec![[0; 4]; width * height],
            min_x,
            min_y,
            width,
            height,
            z_order,
            seen_this_frame: true,
            rendered_this_frame: true,
            occupied_tiles: Vec::with_capacity(64),
        }
    }

    fn update_occupied_tiles(&mut self, tiles_wide: usize, tiles_tall: usize) {
        // list which tiles contain a pixel with that isn't fully transparent (also containing not color info)
        self.occupied_tiles.clear();
        let max_x = self.min_x + self.width;
        let max_y = self.min_y + self.height;
        let first_tile_x = (self.min_x / TILE_SIZE).min(tiles_wide);
        let first_tile_y = (self.min_y / TILE_SIZE).min(tiles_tall);
        let last_tile_x = max_x.div_ceil(TILE_SIZE).min(tiles_wide);
        let last_tile_y = max_y.div_ceil(TILE_SIZE).min(tiles_tall);

        for tile_y in first_tile_y..last_tile_y {
            let mut px_start_y = (tile_y * TILE_SIZE).max(self.min_y);
            let mut px_end_y = (px_start_y + TILE_SIZE).min(max_y);
            px_start_y -= self.min_y;
            px_end_y -= self.min_y;
            for tile_x in first_tile_x..last_tile_x {
                let mut px_start_x = (tile_x * TILE_SIZE).max(self.min_x);
                let mut px_end_x = (px_start_x + TILE_SIZE).min(max_x);
                px_start_x -= self.min_x;
                px_end_x -= self.min_x;

                'px_outer: for y in px_start_y..px_end_y {
                    for x in px_start_x..px_end_x {
                        // Purposefully panicing when out of bounds. If it's out of bounds then the math is wrong and
                        // the tile is not being calculated correctly.
                        if u32::from_le_bytes(self.buffer[x + y * self.width]) > 0 {
                            self.occupied_tiles.push([tile_x as u16, tile_y as u16]);
                            break 'px_outer;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct BufferMutRef<'a> {
    pub data: &'a mut [[u8; 4]],
    pub width: usize,
    pub height: usize,
    pub width_extent: usize,
    pub height_extent: usize,
}

impl<'a> BufferMutRef<'a> {
    pub fn new(data: &'a mut [[u8; 4]], width: usize, height: usize) -> Self {
        assert!(width > 0);
        assert!(height > 0);
        BufferMutRef {
            data,
            width,
            height,
            width_extent: width - 1,
            height_extent: height - 1,
        }
    }

    #[inline(always)]
    pub fn get_mut_clamped(&mut self, x: usize, y: usize) -> &mut [u8; 4] {
        let x = x.min(self.width_extent);
        let y = y.min(self.height_extent);
        &mut self.data[x + y * self.width]
    }
    #[inline(always)]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut [u8; 4] {
        &mut self.data[x + y * self.width]
    }
}

#[derive(Debug)]
pub struct BufferRef<'a> {
    pub data: &'a [[u8; 4]],
    pub width: usize,
    pub height: usize,
    pub width_extent: usize,
    pub height_extent: usize,
}

impl<'a> BufferRef<'a> {
    #[inline(always)]
    pub fn get_ref_clamped(&self, x: usize, y: usize) -> &[u8; 4] {
        let x = x.min(self.width_extent);
        let y = y.min(self.height_extent);
        &self.data[x + y * self.width]
    }
    #[inline(always)]
    pub fn get_ref(&self, x: usize, y: usize) -> &[u8; 4] {
        &self.data[x + y * self.width]
    }
}
