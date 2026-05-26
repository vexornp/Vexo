use std::sync::Arc;

use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::core::{Logical, Physical, Point, Scale, Size};
use crate::input::{InputEvent, Modifiers};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};
use crate::render::{RenderBackend, WgpuBackend};
use crate::render_pipeline::RenderPipeline;
use crate::{ThreeTreePipeline, Widget as RetainWidget};
use crate::state::CursorBlinkState;
use crate::Application;

pub struct WindowState<A: Application + 'static> {
    // GPU rendering backend
    backend: WgpuBackend,
    window: Option<Arc<dyn Window>>,

    batcher: crate::UiBatcher,
    layout_engine: Box<dyn LayoutEngine>,

    // Font system for text rendering
    font_system: glyphon::FontSystem,
    // Scale factor for logical-to-physical conversion
    scale: Scale,
    // Physical cursor position
    cursor_pos: Point<Physical>,

    // User's application state
    user_app_state: A::State,
    _phantom: std::marker::PhantomData<A>,

    // Cursor blink state (global - only one focused widget at a time)
    cursor_blink: CursorBlinkState,

    
    // Render pipeline for orchestrating render stages
    render_pipeline: RenderPipeline,

    // Three-tree pipeline (new retain-mode system)
    retain_pipeline: Option<ThreeTreePipeline>,
}


impl<A: Application + 'static> WindowState<A> {
    pub async fn new(window: Arc<dyn Window>) -> anyhow::Result<Self> {
        let backend = WgpuBackend::new(window.clone()).await?;

        // Initialize font system with embedded font
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
        let font_system = glyphon::FontSystem::new_with_fonts([binary]);

        let layout_engine = Box::new(TaffyLayoutEngine::new());

        Ok(Self {
            backend,
            window: Some(window),
            batcher: crate::UiBatcher::new(),
            layout_engine,
            font_system,
            scale: Scale::new(1.0),
            cursor_pos: Point::new(0.0, 0.0),
            user_app_state: A::new(),
            _phantom: std::marker::PhantomData,
            cursor_blink: CursorBlinkState::new(),
            render_pipeline: RenderPipeline::new(),
            retain_pipeline: Some(ThreeTreePipeline::new()),
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
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = Scale::new(*scale_factor);
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
                self.cursor_pos =
                    Point::<Physical>::new(position.x as f32, position.y as f32);

                // Pass to widget tree for hit-testing
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.scale)
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
                    InputEvent::from_winit(event, self.scale)
                {
                    self.process_input_event(input_event);
                }
            }

            // Other events that may convert to InputEvent
            _ => {
                if let Some(input_event) =
                    InputEvent::from_winit(event, self.scale)
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
            _ => Point::new(0.0, 0.0),
        };

        // Get current modifiers - use default for now
        let modifiers = Modifiers::default();

        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return,
        };

        let message = pipeline.handle_event(position, &input_event, modifiers, &mut self.font_system);

        // Reset cursor blink on keyboard input so cursor becomes visible
        if matches!(input_event, InputEvent::Keyboard { .. }) {
            pipeline.reset_cursor_blink();
        }

        if let Some(msg) = message {
            // Retain mode widgets return Box<dyn Any> as their message type.
            // For now, we don't have a way to convert this to A::Message.
            let _ = msg;
        }

        // If events triggered setState (pending rebuilds), request a redraw
        // so the state-driven rebuilds are processed on the next frame.
        if let Some(pipeline) = &self.retain_pipeline {
            if pipeline.has_pending_rebuilds() {
                log::info!("process_input_event: pending rebuilds detected, requesting redraw");
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
        }
    }

    /// Generate a widget tree from the application.
    fn view(&mut self) -> Box<dyn RetainWidget> {
        A::view(&mut self.user_app_state, &mut self.font_system)
    }

    /// Frame tick - called each frame to update timing.
    pub fn frame(&mut self) {
        self.cursor_blink.tick();
    }

    /// Get the window reference.
    pub fn window(&self) -> Option<&Arc<dyn Window>> {
        self.window.as_ref()
    }

    /// Render using the three-tree retain-mode pipeline.
    ///
    /// This method implements the full retain-mode rendering flow:
    /// 1. Generate widget tree from view()
    /// 2. Reconcile widget tree with element tree
    /// 3. Layout dirty render objects
    /// 4. Paint dirty render objects
    /// 5. Process RenderCommands through batcher
    /// 6. Submit to GPU
    pub fn render_retain(&mut self) -> Result<(), wgpu::SurfaceError> {
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
        let widget_tree = self.view();

        // 4. Get pipeline
        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return Ok(()),
        };

        // 4b. Tick cursor blink (use pipeline's blink state in retain mode)
        pipeline.tick_cursor_blink();

        // 5. Clear batcher
        self.batcher.clear();

        log::debug!("\n========================================");
        log::debug!("[RetainMode] === FRAME START ===");

        // 6. Perform state-driven rebuilds FIRST (before widget tree reconcile)
        pipeline.perform_rebuilds();

        // 7. Update widget tree (targeted rebuild or full reconcile)
        pipeline.update(widget_tree);

        // 7. Compute logical size
        let scale = self.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        self.batcher.set_screen_size(logical_size);

        // 8. Layout dirty render objects
        pipeline.layout(
            logical_size,
            self.layout_engine.as_mut(),
            &mut self.font_system,
        );

        // 8b. Inject cursor focus/blink state into render objects before paint
        pipeline.prepare_cursor_state();

        // 9. Paint dirty render objects
        let commands = pipeline.paint();

        log::debug!("[RetainMode] === FRAME END ===");
        log::debug!(
            "[RetainMode] Summary: {} elements retained",
            pipeline.element_registry().len()
        );
        log::debug!("========================================\n");

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
                crate::render::RenderCommand::Caret {
                    position,
                    height,
                    color,
                } => {
                    let bounds =
                        crate::core::Bounds::from_xywh(position.x, position.y, 2.0, height);
                    self.batcher.add_rect(bounds, color, None, 0.0);
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
            &mut self.font_system,
            scale,
            physical_size,
        );

        // 13. Execute render
        self.render_pipeline
            .execute_render(
                &mut self.backend,
                &self.batcher,
                prepared_text,
                &mut self.font_system,
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