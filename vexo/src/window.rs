use std::sync::Arc;

use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::animation::AnimationTicker;
use crate::core::{Absolute, Logical, Physical, Point, Scale, Size};
use crate::input::{ButtonState, InputEvent, Modifiers, SystemCursorKind};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};
use crate::render::{RenderBackend, WgpuBackend};
use crate::text_pipeline::TextPipeline;
use crate::{ThreeTreePipeline, Widget as RetainWidget};
use crate::Application;

fn system_cursor_to_winit(kind: SystemCursorKind) -> winit::cursor::CursorIcon {
    match kind {
        SystemCursorKind::Arrow => winit::cursor::CursorIcon::Default,
        SystemCursorKind::Pointer => winit::cursor::CursorIcon::Pointer,
        SystemCursorKind::Text => winit::cursor::CursorIcon::Text,
        SystemCursorKind::Crosshair => winit::cursor::CursorIcon::Crosshair,
        SystemCursorKind::Move => winit::cursor::CursorIcon::Move,
        SystemCursorKind::NotAllowed => winit::cursor::CursorIcon::NotAllowed,
        SystemCursorKind::ResizeHorizontal => winit::cursor::CursorIcon::EwResize,
        SystemCursorKind::ResizeVertical => winit::cursor::CursorIcon::NsResize,
    }
}

pub struct WindowState<A: Application + 'static> {
    // GPU rendering backend
    backend: WgpuBackend,
    window: Option<Arc<dyn Window>>,

    frame_builder: crate::FrameBuilder,
    layout_engine: Box<dyn LayoutEngine>,

    // Font system for text rendering
    font_system: glyphon::FontSystem,
    // Scale factor for logical-to-physical conversion
    scale: Scale,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Text preparation (glyphon) and GPU submission
    text_pipeline: TextPipeline,

    // Widget/element/render-object trees, reconciliation, and painting
    three_tree_pipeline: ThreeTreePipeline,

    /// Whether a frame needs rendering. Set by state changes, resize,
    /// cursor blink toggle, etc. Cleared after rendering.
    needs_redraw: bool,

    /// Current mouse cursor icon. Updated on PointerMoved events.
    current_cursor: SystemCursorKind,

    /// Last known pointer position in logical coordinates.
    /// Used to provide position for scroll events (winit MouseWheel doesn't include position).
    last_pointer_position: Point<Logical>,

    /// Animation ticker that fires per-frame callbacks for active animations.
    animation_ticker: Arc<AnimationTicker>,
}


impl<A: Application + 'static> WindowState<A> {
    pub async fn new(window: Arc<dyn Window>) -> anyhow::Result<Self> {
        let backend = WgpuBackend::new(window.clone()).await?;

        // Initialize font system with embedded font
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        let font_system = glyphon::FontSystem::new_with_fonts([binary]);

        let layout_engine = Box::new(TaffyLayoutEngine::new());

        let animation_ticker = Arc::new(AnimationTicker::new());

        Ok(Self {
            backend,
            window: Some(window),
            frame_builder: crate::FrameBuilder::new(),
            layout_engine,
            font_system,
            scale: Scale::new(1.0),
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            text_pipeline: TextPipeline::new(),
            three_tree_pipeline: ThreeTreePipeline::new(animation_ticker.clone()),
            needs_redraw: true,
            current_cursor: SystemCursorKind::Arrow,
            last_pointer_position: Point::new(0.0, 0.0),
            animation_ticker,
        })
    }

    pub fn resize(&mut self, size: Size<Physical>) {
        let config =
            crate::render::RenderConfig::new(size, Scale::new(self.scale.factor() as f64));
        self.backend.resize(config);
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, _new_inner_size: winit::dpi::PhysicalSize<u32>) {
        self.scale = Scale::new(scale_factor);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Use retain mode rendering
        self.render_retain()
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
                self.request_frame();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = Scale::new(*scale_factor);
                self.request_frame();
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
                // Track pointer position for scroll events
                let physical = Point::<Physical>::new(position.x as f32, position.y as f32);
                self.last_pointer_position = physical.to_logical(self.scale);
                // Pass to widget tree for hit-testing
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.scale, self.last_pointer_position)
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
                    InputEvent::from_winit(event, self.scale, self.last_pointer_position)
                {
                    self.process_input_event(input_event);
                }
            }

            // Other events that may convert to InputEvent
            _ => {
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.scale, self.last_pointer_position)
                {
                    self.process_input_event(input_event);
                }
            }
        }
    }

    /// Process an InputEvent through the retain-mode pipeline.
    fn process_input_event(&mut self, input_event: InputEvent) {
        let position = match &input_event {
            InputEvent::PointerMoved { position } => *position,
            InputEvent::PointerButton { position, .. } => *position,
            InputEvent::Scroll { position, .. } => *position,
            _ => Point::new(0.0, 0.0),
        };

        // Get current modifiers - use default for now
        let modifiers = Modifiers::default();

        let (frame_needed, rebuilds_pending) = {
            let pipeline = &mut self.three_tree_pipeline;

            let _message = pipeline.handle_event(position, &input_event, modifiers, &mut self.font_system, self.scale);

            // Reset cursor blink on keyboard input so cursor becomes visible
            if matches!(input_event, InputEvent::Keyboard { .. }) {
                if pipeline.reset_cursor_blink() {
                    pipeline.mark_focus_subtree_needs_paint();
                }
            }

            // Reset cursor blink on pointer click so cursor is visible immediately at new position
            if matches!(input_event, InputEvent::PointerButton { state: ButtonState::Pressed, .. }) {
                if pipeline.reset_cursor_blink() {
                    pipeline.mark_focus_subtree_needs_paint();
                }
            }

            // Flutter-style: focus changes trigger rebuild of focused elements
            // so that StatefulWidget::build() re-runs with updated is_focused().
            // The rebuild produces new widget configs → reconciliation →
            // mark_needs_paint → frame_request_needed.
            // Also mark the render object subtree for paint so the cursor
            // appears immediately (prepare_cursor_state() injects focus state
            // before paint, but the render object must be dirty for repaint).
            if pipeline.take_focus_changed() {
                pipeline.mark_focus_needs_build();
                pipeline.mark_focus_subtree_needs_paint();
                // Reset cursor blink so cursor is visible immediately on focus gain
                pipeline.reset_cursor_blink();
            }

            (
                pipeline.take_frame_request_needed(),
                pipeline.has_pending_rebuilds(),
            )
        };

        if frame_needed || rebuilds_pending {
            self.request_frame();
        }

        // Update cursor icon on pointer move
        if matches!(input_event, InputEvent::PointerMoved { .. }) {
            let absolute_position = crate::core::Position::<Logical, Absolute>::new(position.x, position.y);
            self.three_tree_pipeline.mouse_tracker_mut().update_mouse_position(absolute_position);
            let (new_cursor, hover_changed) = self.three_tree_pipeline.cursor_at(absolute_position);
            if new_cursor != self.current_cursor {
                self.current_cursor = new_cursor;
                if let Some(win) = &self.window {
                    win.set_cursor(winit::cursor::Cursor::Icon(system_cursor_to_winit(self.current_cursor)));
                }
            }
            // Hover enter/exit callbacks may have changed StatefulWidget state
            // which sends its element key through the dirty channel, so
            // request_frame() is enough — no full reconcile needed.
            if hover_changed {
                self.request_frame();
            }
        }
    }

    /// Request a frame to be rendered. Sets needs_redraw and
    /// asks winit to deliver a RedrawRequested event.
    pub fn request_frame(&mut self) {
        self.needs_redraw = true;
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    /// Generate a widget tree from the application.
    fn view(&mut self) -> Box<dyn RetainWidget> {
        A::view(&mut self.user_app_state, &mut self.font_system)
    }

    /// Frame tick - called each frame to update timing.
    pub fn frame(&mut self) {
        // No-op: cursor blink is ticked in render_retain() via the pipeline.
    }

    /// Get the window reference.
    pub fn window(&self) -> Option<&Arc<dyn Window>> {
        self.window.as_ref()
    }

    /// Get the animation ticker for this window.
    pub fn animation_ticker(&self) -> &Arc<AnimationTicker> {
        &self.animation_ticker
    }

    /// Check if cursor blink has toggled. Returns true if visibility changed
    /// (caller should request a frame).
    pub fn check_cursor_blink(&mut self) -> bool {
        self.three_tree_pipeline.check_cursor_blink()
    }

    /// Check if this window needs a redraw for cursor blink.
    /// Returns true if a TextEdit is focused (needs blink ticking).
    pub fn needs_blink_redraw(&self) -> bool {
        self.three_tree_pipeline.focused_element().is_some()
    }

    /// Render using the three-tree retain-mode pipeline.
    ///
    /// This method implements the full retain-mode rendering flow:
    /// 1. Generate widget tree from view()
    /// 2. Reconcile widget tree with element tree
    /// 3. Layout dirty render objects
    /// 4. Paint dirty render objects
    /// 5. Process RenderCommands through frame builder
    /// 6. Submit to GPU
    pub fn render_retain(&mut self) -> Result<(), wgpu::SurfaceError> {
        // 1. Backend check
        if !self.backend.is_ready() {
            return Ok(());
        }

        // 2. Check if there's anything to render
        let (has_dirty, needs_reconcile) = {
            let pipeline = &self.three_tree_pipeline;

            (
                pipeline.needs_layout() || pipeline.needs_paint(),
                pipeline.needs_full_reconcile() || pipeline.has_pending_rebuilds(),
            )
        };

        if !self.needs_redraw && !has_dirty && !needs_reconcile {
            // Nothing changed — skip all work.
            return Ok(());
        }

        self.needs_redraw = false;
        self.frame_builder.clear();

        // Fire all active animation callbacks. These may mark elements dirty
        // via the mpsc channel, which perform_rebuilds() will process below.
        self.animation_ticker.tick();

        log::debug!("\n========================================");
        log::debug!("[RetainMode] === FRAME START ===");

        // 4. Perform state-driven rebuilds
        self.three_tree_pipeline.perform_rebuilds();

        // 5. Only call view() + update() when something external triggered it
        if needs_reconcile {
            let widget_tree = self.view();
            self.three_tree_pipeline.update(widget_tree);
        }

        // 6. Compute logical size
        let scale = self.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        // 7. Layout dirty render objects
        self.three_tree_pipeline.layout(
            logical_size,
            self.layout_engine.as_mut(),
            &mut self.font_system,
        );

        // 8. Inject cursor focus/blink state into render objects before paint
        self.three_tree_pipeline.prepare_cursor_state();

        // 8.5. Register any new images with the GPU atlas before paint
        self.three_tree_pipeline.register_images(&mut self.backend);

        // 9. Paint dirty render objects
        let commands = self.three_tree_pipeline.paint();

        // 9.5 Post-frame cursor update: re-hit-test at last mouse position
        // to catch cursor changes from widgets moving under a still mouse.
        if let Some(new_cursor) = self.three_tree_pipeline.post_frame_cursor_update() {
            self.current_cursor = new_cursor;
            if let Some(win) = &self.window {
                win.set_cursor(winit::cursor::Cursor::Icon(system_cursor_to_winit(self.current_cursor)));
            }
        }

        log::debug!("[RetainMode] === FRAME END ===");
        log::debug!(
            "[RetainMode] Summary: {} elements retained",
            self.three_tree_pipeline.element_registry().len()
        );
        log::debug!("========================================\n");

        // 10. Process RenderCommands through frame builder
        crate::render::process_commands(
            &commands,
            &mut self.frame_builder,
            Point::new(0.0, 0.0),
        );

        // 11. Update viewport
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);
        self.backend.update_viewport(physical_size);

        // 12. Collect text through glyphon
        let prepared_text = self.text_pipeline.collect_text(
            &mut self.frame_builder,
            &mut self.font_system,
            scale,
            physical_size,
        );

        // 13. Execute render
        self.text_pipeline
            .execute_render(
                &mut self.backend,
                &self.frame_builder,
                prepared_text,
                &mut self.font_system,
            )
            .map_err(|e| match e {
                crate::render::RenderError::SurfaceNotConfigured => wgpu::SurfaceError::Lost,
                crate::render::RenderError::AcquireFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::TextPrepareFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::GpuError(_) => wgpu::SurfaceError::Lost,
            })?;

        // 14. If a TextEdit is focused, keep the event loop alive so
        //     about_to_wait fires and can check cursor blink timing.
        //     request_redraw() is idempotent; the next render_retain() will
        //     early-return if nothing is dirty (blink hasn't toggled yet).
        if self.three_tree_pipeline.focused_element().is_some() {
            self.request_frame();
        }

        // Keep the frame loop alive while animations are active so that
        // tick() continues to fire each frame.
        if self.animation_ticker.has_active() {
            self.request_frame();
        }

        Ok(())
    }
}