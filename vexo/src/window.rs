use std::sync::Arc;

use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::core::{Logical, Physical, Point, Scale, Size, WidgetId};
use crate::frame_context::FrameContext;
use crate::input::{ButtonState, CursorIcon, InputEvent};
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
    retain_pipeline: Option<ThreeTreePipeline>,

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
            use_retain_mode: false, // Start with immediate mode for compatibility
        })
    }

    /// Enable or disable retain-mode rendering.
    ///
    /// When enabled and the application implements `retain_view()`,
    /// the three-tree pipeline will be used for rendering.
    pub fn set_retain_mode(&mut self, enabled: bool) {
        self.use_retain_mode = enabled;
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
    fn view_retain(&self) -> Option<Box<dyn RetainWidget>> {
        A::retain_view(&self.user_app_state)
    }

    /// Render using the three-tree retain-mode pipeline.
    ///
    /// This method implements the full retain-mode rendering flow:
    /// 1. Generate widget tree from view_retain()
    /// 2. Reconcile widget tree with element tree
    /// 3. Layout dirty render objects
    /// 4. Paint dirty render objects
    /// 5. Submit to GPU
    ///
    /// Currently disabled by default (use_retain_mode = false).
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
            Some(tree) => tree,
            None => return Ok(()), // Application doesn't support retain mode
        };

        // 4. Get pipeline (return early if not initialized)
        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return Ok(()),
        };

        // 5. Reconcile widget tree with element tree
        pipeline.reconcile(widget_tree);

        // 6. Compute logical size
        let scale = self.widget_context.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        // 7. Layout dirty render objects
        pipeline.layout(logical_size, self.layout_engine.as_mut());

        // 8. Paint dirty render objects
        let _commands = pipeline.paint();

        // 9. Submit to GPU (placeholder - will be integrated with batcher)
        // TODO: Process RenderCommands through batcher
        // self.batcher.clear();
        // for cmd in commands {
        //     // Convert RenderCommand to batcher operations
        // }

        Ok(())
    }
}
