use glyphon::{cosmic_text, Metrics, TextBounds};
use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;

use winit::{
    event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window,
};

pub use color::Color;
pub use uniffi;

mod app;
pub use app::{KeyBindingAction, VexoApp};

mod color;
pub mod core;
mod editor;
pub mod input;
pub mod layout;
mod macros;
mod quad_instance;
pub mod render;
mod renderer;
pub use renderer::UiBatcher;
pub mod component;
mod resource;
pub mod state;
pub mod testable;
mod utils;
pub mod widgets;

use render::{RenderBackend, WgpuBackend};
use renderer::TextRequest;
pub use widgets::WidgetExt;
use widgets::{Column, Widget, WidgetContext};
pub use winit::dpi::PhysicalPosition;

use crate::core::{Logical, Physical, Point, Scale, Size, WidgetId};
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{LayoutContext, LayoutEngine, LayoutNodeId, LayoutView, TaffyLayoutEngine};

pub use layout::AlignItems;

extern crate alloc;

// ============================================================================
// TEXT BUFFER CACHE
// ============================================================================

/// Cache key for text buffers to avoid recreating/shaping every frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_size_bits: u32,
    color_bits: [u32; 4],
}

impl TextCacheKey {
    fn from_request(req: &TextRequest) -> Self {
        Self {
            content: req.content.clone(),
            font_size_bits: req.size.to_bits(),
            color_bits: [
                req.color[0].to_bits(),
                req.color[1].to_bits(),
                req.color[2].to_bits(),
                req.color[3].to_bits(),
            ],
        }
    }
}

/// Cached text buffer with its shaped content.
struct CachedTextBuffer {
    buffer: glyphon::Buffer,
    /// Generation counter to detect stale entries
    generation: u64,
}

pub struct WindowState<A: Application + 'static> {
    // GPU rendering backend
    backend: WgpuBackend,
    window: Option<Arc<dyn Window>>,

    batcher: UiBatcher,
    layout_engine: Box<dyn LayoutEngine>,
    root_widget: Box<dyn Widget<A::Message>>,
    root_node_id: LayoutNodeId,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Editor
    focused_widget_id: Option<WidgetId>,
    widget_context: WidgetContext,

    // Cursor blink state (global - only one focused widget at a time)
    cursor_blink: CursorBlinkState,

    // Current cursor icon (for detecting changes)
    current_cursor: CursorIcon,

    // Text buffer cache to avoid recreating/shaping every frame
    text_cache: HashMap<TextCacheKey, CachedTextBuffer>,
    /// Generation counter for cache invalidation
    cache_generation: u64,
}

/// Tracks cursor blink timing for focused text inputs.
pub struct CursorBlinkState {
    /// Time of last tick (frame start)
    last_update: Instant,
    /// Accumulated milliseconds since last blink toggle
    accumulator_ms: f32,
    /// Whether cursor is currently visible (blink phase)
    visible: bool,
    /// Blink period in milliseconds (800ms default)
    blink_period_ms: f32,
}

impl CursorBlinkState {
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            accumulator_ms: 0.0,
            visible: true,
            blink_period_ms: 800.0,
        }
    }

    /// Call each frame to update blink state based on elapsed time.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let elapsed_ms = (now - self.last_update).as_millis() as f32;
        self.last_update = now;
        self.accumulator_ms += elapsed_ms;

        // Toggle visibility each time we exceed the period
        while self.accumulator_ms >= self.blink_period_ms {
            self.accumulator_ms -= self.blink_period_ms;
            self.visible = !self.visible;
        }
    }

    /// Reset blink to visible state (call on keyboard input).
    pub fn reset(&mut self) {
        self.accumulator_ms = 0.0;
        self.visible = true;
        self.last_update = Instant::now();
    }

    /// Is cursor currently visible?
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

/// Convert CursorIcon to winit's Cursor type.
fn winit_cursor_from_icon(icon: CursorIcon) -> winit::cursor::Cursor {
    // Map our CursorIcon to winit's CursorIcon, then convert to Cursor via From trait
    let winit_icon = match icon {
        CursorIcon::Default => winit::cursor::CursorIcon::Default,
        CursorIcon::Pointer => winit::cursor::CursorIcon::Pointer,
        CursorIcon::Text => winit::cursor::CursorIcon::Text,
        CursorIcon::Crosshair => winit::cursor::CursorIcon::Crosshair,
        CursorIcon::Move => winit::cursor::CursorIcon::Move,
        CursorIcon::NotAllowed => winit::cursor::CursorIcon::NotAllowed,
        CursorIcon::ResizeHorizontal => winit::cursor::CursorIcon::EwResize,
        CursorIcon::ResizeVertical => winit::cursor::CursorIcon::NsResize,
    };
    winit::cursor::Cursor::Icon(winit_icon)
}

impl<A: Application + 'static> WindowState<A> {
    pub async fn new(window: Arc<dyn Window>) -> anyhow::Result<Self> {
        let backend = WgpuBackend::new(window.clone()).await?;

        // --- Root Node Id ---
        let mut layout_engine = Box::new(TaffyLayoutEngine::new());
        let mut root_widget = Box::new(Column::new());
        let mut ctx = WidgetContext::new();
        let mut layout_ctx = LayoutContext::new(layout_engine.as_mut());
        let root_node_id = root_widget.layout(&mut layout_ctx, &mut ctx);

        Ok(Self {
            backend,
            window: Some(window),
            batcher: UiBatcher::new(),
            layout_engine,
            root_widget,
            root_node_id,
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            focused_widget_id: None,
            widget_context: ctx,
            cursor_blink: CursorBlinkState::new(),
            current_cursor: CursorIcon::default(),
            text_cache: HashMap::new(),
            cache_generation: 0,
        })
    }

    pub fn resize_physical(&mut self, size: Size<Physical>) {
        let config =
            render::RenderConfig::new(size, Scale::new(self.widget_context.scale.factor() as f64));
        self.backend.resize(config);

        if size.width > 0.0 && size.height > 0.0 {
            //Force re-layout - create a dummy leaf node
            self.root_node_id = self.layout_engine.create_leaf(&layout::Layout::default());
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if let Some(win) = &self.window {
            win.request_redraw();
        }

        if !self.backend.is_ready() {
            return Ok(());
        }

        // Update cursor blink state
        self.cursor_blink.tick();

        let mut new_root_widget = self.view();
        self.layout_engine.clear();
        self.batcher.clear();

        let scale = self.widget_context.scale;

        // Layout should work in logical points so that 24.0 size means 24 points.
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        // Set screen size once per frame
        self.batcher.set_screen_size(logical_size);

        // Build layout tree
        let mut layout_ctx = LayoutContext::new(self.layout_engine.as_mut());
        let new_root_node_id = new_root_widget.layout(&mut layout_ctx, &mut self.widget_context);

        // Compute layout
        self.layout_engine.compute(
            new_root_node_id,
            logical_size,
            &mut self.widget_context.font_system,
        );

        self.root_widget = new_root_widget;
        self.root_node_id = new_root_node_id;

        // 1. DRAW RECTANGLES: Generate geometry data
        let layout_view = LayoutView::new(self.layout_engine.as_ref());
        self.root_widget.draw(
            &layout_view,
            self.root_node_id,
            &mut self.batcher,
            Point::new(0.0, 0.0),
            self.focused_widget_id,
            &self.cursor_blink,
            &mut self.widget_context,
        );

        // 2. GLYPHON PREPARATION: Prepare text geometry using Taffy positions
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);

        // Update viewport resolution
        self.backend.update_viewport(physical_size);

        // Increment generation for cache eviction tracking
        self.cache_generation += 1;
        let current_gen = self.cache_generation;

        let mut processed_texts: Vec<(glyphon::Buffer, TextRequest)> = Vec::new();

        for req in self.batcher.text_requests.drain(..) {
            let cache_key = TextCacheKey::from_request(&req);

            // Try to get cached buffer
            let buffer = if let Some(cached) = self.text_cache.get_mut(&cache_key) {
                cached.generation = current_gen;
                cached.buffer.clone()
            } else {
                // Create and shape new buffer
                let mut buffer = glyphon::Buffer::new(
                    &mut self.widget_context.font_system,
                    Metrics::new(req.size, req.size * 1.2),
                );

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

                // Cache the buffer
                self.text_cache.insert(cache_key, CachedTextBuffer {
                    buffer: buffer.clone(),
                    generation: current_gen,
                });

                buffer
            };

            processed_texts.push((buffer, req));
        }

        // Periodically evict stale cache entries (every 100 frames)
        if current_gen % 100 == 0 {
            self.text_cache.retain(|_, cached| {
                current_gen - cached.generation < 100
            });
        }

        // Create Text Areas from the processed buffers and Taffy positions
        let text_areas: Vec<glyphon::TextArea> = processed_texts
            .iter_mut()
            .map(|(buffer, req)| {
                // Convert logical position to physical for glyphon
                let physical_pos = req.position.to_physical(scale);

                // Use clip bounds if set, otherwise use screen bounds
                let (bounds_left, bounds_top, bounds_right, bounds_bottom) = if req.clip_bounds[2]
                    > 0.0
                {
                    // Clip bounds are in logical coordinates - convert to physical
                    let clip_left = req.clip_bounds[0] * scale.factor();
                    let clip_top = req.clip_bounds[1] * scale.factor();
                    let clip_right = (req.clip_bounds[0] + req.clip_bounds[2]) * scale.factor();
                    let clip_bottom = (req.clip_bounds[1] + req.clip_bounds[3]) * scale.factor();
                    (
                        clip_left.floor() as i32,
                        clip_top.floor() as i32,
                        clip_right.ceil() as i32,
                        clip_bottom.ceil() as i32,
                    )
                } else {
                    // No clipping - use full screen
                    (
                        physical_pos.x.floor() as i32,
                        physical_pos.y.floor() as i32,
                        physical_size.width_u32() as i32,
                        physical_size.height_u32() as i32,
                    )
                };

                let color_rgba_u8 = cosmic_text::Color::rgba(
                    (req.color[0] * 255.0) as u8,
                    (req.color[1] * 255.0) as u8,
                    (req.color[2] * 255.0) as u8,
                    (req.color[3] * 255.0) as u8,
                );

                glyphon::TextArea {
                    buffer: buffer,
                    left: physical_pos.x,
                    top: physical_pos.y,
                    scale: scale.factor(),
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
            // Convert logical bounds to physical
            let physical_rect = req.bounds.to_physical(scale);

            let bounds_left: i32 = physical_rect.origin.x.floor() as i32;
            let bounds_top: i32 = physical_rect.origin.y.floor() as i32;
            let bounds_right: i32 =
                (physical_rect.origin.x + physical_rect.size.width).ceil() as i32;
            let bounds_bottom: i32 =
                (physical_rect.origin.y + physical_rect.size.height).ceil() as i32;

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
                physical_rect.origin.x,
                physical_rect.origin.y,
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

        // Upload geometry to backend
        self.backend.upload_geometry(&self.batcher);

        // Combine text areas (regular + editor) and prepare glyphon once.
        let mut combined_text_areas = text_areas;
        combined_text_areas.extend(editor_areas.into_iter());

        // Prepare text rendering
        self.backend
            .prepare_text(&mut self.widget_context.font_system, combined_text_areas);

        // Execute render pass
        let instance_count = self.batcher.quad_instances.len();
        self.backend
            .execute_render_pass(instance_count)
            .map_err(|e| match e {
                render::RenderError::SurfaceNotConfigured => wgpu::SurfaceError::Lost,
                render::RenderError::AcquireFailed(_) => wgpu::SurfaceError::Lost,
                render::RenderError::TextPrepareFailed(_) => wgpu::SurfaceError::Lost,
                render::RenderError::GpuError(_) => wgpu::SurfaceError::Lost,
            })?;

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
        _event_loop: &dyn ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: &winit::event::WindowEvent,
    ) {
        // Convert winit event to InputEvent
        let input_event =
            crate::input::InputEvent::from_winit(event, self.widget_context.scale.clone());

        // Only process events that convert to InputEvent
        let Some(input_event) = input_event else {
            return;
        };

        // Pass the event to the root widget (which passes it down)
        let layout_view = LayoutView::new(self.layout_engine.as_ref());
        let widget_response = self.root_widget.on_event(
            &layout_view,
            self.root_node_id,
            Point::new(0.0, 0.0),
            &input_event,
            self.focused_widget_id,
            &mut self.widget_context,
        );

        // Handle Framework Logic
        if let Some(focus_request) = widget_response.focus_request {
            self.focused_widget_id = Some(focus_request);
            println!("Focus requested by widget: {:?}", focus_request);
        } else if widget_response.clear_focus {
            self.focused_widget_id = None;
        } else if !widget_response.handled {
            if let crate::input::InputEvent::PointerButton {
                state: crate::input::ButtonState::Pressed,
                ..
            } = input_event
            {
                // Click outside any focusable widget - clear focus
                self.focused_widget_id = None;
            }
        }

        // Check if event if handled, notify if needed
        if widget_response.handled {
            // Reset cursor blink on keyboard input
            if let crate::input::InputEvent::Keyboard { .. } = input_event {
                self.cursor_blink.reset();
            }
        }

        //  Handle User Logic
        if let Some(msg) = widget_response.message {
            println!("User message received: {:?}", msg);
            self.update(msg);
        }

        // Handle cursor changes
        // Only update cursor on PointerMoved events to avoid resetting during clicks
        if let Some(cursor) = widget_response.cursor {
            if cursor != self.current_cursor {
                self.current_cursor = cursor;
                if let Some(window) = &self.window {
                    window.set_cursor(winit_cursor_from_icon(cursor));
                }
            }
        } else if matches!(input_event, InputEvent::PointerMoved { .. }) {
            // Only reset to default on PointerMoved when no cursor is requested
            // This prevents cursor from resetting during click/release events
            if self.current_cursor != CursorIcon::Default {
                self.current_cursor = CursorIcon::Default;
                if let Some(window) = &self.window {
                    window.set_cursor(winit_cursor_from_icon(CursorIcon::Default));
                }
            }
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
        self.resize_physical(Size::new(size.width as f32, size.height as f32));
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
        let _event_loop_proxy = event_loop.create_proxy();
        let _sender = sender.clone();
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

    let app = VexoApp::<A>::new(&event_loop, receiver, sender);

    // let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    // let mut app = crate::VexoApp::<A>::new();
    // event_loop.run_app(&mut app)?;
    Result::Ok(event_loop.run_app(app)?)
}
