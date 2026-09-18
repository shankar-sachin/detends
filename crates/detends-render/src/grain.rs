//! The blue-noise tile.
//
//! Used twice: a whisper of grain inside the glass, and the dither that breaks
//! up 8-bit quantisation in the final present pass.
//!
//! It has to be *blue* noise — noise whose energy sits in high spatial
//! frequencies. White noise of the same amplitude is equally effective at
//! removing banding and looks dramatically worse, because the eye is far more
//! sensitive to the low-frequency clumps white noise contains. Blue noise
//! reads as fine texture; white noise reads as dirt.

/// Tile edge length. Large enough not to show its own repeat, small enough to
/// stay in cache.
pub const TILE: usize = 64;

/// Generate a blue-noise tile by energy minimisation.
///
/// Start from white noise, then repeatedly swap pairs of pixels whenever the
/// swap lowers a Gaussian-weighted "clumpiness" energy. Similar values repel
/// each other, so the low-frequency content drains away and what remains is
/// high-frequency — which is the definition of blue noise.
///
/// Deterministic from a fixed seed, so the pattern is identical on every
/// machine and every run. A dither pattern that changed between builds would
/// make screenshots impossible to compare.
pub fn tile() -> Vec<u8> {
    const N: usize = TILE;
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);

    let mut values: Vec<f32> = (0..N * N).map(|_| rng.next_f32()).collect();

    // Energy falls off over a few pixels; beyond that, pixels do not interact.
    const SIGMA: f32 = 1.9;
    const RADIUS: i32 = 4;
    let mut kernel = [[0.0f32; (2 * RADIUS + 1) as usize]; (2 * RADIUS + 1) as usize];
    for (dy, row) in kernel.iter_mut().enumerate() {
        for (dx, k) in row.iter_mut().enumerate() {
            let x = dx as f32 - RADIUS as f32;
            let y = dy as f32 - RADIUS as f32;
            *k = (-(x * x + y * y) / (2.0 * SIGMA * SIGMA)).exp();
        }
    }

    // Energy contributed by one pixel against its neighbourhood.
    let local_energy = |values: &[f32], index: usize| -> f32 {
        let (x, y) = ((index % N) as i32, (index / N) as i32);
        let mut sum = 0.0;
        for dy in -RADIUS..=RADIUS {
            for dx in -RADIUS..=RADIUS {
                if dx == 0 && dy == 0 {
                    continue;
                }
                // Wrap, so the tile is seamless when repeated.
                let nx = (x + dx).rem_euclid(N as i32) as usize;
                let ny = (y + dy).rem_euclid(N as i32) as usize;
                let weight = kernel[(dy + RADIUS) as usize][(dx + RADIUS) as usize];
                // Close values in close pixels is exactly what we penalise.
                let similarity = 1.0 - (values[index] - values[ny * N + nx]).abs();
                sum += weight * similarity * similarity;
            }
        }
        sum
    };

    // Enough passes to converge without making startup noticeable.
    for _ in 0..N * N * 12 {
        let a = rng.next_usize(N * N);
        let b = rng.next_usize(N * N);
        if a == b {
            continue;
        }

        let before = local_energy(&values, a) + local_energy(&values, b);
        values.swap(a, b);
        let after = local_energy(&values, a) + local_energy(&values, b);

        if after > before {
            values.swap(a, b); // no improvement, put them back
        }
    }

    values
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

/// Upload the tile as an R8 texture.
pub fn upload(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Texture, wgpu::TextureView) {
    let data = tile();
    let size = wgpu::Extent3d {
        width: TILE as u32,
        height: TILE as u32,
        depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blue-noise"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
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
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(TILE as u32),
            rows_per_image: Some(TILE as u32),
        },
        size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// A small deterministic PRNG. Nothing here is cryptographic; it only needs to
/// be reproducible across platforms, which `std`'s hasher is not.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_is_the_right_shape_and_uses_the_full_range() {
        let t = tile();
        assert_eq!(t.len(), TILE * TILE);
        let min = *t.iter().min().unwrap();
        let max = *t.iter().max().unwrap();
        assert!(min < 16 && max > 239, "range {min}..{max} is too narrow");
    }

    #[test]
    fn it_is_generated_deterministically() {
        // Same pattern on every machine and every build, so screenshots stay
        // comparable and the grain never shifts under a rebuild.
        assert_eq!(tile(), tile());
    }

    #[test]
    fn the_distribution_is_flat() {
        // A dither pattern must be uniform, or it biases the image it corrects.
        let t = tile();
        let mut buckets = [0usize; 8];
        for v in &t {
            buckets[(*v as usize * 8) / 256] += 1;
        }
        let expected = (TILE * TILE) / 8;
        for (i, count) in buckets.iter().enumerate() {
            let drift = (*count as f32 - expected as f32).abs() / expected as f32;
            assert!(
                drift < 0.3,
                "bucket {i} holds {count}, expected ~{expected}"
            );
        }
    }

    #[test]
    fn it_is_actually_blue_not_white() {
        // The property that matters. Measured as average difference between
        // neighbours: blue noise varies sharply pixel to pixel, white noise
        // contains smooth low-frequency clumps and so varies less.
        let blue = tile();

        let mut rng = Rng::new(12345);
        let white: Vec<u8> = (0..TILE * TILE)
            .map(|_| (rng.next_f32() * 255.0) as u8)
            .collect();

        let neighbour_delta = |t: &[u8]| -> f32 {
            let mut sum = 0.0;
            for y in 0..TILE {
                for x in 0..TILE {
                    let here = t[y * TILE + x] as f32;
                    let right = t[y * TILE + (x + 1) % TILE] as f32;
                    let below = t[((y + 1) % TILE) * TILE + x] as f32;
                    sum += (here - right).abs() + (here - below).abs();
                }
            }
            sum / (TILE * TILE * 2) as f32
        };

        let blue_delta = neighbour_delta(&blue);
        let white_delta = neighbour_delta(&white);
        assert!(
            blue_delta > white_delta * 1.08,
            "not blue enough: {blue_delta} vs white {white_delta}"
        );
    }

    #[test]
    fn the_tile_wraps_seamlessly() {
        // Energy is computed with wrapping, so the seam should be no more
        // contrasty than the interior — otherwise the repeat would be visible
        // as a grid across the screen.
        let t = tile();
        let edge: f32 = (0..TILE)
            .map(|y| (t[y * TILE] as f32 - t[y * TILE + TILE - 1] as f32).abs())
            .sum::<f32>()
            / TILE as f32;
        let interior: f32 = (0..TILE)
            .map(|y| (t[y * TILE + 10] as f32 - t[y * TILE + 11] as f32).abs())
            .sum::<f32>()
            / TILE as f32;
        assert!(edge < interior * 1.35, "seam {edge} vs interior {interior}");
    }
}
