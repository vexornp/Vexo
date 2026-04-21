//! WGPU-based render backend implementation.
//!
//! This module provides a production-ready GPU rendering backend using wgpu.

use std::sync::Arc;

use glyphon::{FontSystem, Viewport};
use wgpu::util::DeviceExt;

use crate::core::{Scale, Size};
use crate::quad_instance::QuadInstance;
use crate::render::backend::{RenderBackend, RenderConfig, RenderError};
use crate::renderer::{UiBatcher, Vertex};
use crate::Color;

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
    global_uniform_buffer: wgpu::Buffer,
    global_bind_group: wgpu::BindGroup,

    // Text rendering
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
    viewport: glyphon::Viewport,
    cache: glyphon::Cache,

    // Current configuration
    current_config: Option<RenderConfig>,

    // Clear color
    clear_color: wgpu::Color,
}

impl WgpuBackend {
    /// Create a new WGPU backend with a window.
    pub async fn new(window: Arc<dyn winit::window::Window>) -> anyhow::Result<Self> {
        let size = window.surface_size();
        let scale_factor = window.scale_factor() as f32;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();

        let physical_width = size.width as f32;
        let physical_height = size.height as f32;

        Self::init(surface, instance, physical_width, physical_height, scale_factor).await
    }

    async fn init(
        surface: wgpu::Surface<'static>,
        instance: wgpu::Instance,
        physical_width: f32,
        physical_height: f32,
        scale_factor: f32,
    ) -> anyhow::Result<Self> {
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
            width: physical_width as u32,
            height: physical_height as u32,
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

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (std::mem::size_of::<QuadInstance>() * 10000) as wgpu::BufferAddress,
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
        let is_configured = if physical_width > 0.0 && physical_height > 0.0 {
            surface.configure(&device, &config);
            true
        } else {
            false
        };

        // Write initial uniforms
        let uniform = GlobalUniforms {
            screen_size: [physical_width, physical_height],
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
            global_uniform_buffer,
            global_bind_group,
            atlas,
            text_renderer,
            viewport,
            cache,
            current_config: Some(RenderConfig::new(
                Size::new(physical_width as f32, physical_height as f32),
                Scale::new(scale_factor as f64),
            )),
            clear_color: Color::BLUE.to_wgpu_color(),
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
    pub fn update_viewport(&mut self, width: u32, height: u32) {
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width,
                height,
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

    /// Upload geometry from batcher to GPU buffers.
    pub fn upload_geometry(&mut self, batcher: &UiBatcher) {
        if !batcher.vertices.is_empty() {
            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&batcher.vertices),
            );
            self.queue.write_buffer(
                &self.index_buffer,
                0,
                bytemuck::cast_slice(&batcher.indices),
            );
        }

        if !batcher.quad_instances.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&batcher.quad_instances),
            );
        }
    }

    /// Execute the render pass with the given instance count.
    pub fn execute_render_pass(&mut self, instance_count: usize) -> Result<(), RenderError> {
        if !self.is_configured {
            return Err(RenderError::SurfaceNotConfigured);
        }

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

            let instance_count = instance_count as u32;
            if instance_count > 0 {
                render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..instance_count);
            }

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
        batcher: &mut UiBatcher,
        _font_system: &mut FontSystem,
        config: RenderConfig,
    ) {
        self.current_config = Some(config.clone());

        // Update global uniforms if size changed
        let uniform = GlobalUniforms {
            screen_size: config.screen_size_array(),
            scale_factor: config.scale_factor(),
            _padding: 0.0,
        };
        self.queue.write_buffer(&self.global_uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        // Upload geometry buffers
        self.upload_geometry(batcher);

        // Update viewport
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: config.width(),
                height: config.height(),
            },
        );
    }

    fn render(&mut self) -> Result<(), RenderError> {
        // Use a default instance count for the trait method
        self.execute_render_pass(0)
    }

    fn resize(&mut self, config: RenderConfig) {
        let width = config.width();
        let height = config.height();
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_configured = true;

            // Update uniforms
            let uniform = GlobalUniforms {
                screen_size: config.screen_size_array(),
                scale_factor: config.scale_factor(),
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
