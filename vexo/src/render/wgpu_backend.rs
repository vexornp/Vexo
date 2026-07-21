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
use crate::frame_builder::{FrameBuilder, OpKind};

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

/// Per-op rounded-rect clip data, uploaded to the GPU as a uniform.
///
/// Layout matches the WGSL `RClipUniform` struct in shader.wgsl and
/// image_shader.wgsl. Sized for `MAX_RCLIP_DEPTH` entries. Each op gets
/// its own slot in the uniform buffer at a dynamic offset.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RClipUniform {
    /// Number of active rclip entries (0..=8). Padded to vec4 alignment.
    count: [f32; 4],
    /// Bounds for each entry: (left, top, right, bottom) in logical pixels.
    bounds: [[f32; 4]; 8],
    /// Radii for each entry, packed two-per-vec4 for alignment.
    radii: [[f32; 4]; 2],
}

impl RClipUniform {
    /// All zeros — count=0 means "no rclip active" (shader fast path).
    const ZERO: Self = Self {
        count: [0.0; 4],
        bounds: [[0.0; 4]; 8],
        radii: [[0.0; 4]; 2],
    };

    fn from_entries(entries: &[(crate::frame_builder::Bounds, f32)]) -> Self {
        let mut u = Self::ZERO;
        let n = entries.len().min(8);
        u.count[0] = n as f32;
        for i in 0..n {
            let (b, r) = &entries[i];
            u.bounds[i] = [b.left, b.top, b.right, b.bottom];
            // Pack radii: indices 0-3 in radii[0], 4-7 in radii[1].
            u.radii[i / 4][i % 4] = *r;
        }
        u
    }
}

/// Dynamic offset alignment for the rclip uniform buffer.
/// wgpu requires uniform buffer offsets to be aligned to
/// `min_uniform_buffer_offset_alignment` (typically 256 bytes).
const RCLIP_UNIFORM_ALIGN: wgpu::BufferAddress = 256;

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

    // Current frame's op locations + clips, populated by upload_geometry.
    current_op_locations: Vec<crate::frame_builder::OpLocation>,
    current_op_clips: Vec<Option<crate::core::Bounds<crate::core::Logical>>>,
    // Per-op rounded-rect clip data
    rclip_uniform_buffer: wgpu::Buffer,
    rclip_bind_group_layout: wgpu::BindGroupLayout,
    rclip_bind_group: wgpu::BindGroup,
    /// Per-op dynamic offsets into rclip_uniform_buffer. Index aligns
    /// with current_op_locations. Offset 0 is always the ZERO slot.
    current_op_rclip_offsets: Vec<u32>,

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

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
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
                required_limits: adapter.limits(),
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

        let rclip_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("RClip Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(
                            (std::mem::size_of::<RClipUniform>() as u64).try_into().unwrap(),
                        ),
                    },
                    count: None,
                }],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[Some(&global_bind_group_layout), Some(&rclip_bind_group_layout)],
                immediate_size: 0,
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
            multiview_mask: None,
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

        const INITIAL_RCLIP_CAPACITY: usize = 1_000;
        let rclip_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RClip Uniform Buffer"),
            size: RCLIP_UNIFORM_ALIGN * INITIAL_RCLIP_CAPACITY as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let rclip_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RClip Bind Group"),
            layout: &rclip_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: rclip_uniform_buffer.as_entire_binding(),
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
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
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
                bind_group_layouts: &[Some(&global_bind_group_layout), Some(&image_atlas_bind_group_layout), Some(&rclip_bind_group_layout)],
                immediate_size: 0,
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
            multiview_mask: None,
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
            current_op_locations: Vec::new(),
            current_op_clips: Vec::new(),
            rclip_uniform_buffer,
            rclip_bind_group_layout,
            rclip_bind_group,
            current_op_rclip_offsets: Vec::new(),
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

    /// Upload geometry (quads + images) from frame builder to GPU buffers.
    /// Also records per-op typed-buffer locations for draw iteration.
    pub fn upload_geometry(&mut self, frame_builder: &FrameBuilder) {
        let op_locations = frame_builder.compute_op_locations();
        let op_clips: Vec<Option<crate::core::Bounds<crate::core::Logical>>> = frame_builder
            .ops()
            .iter()
            .map(|(_, clip, _)| *clip)
            .collect();

        let mut quad_instances: Vec<QuadInstance> = Vec::new();
        let mut image_instances: Vec<ImageInstance> = Vec::new();
        let atlas_size = [
            self.image_allocator.atlas_width() as f32,
            self.image_allocator.atlas_height() as f32,
        ];

        for (op, _, _) in frame_builder.ops() {
            match op {
                crate::frame_builder::DrawOp::Quad(q) => {
                    quad_instances.push(*q);
                }
                crate::frame_builder::DrawOp::Image(req) => {
                    let region = self
                        .image_allocator
                        .get_region(req.image_key)
                        .expect("Image key not found in atlas");
                    let instance = ImageInstance::from_logical(
                        req.position,
                        req.size,
                        region,
                        atlas_size,
                        req.corner_radius,
                        AffineTransform::from_array(req.transform),
                        req.opacity,
                    );
                    image_instances.push(instance);
                }
            }
        }

        if !quad_instances.is_empty() {
            self.ensure_instance_capacity(quad_instances.len());
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&quad_instances),
            );
        }
        if !image_instances.is_empty() {
            self.ensure_image_instance_capacity(image_instances.len());
            self.queue.write_buffer(
                &self.image_instance_buffer,
                0,
                bytemuck::cast_slice(&image_instances),
            );
        }

        self.current_op_locations = op_locations;
        self.current_op_clips = op_clips;

        // Compute per-op rclip offsets. Each op gets a slot in the
        // rclip uniform buffer. Ops with no rclip point to offset 0
        // (the ZERO slot). Ops with rclip data point to their slot.
        let mut rclip_offsets: Vec<u32> = Vec::with_capacity(frame_builder.ops().len());
        let mut next_slot: u32 = 1; // slot 0 is ZERO
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if rclip_snapshot.is_empty() {
                rclip_offsets.push(0);
            } else {
                rclip_offsets.push(next_slot * RCLIP_UNIFORM_ALIGN as u32);
                next_slot += 1;
            }
        }

        // Write the ZERO slot.
        self.queue.write_buffer(
            &self.rclip_uniform_buffer,
            0,
            bytemuck::bytes_of(&RClipUniform::ZERO),
        );

        // Write each non-zero op's rclip data.
        let mut slot: u32 = 1;
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if !rclip_snapshot.is_empty() {
                let uniform = RClipUniform::from_entries(rclip_snapshot);
                self.queue.write_buffer(
                    &self.rclip_uniform_buffer,
                    (slot as wgpu::BufferAddress) * RCLIP_UNIFORM_ALIGN,
                    bytemuck::bytes_of(&uniform),
                );
                slot += 1;
            }
        }

        self.current_op_rclip_offsets = rclip_offsets;
    }

    /// Execute the render pass, iterating `current_op_locations` in paint order.
    /// Scissor rect and pipeline are set only when they change between ops;
    /// each op draws exactly one instance at its typed-buffer index. Text is
    /// rendered last as a single full-viewport pass.
    pub fn execute_render_pass(
        &mut self,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), RenderError> {
        if !self.is_configured {
            return Err(RenderError::SurfaceNotConfigured);
        }

        let scale_factor = self.scale_source.get().factor();

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Texture acquired successfully, but the surface config is
                // suboptimal (e.g. after a resize). Render this frame; the
                // next SurfaceResized event will reconfigure.
                frame
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                // Window not fully on screen yet (common at startup).
                // Caller should retry next frame.
                return Err(RenderError::SurfaceTransient("Occluded".to_string()));
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                return Err(RenderError::SurfaceTransient("Timeout".to_string()));
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                // Surface config is stale — reconfigure and let the next
                // frame retry. SurfaceResized normally handles this, but
                // race conditions are possible.
                return Err(RenderError::SurfaceTransient("Outdated".to_string()));
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err(RenderError::AcquireFailed("Lost".to_string()));
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RenderError::AcquireFailed("Validation".to_string()));
            }
        };

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
                multiview_mask: None,
            });

            // Iterate ops in paint order. Set scissor + pipeline only on change.
            // prev_clip uses Option<Option<Bounds>> so the initial state (no
            // scissor ever set yet) is distinguishable from "scissor set to None
            // (full viewport)". This matters when the first op has clip == None:
            // we still need to set the scissor once.
            let mut prev_kind: Option<OpKind> = None;
            let mut prev_clip: Option<Option<crate::core::Bounds<crate::core::Logical>>> = None;

            for (loc, clip) in self.current_op_locations.iter().zip(self.current_op_clips.iter()) {
                // 1. Scissor: only set when clip changes.
                //    Compare Option<Bounds> by value via the Option<Option> sentinel.
                let clip_value = *clip;
                if prev_clip != Some(clip_value) {
                    match clip {
                        Some(c) => {
                            let x = (c.left * scale_factor).max(0.0) as u32;
                            let y = (c.top * scale_factor).max(0.0) as u32;
                            let right = (c.right * scale_factor).min(viewport_width as f32) as u32;
                            let bottom = (c.bottom * scale_factor).min(viewport_height as f32) as u32;
                            let w = right.saturating_sub(x);
                            let h = bottom.saturating_sub(y);
                            if w == 0 || h == 0 {
                                // Fully clipped — skip this op. Still advance prev_clip
                                // so we don't repeatedly re-set scissor for adjacent
                                // ops with the same degenerate clip.
                                prev_clip = Some(clip_value);
                                continue;
                            }
                            render_pass.set_scissor_rect(x, y, w, h);
                        }
                        None => {
                            render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                        }
                    }
                    prev_clip = Some(clip_value);
                }

                // 2. Pipeline: only switch when op kind changes.
                let kind = loc.kind();
                if Some(kind) != prev_kind {
                    match kind {
                        OpKind::Quad => {
                            render_pass.set_pipeline(&self.render_pipeline);
                            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
                            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                        }
                        OpKind::Image => {
                            render_pass.set_pipeline(&self.image_pipeline);
                            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
                            render_pass.set_bind_group(1, &self.image_atlas_bind_group, &[]);
                            render_pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
                            render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(..));
                        }
                    }
                    prev_kind = Some(kind);
                }

                // 3. Draw one instance. Index buffer is per-pipeline (same indices 0..6).
                match kind {
                    OpKind::Quad => {
                        render_pass.set_index_buffer(
                            self.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                    }
                    OpKind::Image => {
                        render_pass.set_index_buffer(
                            self.image_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                    }
                }
                let idx = match loc {
                    crate::frame_builder::OpLocation::Quad { index } => *index,
                    crate::frame_builder::OpLocation::Image { index } => *index,
                };
                render_pass.draw_indexed(0..6, 0, idx..idx + 1);
            }

            // Text pass — full-viewport scissor, unchanged.
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
        // The trait method is not used by the flat-op pipeline.
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
