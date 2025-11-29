use glyphon::{Metrics, TextBounds, Viewport, cosmic_text};
use std::sync::Arc;
use taffy::prelude::*;
use winit::event::*;
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

const CLEAR_COLOR: wgpu::Color = wgpu::Color::BLACK;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}
pub struct FrameworkState<A: Application + 'static> {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    window: Arc<Window>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    batcher: UiBatcher,
    cursor_pos: (f32, f32),
    taffy: taffy::TaffyTree,
    root_widget: Box<dyn Widget<A::Message>>,
    root_node_id: NodeId,

    // --- Text Rendering Fields --
    font_system: glyphon::FontSystem,
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
    swash_cache: glyphon::SwashCache,
    scale_factor: f32,
    viewport: glyphon::Viewport,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,
}

impl<A: Application + 'static> FrameworkState<A> {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
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
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
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

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertext Buffer"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::wgt::BufferDescriptor {
            label: Some("Index Buffer"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Glyphone Initialization ---
        let font_system = glyphon::FontSystem::new();
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
        let scale_factor = window.scale_factor() as f32;

        // --- Root Node Id ---
        let mut taffy = taffy::TaffyTree::new();
        let root_widget = Box::new(Column::new());
        let root_node_id = root_widget.layout(&mut taffy);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            window,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            batcher: UiBatcher::new(),
            cursor_pos: (0.0, 0.0),
            taffy,
            root_widget,
            root_node_id,
            font_system,
            atlas,
            text_renderer,
            swash_cache,
            scale_factor,
            viewport,
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.window.request_redraw();
        if !self.is_surface_configured {
            return Ok(());
        }

        let new_root_widget = self.view();
        self.taffy.clear();
        self.batcher.clear();

        // Set screen size once per frame
        self.batcher
            .set_screen_size(self.config.width, self.config.height);

        let new_root_node_id = new_root_widget.layout(&mut self.taffy);
        let window_width = self.config.width as f32;
        let window_height = self.config.height as f32;
        self.taffy
            .compute_layout(
                new_root_node_id,
                Size {
                    width: AvailableSpace::Definite(window_width),
                    height: AvailableSpace::Definite(window_height),
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
        );

        // 2. GLYPHON PREPARATION: Prepare text geometry using Taffy positions
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: window_width as u32,
                height: window_height as u32,
            },
        );

        let mut processed_texts: Vec<(glyphon::Buffer, TextRequest)> = Vec::new();

        for req in self.batcher.text_requests.drain(..) {
            // Create the Glyphon text buffer
            let mut buffer = glyphon::Buffer::new(
                &mut self.font_system,
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
                &mut self.font_system,
                &req.content,
                &glyphon::Attrs::new().color(color_rgba_u8),
                glyphon::Shaping::Advanced,
            );
            buffer.shape_until_scroll(&mut self.font_system, true);
            processed_texts.push((buffer, req));
        }

        // Create Text Areas from the processed buffers and Taffy positions
        let text_areas: Vec<glyphon::TextArea> = processed_texts
            .iter_mut()
            .map(|(buffer, req)| {
                // Use Taffy's calculated position and scale for high-DPI
                let left_pos = req.position.0 * self.scale_factor;
                let top_pos = req.position.1 * self.scale_factor;

                let bounds_left: i32 = left_pos.floor() as i32;
                let bounds_top = top_pos.floor() as i32;
                let bounds_right = (window_width * self.scale_factor) as i32;
                let bounds_bottom: i32 = (window_height * self.scale_factor) as i32;

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
                    scale: self.scale_factor,
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

        // Prepare Glyphon's resources
        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
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

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.batcher.indices.len() as u32, 0, 0..1);

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
        self.window.request_redraw();
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        match (code, is_pressed) {
            (KeyCode::Escape, true) => event_loop.exit(),
            (KeyCode::Space, true) => {
                return;
            }
            _ => {}
        }
    }

    fn handle_mouse_click(&mut self, state: ElementState, button: MouseButton) {
        if state == ElementState::Pressed && button == MouseButton::Left {
            let event = winit::event::WindowEvent::MouseInput {
                device_id: unsafe { std::mem::zeroed() },
                state,
                button,
            };

            if let Some(msg) = self.root_widget.on_event(
                &self.taffy,
                self.root_node_id,
                (0.0, 0.0),
                &event,
                self.cursor_pos,
            ) {
                self.update(msg);
            }
        }
    }

    fn view(&self) -> Box<dyn Widget<A::Message>> {
        A::view(&self.user_app_state)
    }
}

pub struct MyApp<A: Application + 'static> {
    framework_state: Option<FrameworkState<A>>,
}

impl<A: Application + 'static> MyApp<A> {
    pub fn new() -> Self {
        Self {
            framework_state: None,
        }
    }
}

impl<A: Application + 'static> ApplicationHandler<FrameworkState<A>> for MyApp<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.framework_state.is_some() {
            return;
        }

        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        self.framework_state = Some(pollster::block_on(FrameworkState::new(window)).unwrap());
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: FrameworkState<A>) {
        self.framework_state = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.framework_state {
            Some(canvas) => canvas,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => match state.render() {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    let size = state.window.inner_size();
                    state.resize(size.width, size.height);
                }
                Err(e) => {
                    log::error!("Unable to render {}", e);
                }
            },
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => state.handle_key(event_loop, code, key_state.is_pressed()),
            WindowEvent::MouseWheel {
                device_id: _,
                delta,
                phase: _,
            } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    println!("Mouse scroll: {}, {}", x, y);
                }
                _ => {
                    println!("Unkonwn mouse event");
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                state: m_state,
                button,
                ..
            } => {
                state.handle_mouse_click(m_state, button);
            }
            _ => {}
        }
    }
}

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId;
    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    );
    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M>;
}

pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
}

impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Rectangle {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId {
        taffy
            .new_leaf(Style {
                size: Size {
                    width: length(self.width),
                    height: length(self.height),
                },
                ..Default::default()
            })
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_rect(x, y, layout.size.width, layout.size.height, self.color);
    }

    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M> {
        None
    }
}

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Column<M> {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId {
        let child_nodes: Vec<NodeId> = self
            .children
            .iter()
            .map(|child| child.layout(taffy))
            .collect();

        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    gap: Size {
                        width: length(0.0),
                        height: length(10.0),
                    },
                    ..Default::default()
                },
                &child_nodes,
            )
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    ) {
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(taffy, child_node_id, renderer, (my_x, my_y));
        }
    }

    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M> {
        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;

        for (child, child_node) in self.children.iter().zip(child_ids) {
            if let Some(msg) = child.on_event(taffy, child_node, (my_x, my_y), event, cursor_pos) {
                return Some(msg);
            }
        }
        None
    }
}

pub struct Row<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
}

impl<M: Clone + std::fmt::Debug + Send> Row<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Row<M> {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId {
        let child_nodes: Vec<NodeId> = self
            .children
            .iter()
            .map(|child| child.layout(taffy))
            .collect();

        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    gap: Size {
                        width: length(10.0),
                        height: length(0.0),
                    },
                    ..Default::default()
                },
                &child_nodes,
            )
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    ) {
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(taffy, child_node_id, renderer, (my_x, my_y));
        }
    }

    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M> {
        None
    }
}

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: [f32; 3],
    pub padding: f32,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: [0.2, 0.2, 0.2],
            padding: 10.0,
        }
    }

    pub fn color(mut self, color: [f32; 3]) -> Self {
        self.background_color = color;
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Button<M> {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId {
        let content_node = self.content.layout(taffy);
        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    padding: Rect {
                        left: length(self.padding),
                        right: length(self.padding),
                        top: length(self.padding),
                        bottom: length(self.padding),
                    },
                    size: Size {
                        width: auto(),
                        height: auto(),
                    },
                    ..Default::default()
                },
                &[content_node],
            )
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_rect(
            x,
            y,
            layout.size.width,
            layout.size.height,
            self.background_color,
        );

        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = (x, y);
            self.content
                .draw(taffy, *content_node, renderer, content_offset);
        }
    }

    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M> {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        let width = layout.size.width;
        let height = layout.size.height;

        let is_over = cursor_pos.0 >= x
            && cursor_pos.0 <= x + width
            && cursor_pos.1 >= y
            && cursor_pos.1 <= y + height;

        // 1. CLICK HANDLING
        if is_over {
            if let WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } = event
            {
                return Some(self.on_press.clone());
            }
        }

        // 2. CHILD EVENT PROPAGATION
        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = (x, y);
            return self.content.on_event(
                taffy,
                *content_node, // Pass event to the content node
                content_offset,
                event,
                cursor_pos,
            );
        }

        None
    }
}

pub struct Text {
    pub content: String,
    pub size: f32,
    pub color: [f32; 3],
    pub style: taffy::Style,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            size: 24.0,
            color: [0.0, 0.0, 0.0],
            style: Style::default(),
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn flex_grow(mut self, value: f32) -> Self {
        self.style.flex_grow = value;
        self
    }

    pub fn width(mut self, dim: taffy::Dimension) -> Self {
        self.style.size.width = dim;
        self
    }

    pub fn height(mut self, dim: Dimension) -> Self {
        self.style.size.height = dim;
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn layout(&self, taffy: &mut taffy::TaffyTree) -> NodeId {
        // Glyphon calculation: This is where we calculate the precise bounds.
        // NOTE: In a final structure, FontSystem should be passed here,

        let mut style: Style = self.style.clone();
        let width_guess = self.content.len() as f32 * (self.size * 0.5);
        let height_guess = self.size * 1.2;

        // If the user's style is Auto, use the calculated intrinsic width.
        // Otherwise, use the user's custom width (Percent, Length, etc.).
        style.size.width = match style.size.width {
            Dimension::AUTO => length(width_guess),
            _ => style.size.width,
        };
        style.size.height = match style.size.height {
            Dimension::AUTO => length(height_guess),
            _ => style.size.height,
        };

        taffy.new_leaf(style).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_text(self.content.clone(), x, y, self.size, self.color);
    }

    fn on_event(
        &self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
    ) -> Option<M> {
        None
    }
}

pub enum Primitive {
    Quad {
        bounds: taffy::Rect<f32>,
        color: [f32; 4],
    },
    Text {
        content: String,
        bounds: Rect<f32>,
    },
}

pub struct Renderer {
    primitives: Vec<Primitive>,
}

impl Renderer {
    pub fn fill_quad(&mut self, bounds: taffy::Rect<f32>, color: [f32; 4]) {
        self.primitives.push(Primitive::Quad { bounds, color });
    }
}

pub struct TextRequest {
    pub content: String,
    pub position: (f32, f32),
    pub size: f32,
    pub color: [f32; 4],
}

pub struct UiBatcher {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_requests: Vec<TextRequest>,

    screen_width: u32,
    screen_height: u32,
}

impl UiBatcher {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            text_requests: Vec::new(),
            screen_width: 1,
            screen_height: 1,
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.text_requests.clear();
    }

    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn add_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 3]) {
        let sw = self.screen_width as f32;
        let sh = self.screen_height as f32;

        let normalize =
            |px: f32, py: f32| -> [f32; 3] { [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0, 0.0] };
        let i = self.vertices.len() as u16;
        let tl = normalize(x, y);
        let tr = normalize(x + width, y);
        let br = normalize(x + width, y + height);
        let bl = normalize(x, y + height);

        self.vertices.push(Vertex { pos: tl, color });
        self.vertices.push(Vertex { pos: tr, color });
        self.vertices.push(Vertex { pos: br, color });
        self.vertices.push(Vertex { pos: bl, color });

        self.indices.extend_from_slice(&[
            i,
            i + 1,
            i + 2, // First Triangle
            i,
            i + 2,
            i + 3, // Secode Triangle
        ]);
    }

    pub fn add_text(&mut self, content: String, x: f32, y: f32, size: f32, color: [f32; 3]) {
        let color_rgba = [color[0], color[1], color[2], 1.0];

        self.text_requests.push(TextRequest {
            content,
            position: (x, y),
            size,
            color: color_rgba,
        });
    }
}

pub fn run() -> anyhow::Result<()> {
    env_logger::init();
    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = MyApp::<State>::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

pub trait Application {
    type Message: Clone + std::fmt::Debug + Send;
    type State: Sized;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;
}

fn main() -> anyhow::Result<()> {
    run()
}

// --- The User's Code ---
#[derive(Debug, Clone, Copy)]
pub enum Message {
    None,
    Clicked,
}

pub struct State {
    click_count: u32,
}

impl Application for State {
    type Message = Message;
    type State = Self;

    fn new() -> Self::State {
        Self { click_count: 0 }
    }

    fn update(state: &mut Self::State, message: Self::Message) {
        match message {
            Message::Clicked => {
                state.click_count += 1;
            }
            Message::None => {}
        }
    }

    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>> {
        let text_content = format!("You clicked {} times!", state.click_count);

        let row = Row::new()
            .push(Box::new(Rectangle {
                width: 60.0,
                height: 70.0,
                color: [1.0, 0.0, 0.0],
            }))
            .push(Box::new(Rectangle {
                width: 90.0,
                height: 40.0,
                color: [1.0, 1.0, 0.0],
            }));

        let clm = Column::new()
            .push(Box::new(
                Button::new(
                    Box::new(Text::new(text_content).size(24.0)),
                    Message::Clicked,
                )
                .color([0.1, 0.4, 0.1]),
            ))
            .push(Box::new(Rectangle {
                width: 150.0,
                height: 50.0,
                color: [0.0, 0.0, 1.0],
            }))
            .push(Box::new(Rectangle {
                width: 50.0,
                height: 150.0,
                color: [0.0, 1.0, 1.0], // Cyan
            }))
            .push(Box::new(row));
        Box::new(clm)
    }
}
