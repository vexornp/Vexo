use glyphon::{cosmic_text, Metrics, TextBounds, Viewport};
use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use taffy::prelude::*;
use wgpu::util::DeviceExt;
use wgpu::wgc::device::global;
use wgpu::wgc::instance;

use winit::dpi::PhysicalSize;
use winit::event::*;
use winit::event_loop::EventLoop;
use winit::window::{WindowAttributes, WindowId};

use winit::{
    application::ApplicationHandler, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window,
};

pub use color::Color;
pub use uniffi;

const CLEAR_COLOR: wgpu::Color = Color::BLUE.to_wgpu_color();

mod color;
mod editor;
mod macros;
mod quad_instance;
mod renderer;
mod resource;
mod utils;
pub mod widgets;

use renderer::{TextRequest, UiBatcher, Vertex};
use widgets::{Column, Widget, WidgetContext, WidgetId};
pub use winit::dpi::PhysicalPosition;

use crate::utils::{PhysicalLocation, Scale};

pub use taffy::prelude::AlignItems;

extern crate alloc;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalUniforms {
    pub screen_size: [f32; 2],
    scale_factor: f32,
    pub _padding: f32, // Pad to 16 bytes (4 floats total)
}

pub struct WindowState<A: Application + 'static> {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    window: Option<Arc<dyn Window>>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer, // The 4 corners of the quad mesh
    index_buffer: wgpu::Buffer,  // The indices for the quad mesh (2 triangles)

    // DYNAMIC BUFFERS (updated every frame)
    instance_buffer: wgpu::Buffer, // The list of quads to draw (position/size/color)

    // UNIFORM BUFFER (updated on resize)
    global_uniform_buffer: wgpu::Buffer,

    // Bind group for global uniforms (like screen size)
    global_bind_group: wgpu::BindGroup,

    batcher: UiBatcher,
    taffy: taffy::TaffyTree,
    root_widget: Box<dyn Widget<A::Message>>,
    root_node_id: NodeId,

    // --- Text Rendering Fields --
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
    swash_cache: glyphon::SwashCache,
    viewport: glyphon::Viewport,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Editor
    focused_widget_id: Option<WidgetId>,
    widget_context: WidgetContext,
}

impl<A: Application + 'static> WindowState<A> {
    pub async fn new(window: Arc<dyn Window>) -> anyhow::Result<Self> {
        let size = window.surface_size();
        let scale_factor = window.scale_factor() as f32;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();

        let physical_width = size.width as f32;
        let physical_height = size.height as f32;

        Self::init(
            surface,
            instance,
            physical_width,
            physical_height,
            scale_factor,
            Some(window),
        )
        .await
    }

    async fn init(
        surface: wgpu::Surface<'static>,
        instance: wgpu::Instance,
        physical_width: f32,
        physical_height: f32,
        scale_factor: f32,
        window: Option<Arc<dyn Window>>,
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(resource::file::WGSL.into()),
        });

        // Define the Bind Group Layout for global uniforms (like screen size)
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

        // Put that bind group layout into a pipeline layout
        // (even if we don't have bind groups yet, we need this for the pipeline)
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&global_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Create the render pipeline with the shader and pipeline layout
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),

                // Order matters: The first buffer layout is slot 0, the second is slot 1, etc.
                buffers: &[
                    Vertex::desc(),                      // Standard mesh data (Position/UV) -> Slot 0
                    quad_instance::QuadInstance::desc(), // Instance data (position/size/color) -> Slot 1
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING), // Allow alpha blending for transparency
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

        const QUAD_VERTICES: &[Vertex] = &[
            Vertex {
                pos: [0.0, 0.0, 0.0],
            }, // Top-left
            Vertex {
                pos: [1.0, 0.0, 0.0],
            }, // Top-right
            Vertex {
                pos: [1.0, 1.0, 0.0],
            }, // Bottom-right
            Vertex {
                pos: [0.0, 1.0, 0.0],
            }, // Bottom-left
        ];

        const QUAD_INDICES: &[u16] = &[
            0, 1, 2, // First triangle (top-left, top-right, bottom-right)
            0, 2, 3, // Second triangle (top-left, bottom-right, bottom-left)
        ];

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
            // Allocate space for 10,000 instances initially
            size: (std::mem::size_of::<quad_instance::QuadInstance>() * 10000)
                as wgpu::BufferAddress,
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

        // --- Glyphone Initialization ---

        let swash_cache = glyphon::SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = glyphon::TextAtlas::new(&device, &queue, &cache, config.format);
        let text_renderer = glyphon::TextRenderer::new(
            &mut atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        // --- Root Node Id ---
        let mut taffy = taffy::TaffyTree::new();
        let mut root_widget = Box::new(Column::new());
        let mut ctx = WidgetContext::new();
        let root_node_id = root_widget.layout(&mut taffy, &mut ctx);

        if physical_width > 0.0 && physical_height > 0.0 {
            // Necessary for android, which does not get resize event later:
            surface.configure(&device, &config);
        }

        Ok(Self {
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            window: window,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            global_uniform_buffer,
            global_bind_group,
            batcher: UiBatcher::new(),
            taffy,
            root_widget,
            root_node_id,
            atlas,
            text_renderer,
            swash_cache,
            viewport,
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            focused_widget_id: None,
            widget_context: ctx,
        })
    }

    pub fn resize_physical(&mut self, width: f32, height: f32) {
        let scale_factor = self.widget_context.scale.factor();

        let uniform = GlobalUniforms {
            screen_size: [width, height],
            scale_factor,
            _padding: 0.0,
        };

        self.queue
            .write_buffer(&self.global_uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        if width > 0.0 && height > 0.0 {
            self.config.width = width as u32;
            self.config.height = height as u32;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;

            //Force re-layout
            self.root_node_id = self.taffy.new_leaf(Style::default()).unwrap();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if let Some(win) = &self.window {
            win.request_redraw();
        }

        if !self.is_surface_configured {
            return Ok(());
        }

        let mut new_root_widget = self.view();
        self.taffy.clear();
        self.batcher.clear();

        // Reset deterministic id stack before building widget tree
        self.widget_context.reset_id_stack();
        // push root slot
        self.widget_context.push_index(0);

        let scale_factor = self.widget_context.scale.factor();

        // Taffy should layout in logical points so that 24.0 size means 24 points.
        let logical_width = self.config.width as f32 / scale_factor;
        let logical_height = self.config.height as f32 / scale_factor;

        // Set screen size once per frame
        self.batcher.set_screen_size(logical_width, logical_height);

        let new_root_node_id = new_root_widget.layout(&mut self.taffy, &mut self.widget_context);

        // pop the root slot
        self.widget_context.pop();

        self.taffy
            .compute_layout(
                new_root_node_id,
                Size {
                    width: AvailableSpace::Definite(logical_width),
                    height: AvailableSpace::Definite(logical_height),
                },
            )
            .unwrap();
        self.root_widget = new_root_widget;
        self.root_node_id = new_root_node_id;

        // 1. DRAW RECTANGLES: Generate geometry data
        self.root_widget.draw(
            &mut self.taffy,
            self.root_node_id,
            &mut self.batcher,
            (0.0, 0.0),
            self.focused_widget_id,
            &mut self.widget_context,
        );

        // 2. GLYPHON PREPARATION: Prepare text geometry using Taffy positions
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );

        let mut processed_texts: Vec<(glyphon::Buffer, TextRequest)> = Vec::new();

        for req in self.batcher.text_requests.drain(..) {
            // Create the Glyphon text buffer
            let mut buffer = glyphon::Buffer::new(
                &mut self.widget_context.font_system,
                Metrics::new(req.size, req.size * 1.2),
            );

            // Convert float color to u8 color for cosmic_text::Color (Glyphon's color type)
            let color_rgba_u8 = cosmic_text::Color::rgba(
                (req.color[0] * 255.0) as u8,
                (req.color[1] * 255.0) as u8,
                (req.color[2] * 255.0) as u8,
                (req.color[3] * 255.0) as u8,
            );

            buffer.set_text(
                &mut self.widget_context.font_system,
                &req.content,
                &glyphon::Attrs::new().color(color_rgba_u8),
                glyphon::Shaping::Advanced,
            );
            buffer.shape_until_scroll(&mut self.widget_context.font_system, true);
            processed_texts.push((buffer, req));
        }

        // Create Text Areas from the processed buffers and Taffy positions
        let text_areas: Vec<glyphon::TextArea> = processed_texts
            .iter_mut()
            .map(|(buffer, req)| {
                // Taffy (req.position) gives Logical coordinates.
                // Glyphon expects Physical coordinates.
                // So we multiply by scale_factor.
                let left_pos = req.position.0 * scale_factor;
                let top_pos = req.position.1 * scale_factor;

                let bounds_left: i32 = left_pos.floor() as i32;
                let bounds_top = top_pos.floor() as i32;
                let bounds_right = self.config.width as i32;
                let bounds_bottom: i32 = self.config.height as i32;

                let color_rgba_u8 = cosmic_text::Color::rgba(
                    (req.color[0] * 255.0) as u8,
                    (req.color[1] * 255.0) as u8,
                    (req.color[2] * 255.0) as u8,
                    (req.color[3] * 255.0) as u8,
                );

                glyphon::TextArea {
                    buffer: buffer,
                    left: left_pos,
                    top: top_pos,
                    scale: scale_factor,
                    bounds: TextBounds {
                        left: bounds_left,
                        top: bounds_top,
                        right: bounds_right,
                        bottom: bounds_bottom,
                    },
                    default_color: color_rgba_u8,
                    custom_glyphs: &[],
                }
            })
            .collect();

        // For editor text areas we must provide a `&Buffer` that lives long
        // enough for `text_renderer.prepare`. To do that without changing
        // glyphon's API we clone each editor `Buffer` into a local Vec and
        // then create `TextArea` instances that borrow from that Vec. The
        // `editor_buffers` Vec must stay alive until after `prepare`/`render`.
        // Collect owned editor buffers and metadata first, then create
        // `TextArea` instances in a separate pass to avoid holding
        // simultaneous mutable/immutable borrows of `editor_buffers`.
        let mut editor_buffers: Vec<glyphon::Buffer> = Vec::new();
        // Metadata: (left, top, left_i, top_i, right_i, bottom_i, color)
        let mut editor_meta: Vec<(f32, f32, i32, i32, i32, i32, cosmic_text::Color)> = Vec::new();

        for req in self.batcher.editor_requests.iter_mut() {
            // req.bounds is (x, y, width, height) in logical points (taffy layout)
            // Convert to physical pixels and compute absolute bounds (left..right, top..bottom)
            let bx = req.bounds.x;
            let by = req.bounds.y;
            let bw = req.bounds.width;
            let bh = req.bounds.height;

            let left_pos = bx * scale_factor;
            let top_pos = by * scale_factor;

            let bounds_left: i32 = left_pos.floor() as i32;
            let bounds_top: i32 = top_pos.floor() as i32;
            let bounds_right: i32 = ((bx + bw) * scale_factor).ceil() as i32;
            let bounds_bottom: i32 = ((by + bh) * scale_factor).ceil() as i32;

            let color_rgba_u8 = cosmic_text::Color::rgba(
                (req.color[0] * 255.0) as u8,
                (req.color[1] * 255.0) as u8,
                (req.color[2] * 255.0) as u8,
                (req.color[3] * 255.0) as u8,
            );

            let editor_ref = self
                .widget_context
                .get_or_create_editor(&req.id, "initial_text");
            let editor = editor_ref.borrow();
            let buf = editor.buffer().clone();
            editor_buffers.push(buf);
            editor_meta.push((
                left_pos,
                top_pos,
                bounds_left,
                bounds_top,
                bounds_right,
                bounds_bottom,
                color_rgba_u8,
            ));
        }

        // Now build TextArea instances borrowing from the owned `editor_buffers`.
        let mut editor_areas: Vec<glyphon::TextArea> = Vec::new();
        for (i, buf) in editor_buffers.iter_mut().enumerate() {
            let (left_pos, top_pos, bounds_left, bounds_top, bounds_right, bounds_bottom, color) =
                editor_meta[i];
            buf.shape_until_scroll(&mut self.widget_context.font_system, true);

            editor_areas.push(glyphon::TextArea {
                buffer: buf,
                left: left_pos,
                top: top_pos,
                scale: self.widget_context.scale.factor(),
                bounds: TextBounds {
                    left: bounds_left,
                    top: bounds_top,
                    right: bounds_right,
                    bottom: bounds_bottom,
                },
                default_color: color,
                custom_glyphs: &[],
            });
        }

        // --- WGPU Drawing Phase ---
        // Upload rectangle geometry
        if !self.batcher.vertices.is_empty() {
            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.batcher.vertices),
            );
            self.queue.write_buffer(
                &self.index_buffer,
                0,
                bytemuck::cast_slice(&self.batcher.indices),
            );
        }

        if !self.batcher.quad_instances.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.batcher.quad_instances),
            );
        }

        // Combine text areas (regular + editor) and prepare glyphon once.
        let mut combined_text_areas = text_areas;
        combined_text_areas.extend(editor_areas.into_iter());

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.widget_context.font_system,
                &mut self.atlas,
                &self.viewport,
                combined_text_areas,
                &mut self.swash_cache,
            )
            .unwrap();

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            //Set the Pipeline
            render_pass.set_pipeline(&self.render_pipeline);

            // Bind the screen size global uniform
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);

            // Set the Quad Mesh (The 4 corners)
            // This is Slot 0 in your VertexBufferLayout
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            // Set the Instance Data (The specific quads)
            // This is Slot 1 in your VertexBufferLayout
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

            // Draw
            let instance_count = self.batcher.quad_instances.len() as u32;
            if instance_count > 0 {
                render_pass
                    .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..instance_count);
            }

            // Render Text on top
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut render_pass)
                .unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.atlas.trim();
        Ok(())
    }

    fn update(&mut self, message: A::Message) {
        A::update(&mut self.user_app_state, message);
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    fn handle_window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: &winit::event::WindowEvent,
    ) {
        // Pass the event to the root widget (which passes it down)
        let widget_response = self.root_widget.on_event(
            &self.taffy,
            self.root_node_id,
            (0.0, 0.0),
            event,
            self.focused_widget_id,
            &mut self.widget_context,
        );

        // Handle Framework Logic
        if let Some(focus_request) = widget_response.focus_request {
            self.focused_widget_id = Some(focus_request);
            println!("Focus requested by widget: {:?}", focus_request);
        } else if let WindowEvent::PointerButton {
            state: winit::event::ElementState::Pressed,
            ..
        } = event
        {
            // Click outside any focusable widget - clear focus
            self.focused_widget_id = None;
        }

        // Check if event if handled, notify if needed
        if widget_response.handled {
            println!("Event handled by widget");
        }

        //  Handle User Logic
        if let Some(msg) = widget_response.message {
            println!("User message received: {:?}", msg);
            self.update(msg);
        }
    }

    #[allow(dead_code)]
    fn handle_key(&mut self, event_loop: &dyn ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        match (code, is_pressed) {
            (KeyCode::Escape, true) => event_loop.exit(),
            (KeyCode::Space, true) => {
                return;
            }
            _ => {}
        }
    }

    fn view(&self) -> Box<dyn Widget<A::Message>> {
        A::view(&self.user_app_state)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.resize_physical(size.width as f32, size.height as f32);
    }
}

pub struct MyApp<A: Application + 'static> {
    receiver: Receiver<KeyBindingAction>,
    sender: Sender<KeyBindingAction>,
    windows: HashMap<WindowId, WindowState<A>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBindingAction {
    CloseWindow,
    Message,
}

impl<A: Application + 'static> MyApp<A> {
    pub fn new(
        event_loop: &EventLoop,
        receiver: Receiver<KeyBindingAction>,
        sender: Sender<KeyBindingAction>,
    ) -> Self {
        Self {
            receiver,
            sender,
            windows: Default::default(),
        }
    }

    pub fn try_init_framework_state(&mut self, window: Box<dyn Window>) -> Option<WindowId> {
        let window: Arc<dyn Window> = Arc::from(window);
        let window_id = window.id();
        let size = window.surface_size();
        let width = size.width;
        let height = size.height;
        let window_state = self.windows.get(&window_id);
        if width > 0 && height > 0 && window_state.is_none() {
            println!(
                "SUCCESS: Window ready at {}x{}, scale: {}",
                size.width,
                size.height,
                window.scale_factor()
            );
            let mut state = pollster::block_on(WindowState::new(window.clone())).unwrap();
            state.resize_physical(width as f32, height as f32);
            self.windows.insert(window_id, state);
            return Some(window_id);
        }

        return None;
    }

    fn handle_action_from_proxy(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: KeyBindingAction,
    ) {
        match action {
            KeyBindingAction::Message => {
                println!("Use wake up")
            }
            _ => {}
        }
    }

    fn create_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> Result<WindowId, Box<dyn Error>> {
        let window_attr = WindowAttributes::default();
        let window = event_loop.create_window(window_attr).unwrap();
        let wid = self.try_init_framework_state(window);
        return Result::Ok(wid.unwrap());
    }
}

impl<A: Application + 'static> ApplicationHandler for MyApp<A> {
    // fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
    //     if !self.windows.is_empty() {
    //         println!("app resumed, already have window");
    //         return;
    //     }

    //     println!("app resumed, create initial window");
    //     let window_attributes = WindowAttributes::default();
    //     let window = event_loop.create_window(window_attributes).unwrap();
    //     self.try_init_framework_state(window);
    // }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let window_state = match self.windows.get_mut(&window_id) {
            Some(ws) => ws,
            None => return,
        };

        match event {
            WindowEvent::SurfaceResized(size) => {
                window_state.resize(size);
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                surface_size_writer: _,
            } => {
                window_state.widget_context.scale = Scale::new(scale_factor);
                println!("Scale factor changed to {}", scale_factor);
            }
            WindowEvent::PointerMoved {
                device_id,
                position,
                primary,
                source: _,
            } => {
                window_state.widget_context.cursor_pos = PhysicalLocation::new(position);
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = window_state.render() {
                    println!("Error drawing window: {err}")
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
                println!("Window closed by user");
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                event_loop.exit();
                println!("Escape pressed, exiting");
            }
            _ => (),
        }

        window_state.handle_window_event(event_loop, window_id, &event);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(action) = self.receiver.try_recv() {
            self.handle_action_from_proxy(event_loop, action);
        }
    }

    fn device_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        device_id: Option<DeviceId>,
        event: DeviceEvent,
    ) {
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        println!("Ready to create surfaces");
        self.create_window(event_loop)
            .expect("Failed to create initial window");
    }
}

pub trait Application {
    type Message: Clone + std::fmt::Debug + Send;
    type State: Sized;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;
}

pub fn run_desktop_demo<A: Application + 'static>() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    let (sender, receiver) = mpsc::channel();

    {
        // Wire the user event from another thread.
        let event_loop_proxy = event_loop.create_proxy();
        let sender = sender.clone();
        std::thread::spawn(move || {
            // Wake up the `event_loop` once every second and dispatch a custom event
            // from a different thread.
            println!("Starting to send user event every second");
            // loop {
            //     let _ = sender.send(KeyBindingAction::Message);
            //     event_loop_proxy.wake_up();
            //     std::thread::sleep(std::time::Duration::from_secs(1));
            // }
        });
    }

    let app = MyApp::<A>::new(&event_loop, receiver, sender);

    // let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    // let mut app = crate::MyApp::<A>::new();
    // event_loop.run_app(&mut app)?;
    Result::Ok(event_loop.run_app(app)?)
}
