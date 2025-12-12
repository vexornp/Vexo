use glyphon::{
    cosmic_text, Action, Attrs, Buffer, Color, Edit, Editor, FontSystem, Metrics, Shaping,
    SwashCache, TextBounds, Viewport,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use taffy::prelude::*;
use winit::event::*;
use winit::keyboard::{Key, NamedKey};
use winit::{
    application::ApplicationHandler, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window,
};

pub use uniffi;

const CLEAR_COLOR: wgpu::Color = wgpu::Color::BLUE;

mod shaders {
    // Use include_str! to load separate "shader.wgsl" file at compile time.
    // This ensures it works on iOS/Android without complex file IO.
    // Assumes shader.wgsl is in the project root (parent of src/).
    pub const WGSL: &str = include_str!("./shader.wgsl");
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
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

pub struct WidgetContext {
    pub editors: HashMap<String, Arc<Mutex<Editor<'static>>>>,
    // id stack for deterministic widget id generation
    pub id_stack: Vec<u64>,
    // Mapping from layout NodeId -> computed WidgetId for this frame
    pub node_to_widget: HashMap<NodeId, WidgetId>,

    pub font_system: FontSystem,
}

extern crate alloc;

impl WidgetContext {
    fn new() -> Self {
        // Embed a font so we are guaranteed to have one available.
        // Eg: we can't get the system font on ios platform
        let font_data = include_bytes!("../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        // font_system.db_mut().load_font_data(font_data);
        let font_system = FontSystem::new_with_fonts([binary]);

        Self {
            editors: HashMap::new(),
            id_stack: vec![0x9E3779B97F4A7C15u64],
            node_to_widget: HashMap::new(),
            font_system,
        }
    }

    pub fn reset_id_stack(&mut self) {
        self.id_stack.clear();
        self.id_stack.push(0x9E3779B97F4A7C15u64);
        // clear per-frame node->widget mapping
        self.node_to_widget.clear();
    }

    pub fn push_index(&mut self, idx: usize) {
        let parent = *self.id_stack.last().unwrap();
        let child = parent
            .wrapping_mul(0x9E3779B97F4A7C15u64)
            .wrapping_add(idx as u64 + 1);
        self.id_stack.push(child);
    }

    pub fn push_key(&mut self, key: &str) {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        let key_hash = s.finish();
        let parent = *self.id_stack.last().unwrap();
        let child = parent
            .wrapping_mul(0x9E3779B97F4A7C15u64)
            .wrapping_add(key_hash);
        self.id_stack.push(child);
    }

    pub fn pop(&mut self) {
        if self.id_stack.len() > 1 {
            self.id_stack.pop();
        }
    }

    pub fn current_widget_id(&self) -> WidgetId {
        WidgetId(*self.id_stack.last().unwrap())
    }

    pub fn record_node_widget(&mut self, node: NodeId) {
        let wid = self.current_widget_id();
        self.node_to_widget.insert(node, wid);
    }

    pub fn get_widget_id(&self, node: NodeId) -> Option<WidgetId> {
        self.node_to_widget.get(&node).copied()
    }

    pub fn get_or_create_editor(
        &mut self,
        id: &str,
        initial_text: &str,
        font_size: f32,
    ) -> Arc<Mutex<Editor<'static>>> {
        self.editors
            .entry(id.to_string())
            .or_insert_with(|| {
                let metrics = Metrics::new(font_size, font_size * 1.25);
                let mut editor = Editor::new(Buffer::new_empty(metrics));
                editor.with_buffer_mut(|buffer| {
                    buffer.set_text(
                        &mut self.font_system,
                        initial_text,
                        &Attrs::new(),
                        Shaping::Advanced,
                    );
                });
                Arc::new(Mutex::new(editor))
            })
            .clone()
    }
}

pub struct FrameworkState<A: Application + 'static> {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    window: Option<Arc<Window>>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    batcher: UiBatcher,
    cursor_pos: (f32, f32),
    taffy: taffy::TaffyTree,
    root_widget: Box<dyn Widget<A::Message>>,
    root_node_id: NodeId,

    // --- Text Rendering Fields --
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
    swash_cache: glyphon::SwashCache,
    scale_factor: f32,
    viewport: glyphon::Viewport,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Editor
    focused_widget_id: Option<WidgetId>,
    widget_context: WidgetContext,
}

impl<A: Application + 'static> FrameworkState<A> {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
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

    pub async fn new_with_ios(
        metal_layer_ptr: *mut std::ffi::c_void,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(
                    metal_layer_ptr,
                ))
                .unwrap()
        };

        let physical_width = logical_width * scale_factor;
        let physical_height = logical_height * scale_factor;

        Self::init(
            surface,
            instance,
            physical_width,
            physical_height,
            scale_factor,
            None,
        )
        .await
    }

    async fn init(
        surface: wgpu::Surface<'static>,
        instance: wgpu::Instance,
        physical_width: f32,
        physical_height: f32,
        scale_factor: f32,
        window: Option<Arc<Window>>,
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
            source: wgpu::ShaderSource::Wgsl(shaders::WGSL.into()),
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
            batcher: UiBatcher::new(),
            cursor_pos: (0.0, 0.0),
            taffy,
            root_widget,
            root_node_id,
            atlas,
            text_renderer,
            swash_cache,
            scale_factor,
            viewport,
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            focused_widget_id: None,
            widget_context: ctx,
        })
    }

    pub fn resize_by_logical_point(&mut self, width: f32, height: f32) {
        self.resize_by_pixel_point(width * self.scale_factor, height * self.scale_factor);
    }

    pub fn resize_by_pixel_point(&mut self, width: f32, height: f32) {
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

        // Taffy should layout in logical points so that 24.0 size means 24 points.
        let logical_width = self.config.width as f32 / self.scale_factor;
        let logical_height = self.config.height as f32 / self.scale_factor;

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
                let left_pos = req.position.0 * self.scale_factor;
                let top_pos = req.position.1 * self.scale_factor;

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

        // Prepare renderer for Text
        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.widget_context.font_system,
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
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: &winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                println!("Window closed by user");
                return;
            }
            WindowEvent::Resized(size) => {
                self.resize_by_pixel_point(size.width as f32, size.height as f32);
                println!("Window resized to: {}x{}", size.width, size.height);
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Convert logical position to physical position
                self.cursor_pos = (
                    position.x as f32 / self.scale_factor,
                    position.y as f32 / self.scale_factor,
                );
                return;
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    log::error!("Render error: {}", e);
                }
            }
            _ => {
                println!("Unhandled window event: {:?}", event);
            }
        }

        // Pass the event to the root widget (which passes it down)
        let widget_response = self.root_widget.on_event(
            &self.taffy,
            self.root_node_id,
            (0.0, 0.0),
            event,
            self.cursor_pos,
            self.focused_widget_id,
            &mut self.widget_context,
        );

        // Handle Framework Logic
        if let Some(focus_request) = widget_response.focus_request {
            self.focused_widget_id = Some(focus_request);
            println!("Focus requested by widget: {:?}", focus_request);
        }

        //  Handle User Logic
        if let Some(msg) = widget_response.message {
            println!("User message received: {:?}", msg);
            self.update(msg);
        }
    }

    #[allow(dead_code)]
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

            let widget_response = self.root_widget.on_event(
                &self.taffy,
                self.root_node_id,
                (0.0, 0.0),
                &event,
                self.cursor_pos,
                self.focused_widget_id,
                &mut self.widget_context,
            );

            if let Some(msg) = widget_response.message {
                self.update(msg);
            }
        }
    }

    fn view(&self) -> Box<dyn Widget<A::Message>> {
        A::view(&self.user_app_state)
    }

    pub fn handle_tap(&mut self, x: f32, y: f32) {
        //Convert the mobile platform input logical size to Winit's PhysicalPosition
        let physical_x = x * self.scale_factor;
        let physical_y = y * self.scale_factor;
        self.cursor_pos = (physical_x, physical_y);

        // Simulate Press Event (Mouse Down)
        self.handle_mouse_click(ElementState::Pressed, MouseButton::Left);
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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: FrameworkState<A>) {
        self.framework_state = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.framework_state {
            Some(canvas) => canvas,
            None => return,
        };
        state.handle_window_event(event_loop, _window_id, &event);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// Create a WidgetId deterministically from a stable `key` string.
    ///
    /// This uses the default std hasher to produce a 64-bit value.
    /// Prefer using the framework-provided path-mixing (via `WidgetContext`) when
    /// deriving ids from traversal paths, but this helper is convenient when
    /// you only have a developer-provided key and want a `WidgetId`.
    pub fn from_key(key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        WidgetId(s.finish())
    }

    /// Mix this id with a child index to derive a child WidgetId along the
    /// deterministic path (same mixing constant used in `WidgetContext`).
    pub fn mix_with_index(&self, idx: usize) -> Self {
        WidgetId(
            self.0
                .wrapping_mul(0x9E3779B97F4A7C15u64)
                .wrapping_add(idx as u64 + 1),
        )
    }

    /// Mix this id with a key string to derive a child WidgetId along the
    /// deterministic path (same mixing constant used in `WidgetContext`).
    pub fn mix_with_key(&self, key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        let key_hash = s.finish();
        WidgetId(
            self.0
                .wrapping_mul(0x9E3779B97F4A7C15u64)
                .wrapping_add(key_hash),
        )
    }
}

pub struct WidgetResponse<M> {
    /// The user-defined message
    pub message: Option<M>,

    /// If Some(id), this widget want to grab the keyboard focus.
    pub focus_request: Option<WidgetId>,

    /// Did the widget consume this event? (Stops propagation)
    pub handled: bool,
}

impl<M> Default for WidgetResponse<M> {
    fn default() -> Self {
        Self {
            message: None,
            focus_request: None,
            handled: false,
        }
    }
}

// The deterministic id stack + NodeId->WidgetId mapping is used instead of
// a global incremental id generator.

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    /// Widget unique ID. (Used for focus tracking)
    /// Default implementation returns `WidgetId(0)`; prefer using the
    /// NodeId->WidgetId mapping stored in `WidgetContext` instead.
    fn id(&self) -> WidgetId {
        WidgetId(0)
    }
    /// Optional stable key for identity across reorders.
    fn key(&self) -> Option<&str> {
        None
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId;

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>, // Current focused widget (if have one), // Pass focus here for drawing. (eg: draw a blue border when focused)
        ctx: &mut WidgetContext,
    );

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>, // Current focused widget (if have one)
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M>;
}

pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
    pub key: Option<String>,
}

impl Rectangle {
    pub fn new(width: f32, height: f32, color: [f32; 3]) -> Self {
        Self {
            width,
            height,
            color,
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Rectangle {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let node = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(self.width),
                    height: length(self.height),
                },
                ..Default::default()
            })
            .unwrap();

        // record the mapping node -> computed WidgetId for this frame
        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_rect(x, y, layout.size.width, layout.size.height, self.color);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Column<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for (i, child) in self.children.iter_mut().enumerate() {
            if let Some(k) = child.key() {
                ctx.push_key(k);
            } else {
                ctx.push_index(i);
            }
            let node = child.layout(taffy, ctx);
            child_nodes.push(node);
            ctx.pop();
        }
        let node = taffy
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
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,

        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                (my_x, my_y),
                focused_id,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let my_offset = (my_x, my_y);

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response = child.on_event(
                taffy,
                child_node_id,
                my_offset,
                event,
                cursor_pos,
                focused_id,
                ctx,
            );

            // If a child handled it or request focus, return imediately
            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}

pub struct Row<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Row<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Row<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for (i, child) in self.children.iter_mut().enumerate() {
            if let Some(k) = child.key() {
                ctx.push_key(k);
            } else {
                ctx.push_index(i);
            }
            let node = child.layout(taffy, ctx);
            child_nodes.push(node);
            ctx.pop();
        }
        let node = taffy
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
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                (my_x, my_y),
                focused_id,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_x = offset.0 + layout.location.x;
        let my_y = offset.1 + layout.location.y;
        let my_offset = (my_x, my_y);

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response = child.on_event(
                taffy,
                child_node_id,
                my_offset,
                event,
                cursor_pos,
                focused_id,
                ctx,
            );

            // If a child handled it or request focus, return imediately
            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: [f32; 3],
    pub padding: f32,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: [0.2, 0.2, 0.2],
            padding: 10.0,
            key: None,
        }
    }

    pub fn color(mut self, color: [f32; 3]) -> Self {
        self.background_color = color;
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Button<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // push content index (single child)
        ctx.push_index(1);
        let content_node = self.content.layout(taffy, ctx);
        ctx.pop();
        let node = taffy
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
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
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
            self.content.draw(
                taffy,
                *content_node,
                renderer,
                content_offset,
                focused_id,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
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
                return WidgetResponse {
                    message: Some(self.on_press.clone()),
                    focus_request: None,
                    handled: true,
                };
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
                focused_id,
                ctx,
            );
        }

        WidgetResponse::default()
    }
}

pub struct Text {
    pub content: String,
    pub size: f32,
    pub color: [f32; 3],
    pub style: taffy::Style,
    pub key: Option<String>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            size: 24.0,
            color: [0.0, 0.0, 0.0],
            style: Style::default(),
            key: None,
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

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
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

        let node = taffy.new_leaf(style).unwrap();
        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_text(self.content.clone(), x, y, self.size, self.color);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}

pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: [f32; 3],
    pub style: taffy::Style,
    pub key: Option<String>,
}

impl TextEdit {
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            editor_id: id.into(),
            initial_text: initial_text.into(),
            swash_cache: SwashCache::new(),
            text_color: [1.0, 1.0, 1.0],
            style: Style::default(),
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn style(mut self, style: taffy::Style) -> Self {
        self.style = style;
        self
    }

    pub fn size(self, size: (f32, f32)) -> Self {
        self.style(Style {
            size: Size {
                width: Dimension::length(size.0),
                height: Dimension::length(size.1),
            },
            ..Default::default()
        })
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for TextEdit {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let node_id = taffy.new_leaf(self.style.clone()).unwrap();
        // record mapping for this TextEdit node
        ctx.record_node_widget(node_id);
        let layout = taffy.layout(node_id).unwrap();
        let w = layout.size.width;
        let h = layout.size.height;

        let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text, 24.0);
        let mut editor = editor_arc.lock().unwrap();

        editor.with_buffer_mut(|buffer| {
            buffer.set_size(&mut ctx.font_system, Some(w), Some(h));
        });
        editor.shape_as_needed(&mut ctx.font_system, true);

        node_id
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        _focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;

        // For-Debug
        let w = layout.size.width;
        let h = layout.size.height;
        renderer.add_rect(x, y, w, h, [1.0, 0.0, 0.0]);

        // Retrieve Editor
        let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text, 24.0);
        let editor = editor_arc.lock().unwrap();

        // Extract text content from buffer
        let text_content = editor.with_buffer(|buffer| {
            let mut content = String::new();
            for line in buffer.lines.iter() {
                content.push_str(line.text());
                content.push('\n');
            }
            if content.ends_with('\n') {
                content.pop();
            }
            content
        });

        renderer.add_text(text_content, x, y, 24.0, self.text_color);

        let text_color = Color::rgb(0xFF, 0xFF, 0xFF);
        let cursor_color = Color::rgb(0xFF, 0xFF, 0xFF);
        let selection_color = Color::rgba(0xFF, 0xFF, 0xFF, 0x33);
        let selected_text_color = Color::rgb(0xA0, 0xA0, 0xFF);

        let mut cache = SwashCache::new();
    }

    fn on_event(
        &mut self,
        _taffy: &taffy::TaffyTree,
        _node: NodeId,
        _offset: (f32, f32),
        _event: &winit::event::WindowEvent,
        _cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        // Determine our widget id from the node->widget mapping
        let my_id = ctx.get_widget_id(_node);
        let is_focused = focused_id == my_id;

        if !is_focused {
            // Check for click to grab focus
            if let WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } = _event
            {
                let layout = _taffy.layout(_node).unwrap();
                let x = _offset.0 + layout.location.x;
                let y = _offset.1 + layout.location.y;
                let width = layout.size.width;
                let height = layout.size.height;

                let is_over = _cursor_pos.0 >= x
                    && _cursor_pos.0 <= x + width
                    && _cursor_pos.1 >= y
                    && _cursor_pos.1 <= y + height;

                if is_over {
                    // Request focus
                    return WidgetResponse {
                        message: None,
                        focus_request: my_id,
                        handled: true,
                    };
                }
            }
            return WidgetResponse::default();
        }

        // We are focused, so handle keyboard input
        let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text, 24.0);
        let mut editor = editor_arc.lock().unwrap();
        let mut _ctrl_pressed = false;
        let mut _mouse_x: f64 = 0.0;
        let mut _mouse_y: f64 = 0.0;
        let _mouse_left = ElementState::Released;

        match _event {
            WindowEvent::ModifiersChanged(modifiers) => {
                _ctrl_pressed = modifiers.state().control_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let KeyEvent {
                    logical_key, state, ..
                } = event;

                if state.is_pressed() {
                    match logical_key {
                        Key::Named(NamedKey::ArrowLeft) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::Left),
                            );
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::Right),
                            );
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::Up),
                            );
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::Down),
                            );
                        }
                        Key::Named(NamedKey::Home) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::Home),
                            );
                        }
                        Key::Named(NamedKey::End) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::End),
                            );
                        }
                        Key::Named(NamedKey::PageUp) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::PageUp),
                            );
                        }
                        Key::Named(NamedKey::PageDown) => {
                            editor.action(
                                &mut ctx.font_system,
                                Action::Motion(cosmic_text::Motion::PageDown),
                            );
                        }
                        Key::Named(NamedKey::Escape) => {
                            editor.action(&mut ctx.font_system, Action::Escape);
                        }
                        Key::Named(NamedKey::Enter) => {
                            editor.action(&mut ctx.font_system, Action::Enter);
                        }
                        Key::Named(NamedKey::Backspace) => {
                            editor.action(&mut ctx.font_system, Action::Backspace);
                        }
                        Key::Named(NamedKey::Delete) => {
                            editor.action(&mut ctx.font_system, Action::Delete);
                        }
                        Key::Character(text) => {
                            if _ctrl_pressed {
                                // Handle Ctrl + Char
                                match text.as_str() {
                                    "c" => {
                                        // TODO: Copy
                                    }
                                    "v" => {
                                        // TOOD: Paste
                                    }
                                    "x" => {
                                        // TODO: Cut
                                    }
                                    _ => {
                                        // Ignore other Ctrl + Char combinations
                                    }
                                }
                            } else {
                                // Normal character input
                                for c in text.chars() {
                                    if c.is_control() {
                                        // Ignore control characters
                                        continue;
                                    }
                                    editor.action(&mut ctx.font_system, Action::Insert(c));
                                }
                            }
                        }
                        _ => {
                            // Ignore other keys
                        }
                    }
                }
            }
            WindowEvent::CursorMoved {
                device_id: _,
                position,
            } => {
                // Update saved mouse position for use when handling click events
                // This is used to handle mouse click events later
                _mouse_x = position.x;
                _mouse_y = position.y;

                if _mouse_left.is_pressed() {
                    // Update selection
                    editor.action(
                        &mut ctx.font_system,
                        Action::Drag {
                            x: position.x as i32,
                            y: position.y as i32,
                        },
                    );
                }
            }
            _ => {}
        }

        WidgetResponse {
            message: None,
            focus_request: None,
            handled: true,
        }
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

pub struct EditorRequest<'a> {
    pub origin_x: f32,
    pub origin_y: f32,
    pub editor: Editor<'a>,
}

pub struct UiBatcher {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_requests: Vec<TextRequest>, // For normal Text widget
    pub editor_request: Option<EditorRequest<'static>>, // For TextEdit widget

    screen_width: f32,  // Logical width: pixel_width * scale_factor
    screen_height: f32, // Logical height: pixel_height * scale_factor
}

impl UiBatcher {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            text_requests: Vec::new(),
            editor_request: None,
            screen_width: 1.0,
            screen_height: 1.0,
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.text_requests.clear();
    }

    // Set logical size
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn add_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 3]) {
        let sw = self.screen_width;
        let sh = self.screen_height;

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

pub trait Application {
    type Message: Clone + std::fmt::Debug + Send;
    type State: Sized;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;
}

pub fn run_desktop_demo<A: Application + 'static>() -> anyhow::Result<()> {
    env_logger::init();
    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    let mut app = crate::MyApp::<A>::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
