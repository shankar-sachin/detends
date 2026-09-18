//! Device, surface, and the colour pipeline's two ends.

use std::sync::Arc;

/// How the finished linear image is delivered to the display.
///
/// détends renders everything in linear light and encodes exactly once, here.
/// Keeping that decision in one enum is what makes an HDR path later a small
/// change rather than an audit of every shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    /// Encode to sRGB in the shader and dither, into a non-sRGB 8-bit surface.
    ///
    /// The encode is manual rather than handed to a `*Srgb` surface format for
    /// one specific reason: the dither has to be applied *after* the transfer
    /// curve, at ±1 unit of the final 8-bit value. A hardware sRGB surface
    /// encodes after the fragment shader runs, which would compress the dither
    /// in the shadows — exactly where a calm dark gradient bands worst.
    SrgbDithered,
    /// Extended-range linear straight out, no encode and no dither.
    /// For a display that can take it; the extra range lets rim highlights
    /// actually glow instead of clipping at white.
    ExtendedLinear,
}

/// Everything rendering needs from the platform.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub output: OutputMode,
}

/// The format every offscreen target uses.
///
/// Linear and filterable on Apple Silicon without extra features. Sixteen bits
/// per channel leaves room for rim highlights above 1.0 and keeps the blur
/// pyramid free of the quantisation that would otherwise show up as banding
/// long before the dither gets a chance to help.
pub const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl Gpu {
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        // `InstanceDescriptor` has no `Default` in wgpu 30; the constructors
        // are how you say whether a display handle is available. macOS needs
        // none. Wayland will use `new_with_display_handle`.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        let surface = instance
            .create_surface(target)
            .map_err(|e| GpuError::Surface(e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                // Bucketing exists to stop untrusted content fingerprinting the
                // machine through adapter limits. détends is trusted native
                // code, so it takes the real limits.
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;

        let info = adapter.get_info();
        log::info!("détends on {} ({:?})", info.name, info.backend);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("détends"),
                required_features: wgpu::Features::empty(),
                // Conservative limits everywhere except resolution, which is
                // raised to whatever the adapter actually offers.
                //
                // The downlevel defaults cap textures at 2048, which is
                // smaller than a single modern display — keeping them would
                // put a Retina screen out of reach in the name of supporting
                // weak hardware. `using_resolution` keeps the rest of the
                // conservative profile, which is what genuinely helps there.
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| GpuError::NoDevice(e.to_string()))?;

        // By default wgpu treats a validation error as fatal and panics. On
        // macOS that panic happens inside an Objective-C callback that cannot
        // unwind, so the process aborts and the user gets a system crash
        // dialog — for what is usually a one-line shader mistake.
        //
        // A shell has no business taking the machine down over a bad uniform
        // layout. Log it loudly and keep running: the frame will look wrong,
        // which is the right amount of alarming, and the message names the
        // problem far better than a stack trace through the window server.
        device.on_uncaptured_error(std::sync::Arc::new(|error| {
            log::error!("GPU error: {error}");
        }));

        let caps = surface.get_capabilities(&adapter);
        let (format, color_space, output) = choose_output(&caps);
        log::info!("presenting {format:?} / {color_space:?} as {output:?}");

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            // Fifo is the only mode Metal actually honours, and it is what
            // ProMotion ramps against. Mailbox silently falls back to it.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            // Two, never one. A latency of 1 stalls the CPU on drawable
            // acquisition; macOS reads the resulting missed frames as an
            // inconsistent submitter and drops the panel to 60Hz.
            desired_maximum_frame_latency: 2,
            color_space,
        };
        surface.configure(&device, &config);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            output,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Acquire the next drawable, reconfiguring if the surface went stale.
    ///
    /// In wgpu 30 this is an enum rather than a `Result`, and several of its
    /// variants still hand back a usable texture — treating it as a plain
    /// error would throw away perfectly good frames.
    pub fn acquire(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Some(t),
            // Usable now; the surface wants reconfiguring afterwards.
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                let (w, h) = self.size();
                self.resize(w, h);
                Some(t)
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let (w, h) = self.size();
                self.resize(w, h);
                None
            }
            // Occluded means nobody can see it, so drawing would be waste.
            wgpu::CurrentSurfaceTexture::Occluded => None,
            other => {
                log::warn!("surface unavailable: {other:?}");
                None
            }
        }
    }
}

/// Pick the surface format and colour space, preferring extended-range linear
/// where the display offers it.
fn choose_output(
    caps: &wgpu::SurfaceCapabilities,
) -> (wgpu::TextureFormat, wgpu::SurfaceColorSpace, OutputMode) {
    let supports = |f: wgpu::TextureFormat| caps.formats.contains(&f);

    if supports(WORKING_FORMAT)
        && caps
            .color_spaces(WORKING_FORMAT)
            .contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR)
    {
        return (
            WORKING_FORMAT,
            wgpu::SurfaceColorSpace::ExtendedSrgbLinear,
            OutputMode::ExtendedLinear,
        );
    }

    // BGRA is Metal's native swapchain layout; asking for RGBA costs a
    // conversion. Non-sRGB on purpose — see `OutputMode::SrgbDithered`.
    let format = if supports(wgpu::TextureFormat::Bgra8Unorm) {
        wgpu::TextureFormat::Bgra8Unorm
    } else {
        *caps
            .formats
            .first()
            .expect("a surface with no supported formats")
    };
    (
        format,
        wgpu::SurfaceColorSpace::Srgb,
        OutputMode::SrgbDithered,
    )
}

#[derive(Debug)]
pub enum GpuError {
    Surface(String),
    NoAdapter(String),
    NoDevice(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(e) => write!(f, "could not create a surface: {e}"),
            Self::NoAdapter(e) => write!(f, "no suitable graphics adapter: {e}"),
            Self::NoDevice(e) => write!(f, "could not open the graphics device: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// Shared by every pass that draws a fullscreen triangle.
pub fn fullscreen_vertex(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fullscreen"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/fullscreen.wgsl").into()),
    })
}

/// A linear-filtering, edge-clamping sampler.
///
/// Both properties matter for the blur: the half-texel offsets in a Kawase
/// kernel only work because bilinear filtering turns each tap into a 2×2
/// average, and clamping stops the wallpaper wrapping around the screen edge
/// into the blur.
pub fn linear_sampler(device: &wgpu::Device) -> Arc<wgpu::Sampler> {
    Arc::new(linear_sampler_raw(device))
}

/// The same sampler, owned outright.
pub fn linear_sampler_raw(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("linear-clamp"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}
