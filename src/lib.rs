#![no_std]
extern crate alloc;

use core::ops::Range;

use alloc::borrow::Cow;
use egui::{Color32, Mesh, Vec2, ahash::HashMap, vec2};

use crate::{
    color::swizzle_rgba_bgra,
    egui_texture::EguiTexture,
    render::{draw_egui_mesh, egui_orient2df},
};

pub(crate) mod color;
pub(crate) mod egui_texture;
pub(crate) mod math;
pub(crate) mod raster;
pub(crate) mod render;
#[cfg(feature = "test_render")]
pub mod test_render;

#[derive(Copy, Clone, Default)]
pub enum ColorFieldOrder {
    #[default]
    RGBA,
    BGRA,
}

pub struct EguiSoftwareRender {
    textures: HashMap<egui::TextureId, EguiTexture>,
    target_size: Vec2,
    output_field_order: ColorFieldOrder,
    allow_raster_opt: bool,
}

impl EguiSoftwareRender {
    /// # Arguments
    /// * `output_field_order` - egui textures and vertex colors will be swizzled before rendering to match the desired
    ///   output buffer order.
    pub fn new(output_field_order: ColorFieldOrder) -> Self {
        EguiSoftwareRender {
            textures: Default::default(),
            target_size: Default::default(),
            output_field_order,
            allow_raster_opt: true,
        }
    }

    /// If false: Rasterize everything with triangles, always calculate vertex colors & uvs
    pub fn with_allow_raster_opt(mut self, set: bool) -> Self {
        self.allow_raster_opt = set;
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
        self.set_textures(textures_delta);

        self.target_size = vec2(buffer_ref.width as f32, buffer_ref.height as f32);

        for egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } in paint_jobs.iter()
        {
            let input_mesh = match primitive {
                egui::epaint::Primitive::Mesh(input_mesh) => input_mesh,
                egui::epaint::Primitive::Callback(_) => {
                    // eprintln!("egui::epaint::Primitive::Callback(PaintCallback) not supported");
                    continue;
                }
            };

            if input_mesh.vertices.is_empty() || input_mesh.indices.is_empty() {
                continue;
            }

            let clip_rect = egui::Rect {
                min: clip_rect.min * pixels_per_point,
                max: clip_rect.max * pixels_per_point,
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
                    buffer_ref,
                    &clip_rect,
                    &px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
                );
            } else {
                draw_egui_mesh::<8>(
                    &self.textures,
                    buffer_ref,
                    &clip_rect,
                    &px_mesh,
                    Vec2::ZERO,
                    self.allow_raster_opt,
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
                let new_texture = EguiTexture::new(self.output_field_order, size, &pixels);
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

#[derive(Debug)]
pub struct BufferMutRef<'a> {
    pub data: &'a mut [[u8; 4]],
    pub width: usize,
    pub height: usize,
}

impl<'a> BufferMutRef<'a> {
    pub fn new(data: &'a mut [[u8; 4]], width: usize, height: usize) -> Self {
        assert!(width > 0);
        assert!(height > 0);
        BufferMutRef {
            data,
            width,
            height,
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
        let x = x.min(self.width - 1);
        let y = y.min(self.height - 1);
        &mut self.data[x + y * self.width]
    }

    #[inline(always)]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut [u8; 4] {
        &mut self.data[x + y * self.width]
    }
}
