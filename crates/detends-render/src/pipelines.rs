//! Pipeline, layout and bind-group construction.
//!
//! All of it happens at startup and on resize, never per frame. Building a
//! bind group costs a few microseconds — trivial once, and the easiest way to
//! lose a 120Hz budget if done for every pyramid level of every frame.

use crate::gpu::WORKING_FORMAT;
use crate::instance::*;
use crate::renderer::{Bindings, Layouts, Pipelines, Uniforms};
use crate::targets::Targets;

/// Widens the blur without adding a pyramid level.
///
/// Tuning radius this way rather than by depth keeps the result temporally
/// stable: a non-integer scale changes the kernel smoothly, where another level
/// changes the sampling structure and makes the blur visibly "swim" when
/// content moves behind it.
const OFFSET_SCALE: f32 = 1.35;

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let make = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };

        Self {
            env: make("env", &[uniform_entry(0)]),
            blur: make(
                "blur",
                &[uniform_entry(0), texture_entry(1), sampler_entry(2)],
            ),
            blit: make("blit", &[texture_entry(0), sampler_entry(1)]),
            glass: make(
                "glass",
                &[
                    uniform_entry(0),
                    texture_entry(1), // backdrop, sharp
                    texture_entry(2), // backdrop, blurred
                    sampler_entry(3),
                    texture_entry(4), // grain
                ],
            ),
            flat: make(
                "flat",
                &[uniform_entry(0), texture_entry(1), sampler_entry(2)],
            ),
            present: make(
                "present",
                &[
                    uniform_entry(0),
                    texture_entry(1),
                    sampler_entry(2),
                    texture_entry(3),
                ],
            ),
        }
    }
}

/// Premultiplied-alpha over. Matches what the flat shader emits and what
/// glyphon requires, so everything composites consistently.
fn premultiplied() -> Option<wgpu::BlendState> {
    Some(wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    buffers: &[Option<wgpu::VertexBufferLayout>],
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // No culling: fullscreen triangles and instanced quads are emitted
            // in a fixed order and never seen from behind.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        // No MSAA anywhere. Glass edges are analytically antialiased from an
        // exact SDF and glyphs from atlas coverage, so multisampling would be
        // four times the bandwidth for no visible change.
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn module(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        layouts: &Layouts,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let env_mod = module(device, "env", include_str!("shaders/env.wgsl"));
        let blur_mod = module(device, "blur", include_str!("shaders/blur.wgsl"));
        let blit_mod = module(device, "blit", include_str!("shaders/blit.wgsl"));
        let glass_mod = module(device, "glass", include_str!("shaders/glass.wgsl"));
        let flat_mod = module(device, "flat", include_str!("shaders/flat.wgsl"));
        let present_mod = module(device, "present", include_str!("shaders/present.wgsl"));

        let glass_buffer = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlassInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &GlassInstance::LAYOUT,
        };
        let flat_buffer = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<FlatInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &FlatInstance::LAYOUT,
        };

        Self {
            env: pipeline(
                device,
                "env",
                &layouts.env,
                &env_mod,
                "vs_main",
                "fs_main",
                WORKING_FORMAT,
                None,
                &[],
            ),
            blur_down: pipeline(
                device,
                "blur-down",
                &layouts.blur,
                &blur_mod,
                "vs_main",
                "downsample",
                WORKING_FORMAT,
                None,
                &[],
            ),
            blur_up: pipeline(
                device,
                "blur-up",
                &layouts.blur,
                &blur_mod,
                "vs_main",
                "upsample",
                WORKING_FORMAT,
                None,
                &[],
            ),
            blit: pipeline(
                device,
                "blit",
                &layouts.blit,
                &blit_mod,
                "vs_main",
                "fs_main",
                WORKING_FORMAT,
                None,
                &[],
            ),
            glass: pipeline(
                device,
                "glass",
                &layouts.glass,
                &glass_mod,
                "vs_main",
                "fs_main",
                WORKING_FORMAT,
                premultiplied(),
                &[Some(glass_buffer)],
            ),
            flat: pipeline(
                device,
                "flat",
                &layouts.flat,
                &flat_mod,
                "vs_main",
                "fs_main",
                WORKING_FORMAT,
                premultiplied(),
                &[Some(flat_buffer)],
            ),
            present: pipeline(
                device,
                "present",
                &layouts.present,
                &present_mod,
                "vs_main",
                "fs_main",
                surface_format,
                None,
                &[],
            ),
            present_capture: pipeline(
                device,
                "present-capture",
                &layouts.present,
                &present_mod,
                "vs_main",
                "fs_main",
                wgpu::TextureFormat::Rgba8Unorm,
                None,
                &[],
            ),
        }
    }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Uniforms {
    pub fn new(device: &wgpu::Device, targets: &Targets) -> Self {
        // One blur uniform per distinct source size. Index 0 is full
        // resolution, used by whichever texture a layer refracts; the rest
        // follow the pyramid down and back up.
        let mut sizes = vec![[1.0 / targets.width as f32, 1.0 / targets.height as f32]];
        for level in &targets.down {
            sizes.push(level.inv_size());
        }
        for level in targets.up.iter().take(targets.up.len().saturating_sub(1)) {
            sizes.push(level.inv_size());
        }

        let blur = sizes
            .iter()
            .map(|_| {
                uniform_buffer(
                    device,
                    "blur-level",
                    std::mem::size_of::<BlurLevel>() as u64,
                )
            })
            .collect::<Vec<_>>();

        Self {
            env: uniform_buffer(device, "env", std::mem::size_of::<EnvUniforms>() as u64),
            glass: uniform_buffer(device, "glass", std::mem::size_of::<GlassUniforms>() as u64),
            flat: uniform_buffer(device, "flat", std::mem::size_of::<FlatUniforms>() as u64),
            present: uniform_buffer(
                device,
                "present",
                std::mem::size_of::<PresentUniforms>() as u64,
            ),
            blur,
            blur_sizes: sizes,
        }
    }

    /// Blur offsets never change between resizes, so they are written once here
    /// rather than every frame.
    pub fn write_blur(&self, queue: &wgpu::Queue) {
        for (buffer, inv_source) in self.blur.iter().zip(&self.blur_sizes) {
            queue.write_buffer(
                buffer,
                0,
                bytemuck::bytes_of(&BlurLevel {
                    inv_source: *inv_source,
                    scale: OFFSET_SCALE,
                    _pad: 0.0,
                }),
            );
        }
    }
}

/// A 1×1 opaque white texture, bound wherever the flat pipeline is drawing a
/// plain fill rather than an image. Simpler than a second pipeline.
fn white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
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
        &[255, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl Bindings {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: &Layouts,
        targets: &Targets,
        uniforms: &Uniforms,
    ) -> Self {
        uniforms.write_blur(queue);

        let sampler = crate::gpu::linear_sampler_raw(device);
        let (grain_texture, grain_view) = crate::grain::upload(device, queue);
        let (white_texture, white_view) = white_texture(device, queue);

        // The three textures a layer might refract: the environment, then
        // whichever composite the previous layer wrote.
        let sources = [
            &targets.env.view,
            &targets.composite[0].view,
            &targets.composite[1].view,
        ];

        let env = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("env"),
            layout: &layouts.env,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.env.as_entire_binding(),
            }],
        });

        let blur_binding = |uniform: &wgpu::Buffer, texture: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur"),
                layout: &layouts.blur,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(texture),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };

        let blur_first = sources
            .iter()
            .map(|view| blur_binding(&uniforms.blur[0], view))
            .collect();

        // The rest of the chain: each level reads the previous one.
        let mut blur_rest = Vec::new();
        for level in 1..targets.down.len() {
            blur_rest.push(blur_binding(
                &uniforms.blur[level],
                &targets.down[level - 1].view,
            ));
        }
        let deepest = targets.down.len();
        for level in 0..targets.up.len() {
            let source = if level == 0 {
                &targets.down[deepest - 1].view
            } else {
                &targets.up[level - 1].view
            };
            blur_rest.push(blur_binding(&uniforms.blur[deepest + level], source));
        }

        let blit = sources
            .iter()
            .map(|view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("blit"),
                    layout: &layouts.blit,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            })
            .collect();

        let blurred = &targets.blurred().view;
        let glass = sources
            .iter()
            .map(|view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("glass"),
                    layout: &layouts.glass,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniforms.glass.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(blurred),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&grain_view),
                        },
                    ],
                })
            })
            .collect();

        let flat = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flat"),
            layout: &layouts.flat,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.flat.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&white_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let present = targets
            .composite
            .iter()
            .map(|composite| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("present"),
                    layout: &layouts.present,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniforms.present.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&composite.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&grain_view),
                        },
                    ],
                })
            })
            .collect();

        Self {
            env,
            blur_first,
            blur_rest,
            blit,
            glass,
            flat,
            present,
            _grain: grain_texture,
            _white: white_texture,
            _sampler: sampler,
        }
    }
}
