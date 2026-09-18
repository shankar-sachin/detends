//! Offscreen render targets and the blur pyramid.

use crate::gpu::WORKING_FORMAT;

/// A texture plus the view and size that always travel with it.
pub struct Target {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl Target {
    fn new(device: &wgpu::Device, label: &str, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: WORKING_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// 1 / size, which is what every blur offset is derived from.
    pub fn inv_size(&self) -> [f32; 2] {
        [1.0 / self.width as f32, 1.0 / self.height as f32]
    }
}

/// How many halvings the pyramid descends.
///
/// Three rather than four. Four reaches a wider radius, but the blur starts to
/// "swim" when content moves behind it and fine text ghosts across frames.
/// Radius is tuned with the offset scale instead, which stays stable because it
/// does not change the sampling structure.
pub const PYRAMID_DEPTH: usize = 3;

/// Every offscreen surface a frame needs.
///
/// Each pyramid level is its own texture rather than a mip of a shared one:
/// wgpu forbids sampling a texture that is simultaneously a render attachment,
/// and separate textures sidestep the whole view-and-usage argument for a
/// memory cost of about a third of one frame.
pub struct Targets {
    pub width: u32,
    pub height: u32,
    /// The environment, before any glass.
    pub env: Target,
    /// Ping-pong pair for compositing glass layer by layer.
    pub composite: [Target; 2],
    /// Descending half-resolution steps.
    pub down: Vec<Target>,
    /// Ascending steps back up. The last is the blurred result, at half
    /// resolution — plenty, since it is by definition smooth.
    pub up: Vec<Target>,
}

impl Targets {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);

        let mut down = Vec::with_capacity(PYRAMID_DEPTH);
        let (mut w, mut h) = (width, height);
        for level in 0..PYRAMID_DEPTH {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            down.push(Target::new(device, &format!("blur-down-{level}"), w, h));
        }

        let mut up = Vec::with_capacity(PYRAMID_DEPTH - 1);
        for level in (0..PYRAMID_DEPTH - 1).rev() {
            let source = &down[level];
            up.push(Target::new(
                device,
                &format!("blur-up-{level}"),
                source.width,
                source.height,
            ));
        }

        Self {
            width,
            height,
            env: Target::new(device, "env", width, height),
            composite: [
                Target::new(device, "composite-a", width, height),
                Target::new(device, "composite-b", width, height),
            ],
            down,
            up,
        }
    }

    /// The blurred backdrop the glass shader samples.
    pub fn blurred(&self) -> &Target {
        self.up
            .last()
            .expect("pyramid has at least one upsample level")
    }

    pub fn matches(&self, width: u32, height: u32) -> bool {
        self.width == width.max(1) && self.height == height.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pyramid_descends_then_returns() {
        // Pure arithmetic check of the level sizes, no GPU needed.
        let (w, h) = (1512u32, 982u32);
        let mut sizes = vec![];
        let (mut cw, mut ch) = (w, h);
        for _ in 0..PYRAMID_DEPTH {
            cw = (cw / 2).max(1);
            ch = (ch / 2).max(1);
            sizes.push((cw, ch));
        }
        assert_eq!(sizes.len(), 3);
        assert_eq!(sizes[0], (756, 491));
        assert_eq!(sizes[2], (189, 122));

        // Upsampling returns through the same sizes, ending at half res.
        let up: Vec<_> = (0..PYRAMID_DEPTH - 1).rev().map(|l| sizes[l]).collect();
        assert_eq!(up.last().copied(), Some((756, 491)));
    }

    #[test]
    fn odd_and_tiny_sizes_never_collapse_to_zero() {
        let (mut w, mut h) = (3u32, 1u32);
        for _ in 0..PYRAMID_DEPTH {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            assert!(w >= 1 && h >= 1);
        }
    }
}
