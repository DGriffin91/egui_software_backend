use std::{borrow::Cow, collections::HashMap, ops::Range};

use egui::{Color32, Mesh, Pos2, Vec2, vec2};

#[cfg(feature = "raster_stats")]
use crate::stats::RasterStats;
use crate::{
    color::{egui_blend_u8, swizzle_rgba_bgra},
    egui_texture::EguiTexture,
    hash::Hash32,
    render::{draw_egui_mesh, egui_orient2df},
};

pub(crate) mod color;
#[cfg(target_arch = "x86_64")]
pub(crate) mod color_x86_64_simd;
pub(crate) mod egui_texture;
pub(crate) mod hash;
pub(crate) mod math;
pub(crate) mod raster;
pub(crate) mod render;
#[cfg(feature = "raster_stats")]
pub mod stats;
#[cfg(feature = "test_render")]
pub mod test_render;

#[inline(always)]
pub(crate) fn sse41() -> bool {
    #[cfg(target_arch = "x86_64")]
    return is_x86_feature_detected!("sse4.1");
    #[cfg(not(target_arch = "x86_64"))]
    return false;
}

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
    tiles_dim: [usize; 2],
    dirty_tiles: Vec<u8>,
    target_size: Vec2,
    prims_updated_this_frame: usize,
    output_field_order: ColorFieldOrder,
    canvas: Canvas,
    redraw_everything_this_frame: bool,
    convert_tris_to_rects: bool,
    allow_raster_opt: bool,
    cacheing_enabled: bool,
    #[cfg(feature = "raster_stats")]
    pub stats: RasterStats,
}

impl EguiSoftwareRender {
    /// # Arguments
    /// * `output_field_order` - egui textures and vertex colors will be swizzled before rendering to match the desired
    ///   output buffer order.
    pub fn new(output_field_order: ColorFieldOrder) -> Self {
        EguiSoftwareRender {
            textures: Default::default(),
            cached_primitives: Default::default(),
            tiles_dim: Default::default(),
            dirty_tiles: Default::default(),
            target_size: Default::default(),
            prims_updated_this_frame: Default::default(),
            output_field_order,
            canvas: Default::default(),
            redraw_everything_this_frame: Default::default(),
            convert_tris_to_rects: true,
            allow_raster_opt: true,
            cacheing_enabled: true,
            #[cfg(feature = "raster_stats")]
            stats: Default::default(),
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

    /// If true: rasterized ClippedPrimitives are cached and rendered to an intermediate tiled canvas. That canvas is
    /// then rendered over the frame buffer. If false ClippedPrimitives are rendered directly to the frame buffer.
    /// Rendering without caching is much slower and primarily intended for testing.
    pub fn with_caching(mut self, set: bool) -> Self {
        self.cacheing_enabled = set;
        self
    }

    /// Renders the given paint jobs to buffer_ref. Alternatively, when using caching
    /// EguiSoftwareRender::render_to_canvas() and subsequently EguiSoftwareRender::blit_canvas_to_buffer() can be run
    /// separately so that the primary rendering in render_to_canvas() can happen without a lock on the frame buffer.
    ///  
    ///
    /// # Arguments
    /// * `paint_jobs` - List of `egui::ClippedPrimitive` from egui to be rendered.
    /// * `textures_delta` - The change in egui textures since last frame
    /// * `pixels_per_point` - The number of physical pixels for each logical point.
    pub fn render(
        &mut self,
        buffer_ref: &mut BufferMutRef,
        paint_jobs: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        if self.cacheing_enabled {
            self.render_to_canvas(
                buffer_ref.width,
                buffer_ref.height,
                paint_jobs,
                textures_delta,
                pixels_per_point,
            );
            self.blit_canvas_to_buffer(buffer_ref);
        } else {
            self.render_direct(buffer_ref, paint_jobs, textures_delta, pixels_per_point);
        }
    }

    /// Renders the given paint jobs to an intermediate canvas.
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

        #[cfg(feature = "raster_stats")]
        self.stats.clear();

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

        self.render_prims_to_cache(paint_jobs, pixels_per_point);

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

    /// Draw canvas alpha over given buffer.
    /// Only run after EguiSoftwareRender::render_to_canvas(), or use EguiSoftwareRender::render() to run both.
    /// Only writes tile regions that contain pixels that are not fully transparent.
    pub fn blit_canvas_to_buffer(&mut self, buffer: &mut BufferMutRef) {
        // Simple tile-less version
        // buffer.data.iter_mut().zip(self.canvas.iter()).for_each(|(pixel, src)| {
        //     *pixel = egui_blend_u8(*src, *pixel);
        // });

        if self.canvas.data.is_empty() {
            #[cfg(feature = "log")]
            log::error!(
                "Canvas not initialized, call EguiSoftwareRender::blit_canvas_to_buffer() only after EguiSoftwareRender::render_to_canvas()"
            );
            return;
        }

        let width = self.canvas.width;
        let height = self.canvas.height;
        assert_eq!(self.canvas.data.len(), width * height);
        assert_eq!(buffer.data.len(), width * height);

        let tiles_x = self.tiles_dim[0];

        #[cfg(feature = "rayon")]
        {
            use rayon::{
                iter::{IndexedParallelIterator, ParallelIterator},
                slice::ParallelSliceMut,
            };
            // blit rows of tiles in parallel

            let width = buffer.width;
            let px_per_row_of_tiles = width * TILE_SIZE;

            buffer
                .data
                .par_chunks_mut(px_per_row_of_tiles)
                .enumerate()
                .for_each(|(tile_row, tile_height_row)| {
                    let height = tile_height_row.len() / width; // Might be less than TILE_SIZE
                    let buffer_tile_row = &mut BufferMutRef::new(
                        bytemuck::cast_slice_mut(tile_height_row),
                        width,
                        height,
                    );

                    for (tile_idx, &mask) in self.dirty_tiles.iter().enumerate() {
                        if mask & Self::OCCUPIED_TILE_MASK == 0 {
                            continue;
                        }

                        let tile_y = tile_idx / tiles_x;
                        if tile_y != tile_row {
                            continue;
                        }

                        let tile_x = tile_idx % tiles_x;

                        let x_start = tile_x * TILE_SIZE;
                        let y_start = 0;
                        let x_end = (x_start + TILE_SIZE).min(width);
                        let y_end = TILE_SIZE.min(height);

                        let canvas_row_offset = tile_row * TILE_SIZE;

                        self.blit_tile(
                            buffer_tile_row,
                            x_start,
                            y_start,
                            x_end,
                            y_end,
                            canvas_row_offset,
                        );
                    }
                });
        }
        #[cfg(not(feature = "rayon"))]
        {
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

                self.blit_tile(buffer, x_start, y_start, x_end, y_end, 0);
            }
        }
    }

    fn blit_tile(
        &self,
        buffer: &mut BufferMutRef,
        x_start: usize,
        y_start: usize,
        x_end: usize,
        y_end: usize,
        canvas_row_offset: usize,
    ) {
        if sse41() {
            #[cfg(target_arch = "x86_64")]
            for y in y_start..y_end {
                let src_row = self.canvas.get_span(x_start, x_end, y + canvas_row_offset);
                let dst_row = &mut buffer.get_mut_span(x_start, x_end, y);
                // SAFETY: we first check sse41() outside the loop
                unsafe { color_x86_64_simd::egui_blend_u8_slice_sse41(src_row, dst_row) }
            }
        } else {
            for y in y_start..y_end {
                let src_row = self.canvas.get_span(x_start, x_end, y + canvas_row_offset);
                let dst_row = &mut buffer.get_mut_span(x_start, x_end, y);
                for (dst, &src) in dst_row.iter_mut().zip(src_row.iter()) {
                    *dst = egui_blend_u8(src, *dst);
                }
            }
        }
    }

    /// Render directly into buffer without cache. This is much slower and mainly intended for testing.
    fn render_direct(
        &mut self,
        direct_draw_buffer: &mut BufferMutRef,
        paint_jobs: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        #[cfg(feature = "raster_stats")]
        self.stats.clear();

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
                egui::epaint::Primitive::Callback(_) => {
                    #[cfg(feature = "log")]
                    log::error!("egui::epaint::Primitive::Callback(PaintCallback) not supported");
                    continue;
                }
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

            let px_mesh =
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
                    &clip_rect,
                    &px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                    self.convert_tris_to_rects,
                    #[cfg(feature = "raster_stats")]
                    &mut self.stats,
                );
            } else {
                draw_egui_mesh::<8>(
                    &self.textures,
                    direct_draw_buffer,
                    &clip_rect,
                    &px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                    self.convert_tris_to_rects,
                    #[cfg(feature = "raster_stats")]
                    &mut self.stats,
                );
            }
        }

        self.free_textures(textures_delta);
    }

    fn prepare_px_mesh(
        &self,
        pixels_per_point: f32,
        mesh: &egui::Mesh,
        mesh_min: &mut Vec2,
        mesh_max: &mut Vec2,
    ) -> Mesh {
        let mut px_mesh = mesh.clone();

        for v in px_mesh.vertices.iter_mut() {
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
        for i in (0..px_mesh.indices.len()).step_by(3) {
            let i0 = px_mesh.indices[i] as usize;
            let i1 = px_mesh.indices[i + 1] as usize;
            let i2 = px_mesh.indices[i + 2] as usize;
            let v0 = px_mesh.vertices[i0];
            let v1 = px_mesh.vertices[i1];
            let v2 = px_mesh.vertices[i2];
            let area = egui_orient2df(&v0.pos, &v1.pos, &v2.pos);
            if area < 0.0 {
                px_mesh.indices.swap(i + 1, i + 2);
            }
        }
        px_mesh
    }

    fn render_prims_to_cache(
        &mut self,
        paint_jobs: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) {
        struct CacheReuse {
            seen_this_frame: bool,
            z_order: usize,
            min_x: usize,
            min_y: usize,
            rendered_this_frame: bool,
            hash: u32,
        }

        enum CacheUpdate {
            CacheReuse(CacheReuse),
            New(u32, CachedPrimitive),
            None,
        }

        // Render paint jobs in parallel
        #[cfg(feature = "rayon")]
        use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
        #[cfg(feature = "rayon")]
        let iter = paint_jobs.par_iter().enumerate();

        #[cfg(not(feature = "rayon"))]
        let iter = paint_jobs.iter().enumerate();

        let updates: Vec<CacheUpdate> = iter
            .map(
                |(
                    prim_idx,
                    egui::ClippedPrimitive {
                        clip_rect,
                        primitive,
                    },
                )| {
                    let input_mesh = match primitive {
                        egui::epaint::Primitive::Mesh(input_mesh) => input_mesh,
                        egui::epaint::Primitive::Callback(_) => {
                            #[cfg(feature = "log")]
                            log::error!(
                                "egui::epaint::Primitive::Callback(PaintCallback) not supported"
                            );
                            return CacheUpdate::None;
                        }
                    };

                    if input_mesh.vertices.is_empty() || input_mesh.indices.is_empty() {
                        return CacheUpdate::None;
                    }

                    let clip_rect = egui::Rect {
                        min: clip_rect.min * pixels_per_point,
                        // TODO not sure why +1.5 is needed here. Occasionally things are cropped out without it.
                        max: clip_rect.max * pixels_per_point + egui::Vec2::splat(1.5),
                    };

                    let mut mesh_min = egui::Vec2::splat(f32::MAX);
                    let mut mesh_max = egui::Vec2::splat(-f32::MAX);

                    let px_mesh = self.prepare_px_mesh(
                        pixels_per_point,
                        input_mesh,
                        &mut mesh_min,
                        &mut mesh_max,
                    );

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
                        hasher.hash_wrap(match px_mesh.texture_id {
                            egui::TextureId::Managed(id) => id as u32,
                            egui::TextureId::User(id) => id as u32 + 9358476,
                        });
                        for ind in &px_mesh.indices {
                            let v = px_mesh.vertices[*ind as usize];

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
                        hasher.hash_wrap(px_mesh.indices.len() as u32);
                        hasher.finalize()
                    };

                    if self.cached_primitives.contains_key(&hash) {
                        CacheUpdate::CacheReuse(CacheReuse {
                            hash,
                            seen_this_frame: true,
                            z_order: prim_idx,
                            min_x: cropped_min.x as usize,
                            min_y: cropped_min.y as usize,
                            rendered_this_frame: false,
                        })
                    } else {
                        let width = (cropped_max.x - cropped_min.x + 0.5) as usize;
                        let height = (cropped_max.y - cropped_min.y + 0.5) as usize;

                        if width > 8192 || height > 8192 {
                            // TODO it occasionally tries to make giant buffers in the first couple frames initially for some reason.
                            return CacheUpdate::None;
                        }

                        if width == 0 || height == 0 {
                            return CacheUpdate::None;
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
                                &clip_rect,
                                &px_mesh,
                                offset,
                                self.allow_raster_opt,
                                self.convert_tris_to_rects,
                                #[cfg(feature = "raster_stats")]
                                &mut self.stats,
                            );
                        } else {
                            draw_egui_mesh::<8>(
                                &self.textures,
                                &mut buffer_ref,
                                &clip_rect,
                                &px_mesh,
                                offset,
                                self.allow_raster_opt,
                                self.convert_tris_to_rects,
                                #[cfg(feature = "raster_stats")]
                                &mut self.stats,
                            );
                        }
                        prim.update_occupied_tiles(self.tiles_dim[0], self.tiles_dim[1]);
                        CacheUpdate::New(hash, prim)
                    }
                },
            )
            .collect::<Vec<_>>();

        updates.into_iter().for_each(|update| match update {
            CacheUpdate::CacheReuse(cache_reuse) => {
                if let Some(cached_primitive) = self.cached_primitives.get_mut(&cache_reuse.hash) {
                    cached_primitive.seen_this_frame = cache_reuse.seen_this_frame;
                    cached_primitive.z_order = cache_reuse.z_order;
                    cached_primitive.min_x = cache_reuse.min_x;
                    cached_primitive.min_y = cache_reuse.min_y;
                    cached_primitive.rendered_this_frame = cache_reuse.rendered_this_frame;
                }
            }
            CacheUpdate::New(hash, prim) => {
                self.prims_updated_this_frame += 1;
                self.cached_primitives.insert(hash, prim);
            }
            CacheUpdate::None => (),
        });
    }

    fn update_canvas_from_cached(&mut self) {
        let mut sorted_prim_cache = self.cached_primitives.values().collect::<Vec<_>>();
        sorted_prim_cache.sort_unstable_by_key(|prim| prim.z_order);

        #[allow(unused_mut)]
        let mut canvas =
            BufferMutRef::new(&mut self.canvas.data, self.canvas.width, self.canvas.height);

        #[cfg(feature = "rayon")]
        {
            use rayon::{
                iter::{IndexedParallelIterator, ParallelIterator},
                slice::ParallelSliceMut,
            };
            // composite rows of tiles in parallel

            let full_height = self.canvas.height;

            let width = canvas.width;
            let px_per_row_of_tiles = width * TILE_SIZE;

            canvas
                .data
                .par_chunks_mut(px_per_row_of_tiles)
                .enumerate()
                .for_each(|(tile_row, tile_height_row)| {
                    let height = tile_height_row.len() / width; // Might be less than TILE_SIZE
                    let canvas_tile_row = &mut BufferMutRef::new(
                        bytemuck::cast_slice_mut(tile_height_row),
                        width,
                        height,
                    );

                    for (tile_idx, &mask) in self.dirty_tiles.iter().enumerate() {
                        if mask & Self::OCCUPIED_TILE_MASK == 0 {
                            continue;
                        }

                        let tile_y = tile_idx / self.tiles_dim[0];

                        if tile_y != tile_row {
                            continue;
                        }
                        let canvas_row_offset = tile_row * TILE_SIZE;

                        let tile_x = tile_idx % self.tiles_dim[0];

                        update_canvas_tile(
                            &sorted_prim_cache,
                            canvas_tile_row,
                            tile_x,
                            tile_y,
                            full_height,
                            canvas_row_offset,
                        );
                    }
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for tile_idx in self
                .dirty_tiles
                .iter()
                .enumerate()
                .filter(|(_, t)| **t & Self::DIRTY_TILE_MASK != 0)
                .map(|(idx, _)| idx)
            {
                let tile_x = tile_idx % self.tiles_dim[0];
                let tile_y = tile_idx / self.tiles_dim[0];
                let full_height = canvas.height;
                update_canvas_tile(
                    &sorted_prim_cache,
                    &mut canvas,
                    tile_x,
                    tile_y,
                    full_height,
                    0,
                );
            }
        }
    }

    fn clear_unused_cached_prims(&mut self) {
        self.cached_primitives
            .retain(|_hash, prim| prim.seen_this_frame);
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
            if delta.options.magnification != delta.options.minification {
                // Would need helper lanes to impl?
                #[cfg(feature = "log")]
                log::warn!(
                    "TextureOptions magnification and minification not matching is unsupported."
                );
            }
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
                let new_texture =
                    EguiTexture::new(self.output_field_order, delta.options, size, &pixels);

                self.textures.insert(*id, new_texture);
            }
        }
    }

    fn free_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        for free in &textures_delta.free {
            self.textures.remove(free);
        }
    }
}

fn update_canvas_tile(
    sorted_prim_cache: &[&CachedPrimitive],
    canvas: &mut BufferMutRef,
    tile_x: usize,
    tile_y: usize,
    full_height: usize,
    canvas_row_offset: usize,
) {
    let tile_x_start = tile_x * TILE_SIZE;
    let tile_y_start = tile_y * TILE_SIZE;
    let tile_x_end = (tile_x_start + TILE_SIZE).min(canvas.width);
    let tile_y_end = (tile_y_start + TILE_SIZE).min(full_height);

    // clear tile
    for y in (tile_y_start - canvas_row_offset)..(tile_y_end - canvas_row_offset) {
        let row_start = y * canvas.width;
        let start = row_start + tile_x_start;
        let end = row_start + tile_x_end;
        canvas.data[start..end].fill([0; 4]);
    }

    let tile_n = [tile_x as u16, tile_y as u16];
    // redraw cached prims on tile
    for prim in sorted_prim_cache {
        if !prim.occupied_tiles.contains(&tile_n) {
            continue;
        }

        let mut min_x = prim.min_x;
        let mut min_y = prim.min_y;
        let mut max_x = min_x + prim.width;
        let mut max_y = min_y + prim.height;

        min_x = min_x.max(tile_x_start).min(canvas.width);
        min_y = min_y
            .max(tile_y_start)
            .min(canvas.height + canvas_row_offset);
        max_x = max_x.min(tile_x_end).min(canvas.width);
        max_y = max_y.min(tile_y_end).min(canvas.height + canvas_row_offset);

        let prim_buf = prim.get_buffer_ref();

        if max_x <= min_x || max_y <= min_y {
            continue;
        }
        let prim_x_min = (min_x - prim.min_x).min(prim_buf.width);
        let prim_x_max = (max_x - prim.min_x).min(prim_buf.width);

        let get_ranges = |y: usize| -> (Range<usize>, Range<usize>) {
            let canvas_row_start = (y - canvas_row_offset).min(canvas.height) * canvas.width;
            let canvas_start = canvas_row_start + min_x;
            let canvas_end = canvas_row_start + max_x;

            let prim_y = (y - prim.min_y).min(prim_buf.height);
            let prim_row_start = prim_y * prim_buf.width;
            let prim_start = prim_row_start + prim_x_min;
            let prim_end = prim_row_start + prim_x_max;

            (canvas_start..canvas_end, prim_start..prim_end)
        };

        if sse41() {
            #[cfg(target_arch = "x86_64")]
            for y in min_y..max_y {
                let (canvas_slice, prim_slice) = get_ranges(y);
                let src_row = &prim_buf.data[prim_slice];
                let dst_row = &mut canvas.data[canvas_slice];
                // SAFETY: we first check sse41() outside the loop
                unsafe { color_x86_64_simd::egui_blend_u8_slice_sse41(src_row, dst_row) }
            }
        } else {
            for y in min_y..max_y {
                let (canvas_slice, prim_slice) = get_ranges(y);
                let src_row = &prim_buf.data[prim_slice];
                let dst_row = &mut canvas.data[canvas_slice];
                for (pixel, src) in dst_row.iter_mut().zip(src_row) {
                    *pixel = egui_blend_u8(*src, *pixel);
                }
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

    #[inline(always)]
    pub fn get_range(&self, start: usize, end: usize, y: usize) -> Range<usize> {
        let row_start = y * self.width;
        let start = row_start + start;
        let end = row_start + end;
        start..end
    }

    #[inline(always)]
    pub fn get_span(&self, start: usize, end: usize, y: usize) -> &[[u8; 4]] {
        let range = self.get_range(start, end, y);
        &self.data[range]
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
    fn get_buffer_ref(&self) -> BufferRef<'_> {
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
    pub fn get_range(&self, start: usize, end: usize, y: usize) -> Range<usize> {
        let row_start = y * self.width;
        let start = row_start + start;
        let end = row_start + end;
        start..end
    }

    #[inline(always)]
    pub fn get_mut_span(&mut self, start: usize, end: usize, y: usize) -> &mut [[u8; 4]] {
        let range = self.get_range(start, end, y);
        &mut self.data[range]
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
