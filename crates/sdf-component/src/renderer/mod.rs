mod frame_request;
mod staging;
mod staging_state;
mod target;

pub(crate) use frame_request::FrameRequest;

use self::{staging::Staging, staging_state::StagingState, target::Target};
use crate::{SceneData, SdfError, SdfResult, overlay, scene::SdfScene};

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use soul_attributes::soul;
use std::{
    io::Write as _,
    sync::{Arc, mpsc},
};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType, BufferDescriptor, BufferUsages,
    ColorTargetState, ColorWrites, CommandEncoderDescriptor, Device, Extent3d, FragmentState,
    LoadOp, MapMode, MultisampleState, Operations, Origin3d, PipelineLayoutDescriptor,
    PowerPreference, PrimitiveState, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, Sampler, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexState,
};

/// Size of the uniform block in bytes. Must match the WGSL `Uniforms` struct:
/// `resolution: vec2f` (0..8), `time: f32` (8..12), `aspect: f32` (12..16),
/// `mouse: vec2f` (16..24), `view_center: vec2f` (24..32) — the field-uv
/// point at the viewport center, `params: vec4f` (32..48): `.x` is the
/// mip-level selector, `.y` the contour-overlay band width, `.z` the zoom
/// factor, `.w` reserved (stays 0).
const UNIFORM_SIZE: u64 = 48;

/// Number of staging buffers kept in flight; the ring lets the GPU run a
/// little ahead of presentation.
const STAGING_SLOTS: usize = 3;

/// The frame the renderer last submitted: the request and the data identity
/// it was submitted under. A render call whose inputs all match submits
/// nothing — the outstanding frame already carries those pixels, and without
/// the skip every keep-alive paint would enqueue another one, so the ring
/// could never drain and the paint loop could never settle.
struct Submitted {
    data_key: Option<String>,
    request: FrameRequest,
}

/// Polls allowed while the resize test hook parks the staging ring.
#[cfg(test)]
const PARK_POLL_LIMIT: usize = 100;

/// Pause between those polls.
#[cfg(test)]
const PARK_POLL_PAUSE: std::time::Duration = std::time::Duration::from_millis(10);

/// A wgpu render-to-texture pipeline for SDF scenes with a ring of staging
/// buffers for non-blocking CPU readback.
pub(crate) struct Renderer {
    device: Device,
    queue: Queue,
    uniform_buffer: Buffer,
    bind_group_layout: BindGroupLayout,
    linear_sampler: Sampler,
    nearest_sampler: Sampler,
    stub_view: TextureView,
    bind_group: BindGroup,
    field: Option<Texture>,
    field_key: Option<String>,
    /// The data key whose last upload attempt failed, with the error to keep
    /// raising. A failed upload is deterministic — a payload stays missing,
    /// corrupt, or truncated until the scene's data changes — so re-attempting
    /// it on every paint would re-read and re-hash the layer's payloads each
    /// frame. Cleared when a different data identity is rendered, so a
    /// corrected payload is picked up on the next scene switch.
    field_failure: Option<(String, String)>,
    overlay_pipeline: Option<RenderPipeline>,
    overlay_source: Option<String>,
    pipeline_layout: wgpu::PipelineLayout,
    compiled_source: Option<String>,
    pipeline: Option<RenderPipeline>,
    target: Option<Target>,
    staging: Vec<Staging>,
    /// Sequence number for the next submission; slots are stamped with it so
    /// presentation order follows submission order, not ring index. Starts
    /// at 1: sequence 0 is never assigned, so the first submission clears
    /// the initial watermark instead of comparing equal to it and being
    /// rejected as stale.
    next_sequence: u64,
    /// Sequence of the newest frame handed to a caller for presentation.
    /// Completed frames at or below this watermark are stale — superseded by
    /// what was already shown — and are freed without presentation.
    presented_sequence: u64,
    /// The last successful submission, for the skip-unchanged check.
    last_submission: Option<Submitted>,
    adapter_summary: String,
}

impl Renderer {
    /// Creates the wgpu instance/adapter/device. Blocking; call once.
    #[soul(id = "interaction.sdf.render-frame", step = "device bring-up")]
    pub fn new() -> SdfResult<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|_| SdfError::no_adapter())?;

        let adapter_info = adapter.get_info();
        let adapter_summary = format!("{} ({:?})", adapter_info.name, adapter_info.backend);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("sdf-component"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(SdfError::device)?;

        device.on_uncaptured_error(Arc::new(|error| {
            let _ = writeln!(
                std::io::stderr(),
                "sdf-component: uncaptured wgpu error: {error}"
            );
        }));

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("sdf-bind-group-layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("sdf-uniforms"),
            size: UNIFORM_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let linear_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("sdf-field-linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("sdf-field-nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // The inert no-data binding: a 1×1 zero-filled stand-in so one bind
        // group shape serves both scene classes. Buffer memory backing
        // textures is zero-initialized, so the stub reads 0.0 everywhere
        // without an upload.
        let stub_texture = device.create_texture(&TextureDescriptor {
            label: Some("sdf-field-stub"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let stub_view = stub_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&stub_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&nearest_sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sdf-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        Ok(Self {
            device,
            queue,
            uniform_buffer,
            bind_group_layout,
            linear_sampler,
            nearest_sampler,
            stub_view,
            bind_group,
            field: None,
            field_key: None,
            field_failure: None,
            overlay_pipeline: None,
            overlay_source: None,
            pipeline_layout,
            compiled_source: None,
            pipeline: None,
            target: None,
            staging: Vec::new(),
            next_sequence: 1,
            presented_sequence: 0,
            last_submission: None,
            adapter_summary,
        })
    }

    /// One-line summary of the adapter this renderer runs on, for status UIs.
    pub fn adapter_summary(&self) -> &str {
        &self.adapter_summary
    }

    /// Binds the scene's field (or the inert stub) and compiles its module
    /// when the source changed. Returns whether this call (re)compiled the
    /// pipeline — the scene's rendering inputs changed since the last call.
    fn prepare_scene(&mut self, scene: &SdfScene) -> SdfResult<bool> {
        self.ensure_field(scene)?;
        self.ensure_overlay(scene)?;

        let module_source = scene.module_source();
        if self.compiled_source.as_deref() != Some(module_source.as_str()) {
            let pipeline = self.compile_pipeline("sdf-scene", &module_source, "fs_main", None)?;
            self.pipeline = Some(pipeline);
            self.compiled_source = Some(module_source);
            return Ok(true);
        }

        Ok(false)
    }

    /// Submits (or continues) the render for `request` and returns a finished
    /// CPU frame when one became available. Readback is asynchronous: the
    /// first call for a new scene or size usually returns `Ok(None)` while the
    /// GPU works; the caller keeps calling (the canvas drives this with
    /// animation frames) until a frame arrives.
    #[soul(id = "interaction.sdf.render-frame", step = "submit + readback")]
    pub fn render(
        &mut self,
        scene: &SdfScene,
        request: &FrameRequest,
    ) -> SdfResult<Option<Arc<RenderImage>>> {
        if scene.data_requested() && scene.data().is_none() {
            return Err(SdfError::data(
                "the scene carries `data`/`layer` directives but was not loaded from a file, \
                 so its manifest could not be resolved",
            ));
        }

        let recompiled = self.prepare_scene(scene)?;
        self.ensure_target(request.width, request.height)?;
        let unchanged = !recompiled
            && self.last_submission.as_ref().is_some_and(|submitted| {
                submitted.request == *request && submitted.data_key == scene.data_key()
            });
        if !unchanged {
            self.queue
                .write_buffer(&self.uniform_buffer, 0, &uniform_bytes(request));
            if self.submit_frame(request.width, request.height, request) {
                self.last_submission = Some(Submitted {
                    data_key: scene.data_key(),
                    request: *request,
                });
            }
        }
        self.advance_slots()?;

        self.build_presentable_frame(request.width, request.height)
    }

    /// Aligns the bound field texture with the scene's data: uploads the
    /// layer's tiles when the data identity changed, or rebinds the inert
    /// 1×1 stub when the scene carries none. A failed upload is recorded and
    /// re-raised on subsequent paints without re-reading the payloads.
    #[soul(id = "concept.sdf-scene-contract", step = "field texture + upload")]
    fn ensure_field(&mut self, scene: &SdfScene) -> SdfResult<()> {
        let key = scene.data_key();
        if let Some((failed_key, message)) = &self.field_failure
            && Some(failed_key) == key.as_ref()
        {
            return Err(SdfError::data(message));
        }
        // Reaching here with a recorded failure means the data identity
        // changed: the old record is stale and the next attempt is fresh.
        self.field_failure = None;
        if key == self.field_key {
            return Ok(());
        }

        match scene.data() {
            Some(data) => {
                let texture = match self.create_field_texture(data) {
                    Ok(texture) => texture,
                    Err(error) => {
                        let message = match &error {
                            SdfError::Data { message, .. } => String::from(message),
                            other => other.to_string(),
                        };
                        if let Some(key) = key {
                            self.field_failure = Some((key, message.clone()));
                        }
                        return Err(SdfError::data(&message));
                    }
                };
                let view = texture.create_view(&TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::D2Array),
                    ..Default::default()
                });
                self.bind_field(&view);
                self.field = Some(texture);
            }
            None => {
                let view = self.stub_view.clone();
                self.bind_field(&view);
                self.field = None;
            }
        }
        self.field_key = key;

        Ok(())
    }

    /// Rebuilds the bind group around `view`; the samplers and uniform
    /// binding are carried over unchanged.
    fn bind_field(&mut self, view: &TextureView) {
        self.bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.nearest_sampler),
                },
            ],
        });
    }

    /// Creates the field texture array for `data` and uploads every tile:
    /// read the payload, verify its sha256 against the manifest, check its
    /// length against the declared mip sizes, then write one texture level
    /// per mip. Stub tiles carry a constant-fill payload and expand to their
    /// grid region at every level. Anything above a device limit is rejected
    /// with the limit named, never truncated.
    fn create_field_texture(&self, data: &SceneData) -> SdfResult<Texture> {
        let limits = self.device.limits();
        let tile_edge = data.levels[0].size;
        if tile_edge > limits.max_texture_dimension_2d {
            return Err(SdfError::data(&format!(
                "layer `{}`: tile edge {tile_edge} exceeds the device limit max_texture_dimension_2d = {}",
                data.layer, limits.max_texture_dimension_2d
            )));
        }
        let layers = u64::from(data.grid) * u64::from(data.grid);
        if layers > u64::from(limits.max_texture_array_layers) {
            return Err(SdfError::data(&format!(
                "layer `{}`: the {}×{} tile grid needs {layers} texture array layers, exceeding the device limit max_texture_array_layers = {}",
                data.layer, data.grid, data.grid, limits.max_texture_array_layers
            )));
        }
        let layers = layers as u32;
        let level_count = data.level_count();
        let max_levels = tile_edge.ilog2() + 1;
        if level_count > max_levels {
            return Err(SdfError::data(&format!(
                "layer `{}`: the manifest declares {level_count} mip levels for a {tile_edge}² tile, which supports at most {max_levels}",
                data.layer
            )));
        }

        let guard = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("sdf-field"),
            size: Extent3d {
                width: tile_edge,
                height: tile_edge,
                depth_or_array_layers: layers,
            },
            mip_level_count: level_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        if let Some(error) = pollster::block_on(guard.pop()) {
            return Err(SdfError::data(&format!(
                "layer `{}`: field texture creation failed: {error}",
                data.layer
            )));
        }

        for tile in &data.tiles {
            let payload = std::fs::read(&tile.payload).map_err(|error| {
                SdfError::data(&format!(
                    "layer `{}`: tile `{}` at ({}, {}): payload could not be read: {error}",
                    data.layer,
                    tile.payload.display(),
                    tile.x,
                    tile.y
                ))
            })?;

            let actual = sha256_hex(&payload);
            if !actual.eq_ignore_ascii_case(&tile.sha256) {
                return Err(SdfError::data(&format!(
                    "layer `{}`: tile `{}` at ({}, {}): payload sha256 {actual} does not match the manifest's {}",
                    data.layer,
                    tile.payload.display(),
                    tile.x,
                    tile.y,
                    tile.sha256
                )));
            }

            if tile.stub {
                let Some(constant) = payload.first() else {
                    return Err(SdfError::data(&format!(
                        "layer `{}`: tile `{}` at ({}, {}): stub payload is empty",
                        data.layer,
                        tile.payload.display(),
                        tile.x,
                        tile.y
                    )));
                };
                if payload.iter().any(|byte| byte != constant) {
                    return Err(SdfError::data(&format!(
                        "layer `{}`: tile `{}` at ({}, {}): stub payload is not constant-fill",
                        data.layer,
                        tile.payload.display(),
                        tile.x,
                        tile.y
                    )));
                }
                for (index, level) in data.levels.iter().enumerate() {
                    let filled = vec![*constant; level.bytes as usize];
                    self.write_level(
                        &texture,
                        tile.y * data.grid + tile.x,
                        index as u32,
                        level.size,
                        &filled,
                    )?;
                }
            } else {
                let expected = data.tile_payload_len();
                if payload.len() as u64 != expected {
                    return Err(SdfError::data(&format!(
                        "layer `{}`: tile `{}` at ({}, {}): payload is {} bytes, the manifest's mip table declares {expected}",
                        data.layer,
                        tile.payload.display(),
                        tile.x,
                        tile.y,
                        payload.len()
                    )));
                }
                let mut offset = 0usize;
                for (index, level) in data.levels.iter().enumerate() {
                    let end = offset + level.bytes as usize;
                    let Some(level_bytes) = payload.get(offset..end) else {
                        return Err(SdfError::data(&format!(
                            "layer `{}`: tile `{}` at ({}, {}): payload ends before mip level {index}",
                            data.layer,
                            tile.payload.display(),
                            tile.x,
                            tile.y
                        )));
                    };
                    self.write_level(
                        &texture,
                        tile.y * data.grid + tile.x,
                        index as u32,
                        level.size,
                        level_bytes,
                    )?;
                    offset = end;
                }
            }
        }

        Ok(texture)
    }

    /// Writes one mip level of one tile into the field texture array, padding
    /// rows to the copy pitch when the level is narrower than 256 bytes.
    fn write_level(
        &self,
        texture: &Texture,
        layer: u32,
        level: u32,
        size: u32,
        bytes: &[u8],
    ) -> SdfResult<()> {
        let pitch = row_pitch(size);
        let mut padded;
        let source: &[u8] = if pitch == size {
            bytes
        } else {
            padded = vec![0u8; pitch as usize * size as usize];
            for row in 0..size as usize {
                let src = row * size as usize;
                padded[row * pitch as usize..row * pitch as usize + size as usize]
                    .copy_from_slice(&bytes[src..src + size as usize]);
            }
            &padded
        };

        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: TextureAspect::All,
            },
            source,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(size),
            },
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Compiles the contour overlay pipeline when a data texture is bound;
    /// recompiles when the field geometry changed. The overlay pass itself
    /// only runs while `params.y` enables it.
    fn ensure_overlay(&mut self, scene: &SdfScene) -> SdfResult<()> {
        if !self.field_key.is_some() {
            self.overlay_pipeline = None;
            self.overlay_source = None;
            return Ok(());
        }

        let (grid, levels) = scene
            .data()
            .map_or((1, 1), |data| (data.grid, data.level_count()));
        let source = overlay::module_source(grid, levels);
        if self.overlay_source.as_deref() == Some(source.as_str()) {
            return Ok(());
        }
        self.overlay_pipeline = Some(self.compile_pipeline(
            "sdf-overlay",
            &source,
            "fs_overlay",
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
            }),
        )?);
        self.overlay_source = Some(source);
        Ok(())
    }

    /// Compiles one WGSL module into a render pipeline over the shared bind
    /// group layout, capturing validation errors through an error scope
    /// instead of panicking.
    fn compile_pipeline(
        &self,
        label: &str,
        source: &str,
        fragment_entry: &str,
        blend: Option<wgpu::BlendState>,
    ) -> SdfResult<RenderPipeline> {
        let guard = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(label),
            source: ShaderSource::Wgsl(std::borrow::Cow::Owned(String::from(source))),
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        if let Some(error) = pollster::block_on(guard.pop()) {
            return Err(SdfError::scene_compile(&error.to_string()));
        }

        Ok(self
            .device
            .create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&self.pipeline_layout),
                vertex: VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(FragmentState {
                    module: &module,
                    entry_point: Some(fragment_entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba8Unorm,
                        blend,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            }))
    }

    fn ensure_target(&mut self, width: u32, height: u32) -> SdfResult<()> {
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.width == width && target.height == height)
        {
            return Ok(());
        }

        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("sdf-target"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.target = Some(Target {
            texture,
            width,
            height,
        });

        // Retire every staging buffer that cannot serve the new size. Only
        // `InFlight` slots hold a pending map; a `Mapped` slot's buffer was
        // already unmapped by `read_mapped`, so it drops like a free slot —
        // unmapping it again would emit a wgpu validation error.
        let mut still_needed = Vec::new();
        for slot in self.staging.drain(..) {
            let current_size = slot.width == width && slot.height == height;
            if current_size || matches!(slot.state, StagingState::InFlight) {
                still_needed.push(slot);
            }
        }
        self.staging = still_needed;
        while self.slots_for(width, height) < STAGING_SLOTS {
            self.staging.push(self.create_staging_slot(width, height)?);
        }

        Ok(())
    }

    /// Test hook for the size-mismatch retire path: submits a full ring of
    /// frames at `first` and parks them (advance without presenting, so every
    /// completed slot holds `Mapped` data whose buffer `read_mapped` already
    /// unmapped), then resizes into `second` inside a validation error scope.
    /// Returns the parked-slot count and the validation errors the resize
    /// raised — dropping a `Mapped` slot must not unmap it again.
    #[cfg(test)]
    pub(crate) fn resize_over_parked_frames(
        &mut self,
        scene: &SdfScene,
        first: &FrameRequest,
        second: &FrameRequest,
    ) -> SdfResult<(usize, Vec<String>)> {
        self.prepare_scene(scene)?;
        self.ensure_target(first.width, first.height)?;
        self.queue
            .write_buffer(&self.uniform_buffer, 0, &uniform_bytes(first));
        for _ in 0..STAGING_SLOTS {
            self.submit_frame(first.width, first.height, first);
        }
        for _ in 0..PARK_POLL_LIMIT {
            self.advance_slots()?;
            if !self
                .staging
                .iter()
                .any(|slot| matches!(slot.state, StagingState::InFlight))
            {
                break;
            }
            std::thread::sleep(PARK_POLL_PAUSE);
        }

        let parked = self
            .staging
            .iter()
            .filter(|slot| matches!(slot.state, StagingState::Mapped(_)))
            .count();

        let guard = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.ensure_target(second.width, second.height)?;
        let errors = match pollster::block_on(guard.pop()) {
            Some(error) => vec![error.to_string()],
            None => Vec::new(),
        };
        Ok((parked, errors))
    }

    fn create_staging_slot(&self, width: u32, height: u32) -> SdfResult<Staging> {
        let bytes_per_row = row_pitch(width * 4);
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("sdf-staging"),
            size: u64::from(bytes_per_row) * u64::from(height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Staging {
            buffer,
            width,
            height,
            bytes_per_row,
            state: StagingState::Free,
            mapped: None,
            sequence: 0,
        })
    }

    /// Submits the scene render — plus the contour overlay pass when a data
    /// texture is bound and the band width is on — and copies the result into
    /// one free staging slot of the current size. The overlay runs after the
    /// scene pass and before the copy, so captured frames carry it. Returns
    /// whether a submission actually happened; a ring with no free slot drops
    /// the frame and the caller must not record it as submitted.
    #[soul(id = "interaction.sdf.render-frame", step = "render pass + copy")]
    fn submit_frame(&mut self, width: u32, height: u32, request: &FrameRequest) -> bool {
        let Some(target) = self.target.as_ref() else {
            return false;
        };
        let Some(pipeline) = self.pipeline.as_ref() else {
            return false;
        };
        let Some(slot_index) = self.free_slot_index(width, height) else {
            return false;
        };
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let overlay_pipeline = self
            .overlay_pipeline
            .as_ref()
            .filter(|_| self.field_key.is_some() && request.params[1] > 0.0);

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("sdf-encoder"),
            });
        {
            let view = target
                .texture
                .create_view(&TextureViewDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("sdf-render-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: Operations {
                            load: LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            if let Some(overlay_pipeline) = overlay_pipeline {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("sdf-overlay-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: Operations {
                            load: LoadOp::Load,
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(overlay_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        let slot = &mut self.staging[slot_index];
        slot.sequence = sequence;
        encoder.copy_texture_to_buffer(
            target.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &slot.buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(slot.bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let _ = self.queue.submit([encoder.finish()]);

        let (sender, receiver) = mpsc::channel();
        slot.buffer.map_async(MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        slot.state = StagingState::InFlight;
        slot.mapped = Some(receiver);
        true
    }

    /// Polls the device and promotes completed mappings to `Mapped` (dropping
    /// data for sizes that no longer match the target).
    #[soul(id = "interaction.sdf.render-frame", step = "drain maps")]
    fn advance_slots(&mut self) -> SdfResult<()> {
        self.device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| SdfError::readback(&format!("{error}")))?;

        let Some(target) = self.target.as_ref() else {
            return Ok(());
        };

        for slot in &mut self.staging {
            if !matches!(slot.state, StagingState::InFlight) {
                continue;
            }
            let Some(receiver) = slot.mapped.as_mut() else {
                continue;
            };
            match receiver.try_recv() {
                Err(_) => {}
                Ok(Err(error)) => {
                    slot.mapped = None;
                    slot.state = StagingState::Free;
                    let _ = writeln!(
                        std::io::stderr(),
                        "sdf-component: staging map failed: {error}"
                    );
                }
                Ok(Ok(())) => {
                    let current_size = slot.width == target.width && slot.height == target.height;
                    let data = match slot.read_mapped() {
                        Ok(data) => data,
                        Err(error) => {
                            slot.mapped = None;
                            slot.state = StagingState::Free;
                            let _ = writeln!(
                                std::io::stderr(),
                                "sdf-component: staging read failed: {error}"
                            );
                            continue;
                        }
                    };
                    slot.mapped = None;
                    slot.state = if current_size {
                        StagingState::Mapped(data)
                    } else {
                        StagingState::Free
                    };
                }
            }
        }

        Ok(())
    }

    /// Converts the newest finished staging buffer of the current size into a
    /// `RenderImage` with BGRA-ordered bytes, as gpui's atlas expects.
    ///
    /// Presentation is forward-only: the newest completed frame is presented
    /// and every other completed frame of this size is freed as superseded —
    /// an older completion presented after a newer one would step the canvas
    /// backwards. Completed frames at or below the presented watermark are
    /// stale and freed without presentation.
    #[soul(id = "interaction.sdf.render-frame", step = "staging to BGRA")]
    fn build_presentable_frame(
        &mut self,
        width: u32,
        height: u32,
    ) -> SdfResult<Option<Arc<RenderImage>>> {
        let mut newest: Option<usize> = None;
        for (index, slot) in self.staging.iter().enumerate() {
            if slot.width != width
                || slot.height != height
                || slot.sequence <= self.presented_sequence
                || !matches!(slot.state, StagingState::Mapped(_))
            {
                continue;
            }
            let newer = match newest {
                None => true,
                Some(current) => slot.sequence > self.staging[current].sequence,
            };
            if newer {
                newest = Some(index);
            }
        }

        for (index, slot) in self.staging.iter_mut().enumerate() {
            if Some(index) == newest
                || slot.width != width
                || slot.height != height
                || !matches!(slot.state, StagingState::Mapped(_))
            {
                continue;
            }
            slot.state = StagingState::Free;
            slot.mapped = None;
        }

        let Some(slot_index) = newest else {
            return Ok(None);
        };

        let slot = &mut self.staging[slot_index];
        let StagingState::Mapped(data) = &slot.state else {
            return Ok(None);
        };
        let sequence = slot.sequence;
        let bytes = bgra_bytes(data, width, height, slot.bytes_per_row)
            .ok_or_else(|| SdfError::readback("staging data did not fill the frame"))?;
        let frame_buffer = RgbaImage::from_raw(width, height, bytes)
            .ok_or_else(|| SdfError::readback("frame buffer allocation failed"))?;

        slot.state = StagingState::Free;
        self.presented_sequence = sequence;

        Ok(Some(Arc::new(RenderImage::new(SmallVec::from_elem(
            Frame::new(frame_buffer),
            1,
        )))))
    }

    /// Frames submitted but not yet presented, across all sizes: in-flight
    /// copies plus completed-but-unpresented slots. The paint loop keeps
    /// going while this is non-zero, so the last frame presented for a
    /// gesture is always the newest one submitted.
    pub(crate) fn pending_frames(&self) -> usize {
        self.staging
            .iter()
            .filter(|slot| matches!(slot.state, StagingState::InFlight | StagingState::Mapped(_)))
            .count()
    }

    fn free_slot_index(&self, width: u32, height: u32) -> Option<usize> {
        self.staging.iter().position(|slot| {
            slot.width == width && slot.height == height && matches!(slot.state, StagingState::Free)
        })
    }

    fn slots_for(&self, width: u32, height: u32) -> usize {
        self.staging
            .iter()
            .filter(|slot| slot.width == width && slot.height == height)
            .count()
    }
}

fn row_pitch(bytes: u32) -> u32 {
    bytes.div_ceil(256) * 256
}

/// Reorders readback bytes from the RGBA render target into BGRA order and
/// strips row padding, producing tightly packed BGRA frames as expected by
/// gpui's `RenderImage`.
fn bgra_bytes(data: &[u8], width: u32, height: u32, bytes_per_row: u32) -> Option<Vec<u8>> {
    let row = (width * 4) as usize;
    let pitch = bytes_per_row as usize;
    let mut out = vec![0u8; row * height as usize];
    for y in 0..height as usize {
        let src = data.get(y * pitch..y * pitch + row)?;
        let dst = &mut out[y * row..(y + 1) * row];
        for (d, s) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
    }
    Some(out)
}

pub(crate) fn uniform_bytes(request: &FrameRequest) -> [u8; 48] {
    let mut bytes = [0u8; 48];
    let values = [
        request.width as f32,
        request.height as f32,
        request.time,
        request.width as f32 / request.height.max(1) as f32,
        request.mouse[0],
        request.mouse[1],
    ];
    for (index, value) in values.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[24..28].copy_from_slice(&request.view_center[0].to_le_bytes());
    bytes[28..32].copy_from_slice(&request.view_center[1].to_le_bytes());
    bytes[32..36].copy_from_slice(&request.params[0].to_le_bytes());
    bytes[36..40].copy_from_slice(&request.params[1].to_le_bytes());
    bytes[40..44].copy_from_slice(&request.view_zoom.to_le_bytes());
    bytes
}

/// Lowercase hex encoding of the sha256 digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
