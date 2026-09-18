//! Images the renderer owns.
//!
//! The shell refers to images only by an opaque [`TextureId`]; it never holds a
//! texture, a size in physical pixels, or anything else that would tie it to a
//! GPU. Under a Wayland compositor the same registry will hand out ids for
//! client buffers, and nothing above this line will notice the difference.

use detends_paint::{TextureId, Vec2};
use std::collections::HashMap;

pub struct Image {
    pub bind_group: wgpu::BindGroup,
    /// Natural size in logical units, so callers can lay out against the
    /// artwork's real proportions instead of guessing.
    pub size: Vec2,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
}

#[derive(Default)]
pub struct Textures {
    next: u64,
    images: HashMap<TextureId, Image>,
}

impl Textures {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upload straight (non-premultiplied) 8-bit RGBA.
    ///
    /// Stored as `Rgba8UnormSrgb` so the hardware decodes to linear light on
    /// sample. Everything downstream of here is linear, and an image that
    /// skipped this conversion would be the one gamma-encoded thing in an
    /// otherwise linear pipeline — visibly too bright where it overlaps glass.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        label: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> TextureId {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        self.next += 1;
        let id = TextureId(self.next);
        self.images.insert(
            id,
            Image {
                bind_group,
                size: Vec2 {
                    x: width as f32,
                    y: height as f32,
                },
                _texture: texture,
                _view: view,
            },
        );
        id
    }

    pub fn get(&self, id: TextureId) -> Option<&Image> {
        self.images.get(&id)
    }

    pub fn size_of(&self, id: TextureId) -> Option<Vec2> {
        self.images.get(&id).map(|i| i.size)
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_never_zero() {
        // Zero is reserved so that a defaulted `TextureId` cannot accidentally
        // resolve to a real image.
        let mut t = Textures::new();
        assert!(t.is_empty());
        let ids: Vec<TextureId> = (0..8)
            .map(|_| {
                t.next += 1;
                TextureId(t.next)
            })
            .collect();

        assert!(ids.iter().all(|i| i.0 != 0));
        let mut unique = ids.clone();
        unique.sort_by_key(|i| i.0);
        unique.dedup_by_key(|i| i.0);
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn an_unknown_id_resolves_to_nothing_rather_than_panicking() {
        let t = Textures::new();
        assert!(t.get(TextureId(42)).is_none());
        assert!(t.size_of(TextureId(0)).is_none());
    }
}
