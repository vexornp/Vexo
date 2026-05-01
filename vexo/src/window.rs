use std::sync::Arc;

use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::core::{Logical, Physical, Point, Scale, Size, WidgetId};
use crate::frame_context::FrameContext;
use crate::input::{ButtonState, CursorIcon, InputEvent, Modifiers};
use crate::layout::{LayoutContext, LayoutEngine, LayoutNodeId, LayoutView, TaffyLayoutEngine};
use crate::render::{RenderBackend, WgpuBackend};
use crate::render_pipeline::RenderPipeline;
use crate::retain::{ThreeTreePipeline, Widget as RetainWidget};
use crate::state::CursorBlinkState;
use crate::widgets::{Column, Widget, WidgetContext, WidgetResponse};
use crate::Application;

pub struct WindowState<A: Application + 'static> {
    // GPU rendering backend
    backend: WgpuBackend,
    window: Option<Arc<dyn Window>>,

    batcher: crate::UiBatcher,
    layout_engine: Box<dyn LayoutEngine>,
    root_widget: Box<dyn Widget<A::Message>>,
    root_node_id: LayoutNodeId,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Editor
    focused_widget_id: Option<WidgetId>,
    pub widget_context: WidgetContext,

    // Cursor blink state (global - only one focused widget at a time)
    cursor_blink: CursorBlinkState,

    // Current cursor icon (for detecting changes)
    current_cursor: CursorIcon,

    // Render pipeline for orchestrating render stages
    render_pipeline: RenderPipeline,

    // Three-tree pipeline (new retain-mode system)
    retain_pipeline: Option<ThreeTreePipeline<A::Message>>,

    // Flag to enable retain mode (false = use immediate mode for compatibility)
    #[allow(dead_code)]
    use_retain_mode: bool,
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
            batcher: crate::UiBatcher::new(),
            layout_engine,
            root_widget,
            root_node_id,
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            focused_widget_id: None,
            widget_context: ctx,
            cursor_blink: CursorBlinkState::new(),
            current_cursor: CursorIcon::default(),
            render_pipeline: RenderPipeline::new(),
            retain_pipeline: Some(ThreeTreePipeline::new()),
            use_retain_mode: true, // Start with retain mode by default
        })
    }

    /// Enable or disable retain-mode rendering.
    ///
    /// When enabled and the application implements `retain_view()`,
    /// the three-tree pipeline will be used for rendering.
    pub fn set_retain_mode(&mut self, enabled: bool) {
        self.use_retain_mode = enabled;
    }

    /// Toggle retain mode and sync with application state.
    fn toggle_retain_mode(&mut self) {
        self.use_retain_mode = !self.use_retain_mode;
        if let Some(win) = &self.window {
            win.request_redraw();
        }
        println!("Retain mode: {}", self.use_retain_mode);
    }

    pub fn resize(&mut self, size: Size<Physical>) {
        let config =
            crate::render::RenderConfig::new(size, Scale::new(self.widget_context.scale.factor() as f64));
        self.backend.resize(config);

        if size.width > 0.0 && size.height > 0.0 {
            //Force re-layout - create a dummy leaf node
            self.root_node_id = self.layout_engine.create_leaf(&crate::layout::Layout::default());
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Check if we should use retain mode
        if self.use_retain_mode && self.view_retain().is_some() {
            return self.render_retain();
        }

        // Otherwise use immediate mode
        self.render_immediate()
    }

    /// Render using the immediate-mode pipeline (legacy).
    fn render_immediate(&mut self) -> Result<(), wgpu::SurfaceError> {
        // 1. Redraw request & backend check
        if let Some(win) = &self.window {
            win.request_redraw();
        }
        if !self.backend.is_ready() {
            return Ok(());
        }

        // 2. Frame timing
        self.cursor_blink.tick();

        // 3. View generation
        let mut new_root_widget = self.view();

        // 4. Clear state
        self.layout_engine.clear();
        self.batcher.clear();

        // 5. Compute layout
        let scale = self.widget_context.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        self.batcher.set_screen_size(logical_size);

        let layout_output = self.render_pipeline.compute_layout(
            &mut *new_root_widget,
            self.layout_engine.as_mut(),
            self.root_node_id,
            logical_size,
            &mut self.widget_context,
        );

        self.root_widget = new_root_widget;
        self.root_node_id = layout_output.root_node;

        // 6. Build frame context
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);

        let ctx = FrameContext {
            scale,
            viewport_physical: physical_size,
            layout_view: layout_output.layout_view,
            focused_widget_id: self.focused_widget_id,
            cursor_blink: &self.cursor_blink,
        };

        // 7. Generate geometry
        self.render_pipeline.generate_geometry(
            &*self.root_widget,
            &mut self.batcher,
            self.root_node_id,
            &ctx,
            &mut self.widget_context,
        );

        // 8. Update viewport
        self.backend.update_viewport(physical_size);

        // 9. Collect text
        let prepared_text = self.render_pipeline.collect_text(
            &mut self.batcher,
            &mut self.widget_context,
            scale,
            physical_size,
        );

        // 10. Execute render
        self.render_pipeline
            .execute_render(
                &mut self.backend,
                &self.batcher,
                prepared_text,
                &mut self.widget_context,
            )
            .map_err(|e| match e {
                crate::render::RenderError::SurfaceNotConfigured => wgpu::SurfaceError::Lost,
                crate::render::RenderError::AcquireFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::TextPrepareFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::GpuError(_) => wgpu::SurfaceError::Lost,
            })?;

        Ok(())
    }

    fn update(&mut self, message: A::Message) {
        A::update(&mut self.user_app_state, message);
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    pub fn handle_window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        event: &WindowEvent,
    ) {
        match event {
            // Framework events (no InputEvent conversion)
            WindowEvent::SurfaceResized(size) => {
                self.resize(Size::from_winit(*size));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.widget_context.scale = Scale::new(*scale_factor);
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.render() {
                    eprintln!("Error drawing window: {err}");
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            // User input events with special handling
            WindowEvent::PointerMoved { position, .. } => {
                // Store physical coords for rendering
                self.widget_context.cursor_pos =
                    Point::<Physical>::new(position.x as f32, position.y as f32);

                // Pass to widget tree for hit-testing
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.widget_context.scale)
                {
                    self.process_input_event(input_event);
                }
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                // Handle Escape key for app exit (framework-level shortcut)
                if matches!(
                    key_event,
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    }
                ) {
                    event_loop.exit();
                    return;
                }

                // Handle 'R' key to toggle retain mode
                if matches!(
                    key_event,
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::KeyR),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    }
                ) {
                    self.toggle_retain_mode();
                    return;
                }

                // Other keyboard input goes to widgets
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.widget_context.scale)
                {
                    self.process_input_event(input_event);
                }
            }

            // Other events that may convert to InputEvent
            _ => {
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.widget_context.scale)
                {
                    self.process_input_event(input_event);
                }
            }
        }
    }

    /// Process an InputEvent through the widget tree and handle responses.
    fn process_input_event(&mut self, input_event: InputEvent) {
        // Check if we should use retain mode
        if self.use_retain_mode && self.view_retain().is_some() {
            self.process_input_event_retain(input_event);
            return;
        }

        // Otherwise use immediate mode
        let layout_view = LayoutView::new(self.layout_engine.as_ref());
        let widget_response = self.root_widget.on_event(
            &layout_view,
            self.root_node_id,
            Point::new(0.0, 0.0),
            &input_event,
            self.focused_widget_id,
            &mut self.widget_context,
        );

        self.handle_widget_response(&widget_response, &input_event);
    }

    /// Process an input event through the retain-mode pipeline.
    fn process_input_event_retain(&mut self, input_event: InputEvent) {
        let position = match &input_event {
            InputEvent::PointerMoved { position } => *position,
            InputEvent::PointerButton { position, .. } => *position,
            _ => Point::new(0.0, 0.0),
        };

        // Get current modifiers - use default for now
        let modifiers = Modifiers::default();

        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return,
        };

        let message = pipeline.handle_event(position, &input_event, modifiers);

        if let Some(msg) = message {
            // The message is already typed as A::Message
            self.update(msg);
        }
    }

    /// Handle the response from widget event processing.
    fn handle_widget_response(&mut self, response: &WidgetResponse<A::Message>, input_event: &InputEvent) {
        // Handle focus changes
        if let Some(focus_request) = response.focus_request {
            self.focused_widget_id = Some(focus_request);
        } else if response.clear_focus {
            self.focused_widget_id = None;
        } else if !response.handled {
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = input_event
            {
                // Click outside any focusable widget - clear focus
                self.focused_widget_id = None;
            }
        }

        // Reset cursor blink on handled keyboard input
        if response.handled && matches!(input_event, InputEvent::Keyboard { .. }) {
            self.cursor_blink.reset();
        }

        // Handle user messages
        if let Some(msg) = response.message.clone() {
            self.update(msg);
        }

        // Handle cursor changes
        self.update_cursor(response, input_event);
    }

    /// Update cursor based on widget response.
    fn update_cursor(&mut self, response: &WidgetResponse<A::Message>, input_event: &InputEvent) {
        if let Some(cursor) = response.cursor {
            if cursor != self.current_cursor {
                self.current_cursor = cursor;
                if let Some(window) = &self.window {
                    window.set_cursor(winit_cursor_from_icon(cursor));
                }
            }
        } else if matches!(input_event, InputEvent::PointerMoved { .. }) {
            // Only reset to default on PointerMoved when no cursor is requested
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
            (KeyCode::Space, true) => {}
            _ => {}
        }
    }

    fn view(&self) -> Box<dyn Widget<A::Message>> {
        A::view(&self.user_app_state)
    }

    /// Generate a retain-mode widget tree from the application.
    ///
    /// Returns the widget tree from `Application::retain_view()`,
    /// or None if the application doesn't implement retain-mode.
    fn view_retain(&self) -> Option<Box<dyn RetainWidget<A::Message>>> {
        A::retain_view(&self.user_app_state)
    }

    /// Render using the three-tree retain-mode pipeline.
    ///
    /// This method implements the full retain-mode rendering flow:
    /// 1. Generate widget tree from view_retain()
    /// 2. Reconcile widget tree with element tree
    /// 3. Layout dirty render objects
    /// 4. Paint dirty render objects
    /// 5. Process RenderCommands through batcher
    /// 6. Submit to GPU
    #[allow(dead_code)]
    fn render_retain(&mut self) -> Result<(), wgpu::SurfaceError> {
        // 1. Redraw request & backend check
        if let Some(win) = &self.window {
            win.request_redraw();
        }
        if !self.backend.is_ready() {
            return Ok(());
        }

        // 2. Frame timing
        self.cursor_blink.tick();

        // 3. Generate widget tree
        let widget_tree = match self.view_retain() {
            Some(w) => w,
            None => return Ok(()), // No retain view, skip
        };

        // 4. Get pipeline
        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return Ok(()),
        };

        // 5. Clear batcher
        self.batcher.clear();

        // 6. Update widget tree (targeted rebuild or full reconcile)
        pipeline.update(widget_tree);

        // 7. Compute logical size
        let scale = self.widget_context.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        self.batcher.set_screen_size(logical_size);

        // 8. Layout dirty render objects
        pipeline.layout(
            logical_size,
            self.layout_engine.as_mut(),
            &mut self.widget_context.font_system,
        );

        // 9. Paint dirty render objects
        let commands = pipeline.paint();

        // 10. Process RenderCommands through batcher
        for cmd in commands {
            match cmd {
                crate::render::RenderCommand::Rect { bounds, fill, stroke, corner_radius } => {
                    self.batcher.add_rect(bounds, fill, stroke, corner_radius);
                }
                crate::render::RenderCommand::PushCornerRadius { radius } => {
                    self.batcher.push_corner_radius(radius);
                }
                crate::render::RenderCommand::PopCornerRadius => {
                    self.batcher.pop_corner_radius();
                }
                crate::render::RenderCommand::PushClip { bounds } => {
                    self.batcher.push_clip(bounds);
                }
                crate::render::RenderCommand::PopClip => {
                    self.batcher.pop_clip();
                }
                crate::render::RenderCommand::Text { content, position, font_size, color, max_width } => {
                    // Add text request for glyphon processing
                    self.batcher.text_requests.push(crate::renderer::TextRequest {
                        content,
                        position,
                        size: font_size,
                        color,
                        clip_bounds: self.batcher.current_clip(),
                    });
                    let _ = max_width; // TODO: Handle max_width for text wrapping
                }
                crate::render::RenderCommand::Editor { id, bounds, color } => {
                    self.batcher.editor_requests.push(crate::renderer::EditorRequest {
                        id,
                        bounds,
                        color,
                    });
                }
                crate::render::RenderCommand::PushOffset { offset } => {
                    // TODO: Implement offset stack in batcher
                    let _ = offset;
                }
                crate::render::RenderCommand::PopOffset => {
                    // TODO: Implement offset stack in batcher
                }
            }
        }

        // 11. Update viewport
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);
        self.backend.update_viewport(physical_size);

        // 12. Collect text through glyphon
        let prepared_text = self.render_pipeline.collect_text(
            &mut self.batcher,
            &mut self.widget_context,
            scale,
            physical_size,
        );

        // 13. Execute render
        self.render_pipeline
            .execute_render(
                &mut self.backend,
                &self.batcher,
                prepared_text,
                &mut self.widget_context,
            )
            .map_err(|e| match e {
                crate::render::RenderError::SurfaceNotConfigured => wgpu::SurfaceError::Lost,
                crate::render::RenderError::AcquireFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::TextPrepareFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::GpuError(_) => wgpu::SurfaceError::Lost,
            })?;

        Ok(())
    }
}
