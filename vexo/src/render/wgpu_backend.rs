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
use crate::frame_builder::{DrawOp, FrameBuilder, OpKind};

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

/// Depth buffer format used for GPU depth testing (paint-order occlusion).
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Number of slots in the rclip uniform buffer. Slot 0 is the ZERO slot
/// (no rclip); slots 1..INITIAL_RCLIP_CAPACITY hold per-op rclip data.
/// Ops beyond this capacity fall back to the ZERO slot with a warning.
const INITIAL_RCLIP_CAPACITY: usize = 1_000;

/// Maximum number of SaveLayer composite draws per frame. Each composite
/// uses one slot in the image instance buffer beyond the image instance
/// count. Sized for v1 (1-2 groups typical); excess groups log and skip.
const MAX_COMPOSITE_QUADS_PER_FRAME: usize = 8;

#[derive(Clone, Copy)]
enum SaveLayerMarkerKind {
    Begin,
    End,
}

#[derive(Clone, Copy)]
struct SaveLayerMarkerInfo {
    index: usize,
    kind: SaveLayerMarkerKind,
    bounds: crate::core::Bounds<crate::core::Logical>,
    opacity: f32,
    z: f32,
}

/// Resources that must outlive the main encoder's submit. Each composite
/// draw records bind group / sampler / texture references into the main
/// encoder; wgpu destroys these resources when their Rust handles drop,
/// so we hold them here until the encoder is submitted (cleared at the
/// start of the next frame's `execute_render_pass`).
struct PendingOffscreenTextures {
    _color_tex: wgpu::Texture,
    _depth_tex: wgpu::Texture,
    _color_view: wgpu::TextureView,
    _depth_view: wgpu::TextureView,
}

/// Bind group + sampler created per composite draw. Kept alive until the
/// parent encoder is submitted.
struct PendingCompositeBinds {
    _bind_group: wgpu::BindGroup,
    _sampler: wgpu::Sampler,
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
    transparent_render_pipeline: wgpu::RenderPipeline,
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
    image_atlas_bind_group_layout: wgpu::BindGroupLayout,
    image_atlas_texture: wgpu::Texture,
    image_allocator: ShelfAllocator,

    // Current configuration
    current_config: Option<RenderConfig>,

    // Depth buffer for paint-order occlusion (text vs geometry).
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,

    // Current frame's op locations + clips, populated by upload_geometry.
    current_op_locations: Vec<crate::frame_builder::OpLocation>,
    current_op_clips: Vec<Option<crate::core::Bounds<crate::core::Logical>>>,
    // Per-op rounded-rect clip data
    rclip_uniform_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    rclip_bind_group_layout: wgpu::BindGroupLayout,
    rclip_bind_group: wgpu::BindGroup,
    /// Per-op dynamic offsets into rclip_uniform_buffer. Index aligns
    /// with current_op_locations. Offset 0 is always the ZERO slot.
    current_op_rclip_offsets: Vec<u32>,

    // Shared scale factor source
    scale_source: ScaleSource,

    // Clear color
    clear_color: wgpu::Color,

    /// Pool of per-group TextRenderers, sharing the main atlas + font system.
    /// Grows to the max concurrent groups seen. Reused across frames.
    group_text_renderers: Vec<glyphon::TextRenderer>,
    /// Pool of per-group Viewports (one per group, sized to group bounds).
    group_viewports: Vec<glyphon::Viewport>,

    /// SaveLayer marker positions in `current_op_locations`, populated by
    /// `upload_geometry`. Scanned by `render_range` to delimit groups.
    current_save_layer_markers: Vec<SaveLayerMarkerInfo>,

    /// Resources held alive until the main encoder is submitted.
    /// Populated by `render_save_layer_group` (offscreen textures + views)
    /// and `draw_composite_quad` (bind group + sampler). Cleared at the
    /// start of the next `execute_render_pass`.
    pending_offscreen_textures: Vec<PendingOffscreenTextures>,
    pending_composite_binds: Vec<PendingCompositeBinds>,

    /// Number of image instances written to `image_instance_buffer` this
    /// frame. Composite quads are appended after this offset.
    image_instance_count: u32,
    /// Composite draws issued this frame, for dynamic offset into
    /// `image_instance_buffer`. Reset at the start of `execute_render_pass`.
    composite_quad_count: u32,
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
                apply_limit_buckets: false,
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
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: physical_size.width_u32(),
            height: physical_size.height_u32(),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // Create depth texture for paint-order occlusion.
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Shared depth-stencil state for all pipelines (quad, image, text).
        let depth_stencil_state = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
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
                    Some(Vertex::desc()),
                    Some(QuadInstance::desc()),
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
            depth_stencil: Some(depth_stencil_state.clone()),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Transparent quad pipeline: identical to the opaque pipeline but with
        // depth-write disabled. Semi-transparent quads (fill alpha < 1.0, e.g.
        // the context-menu dim barrier) render and blend normally but do NOT
        // write their depth to the depth buffer. This prevents them from
        // occluding text that is rendered in the later glyphon text pass.
        // Without this, a full-screen dim quad at z≈0.996 would write that
        // depth to every pixel, causing background text at z≈0.997 to fail
        // the LessEqual depth test and disappear.
        let transparent_depth_stencil = wgpu::DepthStencilState {
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            ..depth_stencil_state.clone()
        };
        let transparent_render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Transparent Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[
                        Some(Vertex::desc()),
                        Some(QuadInstance::desc()),
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
                depth_stencil: Some(transparent_depth_stencil),
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
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &rclip_uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<RClipUniform>() as u64),
                }),
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
                    Some(ImageVertex::desc()),
                    Some(ImageInstance::desc()),
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
            depth_stencil: Some(depth_stencil_state.clone()),
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
            Some(depth_stencil_state),
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
            transparent_render_pipeline,
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
            image_atlas_bind_group_layout,
            image_atlas_texture,
            image_allocator,
            current_config: Some(RenderConfig::new(physical_size)),
            depth_texture,
            depth_texture_view,
            current_op_locations: Vec::new(),
            current_op_clips: Vec::new(),
            rclip_uniform_buffer,
            rclip_bind_group_layout,
            rclip_bind_group,
            current_op_rclip_offsets: Vec::new(),
            scale_source,
            clear_color: Color::WHITE.to_wgpu_color(),
            group_text_renderers: Vec::new(),
            group_viewports: Vec::new(),
            current_save_layer_markers: Vec::new(),
            pending_offscreen_textures: Vec::new(),
            pending_composite_binds: Vec::new(),
            image_instance_count: 0,
            composite_quad_count: 0,
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

    /// Get or create a pooled TextRenderer for save-layer group `index`.
    /// All group TextRenderers share the main atlas and font system.
    fn group_text_renderer(&mut self, index: usize) -> &mut glyphon::TextRenderer {
        while self.group_text_renderers.len() <= index {
            let renderer = glyphon::TextRenderer::new(
                &mut self.atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
            );
            self.group_text_renderers.push(renderer);
        }
        &mut self.group_text_renderers[index]
    }

    /// Get or create a pooled Viewport for save-layer group `index`.
    fn group_viewport(&mut self, index: usize) -> &mut glyphon::Viewport {
        while self.group_viewports.len() <= index {
            let viewport = glyphon::Viewport::new(&self.device, &self.cache);
            self.group_viewports.push(viewport);
        }
        &mut self.group_viewports[index]
    }

    /// Recursively render ops in `current_op_locations[start..end)` into
    /// `render_pass` using the standard three-phase pipeline (opaque →
    /// text → transparent).
    ///
    /// SaveLayer groups encountered in this range are rendered offscreen
    /// (via `render_save_layer_group`) and composited back as textured
    /// quads in paint order during Phase 3, interleaved with transparent
    /// quads that precede each group.
    ///
    /// `group_text_renderer_idx`: 0 = use the main `text_renderer` (main
    /// surface pass). >0 = use the pooled group text renderer at index
    /// `group_text_renderer_idx - 1`. Per-group text preparation is not
    /// wired up in v1; the pool slot exists but renders nothing until
    /// a later task calls `prepare` on it.
    ///
    /// Offscreen textures are surface-sized (`viewport_width × viewport_height`)
    /// so op positions need no translation — every pass renders at window-
    /// absolute coords. The composite quad samples only the group's bounds
    /// sub-region via UV coordinates.
    #[allow(clippy::too_many_arguments)]
    fn render_range(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        start: usize,
        end: usize,
        group_text_renderer_idx: usize,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) {
        // ── Scan markers for top-level SaveLayer groups in this range ──
        // Nested groups (a Begin inside another Begin within this range)
        // are NOT recorded here — they're handled by the recursive call
        // inside `render_save_layer_group`. We only composite top-level
        // groups in THIS pass.
        let mut save_layer_ranges: Vec<
            (usize, usize, crate::core::Bounds<crate::core::Logical>, f32, f32),
        > = Vec::new();
        let mut begin_stack: Vec<usize> = Vec::new();
        for (mi, marker) in self.current_save_layer_markers.iter().enumerate() {
            if marker.index < start || marker.index >= end {
                continue;
            }
            match marker.kind {
                SaveLayerMarkerKind::Begin => begin_stack.push(mi),
                SaveLayerMarkerKind::End => {
                    if let Some(begin_mi) = begin_stack.pop() {
                        // Only top-level groups (stack empty after pop) are
                        // composited in this pass. Nested groups are rendered
                        // by the recursive call.
                        if begin_stack.is_empty() {
                            let begin = &self.current_save_layer_markers[begin_mi];
                            save_layer_ranges.push((
                                begin.index + 1,
                                marker.index,
                                begin.bounds,
                                begin.opacity,
                                begin.z,
                            ));
                        }
                    }
                }
            }
        }
        // Sort by gstart so we can iterate groups in paint order during
        // both op classification and Phase 3 interleaving.
        save_layer_ranges.sort_by_key(|(gstart, _, _, _, _)| *gstart);

        // ── Classify ops, skipping SaveLayer group ranges ──
        let mut opaque_indices: Vec<usize> = Vec::new();
        let mut transparent_indices: Vec<usize> = Vec::new();
        let mut next_group = 0usize;
        let mut i = start;
        while i < end {
            // If `i` is the Begin marker of the next top-level group,
            // skip the whole group (it's rendered offscreen by recursion).
            if next_group < save_layer_ranges.len() {
                let (gstart, gend, _, _, _) = save_layer_ranges[next_group];
                if i == gstart - 1 {
                    i = gend + 1; // skip past EndSaveLayer marker
                    next_group += 1;
                    continue;
                }
            }
            let loc = self.current_op_locations[i];
            match loc.kind() {
                OpKind::Quad | OpKind::Image => opaque_indices.push(i),
                OpKind::TransparentQuad => transparent_indices.push(i),
                OpKind::SaveLayerMarker => {} // unmatched End marker — skip
            }
            i += 1;
        }

        // ── Phase 1: Opaque quads + images ──
        let mut prev_kind: Option<OpKind> = None;
        let mut prev_clip: Option<Option<crate::core::Bounds<crate::core::Logical>>> = None;
        let mut prev_rclip_offset_per_slot: [Option<u32>; 2] = [None, None];
        for &i in &opaque_indices {
            self.draw_op_in_pass(
                render_pass,
                i,
                &mut prev_kind,
                &mut prev_clip,
                &mut prev_rclip_offset_per_slot,
                scale_factor,
                viewport_width,
                viewport_height,
            );
        }

        // ── Phase 2: Text ──
        render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
        if group_text_renderer_idx == 0 {
            let _ = self.text_renderer.render(&self.atlas, &self.viewport, render_pass);
        } else {
            // Group pass — use the pooled group text renderer. The slot
            // must already exist (created lazily in render_save_layer_group
            // before recursing). If it doesn't, skip text for safety.
            let idx = group_text_renderer_idx - 1;
            if idx < self.group_text_renderers.len() && idx < self.group_viewports.len() {
                let _ = self.group_text_renderers[idx]
                    .render(&self.atlas, &self.group_viewports[idx], render_pass);
            }
        }

        // ── Phase 3: Transparent quads + save-layer composites (paint order) ──
        prev_kind = None;
        prev_clip = None;
        prev_rclip_offset_per_slot = [None, None];

        let mut transparent_iter = transparent_indices.iter().peekable();
        for (gi, &(gstart, gend, bounds, opacity, z)) in save_layer_ranges.iter().enumerate() {
            // Draw transparent quads that come before this group in paint order.
            while let Some(&&ti) = transparent_iter.peek() {
                if ti < gstart {
                    self.draw_op_in_pass(
                        render_pass,
                        ti,
                        &mut prev_kind,
                        &mut prev_clip,
                        &mut prev_rclip_offset_per_slot,
                        scale_factor,
                        viewport_width,
                        viewport_height,
                    );
                    transparent_iter.next();
                } else {
                    break;
                }
            }

            // Render the group offscreen. `group_text_renderer_idx` for the
            // recursive call is `gi + 1` (1-based; 0 = main pass). Each
            // top-level group in this pass gets its own pool slot; nested
            // groups inside the recursive call reuse the same slot indexing
            // scheme — collisions are acceptable in v1 since group text
            // preparation isn't wired up yet.
            let group_text_idx = gi + 1;
            let group_view = self.render_save_layer_group(
                gstart,
                gend,
                bounds,
                group_text_idx,
                scale_factor,
                viewport_width,
                viewport_height,
            );

            // Composite the offscreen result into this pass at the group's
            // paint-order z-depth.
            self.draw_composite_quad(
                render_pass,
                &group_view,
                bounds,
                opacity,
                z,
                scale_factor,
                viewport_width,
                viewport_height,
            );
        }

        // Draw remaining transparent quads after all groups.
        for &ti in transparent_iter {
            self.draw_op_in_pass(
                render_pass,
                ti,
                &mut prev_kind,
                &mut prev_clip,
                &mut prev_rclip_offset_per_slot,
                scale_factor,
                viewport_width,
                viewport_height,
            );
        }
    }

    /// Render a SaveLayer group offscreen and return the color view holding
    /// the result. The caller is responsible for compositing the view into
    /// the parent pass via `draw_composite_quad`.
    ///
    /// Creates a surface-sized offscreen target (no coordinate translation
    /// needed — ops render at window-absolute positions), records the group's
    /// ops via a recursive `render_range` call, submits the offscreen encoder,
    /// and stashes the offscreen textures/views in `pending_offscreen_textures`
    /// so they outlive the parent encoder's submit. The bind group + sampler
    /// are created later by `draw_composite_quad` (which has the offscreen
    /// view in hand and can build the bind group directly).
    #[allow(clippy::too_many_arguments)]
    fn render_save_layer_group(
        &mut self,
        gstart: usize,
        gend: usize,
        _bounds: crate::core::Bounds<crate::core::Logical>,
        group_text_idx: usize,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) -> wgpu::TextureView {
        // Ensure the group's text renderer + viewport slots exist. The
        // slot is created even though v1 doesn't prepare group text —
        // this keeps the pool indexed consistently for future tasks.
        if group_text_idx > 0 {
            let _ = self.group_text_renderer(group_text_idx - 1);
            let _ = self.group_viewport(group_text_idx - 1);
        }

        let (color_tex, color_view, depth_tex, depth_view) =
            self.create_offscreen_target(viewport_width, viewport_height);

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("SaveLayer Encoder") },
        );
        {
            let mut offscreen_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SaveLayer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.render_range(
                &mut offscreen_pass,
                gstart,
                gend,
                group_text_idx,
                scale_factor,
                viewport_width,
                viewport_height,
            );
        }
        self.queue.submit(std::iter::once(encoder.finish()));

        // Stash textures + views so they outlive the parent encoder's
        // submit. wgpu destroys resources when their Rust handles drop,
        // but the parent encoder records resource IDs that must remain
        // valid through submit. Cleared at the start of the next frame.
        let returned_view = color_view.clone();
        self.pending_offscreen_textures.push(PendingOffscreenTextures {
            _color_tex: color_tex,
            _depth_tex: depth_tex,
            _color_view: color_view,
            _depth_view: depth_view,
        });

        returned_view
    }

    /// Draw a composite quad: sample the offscreen texture view and blend
    /// it at `opacity` over the current pass's content. Used to composite
    /// a SaveLayer group's offscreen result into its parent pass.
    ///
    /// Reuses the image pipeline. The bind group + sampler are created
    /// here from `offscreen_view` (not by the caller), and stashed in
    /// `pending_composite_binds` to outlive the parent encoder's submit.
    ///
    /// The composite instance is appended to `image_instance_buffer` past
    /// the image-instance region (offset = `image_instance_count`), using a
    /// dynamic vertex-buffer slice. This fixes the per-call write_buffer
    /// collision from Task 8 where multiple composites per frame would
    /// overwrite each other's instance data at offset 0.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_composite_quad(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        offscreen_view: &wgpu::TextureView,
        logical_bounds: crate::core::Bounds<crate::core::Logical>,
        opacity: f32,
        z: f32,
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) {
        // Append this composite's instance data past the image instances.
        // Each composite gets its own slot; the slot index doubles as the
        // instance index for the draw call.
        if (self.composite_quad_count as usize) >= MAX_COMPOSITE_QUADS_PER_FRAME {
            log::warn!(
                "[SaveLayer] composite quad limit {} reached; skipping further composites this frame",
                MAX_COMPOSITE_QUADS_PER_FRAME
            );
            return;
        }
        let composite_slot = self.image_instance_count + self.composite_quad_count;
        let byte_offset = (composite_slot as wgpu::BufferAddress)
            * (std::mem::size_of::<ImageInstance>() as wgpu::BufferAddress);
        let byte_end = byte_offset + std::mem::size_of::<ImageInstance>() as wgpu::BufferAddress;

        let physical_x = logical_bounds.left * scale_factor;
        let physical_y = logical_bounds.top * scale_factor;
        let physical_w = logical_bounds.width() * scale_factor;
        let physical_h = logical_bounds.height() * scale_factor;

        // UV: the group's bounds sub-region within the surface-sized
        // offscreen texture. Samples only the group's area, not the
        // full offscreen texture.
        let uv_x = physical_x / viewport_width.max(1) as f32;
        let uv_y = physical_y / viewport_height.max(1) as f32;
        let uv_w = physical_w / viewport_width.max(1) as f32;
        let uv_h = physical_h / viewport_height.max(1) as f32;

        let instance = ImageInstance {
            position: [physical_x, physical_y],
            size: [physical_w, physical_h],
            uv_origin: [uv_x, uv_y],
            uv_size: [uv_w, uv_h],
            transform: AffineTransform::identity().to_array(),
            opacity,
            z,
        };

        self.queue.write_buffer(
            &self.image_instance_buffer,
            byte_offset,
            bytemuck::cast_slice(&[instance]),
        );
        self.composite_quad_count += 1;

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SaveLayer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SaveLayer Composite BindGroup"),
            layout: &self.image_atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(offscreen_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        render_pass.set_pipeline(&self.image_pipeline);
        render_pass.set_bind_group(0, &self.global_bind_group, &[]);
        render_pass.set_bind_group(1, &bind_group, &[]);
        render_pass.set_bind_group(2, &self.rclip_bind_group, &[0]);
        render_pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.image_instance_buffer.slice(byte_offset..byte_end));
        render_pass.set_index_buffer(
            self.image_index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );

        let x = physical_x.max(0.0) as u32;
        let y = physical_y.max(0.0) as u32;
        let right = (physical_x + physical_w).min(viewport_width as f32) as u32;
        let bottom = (physical_y + physical_h).min(viewport_height as f32) as u32;
        let w = right.saturating_sub(x);
        let h = bottom.saturating_sub(y);
        if w > 0 && h > 0 {
            render_pass.set_scissor_rect(x, y, w, h);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        // Stash bind group + sampler to keep them alive until the parent
        // encoder is submitted.
        self.pending_composite_binds.push(PendingCompositeBinds {
            _bind_group: bind_group,
            _sampler: sampler,
        });
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

    /// Get the current render config, panicking if unset.
    ///
    /// The config is set during `resize` / surface configuration and must be
    /// available by the time rendering begins.
    pub fn config(&self) -> &RenderConfig {
        self.current_config
            .as_ref()
            .expect("config must be set before render")
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

    /// Prepare text for a save-layer group using its pooled TextRenderer and
    /// Viewport. The pooled slots are created lazily (one per group index,
    /// 0-based here). The group's Viewport is updated to the current surface
    /// resolution each call — offscreen group targets are surface-sized so no
    /// coordinate translation is needed.
    ///
    /// This is the per-group counterpart to `prepare_text`. Errors are
    /// propagated (unlike the main-pass `prepare_text`, which unwraps) because
    /// group text preparation is new code and we want failures to surface
    /// rather than silently rendering empty groups.
    pub fn prepare_group_text(
        &mut self,
        group_idx: usize,
        font_system: &mut FontSystem,
        text_areas: Vec<glyphon::TextArea>,
    ) -> Result<(), RenderError> {
        // Ensure pooled slots exist (lazy grow). The returned borrows are
        // discarded so the split-borrows below are unaffected.
        if self.group_text_renderers.len() <= group_idx {
            let _ = self.group_text_renderer(group_idx);
        }
        if self.group_viewports.len() <= group_idx {
            let _ = self.group_viewport(group_idx);
        }

        let resolution = glyphon::Resolution {
            width: self.config().width(),
            height: self.config().height(),
        };

        // Split-borrow distinct fields directly. The borrow checker permits
        // simultaneous mutable borrows of disjoint struct fields, which is
        // essential here: `prepare` needs &mut atlas, &mut viewport, &mut
        // renderer, plus &device and &queue at once. Method-based accessors
        // would each borrow all of `self` and conflict.
        let renderer = &mut self.group_text_renderers[group_idx];
        let viewport = &mut self.group_viewports[group_idx];
        let atlas = &mut self.atlas;
        let device = &self.device;
        let queue = &self.queue;

        viewport.update(queue, resolution);

        let mut swash_cache = glyphon::SwashCache::new();
        renderer
            .prepare(
                device,
                queue,
                font_system,
                atlas,
                viewport,
                text_areas,
                &mut swash_cache,
            )
            .map_err(|e| RenderError::TextPrepareFailed(format!("{:?}", e)))
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
                        AffineTransform::from_array(req.transform),
                        req.opacity,
                        req.z,
                    );
                    image_instances.push(instance);
                }
                crate::frame_builder::DrawOp::BeginSaveLayer { .. }
                | crate::frame_builder::DrawOp::EndSaveLayer => {}
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
        // Reserve trailing slots for composite quads so draw_composite_quad
        // can append without re-allocating (which would invalidate earlier
        // slices bound in the same pass).
        self.ensure_image_instance_capacity(
            image_instances.len() + MAX_COMPOSITE_QUADS_PER_FRAME,
        );
        self.image_instance_count = image_instances.len() as u32;

        self.current_op_locations = op_locations;
        self.current_op_clips = op_clips;

        // Scan ops for SaveLayer markers (Begin/End) so render_range can
        // delimit groups without re-borrowing the frame builder. Each
        // Begin carries its bounds/opacity/z for later compositing.
        self.current_save_layer_markers.clear();
        for (i, (op, _, _)) in frame_builder.ops().iter().enumerate() {
            match op {
                DrawOp::BeginSaveLayer { bounds, opacity, z } => {
                    self.current_save_layer_markers.push(SaveLayerMarkerInfo {
                        index: i,
                        kind: SaveLayerMarkerKind::Begin,
                        bounds: *bounds,
                        opacity: *opacity,
                        z: *z,
                    });
                }
                DrawOp::EndSaveLayer => {
                    self.current_save_layer_markers.push(SaveLayerMarkerInfo {
                        index: i,
                        kind: SaveLayerMarkerKind::End,
                        bounds: crate::core::Bounds::ZERO,
                        opacity: 0.0,
                        z: 0.0,
                    });
                }
                _ => {}
            }
        }

        // Compute per-op rclip offsets. Each op gets a slot in the
        // rclip uniform buffer. Ops with no rclip point to offset 0
        // (the ZERO slot). Ops with rclip data point to their slot.
        // Slots are bounded by INITIAL_RCLIP_CAPACITY; ops beyond that
        // fall back to the ZERO slot (no rclip) with a warning, matching
        // the warn-and-drop pattern in FrameBuilder::push_rclip.
        let mut rclip_offsets: Vec<u32> = Vec::with_capacity(frame_builder.ops().len());
        let mut next_slot: u32 = 1; // slot 0 is ZERO
        let mut overflow_warned = false;
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if rclip_snapshot.is_empty() {
                rclip_offsets.push(0);
            } else if (next_slot as usize) >= INITIAL_RCLIP_CAPACITY {
                if !overflow_warned {
                    log::warn!(
                        "[ClipRRect] rclip uniform buffer capacity {} exceeded, \
                         excess ops fall back to no rclip",
                        INITIAL_RCLIP_CAPACITY
                    );
                    overflow_warned = true;
                }
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

        // Write each non-zero op's rclip data. The slot counter is
        // already bounded by INITIAL_RCLIP_CAPACITY via the offset loop
        // above, so this loop only writes slots that were allocated.
        let mut slot: u32 = 1;
        for (_, _, rclip_snapshot) in frame_builder.ops() {
            if !rclip_snapshot.is_empty() && (slot as usize) < INITIAL_RCLIP_CAPACITY {
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
    /// Draw a single op in the render pass. Handles scissor, pipeline
    /// switching, rclip binding, and the indexed draw call. Called from
    /// `execute_render_pass` in two phases (opaque before text, transparent
    /// after text). State (`prev_kind`, `prev_clip`, `prev_rclip_offset`)
    /// is passed by reference so it persists across calls within a phase.
    fn draw_op_in_pass(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        i: usize,
        prev_kind: &mut Option<OpKind>,
        prev_clip: &mut Option<Option<crate::core::Bounds<crate::core::Logical>>>,
        prev_rclip_offset_per_slot: &mut [Option<u32>; 2],
        scale_factor: f32,
        viewport_width: u32,
        viewport_height: u32,
    ) {
        let loc = self.current_op_locations[i];
        let clip = self.current_op_clips[i];

        // SaveLayer markers carry no geometry and don't draw in the main
        // pass — the backend scans for them to delimit offscreen groups
        // (wired in a later task). Early-return before any state mutation.
        if matches!(loc, crate::frame_builder::OpLocation::SaveLayerMarker) {
            return;
        }

        // 1. Scissor: only set when clip changes.
        let clip_value = clip;
        if *prev_clip != Some(clip_value) {
            match &clip {
                Some(c) => {
                    let x = (c.left * scale_factor).max(0.0) as u32;
                    let y = (c.top * scale_factor).max(0.0) as u32;
                    let right = (c.right * scale_factor).min(viewport_width as f32) as u32;
                    let bottom = (c.bottom * scale_factor).min(viewport_height as f32) as u32;
                    let w = right.saturating_sub(x);
                    let h = bottom.saturating_sub(y);
                    if w == 0 || h == 0 {
                        *prev_clip = Some(clip_value);
                        return;
                    }
                    render_pass.set_scissor_rect(x, y, w, h);
                }
                None => {
                    render_pass.set_scissor_rect(0, 0, viewport_width, viewport_height);
                }
            }
            *prev_clip = Some(clip_value);
        }

        // 2. Pipeline: only switch when op kind changes.
        let kind = loc.kind();
        let rclip_slot_idx = match kind {
            OpKind::Quad | OpKind::TransparentQuad => 0,
            OpKind::Image => 1,
            OpKind::SaveLayerMarker => return,
        };
        if Some(kind) != *prev_kind {
            match kind {
                OpKind::Quad => {
                    render_pass.set_pipeline(&self.render_pipeline);
                    render_pass.set_bind_group(0, &self.global_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                }
                OpKind::TransparentQuad => {
                    render_pass.set_pipeline(&self.transparent_render_pipeline);
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
                OpKind::SaveLayerMarker => return,
            }
            prev_rclip_offset_per_slot[rclip_slot_idx] = None;
            *prev_kind = Some(kind);
        }

        // 3. RClip bind group: per-op dynamic offset.
        let rclip_offset = self.current_op_rclip_offsets[i];
        if prev_rclip_offset_per_slot[rclip_slot_idx] != Some(rclip_offset) {
            let rclip_group = match kind {
                OpKind::Quad | OpKind::TransparentQuad => 1,
                OpKind::Image => 2,
                OpKind::SaveLayerMarker => return,
            };
            render_pass.set_bind_group(
                rclip_group,
                &self.rclip_bind_group,
                &[rclip_offset],
            );
            prev_rclip_offset_per_slot[rclip_slot_idx] = Some(rclip_offset);
        }

        // 4. Draw one instance.
        match kind {
            OpKind::Quad | OpKind::TransparentQuad => {
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
            OpKind::SaveLayerMarker => return,
        }
        let idx = match loc {
            crate::frame_builder::OpLocation::Quad { index } => index,
            crate::frame_builder::OpLocation::TransparentQuad { index } => index,
            crate::frame_builder::OpLocation::Image { index } => index,
            crate::frame_builder::OpLocation::SaveLayerMarker => return,
        };
        render_pass.draw_indexed(0..6, 0, idx..idx + 1);
    }

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
                // Window not fully on screen (hidden behind other windows
                // or minimized). The caller should NOT retry immediately —
                // doing so spins an infinite render→fail→request_redraw
                // loop. Wait for WindowEvent::Occluded(false) instead.
                return Err(RenderError::SurfaceOccluded);
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

        // Reset per-frame composite state. Dropping last frame's pending
        // resources is safe — its encoder was submitted last frame.
        self.composite_quad_count = 0;
        self.pending_offscreen_textures.clear();
        self.pending_composite_binds.clear();

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
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Recursive three-phase render. SaveLayer groups are rendered
            // offscreen and composited as textured quads at their paint-order
            // z-depth. group_text_renderer_idx=0 means "use the main
            // text_renderer" (this pass).
            self.render_range(
                &mut render_pass,
                0,
                self.current_op_locations.len(),
                0,
                scale_factor,
                viewport_width,
                viewport_height,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(output);
        self.atlas.trim();

        Ok(())
    }

    /// Create an offscreen render target (color + depth) for a SaveLayer group.
    ///
    /// The color texture uses the surface format for zero-conversion compositing.
    /// The depth texture matches the main depth format for three-phase rendering.
    /// Both are sized to the group's physical bounds.
    ///
    /// Per-frame allocation for v1 — texture pooling is a deferred optimization.
    fn create_offscreen_target(
        &self,
        physical_width: u32,
        physical_height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: physical_width.max(1),
            height: physical_height.max(1),
            depth_or_array_layers: 1,
        };

        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SaveLayer Color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SaveLayer Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        (color_texture, color_view, depth_texture, depth_view)
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

            // Recreate depth texture to match new surface size.
            self.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Depth Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.depth_texture_view = self.depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

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
