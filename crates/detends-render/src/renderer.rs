//! Assembling a frame.
//!
//! The pass structure, in order:
//!
//! ```text
//! A  environment   -> env            the wallpaper, plus anything behind glass
//! B  blur pyramid  -> blurred        3 down, 2 up, ending at half resolution
//! C₀ layer 0       -> composite[0]   blit env, then its glass and fills
//! B′ re-blur       -> blurred        because layer 1 refracts layer 0's result
//! C₁ layer 1       -> composite[1]
//! B″ re-blur       -> blurred
//! C₂ layer 2       -> composite[0]   ping-ponging back
//! E  present       -> surface        encode to sRGB, dither, done
//! ```
//!
//! The re-blur between layers is what makes overlapping glass correct. A single
//! pass cannot do it: every pane would sample the same untouched backdrop, so a
//! panel over another panel would refract the wallpaper rather than the panel
//! beneath it, and the lower pane's rim would shine straight through.

use crate::buffers::InstanceBuffer;
use crate::gpu::{Gpu, GpuError, OutputMode, WORKING_FORMAT};
use crate::instance::*;
use crate::targets::Targets;
use crate::text::TextStack;
use crate::textures::Textures;
use detends_paint::{Color, Frame, Layer, Primitive, TextureId, Vec2};

/// Everything about the environment that the shell decides.
#[derive(Clone, Copy, Debug)]
pub struct Environment {
    pub time: f32,
    /// 0 normally, rising to 1 in Focus. The field contracts, so the periphery
    /// dims and attention narrows without anything being announced (§12).
    pub focus: f32,
    pub near: Color,
    pub far: Color,
    /// The two lights the field is lit by. Alpha is strength, not opacity.
    pub glow_warm: Color,
    pub glow_cool: Color,
    pub glass_intensity: f32,
    pub glass_transparency: f32,
    /// How much of the environment has arrived, 0 to 1.
    ///
    /// Used during boot, where the workspace rises behind the mark. Distinct
    /// from `fade`, which takes the mark with it.
    pub presence: f32,
    /// Fades everything to black, for sleep and shutdown (§19).
    pub fade: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            time: 0.0,
            focus: 0.0,
            near: Color::hex(0x1A1F26),
            far: Color::hex(0x0B0D10),
            glow_warm: Color::hex(0x3A2A24).alpha(0.30),
            glow_cool: Color::hex(0x24324A).alpha(0.55),
            glass_intensity: 1.0,
            glass_transparency: 1.0,
            presence: 1.0,
            fade: 1.0,
        }
    }
}

pub struct Renderer {
    gpu: Gpu,
    targets: Targets,
    bindings: Bindings,
    pipelines: Pipelines,
    uniforms: Uniforms,
    glass_instances: InstanceBuffer,
    flat_instances: InstanceBuffer,
    icon_instances: InstanceBuffer,
    text: TextStack,
    textures: Textures,
    // Scratch, reused every frame so steady-state rendering allocates nothing.
    glass_scratch: Vec<GlassInstance>,
    flat_scratch: Vec<FlatInstance>,
    icon_scratch: Vec<IconInstance>,
    /// One entry per layer, describing which slice of the instance buffers
    /// that layer draws.
    batches: Vec<LayerBatch>,
    /// Kept so images loaded after startup can build their bind groups.
    flat_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

pub(crate) struct Pipelines {
    pub(crate) env: wgpu::RenderPipeline,
    pub(crate) blur_down: wgpu::RenderPipeline,
    pub(crate) blur_up: wgpu::RenderPipeline,
    pub(crate) blit: wgpu::RenderPipeline,
    pub(crate) glass: wgpu::RenderPipeline,
    pub(crate) flat: wgpu::RenderPipeline,
    pub(crate) icon: wgpu::RenderPipeline,
    pub(crate) present: wgpu::RenderPipeline,
    /// The same present pass, targeting an 8-bit offscreen texture.
    pub(crate) present_capture: wgpu::RenderPipeline,
}

pub(crate) struct Layouts {
    pub(crate) env: wgpu::BindGroupLayout,
    pub(crate) blur: wgpu::BindGroupLayout,
    pub(crate) blit: wgpu::BindGroupLayout,
    pub(crate) glass: wgpu::BindGroupLayout,
    pub(crate) flat: wgpu::BindGroupLayout,
    pub(crate) icon: wgpu::BindGroupLayout,
    pub(crate) present: wgpu::BindGroupLayout,
}

pub(crate) struct Uniforms {
    pub(crate) env: wgpu::Buffer,
    pub(crate) glass: wgpu::Buffer,
    pub(crate) flat: wgpu::Buffer,
    pub(crate) present: wgpu::Buffer,
    /// One per pyramid step, written once at resize.
    pub(crate) blur: Vec<wgpu::Buffer>,
    /// The source size each blur level samples from.
    pub(crate) blur_sizes: Vec<[f32; 2]>,
}

/// Bind groups, all built at resize and never per frame.
///
/// Creating a bind group costs a couple of microseconds; doing it for every
/// pyramid level every frame is the easiest way to lose the frame budget in
/// this pipeline, and it is entirely avoidable because nothing in them changes
/// between resizes.
pub(crate) struct Bindings {
    pub(crate) env: wgpu::BindGroup,
    /// First downsample, one per texture that might be the blur's input.
    pub(crate) blur_first: Vec<wgpu::BindGroup>,
    /// The rest of the chain, whose sources never vary.
    pub(crate) blur_rest: Vec<wgpu::BindGroup>,
    /// Blit source, indexed the same way as `blur_first`.
    pub(crate) blit: Vec<wgpu::BindGroup>,
    /// Glass backdrop, indexed the same way.
    pub(crate) glass: Vec<wgpu::BindGroup>,
    pub(crate) flat: wgpu::BindGroup,
    pub(crate) icon: wgpu::BindGroup,
    /// Present source, one per composite.
    pub(crate) present: Vec<wgpu::BindGroup>,
    pub(crate) _grain: wgpu::Texture,
    pub(crate) _white: wgpu::Texture,
    pub(crate) _sampler: wgpu::Sampler,
}

/// Which slice of the frame's instance buffers one layer draws.
///
/// Instances for *every* layer are gathered and uploaded before any pass is
/// recorded, because `Queue::write_buffer` does not interleave with command
/// encoding: all writes land before the command buffer runs. Uploading per
/// layer would therefore leave every pass reading whichever layer happened to
/// write last — content glass rendered with an overlay's parameters, which
/// looks like a mysterious dimming rather than like the corruption it is.
#[derive(Default, Clone)]
struct LayerBatch {
    glass: (u32, u32),
    icon: (u32, u32),
    /// Contiguous runs sharing one texture, so each becomes a single draw.
    flat: Vec<(Option<TextureId>, u32, u32)>,
}

/// Which texture a layer reads as its backdrop.
///
/// Layer 0 refracts the environment; each subsequent layer refracts whatever
/// the previous one produced.
fn backdrop_index(layer_index: usize) -> usize {
    match layer_index {
        0 => 0,               // env
        n => 1 + (n - 1) % 2, // composite[0], composite[1], alternating
    }
}

/// Which composite a layer writes into.
fn target_index(layer_index: usize) -> usize {
    layer_index % 2
}

impl Renderer {
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let gpu = Gpu::new(target, width, height).await?;
        let (w, h) = gpu.size();

        let targets = Targets::new(&gpu.device, w, h);
        let layouts = Layouts::new(&gpu.device);
        let pipelines = Pipelines::new(&gpu.device, &layouts, gpu.config.format);
        let uniforms = Uniforms::new(&gpu.device, &targets);
        let bindings = Bindings::new(&gpu.device, &gpu.queue, &layouts, &targets, &uniforms);
        let sampler = crate::gpu::linear_sampler_raw(&gpu.device);

        let text = TextStack::new(&gpu.device, &gpu.queue, WORKING_FORMAT);

        Ok(Self {
            glass_instances: InstanceBuffer::new(
                &gpu.device,
                "glass-instances",
                std::mem::size_of::<GlassInstance>(),
                64,
            ),
            flat_instances: InstanceBuffer::new(
                &gpu.device,
                "flat-instances",
                std::mem::size_of::<FlatInstance>(),
                64,
            ),
            icon_instances: InstanceBuffer::new(
                &gpu.device,
                "icon-instances",
                std::mem::size_of::<IconInstance>(),
                32,
            ),
            gpu,
            targets,
            bindings,
            pipelines,
            uniforms,
            text,
            textures: Textures::new(),
            glass_scratch: Vec::with_capacity(64),
            flat_scratch: Vec::with_capacity(64),
            icon_scratch: Vec::with_capacity(32),
            batches: vec![LayerBatch::default(); Layer::ALL.len()],
            flat_layout: layouts.flat,
            sampler,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.targets.matches(width, height) || width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        let (w, h) = self.gpu.size();

        self.targets = Targets::new(&self.gpu.device, w, h);
        let layouts = Layouts::new(&self.gpu.device);
        self.uniforms = Uniforms::new(&self.gpu.device, &self.targets);
        self.bindings = Bindings::new(
            &self.gpu.device,
            &self.gpu.queue,
            &layouts,
            &self.targets,
            &self.uniforms,
        );
    }

    pub fn size(&self) -> (u32, u32) {
        self.gpu.size()
    }

    /// Upload straight 8-bit RGBA and get back a handle the shell can use.
    pub fn load_image(&mut self, label: &str, rgba: &[u8], width: u32, height: u32) -> TextureId {
        self.textures.load(
            &self.gpu.device,
            &self.gpu.queue,
            &self.flat_layout,
            &self.uniforms.flat,
            &self.sampler,
            label,
            rgba,
            width,
            height,
        )
    }

    /// Natural size of a loaded image, in pixels.
    pub fn image_size(&self, id: TextureId) -> Option<Vec2> {
        self.textures.size_of(id)
    }

    pub fn draw(&mut self, frame: &Frame, env: Environment) {
        let Some(surface_texture) = self.gpu.acquire() else {
            return;
        };

        let (pw, ph) = self.gpu.size();
        self.write_uniforms(frame, &env, pw, ph);

        if let Err(e) = self
            .text
            .prepare(&self.gpu.device, &self.gpu.queue, frame, (pw, ph))
        {
            log::warn!("text preparation failed: {e:?}");
        }

        // Gather and upload every layer's instances before recording anything.
        self.gather(frame);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // A: the environment.
        self.pass_environment(&mut encoder);

        // Everything below draws into `env` first, so layer 0's blur sees it.
        let mut last_written = 0usize;

        for (index, layer) in Layer::ALL.iter().enumerate() {
            let source = backdrop_index(index);
            let target = target_index(index);

            // B: blur whatever this layer will refract — but only if anything
            // in it actually refracts.
            //
            // The blurred texture is read by exactly one pipeline: glass. A
            // layer with no glass in it composites without ever sampling the
            // result, so running the pyramid for it is a full set of
            // downsample and upsample passes at display resolution whose
            // output is then discarded.
            //
            // Three layers meant three pyramids every frame regardless. In
            // practice at most one layer usually holds glass — the environment
            // never does, and the overlay only while System Center or Search
            // is open — so this is most of the frame's GPU time going nowhere.
            let (glass_start, glass_end) = self.batches[index].glass;
            if glass_end > glass_start {
                self.pass_blur(&mut encoder, source);
            }

            // C: composite the layer.
            self.pass_layer(&mut encoder, frame, *layer, source, target);
            last_written = target;
        }

        // E: leave linear light, once.
        self.pass_present(&mut encoder, &surface_texture, last_written);

        self.gpu.queue.submit(Some(encoder.finish()));
        self.gpu.queue.present(surface_texture);
        self.text.trim();
    }

    fn write_uniforms(&self, frame: &Frame, env: &Environment, pw: u32, ph: u32) {
        let q = &self.gpu.queue;
        let logical = [frame.size.x.max(1.0), frame.size.y.max(1.0)];

        q.write_buffer(
            &self.uniforms.env,
            0,
            bytemuck::bytes_of(&EnvUniforms {
                resolution: [pw as f32, ph as f32],
                time: env.time,
                focus: env.focus,
                near: color_array(env.near),
                far: color_array(env.far),
                glow_warm: color_array(env.glow_warm),
                glow_cool: color_array(env.glow_cool),
                presence: env.presence,
                _pad: [0.0; 3],
            }),
        );

        q.write_buffer(
            &self.uniforms.glass,
            0,
            bytemuck::bytes_of(&GlassUniforms {
                resolution: logical,
                scale: frame.scale_factor,
                time: env.time,
                intensity: env.glass_intensity,
                transparency: env.glass_transparency,
                _pad: [0.0; 2],
            }),
        );

        q.write_buffer(
            &self.uniforms.flat,
            0,
            bytemuck::bytes_of(&FlatUniforms {
                resolution: logical,
                scale: frame.scale_factor,
                time: env.time,
            }),
        );

        q.write_buffer(
            &self.uniforms.present,
            0,
            bytemuck::bytes_of(&PresentUniforms {
                encode: match self.gpu.output {
                    OutputMode::SrgbDithered => 1.0,
                    OutputMode::ExtendedLinear => 0.0,
                },
                fade: env.fade,
                _pad: [0.0; 2],
            }),
        );
    }

    fn pass_environment(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("environment"),
            color_attachments: &[Some(attachment(&self.targets.env.view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines.env);
        pass.set_bind_group(0, &self.bindings.env, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Run the pyramid down and back up over `source`.
    fn pass_blur(&self, encoder: &mut wgpu::CommandEncoder, source: usize) {
        let depth = self.targets.down.len();

        // First downsample: the only step whose input varies.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur-down-0"),
                color_attachments: &[Some(attachment(&self.targets.down[0].view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.blur_down);
            pass.set_bind_group(0, &self.bindings.blur_first[source], &[]);
            pass.draw(0..3, 0..1);
        }

        let mut binding = 0;
        for level in 1..depth {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur-down"),
                color_attachments: &[Some(attachment(&self.targets.down[level].view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.blur_down);
            pass.set_bind_group(0, &self.bindings.blur_rest[binding], &[]);
            pass.draw(0..3, 0..1);
            binding += 1;
        }

        for level in 0..self.targets.up.len() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur-up"),
                color_attachments: &[Some(attachment(&self.targets.up[level].view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.blur_up);
            pass.set_bind_group(0, &self.bindings.blur_rest[binding], &[]);
            pass.draw(0..3, 0..1);
            binding += 1;
        }
    }

    /// Collect every layer's instances into the shared buffers, recording where
    /// each layer's slice begins and ends, then upload once.
    fn gather(&mut self, frame: &Frame) {
        let textures = &self.textures;
        build_batches(
            frame,
            &mut self.glass_scratch,
            &mut self.flat_scratch,
            &mut self.icon_scratch,
            &mut self.batches,
            |id| textures.get(id).is_some(),
        );

        self.glass_instances
            .upload(&self.gpu.device, &self.gpu.queue, &self.glass_scratch);
        self.flat_instances
            .upload(&self.gpu.device, &self.gpu.queue, &self.flat_scratch);
        self.icon_instances
            .upload(&self.gpu.device, &self.gpu.queue, &self.icon_scratch);
    }

    fn pass_layer(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        _frame: &Frame,
        layer: Layer,
        source: usize,
        target: usize,
    ) {
        let batch = self.batches[layer as usize].clone();

        let view = &self.targets.composite[target].view;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("layer"),
            color_attachments: &[Some(attachment(view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Carry the previous result forward, since glass covers only part of
        // the screen.
        pass.set_pipeline(&self.pipelines.blit);
        pass.set_bind_group(0, &self.bindings.blit[source], &[]);
        pass.draw(0..3, 0..1);

        let (glass_start, glass_end) = batch.glass;
        if glass_end > glass_start {
            pass.set_pipeline(&self.pipelines.glass);
            pass.set_bind_group(0, &self.bindings.glass[source], &[]);
            pass.set_vertex_buffer(0, self.glass_instances.slice());
            pass.draw(0..6, glass_start..glass_end);
        }

        if !batch.flat.is_empty() {
            pass.set_pipeline(&self.pipelines.flat);
            pass.set_vertex_buffer(0, self.flat_instances.slice());
            for (texture, start, end) in &batch.flat {
                let bind = match texture {
                    // A 1×1 white texture stands in for "no image", so fills
                    // and images can share one pipeline.
                    None => &self.bindings.flat,
                    Some(id) => match self.textures.get(*id) {
                        Some(image) => &image.bind_group,
                        None => continue,
                    },
                };
                pass.set_bind_group(0, bind, &[]);
                pass.draw(0..6, *start..*end);
            }
        }

        let (icon_start, icon_end) = batch.icon;
        if icon_end > icon_start {
            pass.set_pipeline(&self.pipelines.icon);
            pass.set_bind_group(0, &self.bindings.icon, &[]);
            pass.set_vertex_buffer(0, self.icon_instances.slice());
            pass.draw(0..6, icon_start..icon_end);
        }

        // Text shares this pass rather than opening its own. On a tile-based
        // GPU an extra pass means storing and reloading the whole framebuffer.
        if let Err(e) = self.text.render(layer, &mut pass) {
            log::warn!("text rendering failed in {layer:?}: {e:?}");
        }
    }

    fn pass_present(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface: &wgpu::SurfaceTexture,
        source: usize,
    ) {
        let view = surface
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present"),
            color_attachments: &[Some(attachment(&view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines.present);
        pass.set_bind_group(0, &self.bindings.present[source], &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Renderer {
    /// Render one frame into memory instead of onto the screen.
    ///
    /// Exists so the material can be inspected and regression-tested without a
    /// human looking at a display — a shader that is subtly wrong is very hard
    /// to argue about from a description, and trivial to settle from a file.
    ///
    /// Returns tightly-packed RGBA8 rows.
    pub fn capture(&mut self, frame: &Frame, env: Environment) -> Vec<u8> {
        let (w, h) = self.gpu.size();
        let device = &self.gpu.device;

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // Buffer rows must be a multiple of 256 bytes, so the readback is
        // padded and unpacked below.
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture-readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.write_uniforms(frame, &env, w, h);

        // A capture is always 8-bit, so it must always encode — regardless of
        // what the live surface negotiated. On an HDR display the surface path
        // skips the encode and hands linear values straight to the panel; doing
        // that into an 8-bit file would write linear values as if they were
        // sRGB and produce an image far darker than what is on screen.
        self.gpu.queue.write_buffer(
            &self.uniforms.present,
            0,
            bytemuck::bytes_of(&PresentUniforms {
                encode: 1.0,
                fade: env.fade,
                _pad: [0.0; 2],
            }),
        );

        if let Err(e) = self
            .text
            .prepare(&self.gpu.device, &self.gpu.queue, frame, (w, h))
        {
            log::warn!("text preparation failed: {e:?}");
        }

        self.gather(frame);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("capture"),
            });

        self.pass_environment(&mut encoder);
        let mut last = 0usize;
        for (index, layer) in Layer::ALL.iter().enumerate() {
            let source = backdrop_index(index);
            let target_index = target_index(index);
            self.pass_blur(&mut encoder, source);
            self.pass_layer(&mut encoder, frame, *layer, source, target_index);
            last = target_index;
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("capture-present"),
                color_attachments: &[Some(attachment(&view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.present_capture);
            pass.set_bind_group(0, &self.bindings.present[last], &[]);
            pass.draw(0..3, 0..1);
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        self.gpu.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        // Block until the copy has actually landed. Only acceptable because
        // capture is a deliberate, offline operation — never on a live frame.
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());

        let mapped = slice
            .get_mapped_range()
            .expect("readback buffer should be mapped after a blocking poll");
        let mut pixels = Vec::with_capacity((unpadded * h) as usize);
        for row in 0..h {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();
        self.text.trim();

        pixels
    }
}

/// Lay every layer's instances out end to end and record each layer's slice.
///
/// Separated from the GPU so the slicing can be tested directly — this is the
/// logic that, when it was per-layer instead, silently rendered one layer's
/// glass with another's parameters.
fn build_batches(
    frame: &Frame,
    glass: &mut Vec<GlassInstance>,
    flat: &mut Vec<FlatInstance>,
    icons: &mut Vec<IconInstance>,
    batches: &mut [LayerBatch],
    texture_exists: impl Fn(TextureId) -> bool,
) {
    glass.clear();
    flat.clear();
    icons.clear();

    for (index, layer) in Layer::ALL.iter().enumerate() {
        let batch = &mut batches[index];
        batch.flat.clear();

        let glass_start = glass.len() as u32;
        let flat_start = flat.len() as u32;
        let icon_start = icons.len() as u32;

        for item in frame.items.iter().filter(|i| i.layer == *layer) {
            match &item.primitive {
                Primitive::Glass(g) => glass.push(GlassInstance::from_paint(g, item.opacity)),
                Primitive::Fill(f) => flat.push(FlatInstance::from_fill(f, item.opacity)),
                Primitive::Icon(i) => icons.push(IconInstance::from_paint(i, item.opacity)),
                Primitive::Text(_) | Primitive::Image(_) => {}
            }
        }
        batch.glass = (glass_start, glass.len() as u32);
        batch.icon = (icon_start, icons.len() as u32);

        // Fills first, under no texture at all; then images, one run each, so
        // every run is a single instanced draw.
        let flat_end = flat.len() as u32;
        if flat_end > flat_start {
            batch.flat.push((None, flat_start, flat_end));
        }

        for item in frame.items.iter().filter(|i| i.layer == *layer) {
            let Primitive::Image(image) = &item.primitive else {
                continue;
            };
            // Silently skipping an unknown handle keeps a missing asset from
            // taking down the shell; the gap is visible, which is the right
            // amount of alarming.
            if !texture_exists(image.texture) {
                continue;
            }
            let start = flat.len() as u32;
            flat.push(FlatInstance::from_image(image, item.opacity));
            batch.flat.push((Some(image.texture), start, start + 1));
        }
    }
}

fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            // Clear rather than DontCare: every one of these passes fully
            // overwrites its target, and on a tile-based GPU a clear is free
            // while DontCare now needs an unsafe token to opt into.
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detends_paint::{vec2, Fill, Glass, Id, Image, Item, Rect};

    fn glass_at(x: f32) -> Primitive {
        Primitive::Glass(Glass {
            rect: Rect::from_min_size(vec2(x, 0.0), vec2(40.0, 40.0)),
            ..Default::default()
        })
    }

    fn frame_with(items: Vec<Item>) -> Frame {
        let mut frame = Frame::new(vec2(1000.0, 800.0), 2.0);
        for item in items {
            frame.push(item);
        }
        frame
    }

    fn run(frame: &Frame) -> (Vec<GlassInstance>, Vec<FlatInstance>, Vec<LayerBatch>) {
        let mut glass = Vec::new();
        let mut flat = Vec::new();
        let mut icons = Vec::new();
        let mut batches = vec![LayerBatch::default(); Layer::ALL.len()];
        build_batches(frame, &mut glass, &mut flat, &mut icons, &mut batches, |_| true);
        (glass, flat, batches)
    }

    #[test]
    fn each_layer_gets_its_own_slice_and_they_never_overlap() {
        // The regression this exists for: instances for every layer share one
        // buffer, because `Queue::write_buffer` does not interleave with
        // command encoding. If two layers claimed the same range, one would
        // render with the other's parameters.
        let frame = frame_with(vec![
            Item::new(Id(1), Layer::Environment, glass_at(0.0)),
            Item::new(Id(2), Layer::Content, glass_at(100.0)),
            Item::new(Id(3), Layer::Content, glass_at(200.0)),
            Item::new(Id(4), Layer::Overlay, glass_at(300.0)),
        ]);
        let (glass, _, batches) = run(&frame);

        assert_eq!(glass.len(), 4);
        assert_eq!(batches[Layer::Environment as usize].glass, (0, 1));
        assert_eq!(batches[Layer::Content as usize].glass, (1, 3));
        assert_eq!(batches[Layer::Overlay as usize].glass, (3, 4));

        // Every instance belongs to exactly one layer.
        let mut covered = vec![0u32; glass.len()];
        for batch in &batches {
            for i in batch.glass.0..batch.glass.1 {
                covered[i as usize] += 1;
            }
        }
        assert!(covered.iter().all(|&n| n == 1), "coverage was {covered:?}");
    }

    #[test]
    fn an_overlay_never_disturbs_a_lower_layers_instances() {
        // Adding an overlay must not change where content's instances live.
        let without = frame_with(vec![
            Item::new(Id(1), Layer::Content, glass_at(0.0)),
            Item::new(Id(2), Layer::Content, glass_at(100.0)),
        ]);
        let with = frame_with(vec![
            Item::new(Id(1), Layer::Content, glass_at(0.0)),
            Item::new(Id(2), Layer::Content, glass_at(100.0)),
            Item::new(Id(3), Layer::Overlay, glass_at(300.0)),
        ]);

        let (glass_a, _, batches_a) = run(&without);
        let (glass_b, _, batches_b) = run(&with);

        assert_eq!(
            batches_a[Layer::Content as usize].glass,
            batches_b[Layer::Content as usize].glass
        );
        for i in 0..2 {
            assert_eq!(
                glass_a[i].center, glass_b[i].center,
                "content instance {i} moved"
            );
        }
    }

    #[test]
    fn an_empty_layer_claims_an_empty_range() {
        let frame = frame_with(vec![Item::new(Id(1), Layer::Content, glass_at(0.0))]);
        let (_, _, batches) = run(&frame);
        assert_eq!(batches[Layer::Environment as usize].glass, (0, 0));
        assert_eq!(batches[Layer::Overlay as usize].glass, (1, 1));
        assert!(batches[Layer::Overlay as usize].flat.is_empty());
    }

    #[test]
    fn fills_come_before_images_within_a_layer() {
        let frame = frame_with(vec![
            Item::new(
                Id(1),
                Layer::Content,
                Primitive::Image(Image {
                    rect: Rect::from_min_size(vec2(0.0, 0.0), vec2(10.0, 10.0)),
                    texture: TextureId(7),
                    source: Rect::ZERO,
                    radius: 0.0,
                    squircle: 4.0,
                    tint: detends_paint::Color::WHITE,
                }),
            ),
            Item::new(Id(2), Layer::Content, Primitive::Fill(Fill::default())),
        ]);
        let (_, flat, batches) = run(&frame);

        assert_eq!(flat.len(), 2);
        let runs = &batches[Layer::Content as usize].flat;
        assert_eq!(runs[0].0, None, "fills should be first");
        assert_eq!(runs[1].0, Some(TextureId(7)));
    }

    #[test]
    fn an_image_with_no_texture_is_skipped_rather_than_drawn_blank() {
        let frame = frame_with(vec![Item::new(
            Id(1),
            Layer::Content,
            Primitive::Image(Image {
                rect: Rect::from_min_size(vec2(0.0, 0.0), vec2(10.0, 10.0)),
                texture: TextureId(404),
                source: Rect::ZERO,
                radius: 0.0,
                squircle: 4.0,
                tint: detends_paint::Color::WHITE,
            }),
        )]);

        let mut glass = Vec::new();
        let mut flat = Vec::new();
        let mut batches = vec![LayerBatch::default(); Layer::ALL.len()];
        let mut icons = Vec::new();
        build_batches(&frame, &mut glass, &mut flat, &mut icons, &mut batches, |_| false);

        assert!(flat.is_empty());
        assert!(batches[Layer::Content as usize].flat.is_empty());
    }

    #[test]
    fn gathering_twice_does_not_accumulate() {
        let frame = frame_with(vec![Item::new(Id(1), Layer::Content, glass_at(0.0))]);
        let mut glass = Vec::new();
        let mut flat = Vec::new();
        let mut icons = Vec::new();
        let mut batches = vec![LayerBatch::default(); Layer::ALL.len()];
        for _ in 0..3 {
            build_batches(&frame, &mut glass, &mut flat, &mut icons, &mut batches, |_| true);
        }
        assert_eq!(glass.len(), 1, "instances accumulated across frames");
    }
}
