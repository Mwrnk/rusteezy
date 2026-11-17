//! wgpu renderer: instanced grid pass into an HDR target, bloom chain, tonemap.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use winit::window::Window;

use crate::{atlas::GlyphAtlas, color::Rgb, grid::Grid};

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const MAX_BLOOM_LEVELS: usize = 6;
const CAPTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Tunable look parameters.
#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    /// Brightness above which pixels bloom.
    pub bloom_threshold: f32,
    /// Soft knee width around the threshold.
    pub bloom_knee: f32,
    pub bloom_intensity: f32,
    pub exposure: f32,
    /// How strongly `Cell::glow` brightens a glyph (fg × (1 + glow × gain)).
    pub glow_gain: f32,
    /// Color of the area around the grid.
    pub clear: Rgb,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            bloom_threshold: 0.85,
            bloom_knee: 0.35,
            bloom_intensity: 0.9,
            exposure: 1.15,
            glow_gain: 1.6,
            clear: Rgb::BLACK,
        }
    }
}

/// Where a frame's time went (seconds).
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTiming {
    /// Packing, upload, encoding and submit: real work.
    pub cpu: f64,
    /// Waiting for a swapchain image (mostly vsync).
    pub acquire: f64,
    pub presented: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GridUniforms {
    screen: [f32; 2],
    cell: [f32; 2],
    origin: [f32; 2],
    cols: u32,
    atlas_cols: u32,
    glow_gain: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuCell {
    fg: [f32; 4],
    bg: [f32; 3],
    glyph: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BloomParams {
    threshold: f32,
    knee: f32,
    intensity: f32,
    exposure: f32,
}

struct Target {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    atlas: GlyphAtlas,
    atlas_view: wgpu::TextureView,

    grid_uniforms: wgpu::Buffer,
    cell_buf: wgpu::Buffer,
    cell_capacity: usize,
    grid_bgl: wgpu::BindGroupLayout,
    grid_bg: wgpu::BindGroup,
    grid_pipeline: wgpu::RenderPipeline,
    staging: Vec<GpuCell>,

    sampler: wgpu::Sampler,
    bloom_params: wgpu::Buffer,
    bloom_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,
    bright_pipeline: wgpu::RenderPipeline,
    down_pipeline: wgpu::RenderPipeline,
    up_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    capture_pipeline: wgpu::RenderPipeline,

    hdr: Target,
    bloom: Vec<Target>,
    bright_bg: wgpu::BindGroup,
    down_bgs: Vec<wgpu::BindGroup>,
    up_bgs: Vec<wgpu::BindGroup>,
    composite_bg: wgpu::BindGroup,

    bench_target: Option<(Target, u32, u32)>,
    pub settings: RenderSettings,
    /// Top-left pixel of the grid (grid is centered in the window).
    origin: [f32; 2],
}

impl Renderer {
    pub async fn new(window: Arc<Window>, atlas: GlyphAtlas, vsync: bool, verbose: bool) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window).map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rusteezy device"),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("surface not supported by adapter")?;
        config.format = format;
        config.present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        if verbose {
            eprintln!(
                "rusteezy: {} ({:?}) | present modes {:?} -> {:?} | {}x{} {:?}",
                adapter.get_info().name,
                adapter.get_info().backend,
                caps.present_modes,
                config.present_mode,
                config.width,
                config.height,
                config.format
            );
        }

        // --- grid pass ---
        let atlas_view = upload_atlas(&device, &queue, &atlas);
        let grid_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid uniforms"),
            size: std::mem::size_of::<GridUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cell_capacity = 1;
        let cell_buf = make_cell_buf(&device, cell_capacity);

        let grid_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid bgl"),
            entries: &[
                bgl_uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                bgl_texture(2, false),
            ],
        });
        let grid_bg = make_grid_bg(&device, &grid_bgl, &grid_uniforms, &cell_buf, &atlas_view);

        let grid_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/grid.wgsl"));
        let grid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid layout"),
            bind_group_layouts: &[Some(&grid_bgl)],
            immediate_size: 0,
        });
        let grid_pipeline = make_pipeline(&device, &grid_layout, &grid_shader, "vs", "fs", HDR, None);

        // --- bloom ---
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bloom_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom params"),
            size: std::mem::size_of::<BloomParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom bgl"),
            entries: &[bgl_texture(0, true), bgl_sampler(1), bgl_uniform(2, wgpu::ShaderStages::FRAGMENT)],
        });
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite bgl"),
            entries: &[
                bgl_texture(0, true),
                bgl_sampler(1),
                bgl_uniform(2, wgpu::ShaderStages::FRAGMENT),
                bgl_texture(3, true),
            ],
        });
        let bloom_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/bloom.wgsl"));
        let bloom_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom layout"),
            bind_group_layouts: &[Some(&bloom_bgl)],
            immediate_size: 0,
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite layout"),
            bind_group_layouts: &[Some(&composite_bgl)],
            immediate_size: 0,
        });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let bright_pipeline = make_pipeline(&device, &bloom_layout, &bloom_shader, "vs_full", "fs_bright", HDR, None);
        let down_pipeline = make_pipeline(&device, &bloom_layout, &bloom_shader, "vs_full", "fs_down", HDR, None);
        let up_pipeline = make_pipeline(&device, &bloom_layout, &bloom_shader, "vs_full", "fs_up", HDR, Some(additive));
        let composite_pipeline =
            make_pipeline(&device, &composite_layout, &bloom_shader, "vs_full", "fs_composite", format, None);
        let capture_pipeline =
            make_pipeline(&device, &composite_layout, &bloom_shader, "vs_full", "fs_composite", CAPTURE, None);

        let targets = SizedTargets::new(
            &device,
            config.width,
            config.height,
            &bloom_bgl,
            &composite_bgl,
            &sampler,
            &bloom_params,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            atlas,
            atlas_view,
            grid_uniforms,
            cell_buf,
            cell_capacity,
            grid_bgl,
            grid_bg,
            grid_pipeline,
            staging: Vec::new(),
            sampler,
            bloom_params,
            bloom_bgl,
            composite_bgl,
            bright_pipeline,
            down_pipeline,
            up_pipeline,
            composite_pipeline,
            capture_pipeline,
            hdr: targets.hdr,
            bloom: targets.bloom,
            bright_bg: targets.bright_bg,
            down_bgs: targets.down_bgs,
            up_bgs: targets.up_bgs,
            composite_bg: targets.composite_bg,
            bench_target: None,
            settings: RenderSettings::default(),
            origin: [0.0, 0.0],
        })
    }

    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Borrow the atlas and the settings at the same time.
    pub fn parts(&mut self) -> (&GlyphAtlas, &mut RenderSettings) {
        (&self.atlas, &mut self.settings)
    }

    /// Replace the glyph atlas (e.g. after a DPI change).
    pub fn set_atlas(&mut self, atlas: GlyphAtlas) {
        self.atlas_view = upload_atlas(&self.device, &self.queue, &atlas);
        self.atlas = atlas;
        self.grid_bg =
            make_grid_bg(&self.device, &self.grid_bgl, &self.grid_uniforms, &self.cell_buf, &self.atlas_view);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Grid dimensions that fit the current surface.
    pub fn grid_dims(&self) -> (usize, usize) {
        (
            (self.config.width / self.atlas.cell_w).max(1) as usize,
            (self.config.height / self.atlas.cell_h).max(1) as usize,
        )
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let t = SizedTargets::new(
            &self.device,
            width,
            height,
            &self.bloom_bgl,
            &self.composite_bgl,
            &self.sampler,
            &self.bloom_params,
        );
        self.hdr = t.hdr;
        self.bloom = t.bloom;
        self.bright_bg = t.bright_bg;
        self.down_bgs = t.down_bgs;
        self.up_bgs = t.up_bgs;
        self.composite_bg = t.composite_bg;
    }

    /// Save the most recently rendered frame (scene + bloom, tonemapped) as a PNG.
    pub fn capture(&self, path: &std::path::Path) -> Result<(), String> {
        let (w, h, rgb) = self.capture_rgb()?;
        crate::png::write_rgb(path, w, h, &rgb).map_err(|e| e.to_string())
    }

    /// Read back the most recently rendered frame as packed 8-bit sRGB `(width, height, rgb)`.
    /// Re-runs only the composite pass into a readable texture, so call it right after rendering.
    pub fn capture_rgb(&self) -> Result<(u32, u32, Vec<u8>), String> {
        let (w, h) = (self.config.width, self.config.height);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CAPTURE,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
        {
            let mut pass = begin(&mut enc, &view, Some(wgpu::Color::BLACK));
            pass.set_pipeline(&self.capture_pipeline);
            pass.set_bind_group(0, &self.composite_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buf.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = buf.get_mapped_range(..).map_err(|e| format!("{e:?}"))?;
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            let line = &data[(y * row) as usize..(y * row + w * 4) as usize];
            for px in line.chunks_exact(4) {
                rgb.extend_from_slice(&px[..3]);
            }
        }
        drop(data);
        buf.unmap();
        Ok((w, h, rgb))
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// Pack and upload cells + uniforms. Returns the cell count, or None for an empty grid.
    fn prepare(&mut self, grid: &Grid) -> Option<usize> {
        let (gw, gh) = (grid.width(), grid.height());
        let n = gw * gh;
        if n == 0 {
            return None;
        }
        // Pack cells.
        self.staging.clear();
        self.staging.reserve(n);
        for c in grid.cells() {
            self.staging.push(GpuCell {
                fg: [c.fg.r, c.fg.g, c.fg.b, c.glow],
                bg: [c.bg.r, c.bg.g, c.bg.b],
                glyph: self.atlas.slot(c.ch) as u32,
            });
        }
        if n > self.cell_capacity {
            self.cell_capacity = n.next_power_of_two();
            self.cell_buf = make_cell_buf(&self.device, self.cell_capacity);
            self.grid_bg =
                make_grid_bg(&self.device, &self.grid_bgl, &self.grid_uniforms, &self.cell_buf, &self.atlas_view);
        }
        self.queue.write_buffer(&self.cell_buf, 0, bytemuck::cast_slice(&self.staging));

        let (cw, ch) = (self.atlas.cell_w as f32, self.atlas.cell_h as f32);
        let (sw, sh) = (self.config.width as f32, self.config.height as f32);
        self.origin = [((sw - gw as f32 * cw) / 2.0).floor(), ((sh - gh as f32 * ch) / 2.0).floor()];
        let u = GridUniforms {
            screen: [sw, sh],
            cell: [cw, ch],
            origin: self.origin,
            cols: gw as u32,
            atlas_cols: self.atlas.cols,
            glow_gain: self.settings.glow_gain,
            _pad: [0.0; 3],
        };
        self.queue.write_buffer(&self.grid_uniforms, 0, bytemuck::bytes_of(&u));
        let s = self.settings;
        let bp = BloomParams {
            threshold: s.bloom_threshold,
            knee: s.bloom_knee.max(1e-4),
            intensity: s.bloom_intensity,
            exposure: s.exposure,
        };
        self.queue.write_buffer(&self.bloom_params, 0, bytemuck::bytes_of(&bp));
        Some(n)
    }

    /// Upload the grid and draw a frame.
    pub fn render(&mut self, grid: &Grid) -> RenderTiming {
        let start = std::time::Instant::now();
        let mut timing = RenderTiming::default();
        let Some(n) = self.prepare(grid) else { return timing };

        // Acquiring the next swapchain image is where vsync blocks; time it separately.
        let acquire_start = std::time::Instant::now();
        let acquired = self.surface.get_current_texture();
        timing.acquire = acquire_start.elapsed().as_secs_f64();
        let (frame, suboptimal) = match acquired {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.reconfigure();
                timing.cpu = start.elapsed().as_secs_f64() - timing.acquire;
                return timing;
            }
            _ => {
                timing.cpu = start.elapsed().as_secs_f64() - timing.acquire;
                return timing;
            }
        };
        self.encode_and_submit(&frame, n);
        self.queue.present(frame);
        if suboptimal {
            self.reconfigure();
        }
        timing.presented = true;
        timing.cpu = start.elapsed().as_secs_f64() - timing.acquire;
        timing
    }

    fn encode_and_submit(&self, frame: &wgpu::SurfaceTexture, n: usize) {
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.encode_to(&view, &self.composite_pipeline, n);
    }

    /// Benchmark path: run every pass (composite into an off-screen image instead of
    /// the swapchain), then block until the GPU has finished. No vsync involved.
    pub fn render_offscreen_blocking(&mut self, grid: &Grid) -> RenderTiming {
        let start = std::time::Instant::now();
        let mut timing = RenderTiming::default();
        let Some(n) = self.prepare(grid) else { return timing };
        let (w, h) = (self.config.width, self.config.height);
        if self.bench_target.as_ref().is_none_or(|(_, bw, bh)| (*bw, *bh) != (w, h)) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bench target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: CAPTURE,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.bench_target = Some((Target { _tex: tex, view }, w, h));
        }
        let view = &self.bench_target.as_ref().unwrap().0.view;
        self.encode_to(view, &self.capture_pipeline, n);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        timing.cpu = start.elapsed().as_secs_f64();
        timing.presented = true;
        timing
    }

    fn encode_to(&self, view: &wgpu::TextureView, composite: &wgpu::RenderPipeline, n: usize) {
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let c = self.settings.clear;

        // 1. grid -> hdr
        {
            let mut pass = begin(
                &mut enc,
                &self.hdr.view,
                Some(wgpu::Color { r: c.r as f64, g: c.g as f64, b: c.b as f64, a: 1.0 }),
            );
            pass.set_pipeline(&self.grid_pipeline);
            pass.set_bind_group(0, &self.grid_bg, &[]);
            pass.draw(0..6, 0..n as u32);
        }
        // 2. bright pass -> bloom[0]
        {
            let mut pass = begin(&mut enc, &self.bloom[0].view, Some(wgpu::Color::BLACK));
            pass.set_pipeline(&self.bright_pipeline);
            pass.set_bind_group(0, &self.bright_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        // 3. downsample chain
        for i in 1..self.bloom.len() {
            let mut pass = begin(&mut enc, &self.bloom[i].view, Some(wgpu::Color::BLACK));
            pass.set_pipeline(&self.down_pipeline);
            pass.set_bind_group(0, &self.down_bgs[i - 1], &[]);
            pass.draw(0..3, 0..1);
        }
        // 4. additive upsample chain: bloom[i+1] -> bloom[i]
        for i in (0..self.bloom.len() - 1).rev() {
            let mut pass = begin(&mut enc, &self.bloom[i].view, None);
            pass.set_pipeline(&self.up_pipeline);
            pass.set_bind_group(0, &self.up_bgs[i], &[]);
            pass.draw(0..3, 0..1);
        }
        // 5. composite + tonemap -> output
        {
            let mut pass = begin(&mut enc, view, Some(wgpu::Color::BLACK));
            pass.set_pipeline(composite);
            pass.set_bind_group(0, &self.composite_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
    }
}

struct SizedTargets {
    hdr: Target,
    bloom: Vec<Target>,
    bright_bg: wgpu::BindGroup,
    down_bgs: Vec<wgpu::BindGroup>,
    up_bgs: Vec<wgpu::BindGroup>,
    composite_bg: wgpu::BindGroup,
}

impl SizedTargets {
    fn new(
        device: &wgpu::Device,
        w: u32,
        h: u32,
        bloom_bgl: &wgpu::BindGroupLayout,
        composite_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        params: &wgpu::Buffer,
    ) -> Self {
        let hdr = make_target(device, w, h, "hdr");
        let mut bloom = Vec::new();
        let (mut bw, mut bh) = (w / 2, h / 2);
        while bloom.len() < MAX_BLOOM_LEVELS && bw >= 4 && bh >= 4 {
            bloom.push(make_target(device, bw, bh, "bloom"));
            bw /= 2;
            bh /= 2;
        }
        if bloom.is_empty() {
            bloom.push(make_target(device, 1, 1, "bloom"));
        }
        let bg = |src: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom bg"),
                layout: bloom_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
                ],
            })
        };
        let bright_bg = bg(&hdr.view);
        // down_bgs[i] reads bloom[i] (writes bloom[i+1]); up_bgs[i] reads bloom[i+1] (writes bloom[i]).
        let down_bgs = (0..bloom.len().saturating_sub(1)).map(|i| bg(&bloom[i].view)).collect();
        let up_bgs = (0..bloom.len().saturating_sub(1)).map(|i| bg(&bloom[i + 1].view)).collect();
        let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite bg"),
            layout: composite_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&hdr.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&bloom[0].view) },
            ],
        });
        Self { hdr, bloom, bright_bg, down_bgs, up_bgs, composite_bg }
    }
}

fn begin<'a>(
    enc: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    clear: Option<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: match clear {
                    Some(c) => wgpu::LoadOp::Clear(c),
                    None => wgpu::LoadOp::Load,
                },
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    })
}

fn make_target(device: &wgpu::Device, w: u32, h: u32, label: &str) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Target { _tex: tex, view }
}

fn upload_atlas(device: &wgpu::Device, queue: &wgpu::Queue, atlas: &GlyphAtlas) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: atlas.width(), height: atlas.height(), depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph atlas"),
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
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width()),
            rows_per_image: Some(atlas.height()),
        },
        size,
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_cell_buf(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cells"),
        size: (capacity * std::mem::size_of::<GpuCell>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_grid_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    cells: &wgpu::Buffer,
    atlas: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("grid bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: cells.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(atlas) },
        ],
    })
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fs),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend.unwrap_or(wgpu::BlendState::REPLACE)),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn bgl_uniform(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_texture(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn bgl_sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
