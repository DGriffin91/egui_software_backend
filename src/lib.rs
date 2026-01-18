#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::ops::{Deref, DerefMut, Range};

use alloc::{borrow::Cow, vec, vec::Vec};

use egui::{Color32, Mesh, Pos2, Vec2, ahash::HashMap, vec2};
#[cfg(feature = "rayon")]
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};

#[cfg(feature = "raster_stats")]
use crate::stats::RenderStats;
use crate::{
    color::{SelectedImpl, swizzle_rgba_bgra},
    dirty_rect::{ComputeTiledDirtyRects, DirtyRect},
    egui_texture::EguiTexture,
    hash::Hash32,
    render::{draw_egui_mesh, egui_orient2df},
};

pub(crate) mod color;
pub(crate) mod dirty_rect;
pub(crate) mod egui_texture;
pub(crate) mod hash;
pub(crate) mod math;
pub(crate) mod raster;
pub(crate) mod render;
#[cfg(feature = "raster_stats")]
pub mod stats;
#[cfg(feature = "test_render")]
pub mod test_render;

#[cfg(feature = "winit")]
mod winit;

#[cfg(feature = "winit")]
pub use winit::{
    App, SoftwareBackend, SoftwareBackendAppConfiguration, run_app_with_software_backend,
};

const TILE_SIZE: u32 = 64;

#[derive(Copy, Clone, Default)]
pub enum ColorFieldOrder {
    #[default]
    Rgba,
    Bgra,
}

/// Caching mode for the renderer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftwareRenderCaching {
    /// Cache primitives renders, update the dirty tiles
    ///
    /// This is the default mode and often the fastest mode, but it cost the most memory
    ///
    /// # Algorithm
    /// * Prepare Mesh from primitives
    /// * Hash prepared meshes for changes
    /// * Render non already cached meshes to cache
    /// * Mark dirty tiles
    /// * Reclaim unused cached meshes renders
    /// * Render dirty tiles by blending cache renders
    BlendTiled,
    /// Cache primitives meshes, redraw primitives intersecting a set of changed bboxes
    ///
    /// Primitives are rendered clipped per intersection with a non overlapping set
    /// of changed tiled bounding boxes.
    ///
    /// # Algorithm
    /// * Prepare Mesh from primitives
    /// * Hash prepared meshes for changes
    /// * Accumulate dirty primitives bounding boxes
    /// * Reclaim unused cached meshes
    /// * Generate non overlaping set of tiled bounding boxes
    /// * Render primitives intersecting tiled bounding boxes.
    MeshTiled,
    /// Cache primitives meshes, redraw primitives in the smallest changed bbox
    ///
    /// Primitives are rendered clipped to the union of changed bounding boxes.
    ///
    /// # Algorithm
    /// * Prepare Mesh from primitives
    /// * Hash prepared meshes for changes
    /// * Reclaim unused cached meshes
    /// * Render primitives intersecting dirty rect
    Mesh,
    /// No cache, always redraw the whole frame (slow, for testing mostly)
    Direct,
}

struct EguiSoftwareRenderInner {
    cached_size: (u32, u32),
    textures: HashMap<egui::TextureId, EguiTexture>,
    /// Tiles grid size (cols, rows)
    tiles_dim: [u32; 2],
    dirty_tiles: Vec<u8>,
    dirty_rects: ComputeTiledDirtyRects,
    output_field_order: ColorFieldOrder,
    convert_tris_to_rects: bool,
    allow_raster_opt: bool,
    mode: SoftwareRenderCaching,
    #[cfg(feature = "raster_stats")]
    pub stats: RenderStats,
}

/// egui software renderer
pub struct EguiSoftwareRender {
    tiledcached_primitives: HashMap<u32, TiledCachedPrimitive>,
    dirtycached_primitives: HashMap<u32, MeshCachedPrimitive>,
    inner: EguiSoftwareRenderInner,
}

/// egui software renderer to canvas
pub struct EguiSoftwareRenderCanvas {
    canvas: Vec<[u8; 4]>,
    renderer: EguiSoftwareRender,
}

impl Deref for EguiSoftwareRenderCanvas {
    type Target = EguiSoftwareRender;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl DerefMut for EguiSoftwareRenderCanvas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.renderer
    }
}

fn blit_rect(
    simd_impl: impl SelectedImpl,
    canvas: &BufferMutRef,
    buffer: &mut BufferMutRef,
    rect: DirtyRect,
    canvas_row_offset: u32,
) {
    for y in rect.min_y..rect.max_y {
        let src_row = canvas.get_span(rect.min_x, rect.max_x, y + canvas_row_offset);
        let dst_row = &mut buffer.get_mut_span(rect.min_x, rect.max_x, y);

        simd_impl.egui_blend_u8_slice(src_row, dst_row)
    }
}

impl EguiSoftwareRenderCanvas {
    pub fn render(
        &mut self,
        buffer_ref: &mut BufferMutRef,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        if self.renderer.inner.mode == SoftwareRenderCaching::Direct {
            self.renderer.render(
                buffer_ref,
                true,
                paint_jobs,
                textures_delta,
                pixels_per_point,
            );
        } else {
            let redraw_everything_this_frame =
                self.renderer.cached_size() != (buffer_ref.width, buffer_ref.height);
            if redraw_everything_this_frame {
                self.canvas.clear();
                let len = as_usize(buffer_ref.width) * as_usize(buffer_ref.height);
                self.canvas.resize(len, [0; 4]);
                // ^ data is now cleared in a singled memset call
            }
            let mut canvas =
                BufferMutRef::new(&mut self.canvas, buffer_ref.width, buffer_ref.height);
            let dirty_rect = self.renderer.render(
                &mut canvas,
                redraw_everything_this_frame,
                paint_jobs,
                textures_delta,
                pixels_per_point,
            );
            if self.renderer.inner.mode == SoftwareRenderCaching::BlendTiled {
                dispatch_simd_impl!(|simd_impl| self
                    .renderer
                    .inner
                    .blit_to_buffer_from_tiledcanvas(simd_impl, &canvas, buffer_ref));
            } else {
                dispatch_simd_impl!(|simd_impl| blit_rect(
                    simd_impl, &canvas, buffer_ref, dirty_rect, 0
                ));
            }
        }
    }
}

impl EguiSoftwareRender {
    /// # Arguments
    /// * `output_field_order` - egui textures and vertex colors will be swizzled before rendering to match the desired
    ///   output buffer order.
    pub fn new(output_field_order: ColorFieldOrder) -> Self {
        EguiSoftwareRender {
            tiledcached_primitives: Default::default(),
            dirtycached_primitives: Default::default(),
            inner: EguiSoftwareRenderInner {
                cached_size: (0, 0),
                textures: Default::default(),
                tiles_dim: Default::default(),
                dirty_tiles: Default::default(),
                dirty_rects: Default::default(),
                output_field_order,
                convert_tris_to_rects: true,
                allow_raster_opt: true,
                mode: SoftwareRenderCaching::BlendTiled,
                #[cfg(feature = "raster_stats")]
                stats: Default::default(),
            },
        }
    }

    /// If true: attempts to optimize by converting suitable triangle pairs into rectangles for faster rendering.
    ///   Things *should* look the same with this set to `true` while rendering faster.
    pub fn with_convert_tris_to_rects(mut self, set: bool) -> Self {
        self.inner.convert_tris_to_rects = set;
        self
    }

    /// If false: Rasterize everything with triangles, always calculate vertex colors, uvs, use bilinear
    ///   everywhere, etc... Things *should* look the same with this set to `true` while rendering faster.
    pub fn with_allow_raster_opt(mut self, set: bool) -> Self {
        self.inner.allow_raster_opt = set;
        self
    }

    /// If true: rasterized ClippedPrimitives are cached and rendered to an intermediate tiled canvas. That canvas is
    /// then rendered over the frame buffer. If false ClippedPrimitives are rendered directly to the frame buffer.
    /// Rendering without caching is much slower and primarily intended for testing.
    pub fn with_mode(mut self, set: SoftwareRenderCaching) -> Self {
        self.inner.mode = set;
        self
    }

    pub fn with_canvas(self) -> EguiSoftwareRenderCanvas {
        EguiSoftwareRenderCanvas {
            canvas: Vec::new(),
            renderer: self,
        }
    }

    #[cfg(feature = "raster_stats")]
    pub fn stats(&self) -> &RenderStats {
        &self.inner.stats
    }

    /// Get the caching mode of the renderer
    pub fn caching(&self) -> SoftwareRenderCaching {
        self.inner.mode
    }

    /// Change the caching mode of the renderer
    pub fn set_caching(&mut self, caching: SoftwareRenderCaching) {
        if self.inner.mode == caching {
            return;
        }
        self.inner.mode = caching;
        self.clear_cache();
    }

    /// Clear cache and reclaim memory
    ///
    /// This will cause the next render to redraw everything
    pub fn clear_cache(&mut self) {
        self.tiledcached_primitives = Default::default();
        self.dirtycached_primitives = Default::default();
        self.inner.dirty_tiles = Default::default();
        self.inner.dirty_rects = Default::default();
    }

    /// The latest renderer `buffer_ref` width and height, if a cacheing mode is selected
    pub const fn cached_size(&self) -> (u32, u32) {
        self.inner.cached_size
    }

    /// Renders the given paint jobs to buffer_ref. Alternatively, when using caching
    /// EguiSoftwareRender::render_to_canvas() and subsequently EguiSoftwareRender::blit_canvas_to_buffer() can be run
    /// separately so that the primary rendering in render_to_canvas() can happen without a lock on the frame buffer.
    ///
    ///
    /// # Arguments
    /// * `buffer_ref` - Buffer to render into.
    /// * `redraw_everything_this_frame` - Redraw the whole buffer (ie. resize)
    /// * `paint_jobs` - List of `egui::ClippedPrimitive` from egui to be rendered.
    /// * `paint_jobs` - List of `egui::ClippedPrimitive` from egui to be rendered.
    /// * `textures_delta` - The change in egui textures since last frame
    /// * `pixels_per_point` - The number of physical pixels for each logical point.
    ///
    /// # Returns
    /// The smallest rect containing all updated pixels
    ///
    /// # Panics
    /// * `buffer_ref` width or height non positive
    /// * `pixels_per_point` non positive
    /// * `buffer_ref` width or height must match `cached_size()` if `!redraw_everything_this_frame`
    pub fn render(
        &mut self,
        buffer_ref: &mut BufferMutRef,
        redraw_everything_this_frame: bool,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> DirtyRect {
        #[cfg(feature = "raster_stats")]
        self.inner.stats.clear();
        match self.inner.mode {
            SoftwareRenderCaching::Direct => {
                self.inner
                    .render_direct(buffer_ref, paint_jobs, textures_delta, pixels_per_point);
                DirtyRect {
                    min_x: 0,
                    min_y: 0,
                    max_x: buffer_ref.width,
                    max_y: buffer_ref.height,
                }
            }
            SoftwareRenderCaching::MeshTiled | SoftwareRenderCaching::Mesh => self
                .render_meshmaybetiled(
                    buffer_ref,
                    redraw_everything_this_frame,
                    paint_jobs,
                    textures_delta,
                    pixels_per_point,
                ),
            SoftwareRenderCaching::BlendTiled => self.render_blendtiled(
                buffer_ref,
                redraw_everything_this_frame,
                paint_jobs,
                textures_delta,
                pixels_per_point,
            ),
        }
    }

    fn render_blendtiled(
        &mut self,
        canvas: &mut BufferMutRef,
        redraw_everything_this_frame: bool,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> DirtyRect {
        // TODO: need to deal with user textures. Either make the fields of EguiUserTextures pub or need to come up with a replacement.

        let dirty_rect = self.inner.prepare_render_cache(
            &mut self.tiledcached_primitives,
            canvas,
            redraw_everything_this_frame,
            paint_jobs,
            textures_delta,
            pixels_per_point,
            EguiSoftwareRenderInner::render_prim,
            EguiSoftwareRenderInner::update_dirty_tiles,
        );

        if !dirty_rect.is_empty() {
            self.inner
                .render_from_tiledcache(&self.tiledcached_primitives, canvas);
        }
        dirty_rect
    }
    fn render_meshmaybetiled(
        &mut self,
        canvas: &mut BufferMutRef,
        redraw_everything_this_frame: bool,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> DirtyRect {
        let dirty_rect = self.inner.prepare_render_cache(
            &mut self.dirtycached_primitives,
            canvas,
            redraw_everything_this_frame,
            paint_jobs,
            textures_delta,
            pixels_per_point,
            |_self, prim, _cropped_min, _cropped_max, clip_rect, px_mesh| MeshCachedPrimitive {
                inner: prim,
                px_mesh,
                clip_rect,
            },
            EguiSoftwareRenderInner::update_dirty_rects,
        );
        if !dirty_rect.is_empty() {
            self.inner
                .render_from_meshcache(&self.dirtycached_primitives, canvas, dirty_rect);
        }
        dirty_rect
    }
}

impl EguiSoftwareRenderInner {
    #[allow(clippy::too_many_arguments)]
    fn prepare_render_cache<F, U, P>(
        &mut self,
        cached_primitives: &mut HashMap<u32, P>,
        canvas: &mut BufferMutRef,
        redraw_everything_this_frame: bool,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
        f_render_prims_to_cache: F,
        f_update_dirty_tiles: U,
    ) -> DirtyRect
    where
        F: Fn(&Self, CacheReuse, Vec2, Vec2, egui::Rect, Mesh) -> P + Sync + Send,
        U: Fn(&mut Self, &HashMap<u32, P>),
        P: DerefMut<Target = CacheReuse> + Sync + Send,
    {
        // TODO: need to deal with user textures. Either make the fields of EguiUserTextures pub or need to come up with a replacement.

        assert!(canvas.width > 0);
        assert!(canvas.height > 0);
        assert!(pixels_per_point > 0.0);

        if redraw_everything_this_frame {
            cached_primitives.clear();
        } else {
            assert_eq!(self.cached_size, (canvas.width, canvas.height));
        }
        self.cached_size = (canvas.width, canvas.height);

        for (_hash, prim) in cached_primitives.iter_mut() {
            prim.deref_mut().seen_this_frame = false;
        }

        self.tiles_dim = [
            canvas.width.div_ceil(TILE_SIZE),
            canvas.height.div_ceil(TILE_SIZE),
        ];

        self.set_textures(textures_delta);

        self.render_prims_to_cache(
            cached_primitives,
            paint_jobs,
            pixels_per_point,
            f_render_prims_to_cache,
        );

        let mut dirty_rect = self.update_dirty_rect(cached_primitives);

        if !dirty_rect.is_empty() {
            f_update_dirty_tiles(self, cached_primitives);
        }

        // clear_unused_cached_prims
        cached_primitives.retain(|_hash, prim| prim.deref().seen_this_frame);

        if redraw_everything_this_frame {
            dirty_rect = DirtyRect {
                min_x: 0,
                min_y: 0,
                max_x: canvas.width,
                max_y: canvas.height,
            };
        }

        self.free_textures(textures_delta);
        dirty_rect
    }
    /// Draw canvas alpha over given buffer.
    /// Only run after EguiSoftwareRender::render() with TiledCacheing to run both.
    /// Only writes tile regions that contain pixels that are not fully transparent.
    fn blit_to_buffer_from_tiledcanvas(
        &self,
        simd_impl: impl SelectedImpl,
        canvas: &BufferMutRef,
        buffer: &mut BufferMutRef,
    ) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        // Simple tile-less version
        // buffer.data.iter_mut().zip(self.canvas.iter()).for_each(|(pixel, src)| {
        //     *pixel = egui_blend_u8(*src, *pixel);
        // });

        if canvas.data.is_empty() {
            #[cfg(feature = "log")]
            log::error!(
                "Canvas not initialized, call EguiSoftwareRender::blit_canvas_to_buffer() only after EguiSoftwareRender::render_to_canvas()"
            );
            return;
        }

        let width = canvas.width;
        let height = canvas.height;
        assert_eq!(canvas.data.len(), as_usize(width * height));
        assert_eq!(buffer.data.len(), as_usize(width * height));

        let tiles_x = self.tiles_dim[0];

        #[cfg(feature = "rayon")]
        {
            use rayon::{
                iter::{IndexedParallelIterator, ParallelIterator},
                slice::ParallelSliceMut,
            };
            // blit rows of tiles in parallel

            let width = buffer.width;
            let px_per_row_of_tiles = as_usize(width) * as_usize(TILE_SIZE);

            buffer
                .data
                .par_chunks_mut(px_per_row_of_tiles)
                .enumerate()
                .for_each(|(tile_row, tile_height_row)| {
                    let tile_row = tile_row as u32;
                    let height = tile_height_row.len() as u32 / width; // Might be less than TILE_SIZE
                    let buffer_tile_row = &mut BufferMutRef::new(tile_height_row, width, height);

                    for (tile_idx, &mask) in self.dirty_tiles.iter().enumerate() {
                        if mask & EguiSoftwareRenderInner::OCCUPIED_TILE_MASK == 0 {
                            continue;
                        }

                        let tile_idx = tile_idx as u32;
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

                        blit_rect(
                            simd_impl,
                            canvas,
                            buffer_tile_row,
                            DirtyRect {
                                min_x: x_start,
                                min_y: y_start,
                                max_x: x_end,
                                max_y: y_end,
                            },
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

                let tile_idx = tile_idx as u32;
                let tile_x = tile_idx % tiles_x;
                let tile_y = tile_idx / tiles_x;

                let x_start = tile_x * TILE_SIZE;
                let y_start = tile_y * TILE_SIZE;
                let x_end = (x_start + TILE_SIZE).min(width);
                let y_end = (y_start + TILE_SIZE).min(height);

                blit_rect(
                    simd_impl,
                    canvas,
                    buffer,
                    DirtyRect {
                        min_x: x_start,
                        min_y: y_start,
                        max_x: x_end,
                        max_y: y_end,
                    },
                    0,
                )
            }
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.blit_canvas_to_buffer.mark(start);
        }
    }

    /// Render directly into buffer without cache. This is much slower and mainly intended for testing.
    fn render_direct(
        &mut self,
        direct_draw_buffer: &mut BufferMutRef,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        self.set_textures(textures_delta);

        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        for paint_job in paint_jobs {
            // TODO not sure why +1.5 is needed here. Occasionally things are cropped out without it.
            let splat = 1.5f32;
            let (clip_rect, mesh_min, mesh_max, px_mesh) =
                match self.prim_prepare_px_mesh(splat, pixels_per_point, paint_job) {
                    Some(x) => x,
                    None => continue,
                };

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
                    #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
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
                    #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
                    &mut self.stats,
                );
            }
        }
        #[cfg(feature = "raster_stats")]
        {
            self.stats.render_direct.mark(start);
        }

        self.free_textures(textures_delta);
    }

    fn render_prim(
        &self,
        prim: CacheReuse,
        cropped_min: Vec2,
        cropped_max: Vec2,
        _clip_rect: egui::Rect,
        px_mesh: Mesh,
    ) -> TiledCachedPrimitive {
        let (width, height) = (prim.rect.width(), prim.rect.height());
        let mut prim = TiledCachedPrimitive {
            inner: prim,
            buffer: vec![[0u8; 4]; as_usize(width) * as_usize(height)],
            occupied_tiles: Vec::with_capacity(64),
        };
        let mut buffer_ref = BufferMutRef {
            data: &mut prim.buffer,
            width,
            height,
            width_extent: width - 1,
            height_extent: height - 1,
        };

        let clip_rect = egui::Rect {
            min: Pos2::ZERO,
            max: (cropped_max - cropped_min).to_pos2(),
        };
        let offset = -vec2(cropped_min.x.floor(), cropped_min.y.floor());

        let render_in_low_precision = width > 4096 || height > 4096;
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
                #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
                &self.stats,
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
                #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
                &self.stats,
            );
        }
        prim.update_occupied_tiles(self.tiles_dim[0], self.tiles_dim[1]);
        prim
    }

    fn prim_prepare_update<F, P>(
        &self,
        cached_primitives: &HashMap<u32, P>,
        pixels_per_point: f32,
        prim_idx: u32,
        paint_job: egui::ClippedPrimitive,
        f: F,
    ) -> CacheUpdate<P>
    where
        F: Fn(&Self, CacheReuse, Vec2, Vec2, egui::Rect, Mesh) -> P + Sync + Send,
        P: DerefMut<Target = CacheReuse> + Sync + Send,
    {
        let splat = 0.5f32;
        let (clip_rect, mesh_min, mesh_max, px_mesh) =
            match self.prim_prepare_px_mesh(splat, pixels_per_point, paint_job) {
                Some(x) => x,
                None => return CacheUpdate::None,
            };

        let cropped_min = mesh_min.max(clip_rect.min.to_vec2());
        let cropped_max = mesh_max.min(clip_rect.max.to_vec2());
        let cropped_size = (cropped_max - cropped_min).to_pos2();

        let hash = {
            let mut hasher = Hash32::new_fnv();

            hasher.hash_wrap(cropped_size.x.to_bits());
            hasher.hash_wrap(cropped_size.y.to_bits());
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

        let width = (cropped_max.x - cropped_min.x + 0.5) as u32;
        let height = (cropped_max.y - cropped_min.y + 0.5) as u32;
        let rect = DirtyRect {
            min_x: cropped_min.x as u32,
            min_y: cropped_min.y as u32,
            max_x: cropped_min.x as u32 + width,
            max_y: cropped_min.y as u32 + height,
        };
        if cached_primitives.contains_key(&hash) {
            CacheUpdate::CacheReuse(
                hash,
                CacheReuse {
                    z_order: prim_idx,
                    rect,
                    seen_this_frame: true,
                    rendered_this_frame: false,
                },
            )
        } else {
            if width > 8192 || height > 8192 {
                // TODO it occasionally tries to make giant buffers in the first couple frames initially for some reason.
                return CacheUpdate::None;
            }

            if width == 0 || height == 0 {
                return CacheUpdate::None;
            }

            let prim = CacheReuse {
                z_order: prim_idx,
                rect,
                seen_this_frame: true,
                rendered_this_frame: true,
            };
            CacheUpdate::New(
                hash,
                f(self, prim, cropped_min, cropped_max, clip_rect, px_mesh),
            )
        }
    }

    fn prim_prepare_px_mesh(
        &self,
        splat: f32,
        pixels_per_point: f32,
        egui::ClippedPrimitive {
            clip_rect,
            primitive,
        }: egui::ClippedPrimitive,
    ) -> Option<(egui::Rect, Vec2, Vec2, Mesh)> {
        let input_mesh = match primitive {
            egui::epaint::Primitive::Mesh(input_mesh) => input_mesh,
            egui::epaint::Primitive::Callback(_) => {
                #[cfg(feature = "log")]
                log::error!("egui::epaint::Primitive::Callback(PaintCallback) not supported");
                return None;
            }
        };
        if input_mesh.vertices.is_empty() || input_mesh.indices.is_empty() {
            return None;
        }
        let clip_rect = egui::Rect {
            min: clip_rect.min * pixels_per_point,
            max: clip_rect.max * pixels_per_point + egui::Vec2::splat(splat),
        };
        let mut mesh_min = egui::Vec2::splat(f32::MAX);
        let mut mesh_max = egui::Vec2::splat(-f32::MAX);

        let mut px_mesh = input_mesh;

        for v in px_mesh.vertices.iter_mut() {
            v.pos *= pixels_per_point;

            match self.output_field_order {
                ColorFieldOrder::Rgba => (), // egui uses rgba
                ColorFieldOrder::Bgra => {
                    let d = swizzle_rgba_bgra(v.color.to_array());
                    v.color = Color32::from_rgba_premultiplied(d[0], d[1], d[2], d[3]);
                }
            }

            mesh_min = mesh_min.min(v.pos.to_vec2());
            mesh_max = mesh_max.max(v.pos.to_vec2());
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

        Some((clip_rect, mesh_min, mesh_max, px_mesh))
    }

    fn render_prims_to_cache<F, P>(
        &self,
        cached_primitives: &mut HashMap<u32, P>,
        paint_jobs: Vec<egui::ClippedPrimitive>,
        pixels_per_point: f32,
        f: F,
    ) where
        F: Fn(&Self, CacheReuse, Vec2, Vec2, egui::Rect, Mesh) -> P + Sync + Send,
        P: DerefMut<Target = CacheReuse> + Sync + Send,
    {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        // Render paint jobs in parallel
        #[cfg(feature = "rayon")]
        let iter = paint_jobs.into_par_iter().enumerate();

        #[cfg(not(feature = "rayon"))]
        let iter = paint_jobs.into_iter().enumerate();

        let updates: Vec<CacheUpdate<P>> = iter
            .map(|(prim_idx, paint_job)| {
                self.prim_prepare_update(
                    cached_primitives,
                    pixels_per_point,
                    prim_idx as u32,
                    paint_job,
                    &f,
                )
            })
            .collect::<Vec<_>>();

        updates.into_iter().for_each(|update| match update {
            CacheUpdate::CacheReuse(hash, cache_reuse) => {
                if let Some(cached_primitive) = cached_primitives.get_mut(&hash) {
                    *cached_primitive.deref_mut() = cache_reuse;
                }
            }
            CacheUpdate::New(hash, prim) => {
                cached_primitives.insert(hash, prim);
            }
            CacheUpdate::None => (),
        });

        #[cfg(feature = "raster_stats")]
        {
            self.stats.render_prims_to_cache.mark(start);
        }
    }

    fn render_from_meshcache(
        &self,
        cached_primitives: &HashMap<u32, MeshCachedPrimitive>,
        direct_draw_buffer: &mut BufferMutRef,
        dirty_rect: DirtyRect,
    ) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        let mut sorted_prim_cache = cached_primitives.values().collect::<Vec<_>>();
        sorted_prim_cache.sort_unstable_by_key(|prim| prim.inner.z_order);

        let mut render_from_meshcache_prim = |prim: &MeshCachedPrimitive, dirty_rect: DirtyRect| {
            let clip_rect = prim.clip_rect.intersect(dirty_rect.to_egui_rect());
            let (width, height) = (prim.rect.width(), prim.rect.height());
            let render_in_low_precision = width > 4096 || height > 4096;
            if render_in_low_precision {
                draw_egui_mesh::<2>(
                    &self.textures,
                    direct_draw_buffer,
                    &clip_rect,
                    &prim.px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                    self.convert_tris_to_rects,
                    #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
                    &self.stats,
                );
            } else {
                draw_egui_mesh::<8>(
                    &self.textures,
                    direct_draw_buffer,
                    &clip_rect,
                    &prim.px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                    self.convert_tris_to_rects,
                    #[cfg(all(feature = "raster_stats", not(feature = "rayon")))]
                    &self.stats,
                );
            }
        };

        match self.mode {
            SoftwareRenderCaching::MeshTiled => {
                for &prim in &sorted_prim_cache {
                    for dirty_rect in self.dirty_rects.intersections(prim.rect) {
                        render_from_meshcache_prim(prim, dirty_rect);
                    }
                }
            }
            SoftwareRenderCaching::Mesh => {
                for &prim in &sorted_prim_cache {
                    render_from_meshcache_prim(prim, dirty_rect);
                }
            }
            _ => unreachable!(),
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.render_from_meshcache.mark(start);
        }
    }

    fn render_from_tiledcache(
        &mut self,
        cached_primitives: &HashMap<u32, TiledCachedPrimitive>,
        canvas: &mut BufferMutRef,
    ) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        let mut sorted_prim_cache = cached_primitives.values().collect::<Vec<_>>();
        sorted_prim_cache.sort_unstable_by_key(|prim| prim.inner.z_order);

        #[cfg(feature = "rayon")]
        {
            use rayon::{
                iter::{IndexedParallelIterator, ParallelIterator},
                slice::ParallelSliceMut,
            };
            // composite rows of tiles in parallel

            let full_height = canvas.height;

            let width = canvas.width;
            let px_per_row_of_tiles = as_usize(width) * as_usize(TILE_SIZE);

            canvas
                .data
                .par_chunks_mut(px_per_row_of_tiles)
                .enumerate()
                .for_each(|(tile_row, tile_height_row)| {
                    let height = tile_height_row.len() as u32 / width; // Might be less than TILE_SIZE
                    let canvas_tile_row = &mut BufferMutRef::new(tile_height_row, width, height);

                    let dirty_tile_row_start = tile_row * as_usize(self.tiles_dim[0]);
                    let dirty_tile_row_end = dirty_tile_row_start + as_usize(self.tiles_dim[0]);

                    let tile_row = tile_row as u32;
                    self.dirty_tiles
                        .iter()
                        .enumerate()
                        .skip(dirty_tile_row_start)
                        .take(dirty_tile_row_end)
                        .filter(|(_, mask)| **mask & Self::DIRTY_TILE_MASK != 0)
                        .map(|(idx, _)| idx)
                        .for_each(|tile_idx| {
                            let tile_idx = tile_idx as u32;
                            let tile_y = tile_idx / self.tiles_dim[0];

                            if tile_y != tile_row {
                                return;
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
                        });
                });
        }

        #[cfg(not(feature = "rayon"))]
        {
            for tile_idx in self
                .dirty_tiles
                .iter()
                .enumerate()
                .filter(|(_, mask)| **mask & Self::DIRTY_TILE_MASK != 0)
                .map(|(idx, _)| idx)
            {
                let tile_idx = tile_idx as u32;
                let tile_x = tile_idx % self.tiles_dim[0];
                let tile_y = tile_idx / self.tiles_dim[0];
                let full_height = canvas.height;
                update_canvas_tile(&sorted_prim_cache, canvas, tile_x, tile_y, full_height, 0);
            }
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.render_from_tiledcache.mark(start);
        }
    }

    const DIRTY_TILE_MASK: u8 = 0b00000001;
    const OCCUPIED_TILE_MASK: u8 = 0b000000010;
    fn update_dirty_tiles(&mut self, cached_primitives: &HashMap<u32, TiledCachedPrimitive>) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        self.dirty_tiles
            .resize(as_usize(self.tiles_dim[0] * self.tiles_dim[1]), 0);
        self.dirty_tiles.fill(0);
        for prim in cached_primitives.values() {
            for tile in &prim.occupied_tiles {
                let mask = &mut self.dirty_tiles
                    [tile[0] as usize + tile[1] as usize * self.tiles_dim[0] as usize];
                if !prim.inner.seen_this_frame || prim.inner.rendered_this_frame {
                    *mask |= Self::DIRTY_TILE_MASK;
                }
                *mask |= Self::OCCUPIED_TILE_MASK;
            }
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.update_dirty_tiles.mark(start);
        }
    }

    fn update_dirty_rects(&mut self, cached_primitives: &HashMap<u32, MeshCachedPrimitive>) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();
        if self.mode == SoftwareRenderCaching::MeshTiled {
            self.dirty_rects.set_bboxes(
                cached_primitives
                    .values()
                    .filter(|prim| !prim.inner.seen_this_frame || prim.inner.rendered_this_frame)
                    .map(|prim| prim.rect),
            );
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.update_dirty_rects.mark(start);
        }
    }

    fn update_dirty_rect<P>(&mut self, cached_primitives: &HashMap<u32, P>) -> DirtyRect
    where
        P: Deref<Target = CacheReuse>,
    {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

        let mut dirty_rect = DirtyRect::new_empty();
        for prim in cached_primitives.values() {
            let prim = prim.deref();
            if !prim.seen_this_frame || prim.rendered_this_frame {
                if dirty_rect.is_empty() {
                    dirty_rect = prim.rect;
                } else {
                    dirty_rect = dirty_rect.union(prim.rect)
                }
            }
        }

        #[cfg(feature = "raster_stats")]
        {
            self.stats.update_dirty_rect.mark(start);
        }
        dirty_rect
    }

    fn set_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        #[cfg(feature = "raster_stats")]
        let start = std::time::Instant::now();

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
                                ColorFieldOrder::Rgba => pixels[src_pos].to_array(),
                                ColorFieldOrder::Bgra => {
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

        #[cfg(feature = "raster_stats")]
        {
            self.stats.set_textures.mark(start);
        }
    }

    fn free_textures(&mut self, textures_delta: &egui::TexturesDelta) {
        for free in &textures_delta.free {
            self.textures.remove(free);
        }
    }
}

fn update_canvas_tile(
    sorted_prim_cache: &[&TiledCachedPrimitive],
    canvas: &mut BufferMutRef,
    tile_x: u32,
    tile_y: u32,
    full_height: u32,
    canvas_row_offset: u32,
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
        canvas.data[as_usize(start)..as_usize(end)].fill([0; 4]);
    }

    let tile_n = [tile_x as u16, tile_y as u16];
    // redraw cached prims on tile
    for prim in sorted_prim_cache {
        if !prim.occupied_tiles.contains(&tile_n) {
            continue;
        }

        let mut min_x = prim.inner.rect.min_x;
        let mut min_y = prim.inner.rect.min_y;
        let mut max_x = prim.inner.rect.max_x;
        let mut max_y = prim.inner.rect.max_y;

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
        let prim_x_min = (min_x - prim.inner.rect.min_x).min(prim_buf.width);
        let prim_x_max = (max_x - prim.inner.rect.min_x).min(prim_buf.width);

        let get_ranges = |y: u32| -> (Range<usize>, Range<usize>) {
            let canvas_row_start = (y - canvas_row_offset).min(canvas.height) * canvas.width;
            let canvas_start = canvas_row_start + min_x;
            let canvas_end = canvas_row_start + max_x;

            let prim_y = (y - prim.inner.rect.min_y).min(prim_buf.height);
            let prim_row_start = prim_y * prim_buf.width;
            let prim_start = prim_row_start + prim_x_min;
            let prim_end = prim_row_start + prim_x_max;

            (
                as_usize(canvas_start)..as_usize(canvas_end),
                as_usize(prim_start)..as_usize(prim_end),
            )
        };

        dispatch_simd_impl!(|simd_impl| {
            for y in min_y..max_y {
                let (canvas_slice, prim_slice) = get_ranges(y);
                let src_row = &prim_buf.data[prim_slice];
                let dst_row = &mut canvas.data[canvas_slice];
                simd_impl.egui_blend_u8_slice(src_row, dst_row);
            }
        });
    }
}

enum CacheUpdate<P> {
    CacheReuse(u32, CacheReuse),
    New(u32, P),
    None,
}

struct CacheReuse {
    z_order: u32,
    rect: DirtyRect,
    seen_this_frame: bool,
    rendered_this_frame: bool,
}

struct MeshCachedPrimitive {
    inner: CacheReuse,
    px_mesh: Mesh,
    clip_rect: egui::Rect,
}

impl Deref for MeshCachedPrimitive {
    type Target = CacheReuse;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for MeshCachedPrimitive {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

struct TiledCachedPrimitive {
    inner: CacheReuse,
    buffer: Vec<[u8; 4]>,
    occupied_tiles: Vec<[u16; 2]>,
}
impl Deref for TiledCachedPrimitive {
    type Target = CacheReuse;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TiledCachedPrimitive {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl TiledCachedPrimitive {
    fn get_buffer_ref(&self) -> BufferRef<'_> {
        BufferRef {
            data: &self.buffer,
            width: self.inner.rect.width(),
            height: self.inner.rect.height(),
            width_extent: self.inner.rect.width() - 1,
            height_extent: self.inner.rect.height() - 1,
        }
    }
    fn update_occupied_tiles(&mut self, tiles_wide: u32, tiles_tall: u32) {
        // list which tiles contain a pixel with that isn't fully transparent (also containing not color info)
        self.occupied_tiles.clear();
        let width = self.inner.rect.width();
        let max_x = self.inner.rect.max_x;
        let max_y = self.inner.rect.max_y;
        let first_tile_x = (self.inner.rect.min_x / TILE_SIZE).min(tiles_wide);
        let first_tile_y = (self.inner.rect.min_y / TILE_SIZE).min(tiles_tall);
        let last_tile_x = max_x.div_ceil(TILE_SIZE).min(tiles_wide);
        let last_tile_y = max_y.div_ceil(TILE_SIZE).min(tiles_tall);

        for tile_y in first_tile_y..last_tile_y {
            let mut px_start_y = (tile_y * TILE_SIZE).max(self.inner.rect.min_y);
            let mut px_end_y = (px_start_y + TILE_SIZE).min(max_y);
            px_start_y -= self.inner.rect.min_y;
            px_end_y -= self.inner.rect.min_y;
            for tile_x in first_tile_x..last_tile_x {
                let mut px_start_x = (tile_x * TILE_SIZE).max(self.inner.rect.min_x);
                let mut px_end_x = (px_start_x + TILE_SIZE).min(max_x);
                px_start_x -= self.inner.rect.min_x;
                px_end_x -= self.inner.rect.min_x;

                'px_outer: for y in px_start_y..px_end_y {
                    for x in px_start_x..px_end_x {
                        // Purposefully panicing when out of bounds. If it's out of bounds then the math is wrong and
                        // the tile is not being calculated correctly.
                        let offset = as_usize(x) + as_usize(y) * as_usize(width);
                        if u32::from_le_bytes(self.buffer[offset]) > 0 {
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
    pub width: u32,
    pub height: u32,
    pub width_extent: u32,
    pub height_extent: u32,
}

impl<'a> BufferMutRef<'a> {
    pub fn new(data: &'a mut [[u8; 4]], width: u32, height: u32) -> Self {
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
    pub fn get_range(&self, start: u32, end: u32, y: u32) -> Range<usize> {
        let row_start = y * self.width;
        let start = as_usize(row_start + start);
        let end = as_usize(row_start + end);
        start..end
    }

    #[inline(always)]
    pub fn get_span(&self, start: u32, end: u32, y: u32) -> &[[u8; 4]] {
        let range = self.get_range(start, end, y);
        &self.data[range]
    }

    #[inline(always)]
    pub fn get_mut_span(&mut self, start: u32, end: u32, y: u32) -> &mut [[u8; 4]] {
        let range = self.get_range(start, end, y);
        &mut self.data[range]
    }

    #[inline(always)]
    pub fn get_mut_clamped(&mut self, x: u32, y: u32) -> &mut [u8; 4] {
        let x = x.min(self.width_extent);
        let y = y.min(self.height_extent);
        &mut self.data[as_usize(x) + as_usize(y) * as_usize(self.width)]
    }

    #[inline(always)]
    pub fn get_mut(&mut self, x: u32, y: u32) -> &mut [u8; 4] {
        &mut self.data[as_usize(x) + as_usize(y) * as_usize(self.width)]
    }
}

#[derive(Debug)]
pub struct BufferRef<'a> {
    pub data: &'a [[u8; 4]],
    pub width: u32,
    pub height: u32,
    pub width_extent: u32,
    pub height_extent: u32,
}

/// Lossless cast to usize
/// Prevent compilation on < 32bits platforms
#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
#[inline(always)]
fn as_usize(v: u32) -> usize {
    v as usize
}

impl<'a> BufferRef<'a> {
    #[inline(always)]
    pub fn get_ref_clamped(&self, x: u32, y: u32) -> &[u8; 4] {
        let x = x.min(self.width_extent);
        let y = y.min(self.height_extent);
        &self.data[as_usize(x) + as_usize(y) * as_usize(self.width)]
    }

    #[inline(always)]
    pub fn get_ref(&self, x: u32, y: u32) -> &[u8; 4] {
        &self.data[as_usize(x) + as_usize(y) * as_usize(self.width)]
    }
}

#[allow(dead_code)]
fn draw_rect_border_f32(
    buffer_ref: &mut BufferMutRef,
    rect: egui::Rect,
    border_size: f32,
    color: [u8; 4],
) {
    // Convert float to integer pixel coordinates
    let x0 = rect.min.x.floor().max(0.0) as u32;
    let y0 = rect.min.y.floor().max(0.0) as u32;
    let x1 = (rect.max.x.ceil() as u32).min(buffer_ref.width);
    let y1 = (rect.max.y.ceil() as u32).min(buffer_ref.height);
    let border = border_size.ceil().max(0.0) as u32;

    // Helper closure: set pixel if inside buffer
    let mut set_pixel = |px: u32, py: u32| {
        let idx = as_usize(py * buffer_ref.width + px);
        buffer_ref.data[idx] = color;
    };

    // Top & bottom borders
    for dy in 0..border {
        for px in x0..x1 {
            set_pixel(px, y0 + dy); // top
            set_pixel(px, y1.saturating_sub(1) - dy); // bottom
        }
    }

    // Left & right borders
    for py in border..(y1.saturating_sub(y0).saturating_sub(border)) {
        for dx in 0..border {
            set_pixel(x0 + dx, y0 + py); // left
            set_pixel(x1.saturating_sub(1) - dx, y0 + py); // right
        }
    }
}
