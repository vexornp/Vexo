use std::sync::Arc;

use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::animation::AnimationTicker;
use crate::core::{
    Absolute, KeyboardInsetSource, Logical, Physical, Point, ScaleSource, SafeAreaSource, Size,
};
use crate::input::{ButtonState, InputEvent, Modifiers, SystemCursorKind};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};
use crate::platform::{self, Clipboard};
use crate::render::{RenderBackend, RenderError, WgpuBackend};
use crate::text_pipeline::TextPipeline;
use crate::ThreeTreePipeline;
use crate::Application;
use crate::RootComponent;
use crate::widgets::Widget;

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
    // Shared scale factor source (single source of truth)
    scale_source: ScaleSource,

    /// Shared safe-area insets source (logical pixels).
    ///
    /// Updated each frame from `Window::safe_area()`; read by the element tree
    /// via `RenderContext::media_query_sources()`. On desktop the underlying insets are
    /// always zero, so this is a no-op.
    safe_area_source: SafeAreaSource,

    /// Shared keyboard-inset source (logical pixels). Updated by the iOS
    /// keyboard shim + the render-loop interpolation driver each frame; read
    /// by the root `MediaQuery` via `RenderContext::media_query_sources()`.
    /// On desktop this stays at 0 (no shim is installed).
    keyboard_inset_source: KeyboardInsetSource,

    /// Shared keyboard animation source. Written by the iOS shim on each
    /// `keyboardWillShow/Hide`; read + interpolated each frame by the
    /// render loop to drive `keyboard_inset_source.set()`.
    keyboard_animation_source: crate::core::KeyboardAnimationSource,

    /// Shared media-query data source (size, scale, brightness). Updated
    /// each frame from `Window` metrics; read by the root `MediaQuery`
    /// component via `RenderContext::media_query_sources()`.
    media_query_data_source: crate::core::MediaQueryDataSource,

    #[cfg(target_os = "ios")]
    keyboard_observer: Option<crate::platform::keyboard_ios::KeyboardObserver>,

    _phantom: std::marker::PhantomData<A>,

    // Text preparation (glyphon) and GPU submission
    text_pipeline: TextPipeline,

    // Widget/element/render-object trees, reconciliation, and painting
    three_tree_pipeline: ThreeTreePipeline,

    /// Whether a frame needs rendering. Set by state changes, resize,
    /// cursor blink toggle, etc. Cleared after rendering.
    needs_redraw: bool,

    /// Whether the window is currently occluded (fully hidden behind
    /// other windows or minimized). When true, rendering is skipped to
    /// avoid wasting CPU/GPU on a surface that wgpu reports as
    /// `Occluded`. The OS delivers `WindowEvent::Occluded(false)` when
    /// the window becomes visible again, at which point we request a
    /// frame to refresh.
    is_occluded: bool,

    /// Current mouse cursor icon. Updated on PointerMoved events.
    current_cursor: SystemCursorKind,

    /// Last known pointer position in logical coordinates.
    /// Used to provide position for scroll events (winit MouseWheel doesn't include position).
    last_pointer_position: Point<Logical>,

    /// Animation ticker that fires per-frame callbacks for active animations.
    animation_ticker: Arc<AnimationTicker>,

    /// Current keyboard modifier state, updated from winit ModifiersChanged events.
    /// Passed to the pipeline with every input event so widgets see real modifiers.
    current_modifiers: Modifiers,

    /// Platform clipboard backend (arboard on desktop, stub on iOS).
    /// Shared via `Arc` so EventContexts can cheaply clone it during dispatch.
    clipboard: Arc<dyn Clipboard>,

    /// Timestamp of the last keyboard-interpolation frame, for measuring
    /// inter-frame gaps during keyboard animations (diagnostic logging).
    last_kb_frame: Option<std::time::Instant>,

    /// CADisplayLink driver for vsync-rate animation on iOS. Always-on:
    /// started at construction, never explicitly stopped. iOS auto-pauses
    /// when the app is suspended and auto-resumes on foreground. `Drop`
    /// invalidates the link when `WindowState` is torn down. On desktop
    /// this field doesn't exist.
    #[cfg(target_os = "ios")]
    display_link: crate::platform::display_link_ios::DisplayLink,
}


impl<A: Application + 'static> WindowState<A> {
    pub async fn new(
        window: Arc<dyn Window>,
        image_cache: Arc<crate::image_cache::ImageCache>,
    ) -> anyhow::Result<Self> {
        let backend = WgpuBackend::new(window.clone()).await?;

        // Get the shared scale source from the backend
        let scale_source = backend.scale_source();

        // Initialize font system with embedded font + default-family override
        let mut font_system = crate::resource::new_font_system();
        // Give the application a chance to register additional fonts (e.g.
        // icon fonts) before any layout or shaping runs.
        A::register_fonts(&mut font_system);

        let layout_engine = Box::new(TaffyLayoutEngine::new());

        let animation_ticker = Arc::new(AnimationTicker::new());

        let clipboard: Arc<dyn Clipboard> = platform::default_clipboard();

        let safe_area_source = SafeAreaSource::default();

        let keyboard_inset_source = KeyboardInsetSource::default();

        let keyboard_animation_source = crate::core::KeyboardAnimationSource::default();

        let media_query_data_source = crate::core::MediaQueryDataSource::default();

        let mut three_tree_pipeline = ThreeTreePipeline::new(animation_ticker.clone());
        // Share the same atomics so per-frame `safe_area_source.set()` calls
        // below are visible to RenderContext::media_query_sources() during render.
        three_tree_pipeline.set_safe_area_source(safe_area_source.clone());
        three_tree_pipeline.set_keyboard_inset_source(keyboard_inset_source.clone());
        three_tree_pipeline.set_media_query_data_source(media_query_data_source.clone());
        three_tree_pipeline.set_image_cache(image_cache);

        #[cfg(target_os = "ios")]
        let display_link = {
            // CADisplayLink callback: just request a redraw each vsync. The
            // render loop's interpolation driver (in render_retain) advances
            // the keyboard animation one step per vsync. The link is always-on
            // — iOS auto-pauses on background, auto-resumes on foreground.
            let window_for_dl = window.clone();
            let on_frame: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                window_for_dl.request_redraw();
            });
            crate::platform::display_link_ios::DisplayLink::new(on_frame)
        };

        #[cfg(target_os = "ios")]
        let keyboard_observer = {
            let scale = scale_source.get().factor_f64();
            // v1 limitation: the live window size isn't available yet at
            // `WindowState::new()` (the window is just being created), so we
            // pass `f32::MAX` to disable the clamp. The clamp code in
            // `handle_keyboard_notification` is still correct; a future
            // improvement would thread the live height from `SurfaceResized`.
            let window_logical_height = f32::MAX;
            // Frame-request callback handed to the keyboard shim. UIKit posts
            // keyboard notifications outside winit's event loop, so the shim
            // must wake the render loop itself — otherwise the avoidance tween
            // wouldn't start until the next cursor-blink tick (~500ms), long
            // after the OS keyboard animation finished (causing a snap). See
            // KeyboardObserver::install doc. `Arc<dyn Window>` is `Send+Sync`
            // (winit's `Window` trait requires it), so the closure is too.
            let window_for_keyboard = window.clone();
            let request_frame: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                window_for_keyboard.request_redraw();
            });
            Some(crate::platform::keyboard_ios::KeyboardObserver::install(
                keyboard_inset_source.clone(),
                keyboard_animation_source.clone(),
                scale,
                window_logical_height,
                request_frame,
            ))
        };

        Ok(Self {
            backend,
            window: Some(window),
            frame_builder: crate::FrameBuilder::new(),
            layout_engine,
            font_system,
            scale_source,
            safe_area_source,
            keyboard_inset_source,
            keyboard_animation_source,
            media_query_data_source,
            #[cfg(target_os = "ios")]
            keyboard_observer,
            _phantom: std::marker::PhantomData,
            text_pipeline: TextPipeline::new(),
            three_tree_pipeline,
            needs_redraw: true,
            is_occluded: false,
            current_cursor: SystemCursorKind::Arrow,
            last_pointer_position: Point::new(0.0, 0.0),
            animation_ticker,
            current_modifiers: Modifiers::default(),
            clipboard,
            last_kb_frame: None,
            #[cfg(target_os = "ios")]
            display_link,
        })
    }

    pub fn resize(&mut self, size: Size<Physical>) {
        let config = crate::render::RenderConfig::new(size);
        self.backend.resize(config);
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, _new_inner_size: winit::dpi::PhysicalSize<u32>) {
        self.scale_source.set(scale_factor);
    }

    pub fn render(&mut self) -> Result<(), RenderError> {
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
                self.three_tree_pipeline.mark_all_needs_layout();
                self.request_frame();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_source.set(*scale_factor);
                self.three_tree_pipeline.mark_all_needs_layout();
                self.request_frame();
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = self.render() {
                    eprintln!("Error drawing window: {err}");
                    match err {
                        // Surface is hidden — do NOT retry immediately.
                        // Mark ourselves occluded and wait for the OS to
                        // deliver WindowEvent::Occluded(false) when the
                        // window becomes visible again. Retrying here
                        // spins an infinite render→fail→request_redraw
                        // loop, wasting CPU and flooding the log.
                        RenderError::SurfaceOccluded => {
                            self.is_occluded = true;
                        }
                        // Timeout / Outdated — retry next frame.
                        RenderError::SurfaceTransient(_) => {
                            self.request_frame();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.is_occluded = *occluded;
                // When the window becomes visible again, request a fresh
                // frame — the surface needs to be re-acquired and any state
                // changes that happened while occluded must be rendered.
                if !*occluded {
                    self.request_frame();
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            // User input events with special handling
            WindowEvent::PointerMoved { position, .. } => {
                // Track pointer position for scroll events
                let physical = Point::<Physical>::new(position.x as f32, position.y as f32);
                self.last_pointer_position = physical.to_logical(self.scale_source.get());
                // Pass to widget tree for hit-testing
                if let Some(input_event) =
                    InputEvent::from_winit(event, &self.scale_source, self.last_pointer_position)
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
                    InputEvent::from_winit(event, &self.scale_source, self.last_pointer_position)
                {
                    self.process_input_event(input_event);
                }
            }

            // Track modifier state so it can be forwarded with subsequent events.
            // (We still let from_winit produce a ModifiersChanged InputEvent too,
            // but the authoritative state lives here on WindowState.)
            WindowEvent::ModifiersChanged(modifiers) => {
                let mods = modifiers.state();
                self.current_modifiers = Modifiers {
                    shift: mods.shift_key(),
                    control: mods.control_key(),
                    alt: mods.alt_key(),
                    super_key: mods.meta_key(),
                };
                if let Some(input_event) =
                    InputEvent::from_winit(event, &self.scale_source, self.last_pointer_position)
                {
                    self.process_input_event(input_event);
                }
            }

            // Window focus changes — cancel any in-flight gesture when the
            // window loses focus so the arena doesn't leak (a press without a
            // matching release, e.g. Alt-Tab mid-press).
            //
            // On focus gain, request a frame as a defensive fallback: on
            // some platforms the `Occluded` event may not fire when the
            // window becomes visible (e.g. clicking the dock icon), and
            // without this the UI could appear stuck after being inactive.
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.three_tree_pipeline.cancel_current_gesture();
                } else {
                    self.request_frame();
                }
            }

            // Other events that may convert to InputEvent
            _ => {
                if let Some(input_event) =
                    InputEvent::from_winit(event, &self.scale_source, self.last_pointer_position)
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

        // Forward the current modifier state (kept in sync via ModifiersChanged).
        let modifiers = self.current_modifiers;

        let (frame_needed, rebuilds_pending, focus_changed) = {
            let pipeline = &mut self.three_tree_pipeline;

            let _message = pipeline.handle_event(position, &input_event, modifiers, &mut self.font_system, &self.scale_source, &self.clipboard);

            // Drain the dirty channel so that elements whose dirty callbacks
            // fired during event handling (e.g., AnimationController::forward())
            // are visible to has_pending_rebuilds() below.
            pipeline.drain_dirty_to_build_owner();

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
            // so that Component::render() re-runs with updated is_focused().
            // The rebuild produces new widget configs → reconciliation →
            // mark_needs_paint → frame_request_needed.
            // Also mark the render object subtree for paint so the cursor
            // appears immediately (prepare_cursor_state() injects focus state
            // before paint, but the render object must be dirty for repaint).
            let focus_changed = pipeline.take_focus_changed();
            if focus_changed {
                pipeline.mark_focus_needs_build();
                pipeline.mark_focus_subtree_needs_paint();
                // Reset cursor blink so cursor is visible immediately on focus gain
                pipeline.reset_cursor_blink();
            }

            (
                pipeline.take_frame_request_needed(),
                pipeline.has_pending_rebuilds(),
                focus_changed,
            )
        };

        if frame_needed || rebuilds_pending {
            self.request_frame();
        }

        // On mobile (iOS + Android), show / hide the software keyboard to
        // match focus state. winit's UIKit backend implements UIKeyInput on
        // its view; winit's Android backend (GameActivity) implements a
        // BaseInputConnection on its activity. In both cases calling
        // set_ime_allowed(true) brings up the keyboard; typed text is then
        // delivered as WindowEvent::KeyboardInput with `text` set, which the
        // existing TextEdit keyboard handler already inserts. When focus
        // leaves a TextEdit (or nothing is focused), dismiss the keyboard.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if focus_changed {
            let text_input_focused = self.three_tree_pipeline.is_text_input_focused();
            if let Some(win) = &self.window {
                #[allow(deprecated)]
                let _ = win.set_ime_allowed(text_input_focused);
            }
        }

        // On non-mobile platforms `focus_changed` is otherwise unused; drop
        // it here so the binding doesn't trigger an unused-variable warning.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let _ = focus_changed;

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
            // Hover enter/exit callbacks may have changed Component state
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

    /// Get a clone of the scale source.
    pub fn scale_source(&self) -> ScaleSource {
        self.scale_source.clone()
    }

    /// Get a clone of the safe-area source.
    ///
    /// Cheap (`SafeAreaSource` is `Arc`-based); useful for subsystems that
    /// want to observe insets outside the widget tree.
    pub fn safe_area_source(&self) -> SafeAreaSource {
        self.safe_area_source.clone()
    }

    /// Get a clone of the keyboard-inset source.
    ///
    /// Cheap (`KeyboardInsetSource` is `Arc`-based); useful for subsystems
    /// that want to observe insets outside the widget tree, or for tests
    /// that need to drive the source directly.
    pub fn keyboard_inset_source(&self) -> KeyboardInsetSource {
        self.keyboard_inset_source.clone()
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

    /// Poll platform-specific idle frame drivers. On desktop, this checks
    /// cursor blink and animation ticker and requests a frame if either is
    /// active. On iOS, the always-on CADisplayLink drives frames at vsync
    /// rate, so there's nothing to poll — this is a no-op.
    #[cfg(not(target_os = "ios"))]
    pub fn poll_idle_frame_drivers(&mut self) {
        if self.check_cursor_blink() {
            self.request_frame();
        }
        if self.animation_ticker().has_active() {
            self.request_frame();
        }
    }

    #[cfg(target_os = "ios")]
    pub fn poll_idle_frame_drivers(&mut self) {}

    /// Mark only the `RootMediaQuery` element as needing rebuild, rather than
    /// the root element. This avoids re-calling `Application::view()` (which
    /// constructs the entire app widget tree) on every keyboard animation
    /// frame. `RootMediaQuery::render()` reads the platform sources and creates
    /// a new `MediaQuery` widget; the `InheritedWidget` mechanism then
    /// propagates to only the elements that called `MediaQuery::of(ctx)`.
    fn mark_root_media_query_needs_build(&mut self) {
        let rmq_id = self
            .three_tree_pipeline
            .element_registry()
            .root()
            .and_then(|root| {
                self.three_tree_pipeline
                    .element_registry()
                    .children(root)
                    .first()
                    .copied()
            });
        if let Some(id) = rmq_id {
            self.three_tree_pipeline.mark_needs_build(id);
        }
    }

    /// Render using the three-tree retain-mode pipeline.
    ///
    /// This method implements the full retain-mode rendering flow:
    /// 1. Generate widget tree from RootComponent
    /// 2. Reconcile widget tree with element tree (first frame only)
    /// 3. Perform state-driven rebuilds (subsequent frames)
    /// 4. Layout dirty render objects
    /// 5. Paint dirty render objects
    /// 6. Process RenderCommands through frame builder
    /// 7. Submit to GPU
    pub fn render_retain(&mut self) -> Result<(), RenderError> {
        // 1. Backend check
        if !self.backend.is_ready() {
            return Ok(());
        }

        // 1.5. Skip all rendering work while the window is occluded.
        //      wgpu would return `Occluded` from get_current_texture()
        //      anyway, but only after we've done all the rebuild/layout/
        //      paint/command-generation work — wasting CPU and flooding
        //      the log. We keep `needs_redraw` set so that when the
        //      window becomes visible again (WindowEvent::Occluded(false)
        //      or Focused(true)), the next render actually runs.
        if self.is_occluded {
            return Ok(());
        }

        // 2. Check if there's anything to render
        // Check cursor blink BEFORE computing has_dirty — if blink toggled,
        // check_cursor_blink() calls dirty.mark_needs_paint(ro_id), which
        // makes has_dirty true and prevents the early-return below. On iOS
        // the always-on CADisplayLink drives frames but poll_idle_frame_drivers
        // is a no-op, so this is the only place cursor blink is checked on iOS.
        self.check_cursor_blink();

        // Drain the dirty channel so that elements whose dirty callbacks
        // fired outside the event loop (e.g. iOS file-picker delegate,
        // keyboard notifications) are visible to has_pending_rebuilds()
        // below. Without this, the early-return skips the rebuild and the
        // screen doesn't update until the next input event drains the channel.
        self.three_tree_pipeline.drain_dirty_to_build_owner();

        let (has_dirty, needs_reconcile) = {
            let pipeline = &self.three_tree_pipeline;

            (
                pipeline.needs_layout() || pipeline.needs_paint(),
                pipeline.needs_full_reconcile() || pipeline.has_pending_rebuilds(),
            )
        };

        // If the keyboard animation source has pending params, always run
        // (even if nothing is dirty yet) so the interpolation driver at step
        // 4.5 can advance the animation. Same for the animation ticker: if
        // active, we must run so tick() (step 5, below) can fire callbacks.
        // The CADisplayLink callback calls `window.request_redraw()` without
        // setting `needs_redraw`, so without these guards the render would
        // skip and animations would stall.
        let kb_active = self.keyboard_animation_source.has_pending();
        let anim_active = self.animation_ticker.has_active();

        if !self.needs_redraw && !has_dirty && !needs_reconcile && !kb_active && !anim_active {
            // Nothing changed — skip all work.
            return Ok(());
        }

        self.needs_redraw = false;
        self.frame_builder.clear();

        // Fire all active animation callbacks. These may mark elements dirty
        // via the mpsc channel, which perform_rebuilds() will process below.
        self.animation_ticker.tick();

        // Feed a Tick to the active gesture arena so time-based recognizers
        // (long-press) can fire. Must run BEFORE perform_rebuilds() so that
        // a long-press firing (which may set a Signal, e.g. open the menu)
        // has its dirty mark visible to the rebuild pass this frame.
        self.three_tree_pipeline
            .tick_arena(std::time::Instant::now(), &mut self.font_system, &self.clipboard);

        log::debug!("\n========================================");
        log::debug!("[RetainMode] === FRAME START ===");

        // 4. Refresh safe-area insets (logical pixels) from the platform.
        //    winit polls UIKit's `safeAreaInsets` here; on desktop this is
        //    always zero. Done BEFORE rebuilds/reconcile so that widgets
        //    reading `MediaQuery::of(ctx).padding` during `render()` (e.g.
        //    `NavigationStackView`'s nav bar) see the real insets on the
        //    very first frame. A change marks the tree dirty so layout
        //    re-runs (e.g. on device rotation).
        {
            let prev = self.safe_area_source.get();
            if let Some(win) = &self.window {
                let insets = win.safe_area();
                let f = self.scale_source.get().factor();
                self.safe_area_source.set(
                    insets.left as f32 / f,
                    insets.right as f32 / f,
                    insets.top as f32 / f,
                    insets.bottom as f32 / f,
                );
            }
            if self.safe_area_source.get() != prev {
                self.three_tree_pipeline.mark_all_needs_layout();
            }
        }

        // 4.1. Refresh media-query data source (size, scale, brightness).
        //      Read live from the window each frame; mark the tree dirty
        //      when any value changes so the root MediaQuery re-renders.
        {
            let prev = self.media_query_data_source.get();
            if let Some(win) = &self.window {
                let scale = self.scale_source.get().factor();
                let physical_w = self.backend.width() as f32;
                let physical_h = self.backend.height() as f32;
                let logical_w = physical_w / scale;
                let logical_h = physical_h / scale;
                let is_dark = win.theme().unwrap_or(winit::window::Theme::Light)
                    == winit::window::Theme::Dark;
                self.media_query_data_source.set(
                    crate::core::Size::<crate::core::Logical>::new(logical_w, logical_h),
                    scale,
                    is_dark,
                );
            }
            if self.media_query_data_source.get() != prev {
                if let Some(root_id) = self.three_tree_pipeline.element_registry().root() {
                    self.three_tree_pipeline.mark_needs_build(root_id);
                }
                self.three_tree_pipeline.mark_all_needs_layout();
                self.request_frame();
            }
        }

        // 4.5. Keyboard animation interpolation.
        //
        // The iOS shim writes animation params to `keyboard_animation_source`
        // on each keyboardWillShow/Hide notification. Each frame, if an
        // animation is active, we interpolate the current height and write
        // it to `keyboard_inset_source`. The root MediaQuery reads
        // `keyboard_inset_source.get()` via `media_query_sources()`, so
        // this drives `MediaQueryData.view_insets.bottom` frame-by-frame.
        //
        // On desktop the animation source is always `None` (no shim), so
        // this block is a no-op and `keyboard_inset_source` stays at 0.
        {
            if let Some(anim) = self.keyboard_animation_source.take() {
                let now = std::time::Instant::now();
                let elapsed_ms = now.duration_since(anim.start).as_secs_f32() * 1000.0;
                let gap_ms = self
                    .last_kb_frame
                    .map(|t| now.duration_since(t).as_secs_f32() * 1000.0)
                    .unwrap_or(0.0);
                self.last_kb_frame = Some(now);
                match anim.interpolate(now) {
                    Some(height) => {
                        log::debug!(
                            "[KBDBG] frame elapsed={:.0}ms gap={:.0}ms height={:.1} (target={:.1})",
                            elapsed_ms,
                            gap_ms,
                            height,
                            anim.target
                        );
                        self.keyboard_inset_source.set(height);
                        self.keyboard_animation_source.restore(anim);
                        self.mark_root_media_query_needs_build();
                        self.three_tree_pipeline.mark_all_needs_layout();
                        self.request_frame();
                    }
                    None => {
                        log::debug!(
                            "[KBDBG] DONE elapsed={:.0}ms gap={:.0}ms (dur={:.0}ms) target={:.1} — snapping",
                            elapsed_ms,
                            gap_ms,
                            anim.duration_secs * 1000.0,
                            anim.target
                        );
                        self.keyboard_inset_source.set(anim.target);
                        self.mark_root_media_query_needs_build();
                        self.three_tree_pipeline.mark_all_needs_layout();
                        self.request_frame();
                    }
                }
            }
        }

        // 5. Perform state-driven rebuilds.
        //
        // Loop until the rebuild cascade settles: marking `RootMediaQuery`
        // dirty triggers a new `MediaQuery` widget, whose `InheritedWidget`
        // update marks dependents dirty, which may in turn mark their
        // children dirty. Processing all of them in one frame avoids a
        // 1-frame-per-tree-level lag that would make the keyboard animation
        // visibly stutter.
        loop {
            self.three_tree_pipeline.perform_rebuilds();
            if !self.three_tree_pipeline.has_pending_rebuilds() {
                break;
            }
        }

        // 5.5. Render-loop focus / keyboard sync.
        //
        // Focus can change during `perform_rebuilds()` in two ways that the
        // event-phase keyboard sync (in `process_input_event`) cannot see:
        //
        //   1. A deferred unfocus requested via `LifecycleContext::clear_focus()`
        //      — e.g. `NavigationStackView` clearing focus when a pop
        //      transition starts. Applied inside `perform_rebuilds()` above.
        //   2. The focused element unmounting during reconciliation (the
        //      safety net in `FocusManager::remove_node_recursive`), e.g. the
        //      outgoing overlay page at the end of a navigation animation.
        //
        // Both set `focus_changed`; we drain it here and mirror the event-phase
        // handling: rebuild focus-sensitive elements, repaint the cursor, and
        // (mobile only) show/hide the software keyboard. On non-mobile the
        // `focus_changed` flag is still consumed here so it doesn't linger.
        //
        // This is what dismisses the keyboard *immediately* when the user taps
        // Back on a focused chat screen — on the same frame the pop animation
        // begins — instead of leaving it stuck on screen.
        let focus_changed = self.three_tree_pipeline.take_focus_changed();
        if focus_changed {
            self.three_tree_pipeline.mark_focus_needs_build();
            self.three_tree_pipeline.mark_focus_subtree_needs_paint();
            self.three_tree_pipeline.reset_cursor_blink();
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                let text_input_focused = self.three_tree_pipeline.is_text_input_focused();
                if let Some(win) = &self.window {
                    #[allow(deprecated)]
                    let _ = win.set_ime_allowed(text_input_focused);
                }
            }
            // Drive another frame so the border-color rebuild (requested by
            // mark_focus_needs_build) actually renders.
            self.request_frame();
        }

        // 6. On first frame, reconcile the RootComponent into the element tree.
        //    After that, perform_rebuilds() handles everything — when a Signal
        //    on the app state fires, the RootComponent's StatefulElement is
        //    marked dirty and rebuild_from_state() re-calls A::view().
        //    We only pass a new root widget on the initial mount; state-driven
        //    rebuilds are handled entirely by perform_rebuilds() above.
        if self.three_tree_pipeline.needs_full_reconcile() {
            let root_widget = RootComponent::<A>::default().boxed();
            self.three_tree_pipeline.update(root_widget);
        }

        // 7. Compute logical size
        let scale = self.scale_source.get();
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        // 8. Layout dirty render objects
        self.three_tree_pipeline.layout(
            logical_size,
            self.layout_engine.as_mut(),
            &mut self.font_system,
        );

        // 8. Inject cursor focus/blink state into render objects before paint
        self.three_tree_pipeline.prepare_cursor_state();

        // 8.5. Register any new images with the GPU atlas before paint.
        // Reclaim atlas slots from removed image render objects first, so a
        // pop-then-push on the same frame can reuse the freed slot instead of
        // carving new shelf space and slowly filling the 2048x2048 atlas.
        self.three_tree_pipeline.unregister_images(&mut self.backend);
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
            &self.scale_source,
            physical_size,
        );

        // 13. Execute render
        self.text_pipeline
            .execute_render(
                &mut self.backend,
                &self.frame_builder,
                prepared_text,
                &mut self.font_system,
            )?;

        Ok(())
    }
}
