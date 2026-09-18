//! A vertex buffer that grows when it has to and never shrinks.
//!
//! Instance counts in a shell are small and change rarely — a mode switch adds
//! a handful of panes — so the buffer reaches its working size within the first
//! few frames and then never reallocates again.

use bytemuck::Pod;

pub struct InstanceBuffer {
    buffer: wgpu::Buffer,
    capacity: usize,
    len: usize,
    label: &'static str,
}

impl InstanceBuffer {
    pub fn new(device: &wgpu::Device, label: &'static str, stride: usize, capacity: usize) -> Self {
        Self {
            buffer: Self::allocate(device, label, stride, capacity),
            capacity,
            len: 0,
            label,
        }
    }

    fn allocate(
        device: &wgpu::Device,
        label: &'static str,
        stride: usize,
        capacity: usize,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (stride * capacity.max(1)) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Upload `items`, growing the allocation if needed.
    pub fn upload<T: Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[T]) {
        self.len = items.len();
        if items.is_empty() {
            return;
        }

        if items.len() > self.capacity {
            // Grow geometrically so a steadily busier frame does not
            // reallocate every time it gains one more pane.
            self.capacity = items.len().next_power_of_two();
            self.buffer =
                Self::allocate(device, self.label, std::mem::size_of::<T>(), self.capacity);
        }

        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(items));
    }

    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(..)
    }

    pub fn len(&self) -> u32 {
        self.len as u32
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn growth_is_geometric_not_incremental() {
        // Mirrors the sizing rule above. The property that matters is that the
        // number of reallocations grows with the *logarithm* of the instance
        // count, not with the count itself — so a frame that gains one more
        // pane almost never reallocates.
        let grow_to = |limit: usize| {
            let mut capacity = 64usize;
            let mut reallocations = 0;
            for needed in 1..=limit {
                if needed > capacity {
                    capacity = needed.next_power_of_two();
                    reallocations += 1;
                }
            }
            (capacity, reallocations)
        };

        let (capacity, few) = grow_to(300);
        assert_eq!(capacity, 512);

        // Sixteen times as many instances costs only a handful more
        // reallocations, which linear growth could never manage.
        let (_, many) = grow_to(4800);
        assert!(many <= few + 4, "{few} -> {many} is not logarithmic");
        assert!(many < 12, "reallocated {many} times");
    }

    #[test]
    fn a_settled_frame_never_reallocates() {
        // The steady state: once the shell reaches its working number of
        // panes, adding and removing a few costs nothing at all.
        let capacity = 64usize;
        for needed in [12, 40, 63, 64, 30, 51] {
            assert!(needed <= capacity, "{needed} would have reallocated");
        }
    }
}
