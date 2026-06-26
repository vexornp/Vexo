//! WGPU-based render backend implementation.
//!
//! This module provides a production-ready GPU rendering backend using wgpu.

use std::sync::Arc;

use glyphon::{FontSystem, Viewport};
use wgpu::util::DeviceExt;

use crate::core::{AffineTransform, Color, Physical, ScaleSource, Size};
use crate::image_atlas::{AtlasRegion, ImageKey, ShelfAllocator};
use crate::image_data::ImageData;
use crate::image_instance::ImageInstance;
use crate::quad_instance::QuadInstance;
use crate::render::backend::{RenderBackend, RenderConfig, RenderError};
use crate::frame_builder::{ClipGroup, DrawRange, FrameBuilder};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Vertex for image quads (2D position, matching image_shader.wgsl model_pos).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageVertex {
    pos: [f32; 2],
}

impl ImageVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Global uniforms passed to shaders.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalUniforms {
    pub screen_size: [f32; 2],
    scale_factor: f32,
    pub _padding: f32,
}

/// WGPU-based render backend.
///
/// Encapsulates all GPU resources and rendering operations.
pub struct WgpuBackend {
    // Core wgpu resources
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    is_configured: bool,

    // Rendering pipeline
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    global_uniform_buffer: wgpu::Buffer,
    global_bind_group: wgpu::BindGroup,

    // Text rendering
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
    viewport: glyphon::Viewport,
    #[allow(dead_code)]
    cache: glyphon::Cache,

    // Image rendering
    image_pipeline: wgpu::RenderPipeline,
    image_vertex_buffer: wgpu::Buffer,
    image_index_buffer: wgpu::Buffer,
    image_instance_buffer: wgpu::Buffer,
    image_instance_buffer_capacity: usize,
    image_atlas_bind_group: wgpu::BindGroup,
    image_atlas_texture: wgpu::Texture,
    image_allocator: ShelfAllocator,

    // Current configuration
    current_config: Option<RenderConfig>,

    // Shared scale factor source
    scale_source: ScaleSource,

    // Clear color
    clear_color: wgpu::Color,
}

impl WgpuBackend {
    /// Create a new WGPU backend with a window.
    pub async fn new(window: Arc<dyn winit::window::Window>) -> anyhow::Result<Self> {
        let size = window.surface_size();
        let scale_factor = window.scale_factor();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();

        let physical_size = Size::<Physical>::new(size.width as f32, size.height as f32);
        let scale_source = ScaleSource::new(scale_factor);

        Self::init(surface, instance, physical_size, scale_source).await
    }

    async fn init(
        surface: wgpu::Surface<'static>,
        instance: wgpu::Instance,
        physical_size: Size<Physical>,
        scale_source: ScaleSource,
    ) -> anyhow::Result<Self> {
        let scale_factor = scale_source.get().factor();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::wgt::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: physical_size.width_u32(),
            height: physical_size.height_u32(),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // Create shader and pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(crate::resource::file::WGSL.into()),
        });

        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Global Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&global_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Vertex::desc(),
                    QuadInstance::desc(),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create buffers
        const QUAD_VERTICES: &[Vertex] = &[
            Vertex { pos: [0.0, 0.0, 0.0] },
            Vertex { pos: [1.0, 0.0, 0.0] },
            Vertex { pos: [1.0, 1.0, 0.0] },
            Vertex { pos: [0.0, 1.0, 0.0] },
        ];

        const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        const INITIAL_INSTANCE_CAPACITY: usize = 1_000;

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (std::mem::size_of::<QuadInstance>() * INITIAL_INSTANCE_CAPACITY) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let global_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Global Uniform Buffer"),
            size: std::mem::size_of::<GlobalUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Global Bind Group"),
            layout: &global_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: global_uniform_buffer.as_entire_binding(),
            }],
        });

        // Create image pipeline
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Shader"),
            source: wgpu::ShaderSource::Wgsl(crate::resource::file::IMAGE_WGSL.into()),
        });

        const ATLAS_SIZE: u32 = 2048;

        let image_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Atlas Texture"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let image_atlas_view = image_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let image_atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Image Atlas Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let image_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image Atlas Bind Group"),
            layout: &image_atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&image_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&image_sampler),
                },
            ],
        });

        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[&global_bind_group_layout, &image_atlas_bind_group_layout],
                push_constant_ranges: &[],
            });

        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    ImageVertex::desc(),
                    ImageInstance::desc(),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Image quad vertices (2D, matching image_shader.wgsl model_pos)
        const IMAGE_QUAD_VERTICES: &[ImageVertex] = &[
            ImageVertex { pos: [0.0, 0.0] },
            ImageVertex { pos: [1.0, 0.0] },
            ImageVertex { pos: [1.0, 1.0] },
            ImageVertex { pos: [0.0, 1.0] },
        ];

        const IMAGE_QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

        let image_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Image Vertex Buffer"),
            contents: bytemuck::cast_slice(IMAGE_QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let image_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Image Index Buffer"),
            contents: bytemuck::cast_slice(IMAGE_QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        const INITIAL_IMAGE_INSTANCE_CAPACITY: usize = 100;

        let image_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Image Instance Buffer"),
            size: (std::mem::size_of::<ImageInstance>() * INITIAL_IMAGE_INSTANCE_CAPACITY) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let image_allocator = ShelfAllocator::new(ATLAS_SIZE, ATLAS_SIZE);

        // Initialize glyphon
        let cache = glyphon::Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = glyphon::TextAtlas::new(&device, &queue, &cache, config.format);
        let text_renderer = glyphon::TextRenderer::new(
            &mut atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        // Configure surface if we have valid dimensions
        let is_configured = if physical_size.width > 0.0 && physical_size.height > 0.0 {
            surface.configure(&device, &config);
            true
        } else {
            false
        };

        // Write initial uniforms
        let uniform = GlobalUniforms {
            screen_size: physical_size.to_array(),
            scale_factor,
            _padding: 0.0,
        };
        queue.write_buffer(&global_uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        Ok(Self {
            device,
            queue,
            surface,
            config,
            is_configured,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_buffer_capacity: INITIAL_INSTANCE_CAPACITY,
            global_uniform_buffer,
            global_bind_group,
            atlas,
            text_renderer,
            viewport,
            cache,
            image_pipeline,
            image_vertex_buffer,
            image_index_buffer,
            image_instance_buffer,
            image_instance_buffer_capacity: INITIAL_IMAGE_INSTANCE_CAPACITY,
            image_atlas_bind_group,
            image_atlas_texture,
            image_allocator,
            current_config: Some(RenderConfig::new(physical_size)),
            scale_source,
            clear_color: Color::WHITE.to_wgpu_color(),
        })
    }

    /// Get a reference to the device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get a mutable reference to the device.
    pub fn device_mut(&mut self) -> &mut wgpu::Device {
        &mut self.device
    }

    /// Get a reference to the queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get a mutable reference to the queue.
    pub fn queue_mut(&mut self) -> &mut wgpu::Queue {
        &mut self.queue
    }

    /// Get a reference to the text atlas.
    pub fn atlas(&self) -> &glyphon::TextAtlas {
        &self.atlas
    }

    /// Get a mutable reference to the text atlas.
    pub fn atlas_mut(&mut self) -> &mut glyphon::TextAtlas {
        &mut self.atlas
    }

    /// Get a reference to the text renderer.
    pub fn text_renderer(&self) -> &glyphon::TextRenderer {
        &self.text_renderer
    }

    /// Get a mutable reference to the text renderer.
    pub fn text_renderer_mut(&mut self) -> &mut glyphon::TextRenderer {
        &mut self.text_renderer
    }

    /// Get a reference to the viewport.
    pub fn viewport(&self) -> &glyphon::Viewport {
        &self.viewport
    }

    /// Get a mutable reference to the viewport.
    pub fn viewport_mut(&mut self) -> &mut glyphon::Viewport {
        &mut self.viewport
    }

    /// Update viewport resolution.
    pub fn update_viewport(&mut self, size: Size<Physical>) {
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: size.width_u32(),
                height: size.height_u32(),
            },
        );
    }

    /// Get the surface format.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Get the current width in physical pixels.
    pub fn width(&self) -> u32 {
        self.config.width
    }

    /// Get the current height in physical pixels.
    pub fn height(&self) -> u32 {
        self.config.height
    }

    /// Set the clear color.
    pub fn set_clear_color(&mut self, color: Color) {
        self.clear_color = color.to_wgpu_color();
    }

    /// Get the current render config, if available.
    pub fn current_config(&self) -> Option<&RenderConfig> {
        self.current_config.as_ref()
    }

    /// Get a clone of the scale source for distribution.
    pub fn scale_source(&self) -> ScaleSource {
        self.scale_source.clone()
    }

    /// Prepare text rendering.
    pub fn prepare_text(
        &mut self,
        font_system: &mut FontSystem,
        text_areas: Vec<glyphon::TextArea>,
    ) {
        let mut swash_cache = glyphon::SwashCache::new();
        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
                &mut swash_cache,
            )
            .unwrap();
    }

    /// Ensure the instance buffer can hold `required` instances.
    /// Grows the buffer by doubling capacity if needed.
    fn ensure_instance_capacity(&mut self, required: usize) {
        if required <= self.instance_buffer_capacity {
            return;
        }

        let new_capacity = required.max(self.instance_buffer_capacity * 2);
        let new_size = (std::mem::size_of::<QuadInstance>() * new_capacity) as wgpu::BufferAddress;

        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: new_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_buffer_capacity = new_capacity;
    }

    /// Register an image in the atlas. Returns an ImageKey for future reference.
    pub fn register_image(&mut self, image_data: &ImageData) -> ImageKey {
        let (key, region) = self.image_allocator.allocate(image_data.width, image_data.height);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.image_atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: region.x, y: region.y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &image_data.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image_data.width * 4),
                rows_per_image: Some(image_data.height),
            },
            wgpu::Extent3d {
                width: image_data.width,
                height: image_data.height,
                depth_or_array_layers: 1,
            },
        );
        key
    }

    /// Unregister an image from the atlas.
    pub fn unregister_image(&mut self, key: ImageKey) {
        self.image_allocator.remove(key);
    }

    /// Get the atlas region for a registered image.
    pub fn get_image_region(&self, key: ImageKey) -> Option<&AtlasRegion> {
        self.image_allocator.get_region(key)
    }

    /// Ensure the image instance buffer can hold `required` instances.
    fn ensure_image_instance_capacity(&mut self, required: usize) {
        if required <= self.image_instance_buffer_capacity {
            return;
        }
        let new_capacity = required.max(self.image_instance_buffer_capacity * 2);
        self.image_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Image Instance Buffer"),
            size: (std::mem::size_of::<ImageInstance>() * new_capacity) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.image_instance_buffer_capacity = new_capacity;
    }

    /// Upload image geometry from frame builder to GPU buffers.
    pub fn upload_image_geometry(&mut self, frame_builder: &FrameBuilder) {
        let (requests, _ranges) = frame_builder.flatten_image_requests();
        if requests.is_empty() { return; }
        let atlas_size = [self.image_allocator.atlas_width() as f32, self.image_allocator.atlas_height() as f32];
        let instances: Vec<ImageInstance> = requests.iter().map(|req| {
            let region = self.image_allocator.get_region(req.image_key).expect("Image key not found in atlas");
            ImageInstance::from_logical(req.position, req.size, region, atlas_size, req.corner_radius, AffineTransform::from_array(req.transform), req.opacity)
        }).collect();
        self.ensure_image_instance_capacity(instances.len());
        self.queue.write_buffer(&self.image_instance_buffer, 0, bytemuck::cast_slice(&instances));
    }

    /// Upload geometry from frame builder to GPU buffers.
    pub fn upload_geometry(&mut self, frame_builder: &FrameBuilder) {
        let flattened = frame_builder.flatten_quads();
        if !flattened.instances.is_empty() {
            self.ensure_instance_capacity(flattened.instances.len());
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&flattened.instances),
            );
        }
    }

    /// Execute the render pass with per-clip-group scissor rects and draw calls.
    pub fn execute_render_pass(
        &mut self,
        clip_groups: &[ClipGroup],
        draw_ranges: &[DrawRange],
        image_draw_ranges: &[DrawRange],
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), RenderError> {
        if !self.is_configured {
            return Err(RenderError::SurfaceNotConfigured);
        }

        let scale_factor = self.scale_source.get().factor();

        let output = self.surface.get_current_texture()
            .map_err(|e| RenderError::AcquireFailed(format!("{:?}", e)))?;

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

            // Draw quads per clip group with appropriate scissor rect
            for (group, range) in clip_groups.iter().zip(draw_ranges.iter()) {
                if range.count == 0 { continue; }

                // Set scissor rect for this clip group
                if let Some(clip) = &group.clip_bounds {
                    let x = (clip.left * scale_factor).max(0.0) as u32;
                    let y = (clip.top * scale_factor).max(0.0) as u32;
                    let right = (clip.right * scale_factor).min(viewport_width as f32) as u32;
                    let bottom = (clip.bottom * scale_factor).min(viewport_height as f32) as u32;
                    let w = right.saturating_sub(x);
                    let h = bottom.saturating_sub(y);
                    if w == 0 || h == 0 { continue; } // Fully clipped, skip
                    render_pass.set_scissor_rect(x, y, w, h);
                } else {
                    // No clip: scissor defaults to full viewport
                    render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                }

                render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, range.first_instance..range.first_instance + range.count);
            }

            // Draw image quads per clip group with appropriate scissor rect
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
            render_pass.set_bind_group(1, &self.image_atlas_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(..));

            for (group, range) in clip_groups.iter().zip(image_draw_ranges.iter()) {
                if range.count == 0 { continue; }

                // Set scissor rect for this clip group
                if let Some(clip) = &group.clip_bounds {
                    let x = (clip.left * scale_factor).max(0.0) as u32;
                    let y = (clip.top * scale_factor).max(0.0) as u32;
                    let right = (clip.right * scale_factor).min(viewport_width as f32) as u32;
                    let bottom = (clip.bottom * scale_factor).min(viewport_height as f32) as u32;
                    let w = right.saturating_sub(x);
                    let h = bottom.saturating_sub(y);
                    if w == 0 || h == 0 { continue; }
                    render_pass.set_scissor_rect(x, y, w, h);
                } else {
                    render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                }

                render_pass.set_index_buffer(self.image_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, range.first_instance..range.first_instance + range.count);
            }

            // Render all text with full-viewport scissor.
            // Text clipping is handled by glyphon's TextArea.bounds per-request,
            // not by GPU scissor rects (glyphon's prepare() replaces its vertex
            // buffer on each call, so per-clip-group prepare+render is not possible
            // within a single render pass).
            render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut render_pass)
                .map_err(|e| RenderError::TextPrepareFailed(format!("{:?}", e)))?;
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.atlas.trim();

        Ok(())
    }
}

impl RenderBackend for WgpuBackend {
    fn prepare(
        &mut self,
        frame_builder: &mut FrameBuilder,
        _font_system: &mut FontSystem,
        config: RenderConfig,
    ) {
        self.current_config = Some(config.clone());

        let scale_factor = self.scale_source.get().factor();
        let uniform = GlobalUniforms {
            screen_size: config.screen_size_array(),
            scale_factor,
            _padding: 0.0,
        };
        self.queue.write_buffer(&self.global_uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        self.upload_geometry(frame_builder);

        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: config.width(),
                height: config.height(),
            },
        );
    }

    fn render(&mut self) -> Result<(), RenderError> {
        // The trait method is not used by the clip-group-based pipeline.
        // TextPipeline::execute_render() calls execute_render_pass() directly.
        Ok(())
    }

    fn resize(&mut self, config: RenderConfig) {
        let width = config.width();
        let height = config.height();
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_configured = true;
            self.current_config = Some(config.clone());

            let scale_factor = self.scale_source.get().factor();
            let uniform = GlobalUniforms {
                screen_size: config.screen_size_array(),
                scale_factor,
                _padding: 0.0,
            };
            self.queue.write_buffer(&self.global_uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        }
    }

    fn is_ready(&self) -> bool {
        self.is_configured
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_uniforms_size() {
        // Ensure the struct is properly aligned for GPU
        assert_eq!(std::mem::size_of::<GlobalUniforms>(), 16);
    }
}
