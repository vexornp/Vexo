//! Three-tree rendering pipeline for the retain-mode system.
//!
//! This module provides the `ThreeTreePipeline` struct that orchestrates
//! the three trees (Widget/Element/RenderObject) and manages the full
//! rendering lifecycle.
//!
//! # Architecture
//!
//! The pipeline coordinates three trees:
//! 1. **Widget Tree** - Immutable configuration, rebuilt each frame
//! 2. **Element Tree** - Persistent state, updated via reconciliation
//! 3. **RenderObject Tree** - Layout and painting, updated incrementally
//!
//! # Lifecycle
//!
//! ```ignore
//! let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
//!
//! // Reconcile widget tree with element tree
//! pipeline.reconcile(root_widget);
//!
//! // Perform layout (only on dirty objects)
//! pipeline.layout(available_size, &mut layout_engine);
//!
//! // Generate render commands (only on dirty objects)
//! let commands = pipeline.paint();
//!
//! // Hit test for input
//! let result = pipeline.hit_test(position);
//! ```
//!
//! # Incremental Updates
//!
//! The pipeline tracks dirty render objects to minimize work:
//! - `reconcile()` marks affected render objects as needing layout
//! - `layout()` only processes objects marked as dirty
//! - `paint()` recursively collects commands from the root

use std::any::Any;
use std::sync::{mpsc, Arc};

use slotmap::SecondaryMap;

use crate::animation::AnimationTicker;
use crate::core::{Absolute, Bounds, Logical, Point, Position, ScaleSource, Size};
use crate::input::{InputEvent, Modifiers, MouseTrackerAnnotation, SystemCursorKind};
use crate::mouse_tracker::MouseTracker;
use crate::render::RenderCommand;

use super::build_owner::BuildOwner;
use super::child_ops::ChildOps;
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_state::StateStorage;
use super::event_handler::EventHandler;
use super::focus::FocusManager;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::inherited_registry::{InheritedMap, InheritedRegistry};
use super::layouter::Layouter;
use super::painter::Painter;
use super::reconciler::Reconciler;
use super::render_object::RenderObjectRegistry;
use super::widgets::Widget;
use crate::gestures::GestureArena;
use crate::state::CursorBlinkState;

// ============================================================================
// THREE-TREE PIPELINE
// ============================================================================

/// Orchestrates the three trees for retain-mode rendering.
///
/// The pipeline manages the widget tree, element tree, and render object tree
/// for efficient UI rendering with incremental updates.
///
/// # Example
///
/// ```ignore
/// let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
///
/// // Build and reconcile widget tree
/// let widget = Button::new("Click Me");
/// pipeline.reconcile(Box::new(widget));
///
/// // Layout with constraints
/// let mut engine = TaffyLayoutEngine::new();
/// pipeline.layout(Size::new(800.0, 600.0), &mut engine);
///
/// // Paint to get render commands
/// let commands = pipeline.paint();
///
/// // Handle events
/// if let Some(msg) = pipeline.handle_event(position, event, modifiers) {
///     // msg is Box<dyn Any>, downcast to specific message type
/// }
/// ```
pub struct ThreeTreePipeline {
    /// Registry of live elements (middle tree).
    element_registry: ElementRegistry,

    /// Registry of render objects (third tree).
    render_objects: RenderObjectRegistry,

    /// State storage for elements.
    state: StateStorage,

    /// Dirty tracking for incremental updates.
    dirty: DirtyTracking,

    /// Focus manager for the focus tree.
    focus_manager: FocusManager,

    /// Build owner for targeted rebuilds.
    build_owner: BuildOwner,

    /// Accumulator for child operations emitted by element lifecycle methods.
    /// The pipeline drains and executes these after each element method call.
    child_ops: ChildOps,

    /// Channel for receiving dirty element signals from Signal callbacks.
    ///
    /// When a `Signal::set()` fires its dirty callback, it sends
    /// the element ID through this channel instead of directly calling
    /// `mark_needs_build()`. The pipeline drains the channel and calls
    /// `mark_needs_build()` itself, eliminating the need for raw pointers.
    dirty_sender: mpsc::Sender<ElementKey>,
    dirty_receiver: mpsc::Receiver<ElementKey>,

    /// Flag indicating if full reconcile is needed.
    ///
    /// True after initial mount or when root element type changes.
    needs_full_reconcile: bool,

    /// Cursor blink state for text editing cursors.
    cursor_blink: CursorBlinkState,

    /// Mouse tracker for cursor resolution and hover dispatch.
    mouse_tracker: MouseTracker,

    /// Cached render commands from the last paint pass.
    /// Returned on idle frames when nothing needs repainting.
    cached_commands: Option<Vec<RenderCommand>>,

    /// Animation ticker that fires per-frame callbacks for active animations.
    /// Passed to ElementContext so ComponentState::on_mount() can access it.
    animation_ticker: Arc<AnimationTicker>,

    /// Pipeline-owned registry of inherited-value providers and dependents.
    /// Passed by `&` to every `ElementContext` and `RenderContext`.
    inherited_registry: InheritedRegistry,

    /// Per-element `Arc<InheritedMap>`. Built top-down at mount: each element
    /// inherits its parent's map (Arc clone), and `InheritedElement`s insert
    /// their own type. Cleared on unmount.
    inherited_maps: SecondaryMap<ElementKey, Arc<InheritedMap>>,

    /// Per-pointer gesture arena. Created on press, dropped on release.
    /// Single-pointer only (InputEvent has no pointer id).
    pub(crate) current_arena: Option<GestureArena>,
}

impl ThreeTreePipeline {
    /// Create a new empty pipeline.
    pub fn new(animation_ticker: Arc<AnimationTicker>) -> Self {
        let (dirty_sender, dirty_receiver) = mpsc::channel();
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
            focus_manager: FocusManager::new(),
            build_owner: BuildOwner::new(),
            child_ops: ChildOps::new(),
            dirty_sender,
            dirty_receiver,
            needs_full_reconcile: true,
            cursor_blink: CursorBlinkState::new(),
            mouse_tracker: MouseTracker::new(),
            cached_commands: None,
            animation_ticker,
            inherited_registry: InheritedRegistry::new(),
            inherited_maps: SecondaryMap::new(),
            current_arena: None,
        }
    }

    /// Sync focused_element to BuildOwner so Component::render() can access it.
    fn sync_focus_to_build_owner(&self) {
        self.build_owner
            .set_focused_element(self.focus_manager.primary_focus_element());
    }

    /// Install the shared safe-area source on the [`BuildOwner`].
    ///
    /// Called once at window init by
    /// [`WindowState`](crate::window::WindowState) so the same atomics are
    /// shared between the window (which writes insets each frame) and the
    /// element tree (which reads them via
    /// [`RenderContext::media_query_sources()`](crate::stateful_widget::RenderContext::media_query_sources)).
    pub fn set_safe_area_source(&mut self, source: crate::core::SafeAreaSource) {
        self.build_owner.set_safe_area_source(source);
    }

    /// Install the keyboard-inset source into the [`BuildOwner`].
    ///
    /// Called once at window init by
    /// [`WindowState`](crate::window::WindowState) so the same atomics are
    /// shared between the window (which writes the current keyboard height
    /// each frame via the render-loop interpolation driver) and the element
    /// tree (which reads them via
    /// [`RenderContext::media_query_sources()`](crate::stateful_widget::RenderContext::media_query_sources)).
    pub fn set_keyboard_inset_source(&mut self, source: crate::core::KeyboardInsetSource) {
        self.build_owner.set_keyboard_inset_source(source);
    }

    /// Install the media-query data source on the [`BuildOwner`].
    ///
    /// Called once at window init by
    /// [`WindowState`](crate::window::WindowState) so the same atomics are
    /// shared between the window (which writes size/scale/brightness each
    /// frame) and the element tree (which reads them via
    /// [`RenderContext::media_query_sources()`](crate::stateful_widget::RenderContext::media_query_sources)).
    pub fn set_media_query_data_source(&mut self, source: crate::core::MediaQueryDataSource) {
        self.build_owner.set_media_query_data_source(source);
    }

    /// Reconcile a new widget tree with the existing element tree.
    ///
    /// This method:
    /// 1. Diffes the new widget tree against existing elements
    /// 2. Mounts new elements for new widgets
    /// 3. Updates existing elements where widgets match
    /// 4. Unmounts elements for removed widgets
    /// 5. Creates/destroys render objects accordingly
    /// 6. Marks affected render objects as dirty
    ///
    /// After reconciliation, the element tree reflects the new widget tree,
    /// and the dirty tracking indicates which render objects need layout/paint.
    ///
    /// # Arguments
    ///
    /// * `root_widget` - The new root widget configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    ///
    /// // Initial widget tree
    /// pipeline.reconcile(Box::new(Text::new("Hello")));
    ///
    /// // Updated widget tree (reconciliation preserves state for matching elements)
    /// pipeline.reconcile(Box::new(Text::new("Hello World")));
    /// ```
    #[allow(dead_code)]
    pub(crate) fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
        self.sync_focus_to_build_owner();
        Reconciler::reconcile(
            &mut self.element_registry,
            &mut self.render_objects,
            &mut self.state,
            &mut self.dirty,
            &mut self.build_owner,
            &mut self.child_ops,
            &self.dirty_sender,
            &mut self.focus_manager,
            &self.animation_ticker,
            &self.inherited_registry,
            &mut self.inherited_maps,
            root_widget,
        );
    }

    /// Reconcile or rebuild based on current state.
    ///
    /// This is the main entry point for frame updates.
    /// - First, performs any pending state-driven rebuilds
    /// - Then, reconciles the widget tree with the element tree
    ///
    /// After initial mount, prefer calling `mark_needs_build()` for updates.
    pub fn update(&mut self, root_widget: Box<dyn Widget>) {
        self.sync_focus_to_build_owner();
        Reconciler::update(
            &mut self.element_registry,
            &mut self.render_objects,
            &mut self.state,
            &mut self.dirty,
            &mut self.build_owner,
            &mut self.child_ops,
            &self.dirty_sender,
            &self.dirty_receiver,
            &mut self.focus_manager,
            &self.animation_ticker,
            &mut self.needs_full_reconcile,
            &self.inherited_registry,
            &mut self.inherited_maps,
            root_widget,
        );
    }

    /// Perform targeted rebuilds for dirty elements.
    ///
    /// This is the Flutter-style rebuild: only dirty elements and their
    /// subtrees are reconciled. Much more efficient than full-tree reconcile.
    pub fn perform_rebuilds(&mut self) {
        self.sync_focus_to_build_owner();
        Reconciler::perform_rebuilds(
            &mut self.element_registry,
            &mut self.render_objects,
            &mut self.state,
            &mut self.dirty,
            &mut self.build_owner,
            &mut self.child_ops,
            &self.dirty_sender,
            &mut self.focus_manager,
            &self.animation_ticker,
            &self.dirty_receiver,
            &self.inherited_registry,
            &mut self.inherited_maps,
        );

        // Apply deferred unfocus requests made during the rebuild above.
        //
        // Widgets that need to dismiss focus while rendering (notably
        // `NavigationStackView` when a pop transition starts) call
        // `LifecycleContext::clear_focus()`, which only has `&BuildOwner` and so
        // stashes the request there. Now that the mutable borrow of
        // `focus_manager` held by `Reconciler::perform_rebuilds` has been
        // released, we can drain the request and clear primary focus for real.
        //
        // `unfocus()` is a no-op when nothing is focused, and it sets
        // `focus_changed = true` otherwise — which the render-loop keyboard
        // sync in `WindowState::render_retain` picks up to dismiss the
        // software keyboard on mobile.
        if self.build_owner.take_unfocus_requested() {
            self.focus_manager.unfocus();
        }
    }

    /// Feed a `Tick` event to the active gesture arena (if any) and dispatch
    /// the winner if the Tick resolves the arena. Called once per frame from
    /// `WindowState::render_retain` right after `animation_ticker.tick()`.
    ///
    /// This is the clock that drives time-based recognizers (currently only
    /// `LongPressRecognizer`). Without this call, long-press would never
    /// fire — the arena is purely event-driven (Down/Move/Up/Cancel) and
    /// has no way to "wake up" at 500ms.
    ///
    /// If the arena resolves on this Tick (e.g. long-press accepts at
    /// 500ms), the winner element's `on_arena_winner_update` is called with
    /// the `Tick` event so it can fire its `on_long_press` callback. The
    /// `EventContext` is built with the recognizer's `down_position()` as
    /// the position (the press location — semantically the long-press
    /// happened *at* where the finger went down) and `Bounds::default()`
    /// (no hit-test is performed for a Tick; the winner's bounds aren't
    /// needed — long-press dispatch uses the recognizer's position).
    ///
    /// `font_system` and `clipboard` are threaded from `WindowState` (same
    /// as `handle_event`) because the pipeline doesn't own them.
    pub fn tick_arena(
        &mut self,
        now: std::time::Instant,
        font_system: &mut glyphon::FontSystem,
        clipboard: &std::sync::Arc<dyn crate::platform::Clipboard>,
    ) {
        use crate::gestures::{ArenaEvent, ArenaOutcome, LongPressRecognizer};

        // Disjoint field borrows: `current_arena` (mut, for the recognizer
        // reference handed to on_arena_winner_update), `element_registry`
        // (mut, for the element), and `render_objects`/`build_owner`/
        // `dirty_sender` (shared, for the EventContext) must all be alive
        // simultaneously. Splitting at the field level lets the borrow
        // checker see they don't overlap.
        let arena_opt = &mut self.current_arena;
        let element_registry = &mut self.element_registry;
        let render_objects = &self.render_objects;
        let build_owner = &self.build_owner;
        let dirty_sender = &self.dirty_sender;

        let arena = match arena_opt.as_mut() {
            Some(a) => a,
            None => return,
        };
        if arena.is_closed() {
            return;
        }

        let outcome = arena.handle_event(ArenaEvent::Tick { now });
        if !matches!(outcome, ArenaOutcome::Resolved { .. }) {
            // Open or ClosedNoWinner — nothing to dispatch.
            // (ClosedNoWinner on Tick shouldn't happen — Tick never Cancels —
            // but handle it defensively by not dispatching.)
            return;
        }

        let winner_id = match arena.winner_owner() {
            Some(id) => id,
            None => return,
        };

        // Position: the recognizer's down_position (the press location).
        // Only LongPressRecognizer produces Accepted on Tick; if the winner
        // is some other recognizer (defensive), skip dispatch.
        let position = match arena
            .winner_recognizer()
            .and_then(|r| r.as_any().downcast_ref::<LongPressRecognizer>())
        {
            Some(lp) => lp.down_position(),
            None => return,
        };

        let bounds = Bounds::default();

        let mut ctx = crate::event_context::EventContext::with_build_owner(
            winner_id,
            position,
            bounds,
            crate::input::Modifiers::default(),
            font_system,
            build_owner,
            dirty_sender,
            Some(render_objects),
            clipboard.clone(),
        );

        let winner_recognizer = match arena.winner_recognizer() {
            Some(r) => r,
            None => return,
        };
        if let Some(element) = element_registry.get_mut(winner_id) {
            element.on_arena_winner_update(winner_recognizer, &ArenaEvent::Tick { now }, &mut ctx);
        }
    }

    /// Mark an element as needing rebuild.
    ///
    /// This is the entry point for Flutter-style targeted rebuilds.
    /// Elements call this when their state changes (via `Signal::set` or the
    /// dirty callback).
    pub fn mark_needs_build(&mut self, element_id: ElementKey) {
        self.build_owner.mark_needs_build(element_id);
    }

    /// Check if there are pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        self.build_owner.has_pending_rebuilds()
    }

    /// Drain the dirty channel and mark elements for rebuild.
    ///
    /// This must be called after event handling so that elements whose dirty
    /// callbacks sent their ID through the mpsc channel (e.g., AnimationController)
    /// are visible to `has_pending_rebuilds()` before the frame-request check.
    pub fn drain_dirty_to_build_owner(&mut self) {
        while let Ok(element_id) = self.dirty_receiver.try_recv() {
            self.build_owner.mark_needs_build(element_id);
        }
    }

    /// Take the focus-changed flag. Returns true if focus state changed
    /// and clears the flag.
    pub fn take_focus_changed(&mut self) -> bool {
        self.focus_manager.take_focus_changed()
    }

    /// Returns true if a frame is needed due to dirty state changes
    /// (mark_needs_paint/mark_needs_layout was called), and clears the flag.
    pub fn take_frame_request_needed(&mut self) -> bool {
        self.dirty.take_frame_request_needed()
    }

    /// Check if full reconcile is needed (initial mount or root type change).
    pub fn needs_full_reconcile(&self) -> bool {
        self.needs_full_reconcile
    }

    /// Perform layout using Taffy layout engine.
    ///
    /// Three-phase layout:
    /// 1. Build Taffy tree (each RenderObject creates nodes)
    /// 2. Compute layout with Taffy
    /// 3. Apply computed layouts back to RenderObjects
    ///
    /// # Arguments
    ///
    /// * `available_size` - The size available for the root render object
    /// * `engine` - Layout engine for node creation and computation
    /// * `font_system` - Font system for text measurement
    ///
    /// # Example
    ///
    /// ```ignore
    /// pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);
    /// ```
    pub fn layout(
        &mut self,
        available_size: Size<Logical>,
        engine: &mut dyn crate::layout::LayoutEngine,
        font_system: &mut glyphon::FontSystem,
    ) {
        Layouter::layout(
            &mut self.render_objects,
            &mut self.dirty,
            available_size,
            engine,
            font_system,
        );
        self.cached_commands = None;
    }

    /// Generate render commands from the render object tree.
    ///
    /// This method only paints objects that are marked as needing paint.
    /// It traverses from the root but only generates commands for dirty objects.
    ///
    /// # Returns
    ///
    /// A vector of `RenderCommand` that can be submitted to a render backend.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let commands = pipeline.paint();
    /// // Submit commands to renderer
    /// for cmd in commands {
    ///     renderer.submit(cmd);
    /// }
    /// ```
    pub fn paint(&mut self) -> Vec<RenderCommand> {
        if self.dirty.is_paint_empty() && self.cached_commands.is_some() {
            return self.cached_commands.clone().unwrap();
        }

        let commands = Painter::paint(&self.render_objects, &mut self.dirty);
        self.cached_commands = Some(commands.clone());
        commands
    }

    /// Hit test at a given position.
    ///
    /// Determines which render object (if any) is at the given position.
    /// Returns a `HitTestResult` containing the path from root to the hit target.
    ///
    /// # Arguments
    ///
    /// * `position` - The position to test in absolute window coordinates
    ///
    /// # Returns
    ///
    /// A `HitTestResult` with the path to the hit target, or a miss result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = pipeline.hit_test(Position::new(100.0, 100.0));
    /// if let Some(target) = result.target() {
    ///     // Handle input on target render object
    /// }
    /// ```
    pub fn hit_test(&self, position: Position<Logical, Absolute>) -> HitTestResult {
        EventHandler::hit_test(&self.render_objects, position)
    }

    /// Resolve the cursor at the given position using Flutter's
    /// `firstNonDeferred()` annotation traversal.
    ///
    /// Also dispatches hover enter/exit callbacks for MouseRegion widgets.
    /// Returns the resolved cursor and whether the hover set changed
    /// (which requires a frame request to reflect visual changes from
    /// on_enter/on_exit callbacks).
    pub fn cursor_at(&mut self, position: Position<Logical, Absolute>) -> (SystemCursorKind, bool) {
        let hit_result = self.render_objects.hit_test(position);

        let (cursor, annotations) = if hit_result.is_hit() {
            let cursor = MouseTracker::resolve_cursor(hit_result.annotations());
            (cursor, hit_result.annotations().to_vec())
        } else {
            (SystemCursorKind::Arrow, Vec::new())
        };

        self.dispatch_hover(&annotations);

        let hover_changed = self.mouse_tracker.hover_changed();
        self.mouse_tracker.set_current_cursor(cursor);
        (cursor, hover_changed)
    }

    /// Dispatch hover enter/exit callbacks.
    ///
    /// Compares the new hovered annotations against the last set.
    /// Fires on_enter for entering elements, on_exit for leaving elements
    /// (looked up from the render object registry).
    fn dispatch_hover(&mut self, new_hovered: &[(ElementKey, MouseTrackerAnnotation)]) {
        let exiting = self.mouse_tracker.dispatch_hover_changes(new_hovered);

        // Look up on_exit callbacks from the registry for elements leaving hover
        let exit_annotations: Vec<MouseTrackerAnnotation> = exiting
            .iter()
            .filter_map(|&element_key| {
                self.render_objects
                    .cursor_annotation_for_element(element_key)
                    .cloned()
            })
            .collect();

        self.mouse_tracker
            .dispatch_hover_exit_for(&exit_annotations);
    }

    /// Post-frame cursor update. Re-hit-tests at the last mouse position
    /// and returns the new cursor if it changed.
    ///
    /// Also dispatches hover enter/exit for widgets moving under a
    /// stationary mouse.
    pub fn post_frame_cursor_update(&mut self) -> Option<SystemCursorKind> {
        if let Some(position) = self.mouse_tracker.last_mouse_position() {
            let hit_result = self.render_objects.hit_test(position);
            let (new_cursor, annotations) = if hit_result.is_hit() {
                (
                    MouseTracker::resolve_cursor(hit_result.annotations()),
                    hit_result.annotations().to_vec(),
                )
            } else {
                (SystemCursorKind::Arrow, Vec::new())
            };

            // Dispatch hover changes (widgets may have moved under the mouse)
            self.dispatch_hover(&annotations);

            self.mouse_tracker.update_cursor_post_frame(new_cursor)
        } else {
            None
        }
    }

    /// Get mutable access to the mouse tracker.
    pub fn mouse_tracker_mut(&mut self) -> &mut MouseTracker {
        &mut self.mouse_tracker
    }

    /// Handle an input event.
    ///
    /// For pointer events, performs hit testing to find the target element.
    /// For keyboard events, dispatches to the focused element.
    ///
    /// Returns `Some(message)` if the event was handled and produced a message.
    /// The message is returned as `Box<dyn Any>` and should be downcast to the
    /// specific message type by the caller.
    pub fn handle_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        font_system: &mut glyphon::FontSystem,
        scale_source: &ScaleSource,
        clipboard: &std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Option<Box<dyn Any>> {
        let result = EventHandler::handle_event(
            &mut self.element_registry,
            &self.render_objects,
            &mut self.state,
            font_system,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.focus_manager,
            &mut self.current_arena,
            position,
            event,
            modifiers,
            scale_source,
            clipboard,
        );

        // Commit deferred focus changes
        self.focus_manager.apply_focus_changes();

        result
    }

    /// Cancel any active gesture arena (e.g. on window unfocus).
    ///
    /// Feeds Cancel to the arena (all recognizers reject, no winner fires),
    /// then drops it. Safe to call when no arena is active (no-op).
    pub fn cancel_current_gesture(&mut self) {
        if let Some(mut arena) = self.current_arena.take() {
            arena.handle_event(crate::gestures::ArenaEvent::Cancel);
        }
    }

    /// Get the currently focused element.
    pub fn focused_element(&self) -> Option<ElementKey> {
        self.focus_manager.primary_focus_element()
    }

    /// Set focus to an element.
    ///
    /// After FocusAttachment integration, every mounted element already has
    /// a FocusNode. This method looks it up rather than creating one on demand.
    /// Pass `None` to clear focus.
    pub fn set_focus(&mut self, element: Option<ElementKey>) {
        if let Some(element_key) = element {
            let node_id = self
                .focus_manager
                .node_for_element(element_key)
                .expect("Focus node must exist — all mounted elements have FocusAttachments");
            self.focus_manager.request_focus(node_id);
        } else {
            self.focus_manager.unfocus();
        }
        // Apply deferred focus changes immediately for programmatic focus changes.
        self.focus_manager.apply_focus_changes();
    }

    /// Get the element registry.
    pub fn element_registry(&self) -> &ElementRegistry {
        &self.element_registry
    }

    /// Get the render object registry.
    pub fn render_objects(&self) -> &RenderObjectRegistry {
        &self.render_objects
    }

    /// Get the build owner.
    pub fn build_owner(&self) -> &BuildOwner {
        &self.build_owner
    }

    /// Get the focus manager.
    pub fn focus_manager(&self) -> &FocusManager {
        &self.focus_manager
    }

    /// Get mutable access to the focus manager.
    pub fn focus_manager_mut(&mut self) -> &mut FocusManager {
        &mut self.focus_manager
    }

    /// Check if any render objects need layout.
    pub fn needs_layout(&self) -> bool {
        !self.dirty.is_layout_empty()
    }

    /// Check if any render objects need paint.
    pub fn needs_paint(&self) -> bool {
        !self.dirty.is_paint_empty()
    }

    /// Mark the focused and previously-focused render object subtrees
    /// for paint.
    ///
    /// When focus changes or cursor blink toggles, we need to repaint the
    /// TextEditRenderObject (which paints the caret). The focused element's
    /// ProxyRenderObject paints nothing, so we walk its subtree to find the
    /// actual TextEditRenderObject and mark it dirty.
    ///
    /// Also marks the previously-focused subtree so the old cursor disappears.
    pub fn mark_focus_subtree_needs_paint(&mut self) {
        // Mark currently focused subtree (cursor appears)
        if let Some(focused_el) = self.focus_manager.primary_focus_element() {
            if let Some(ro_id) = self
                .element_registry
                .with_element(focused_el, &mut (), |el, _| el.render_object())
                .flatten()
            {
                self.dirty.mark_needs_paint(ro_id);
                Self::mark_subtree_needs_paint(&self.render_objects, ro_id, &mut self.dirty);
            }
        }

        // Mark previously focused subtree (cursor disappears)
        if let Some(prev_el) = self.focus_manager.previous_primary_focus() {
            if let Some(ro_id) = self
                .element_registry
                .with_element(prev_el, &mut (), |el, _| el.render_object())
                .flatten()
            {
                self.dirty.mark_needs_paint(ro_id);
                Self::mark_subtree_needs_paint(&self.render_objects, ro_id, &mut self.dirty);
            }
        }
    }

    /// Recursively mark all render objects in a subtree for paint.
    fn mark_subtree_needs_paint(
        render_objects: &RenderObjectRegistry,
        root: crate::id::RenderObjectKey,
        dirty: &mut DirtyTracking,
    ) {
        if let Some(ro) = render_objects.get(root) {
            for child in ro.children() {
                dirty.mark_needs_paint(*child);
                Self::mark_subtree_needs_paint(render_objects, *child, dirty);
            }
        }
    }

    /// Mark focus-related elements for rebuild when focus changes.
    ///
    /// Focus-dependent styling (e.g., TextEdit's border color) is computed
    /// in Component::render() via RenderContext::is_focused(). A repaint
    /// alone doesn't help because ProxyRenderObject.paint() returns empty
    /// commands — the visual output comes from the child DecoratedBox
    /// which needs a new widget configuration from render().
    ///
    /// Also marks ancestor elements with `on_focus_change` callbacks for
    /// rebuild, so that Focus-wrapped widgets update their visual state
    /// when a descendant gains or loses focus.
    ///
    /// This follows Flutter's model: focus change → setState() →
    /// markNeedsBuild() → render() → reconciliation → markNeedsPaint().
    pub fn mark_focus_needs_build(&mut self) {
        // Mark the currently focused element for rebuild (e.g., gain blue border)
        if let Some(focused_el) = self.focus_manager.primary_focus_element() {
            self.build_owner.mark_needs_build(focused_el);
        }

        // Mark the previously focused element for rebuild (e.g., lose blue border)
        if let Some(prev_el) = self.focus_manager.previous_primary_focus() {
            self.build_owner.mark_needs_build(prev_el);
        }

        // Mark Focus ancestor elements for rebuild when their descendants
        // gain or lose focus, so on_focus_change callbacks take visual effect.
        let ancestors: Vec<crate::id::ElementKey> = {
            let mut result = Vec::new();
            if let Some(focused) = self.focus_manager.primary_focus() {
                result.extend(self.focus_manager.ancestor_elements_with_callbacks(focused));
            }
            if let Some(prev_node) = self.focus_manager.previous_primary_focus_node() {
                if self.focus_manager.primary_focus() != Some(prev_node) {
                    result.extend(
                        self.focus_manager
                            .ancestor_elements_with_callbacks(prev_node),
                    );
                }
            }
            result
        };
        for ek in ancestors {
            self.build_owner.mark_needs_build(ek);
        }
    }

    /// Clear all dirty tracking.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Mark all render objects as needing layout and paint.
    ///
    /// Useful when the window size changes and the entire tree
    /// must be re-laid-out with the new available space.
    pub fn mark_all_needs_layout(&mut self) {
        for id in self.render_objects.keys() {
            self.dirty.mark_needs_layout(id);
        }
    }

    /// Check if blink toggled and mark dirty if so.
    /// Call from the event loop's about_to_wait callback.
    /// Returns true if a repaint is needed due to blink toggle.
    pub fn check_cursor_blink(&mut self) -> bool {
        if self.cursor_blink.check_and_toggle() {
            // Only the focused TextEditRenderObject needs repaint for blink,
            // not its parent containers.
            if let Some(focused_el) = self.focus_manager.primary_focus_element() {
                if let Some(ro_id) = self
                    .element_registry
                    .with_element(focused_el, &mut (), |el, _| el.render_object())
                    .flatten()
                {
                    self.dirty.mark_needs_paint(ro_id);
                }
            }
            return true;
        }
        false
    }

    /// Reset cursor blink to visible. Call on keyboard input or focus gain.
    /// Returns true if visibility changed (repaint needed).
    pub fn reset_cursor_blink(&mut self) -> bool {
        self.cursor_blink.reset()
    }

    /// Inject focus and cursor blink state into TextEditRenderObjects.
    ///
    /// Called between layout and paint. Walks the render object tree,
    /// finds TextEditRenderObject instances, and sets their focus/blink state.
    /// This avoids adding these fields to PaintContext (which every render
    /// object would see).
    pub fn prepare_cursor_state(&mut self) {
        let focused_element = self.focus_manager.primary_focus_element();
        let blink_visible = self.cursor_blink.is_visible();
        // Only the text input's own element (tagged via
        // `ComponentState::requests_focus_on_click()`) should paint a focused
        // cursor. An ancestor (e.g. a ScrollView) that merely contains a
        // TextEdit must not — otherwise clicking anywhere inside it would
        // light up the cursor (and, via `is_text_input_focused`, the keyboard).
        let focused_is_text_input = self.focus_manager.is_primary_focus_text_input();

        // First pass: set all TextEditRenderObjects to unfocused with current blink state
        for (_, ro) in self.render_objects.iter_mut() {
            if let Some(text_edit_ro) = ro
                .as_any_mut()
                .downcast_mut::<crate::render_objects::TextEditRenderObject>()
            {
                text_edit_ro.set_focused(false);
                text_edit_ro.set_cursor_blink_visible(blink_visible);
            }
        }

        // If a text-input element is focused, find its subtree's
        // TextEditRenderObject and set focused=true. Because the focused
        // element *is* the text input, its subtree contains exactly the one
        // TextEditRenderObject to light up.
        if focused_is_text_input {
            if let Some(focused_key) = focused_element {
                // Get the focused element's render object key
                let focused_ro =
                    self.element_registry
                        .with_element(focused_key, &mut (), |element, _| element.render_object());

                if let Some(ro_key) = focused_ro.flatten() {
                    Self::set_cursor_focus_in_subtree(
                        &mut self.render_objects,
                        ro_key,
                        blink_visible,
                    );
                }
            }
        }
    }

    /// Recursively walk a render object subtree to find and focus a TextEditRenderObject.
    fn set_cursor_focus_in_subtree(
        render_objects: &mut RenderObjectRegistry,
        root: crate::id::RenderObjectKey,
        blink_visible: bool,
    ) {
        // Try to downcast the root render object
        if let Some(ro) = render_objects.get_mut(root) {
            if let Some(text_edit_ro) = ro
                .as_any_mut()
                .downcast_mut::<crate::render_objects::TextEditRenderObject>()
            {
                text_edit_ro.set_focused(true);
                text_edit_ro.set_cursor_blink_visible(blink_visible);
                return;
            }
        }

        // Recurse into children
        let children: Vec<_> = render_objects
            .get(root)
            .map(|r| r.children().to_vec())
            .unwrap_or_default();
        for child in children {
            Self::set_cursor_focus_in_subtree(render_objects, child, blink_visible);
        }
    }

    /// Returns true if the currently-focused element is itself a text input.
    ///
    /// Used on iOS to decide whether the software keyboard should be shown:
    /// tapping a `TextEdit` focuses it → this returns true → `set_ime_allowed(true)`.
    /// Tapping elsewhere (or nothing focused) → returns false → `set_ime_allowed(false)`.
    ///
    /// This is an O(1) check of the primary focus node's `is_text_input` flag
    /// (set from `ComponentState::requests_focus_on_click()`). It deliberately
    /// does *not* walk the render-object subtree: an ancestor of a TextEdit
    /// (e.g. a ScrollView) that happens to contain one must not trigger the
    /// keyboard, which the previous subtree walk did.
    pub fn is_text_input_focused(&mut self) -> bool {
        self.focus_manager.is_primary_focus_text_input()
    }

    /// Register any unregistered images with the GPU backend.
    ///
    /// Walks all render objects and checks `needs_image_registration()`.
    /// For any that return `Some(&ImageData)`, registers the image with
    /// the backend and calls `set_image_key()` with the resulting key.
    pub fn register_images(&mut self, backend: &mut crate::render::WgpuBackend) {
        for (_, ro) in self.render_objects.iter_mut() {
            if let Some(image_data) = ro.needs_image_registration() {
                let key = backend.register_image(image_data);
                ro.set_image_key(key);
            }
        }
    }

    /// Return atlas slots from removed image render objects to the backend.
    ///
    /// Drains the orphaned image keys collected by `RenderObjectRegistry::remove`
    /// and calls `unregister_image` on the backend for each. This must run every
    /// frame (before `register_images`) so that slots freed by a pop are
    /// available for reuse by a subsequent push; otherwise the 2048x2048 atlas
    /// fills up after a few dozen push/pop cycles on iOS.
    pub fn unregister_images(&mut self, backend: &mut crate::render::WgpuBackend) {
        for key in self.render_objects.drain_orphaned_image_keys() {
            backend.unregister_image(key);
        }
    }
}

impl Default for ThreeTreePipeline {
    fn default() -> Self {
        Self::new(Arc::new(AnimationTicker::new()))
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::TaffyLayoutEngine;
    use crate::Text;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_pipeline_new() {
        let pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        assert!(pipeline.element_registry().is_empty());
        assert!(pipeline.render_objects().is_empty());
        assert!(!pipeline.needs_layout());
        assert!(!pipeline.needs_paint());
    }

    #[test]
    fn test_pipeline_default() {
        let pipeline = ThreeTreePipeline::default();

        assert!(pipeline.element_registry().is_empty());
        assert!(pipeline.render_objects().is_empty());
    }

    #[test]
    fn test_pipeline_reconcile_single_widget() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Reconcile with a text widget
        let widget = Text::new("Hello");
        pipeline.reconcile(Box::new(widget));

        // Should have one element and one render object
        assert_eq!(pipeline.element_registry().len(), 1);
        assert_eq!(pipeline.render_objects().len(), 1);

        // Should have a root
        assert!(pipeline.element_registry().root().is_some());
        assert!(pipeline.render_objects().root().is_some());

        // Should need layout
        assert!(pipeline.needs_layout());
    }

    #[test]
    fn test_pipeline_reconcile_updates_matching() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Initial widget
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let initial_root = pipeline.element_registry().root();

        // Update with matching widget (same type, same key)
        pipeline.reconcile(Box::new(Text::new("Hello World")));

        // Should have same root element (updated, not remounted)
        assert_eq!(pipeline.element_registry().root(), initial_root);

        // Should still have one element
        assert_eq!(pipeline.element_registry().len(), 1);
    }

    #[test]
    fn test_pipeline_layout() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Reconcile first
        pipeline.reconcile(Box::new(Text::new("Hello")));

        // Layout with available size
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // After layout, dirty flags should be cleared
        assert!(!pipeline.needs_layout());
    }

    #[test]
    fn test_pipeline_paint_empty() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Paint with no render objects
        let commands = pipeline.paint();

        assert!(commands.is_empty());
    }

    #[test]
    fn test_pipeline_paint_with_content() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Paint - text render object returns text commands
        let commands = pipeline.paint();

        // TextRenderObject generates Text render commands
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_pipeline_hit_test_miss() {
        let pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Hit test with no content
        let result = pipeline.hit_test(Position::new(100.0, 100.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_hit_test_with_content() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test inside the text bounds
        let result = pipeline.hit_test(Position::new(5.0, 5.0));

        // Should hit the text render object
        assert!(result.is_hit());
        assert!(result.target().is_some());
    }

    #[test]
    fn test_pipeline_hit_test_outside() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test outside the text bounds but inside the root container
        // (root now fills the viewport, so this hits the container but not the text)
        let result = pipeline.hit_test(Position::new(500.0, 500.0));

        // The root container fills the viewport, so this is a hit
        assert!(result.is_hit());
    }

    #[test]
    fn test_pipeline_clear_dirty() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        pipeline.reconcile(Box::new(Text::new("Hello")));

        // Should need layout after reconcile
        assert!(pipeline.needs_layout());

        // Clear dirty
        pipeline.clear_dirty();

        assert!(!pipeline.needs_layout());
        assert!(!pipeline.needs_paint());
    }

    #[test]
    fn test_pipeline_mark_all_needs_layout() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        pipeline.reconcile(Box::new(Text::new("Hello")));
        pipeline.clear_dirty();

        // Mark all as needing layout
        pipeline.mark_all_needs_layout();

        assert!(pipeline.needs_layout());
    }

    #[test]
    fn test_pipeline_reconcile_replaces_different_type() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Initial widget
        pipeline.reconcile(Box::new(Text::new("Hello")));
        let _initial_root = pipeline.element_registry().root();

        // Clear dirty for comparison
        pipeline.clear_dirty();

        // Different type would cause remount (can't update)
        // For this test, we use a different Text instance with different content
        // Note: Currently Text widgets can update each other (same type, no key)
        // To test remount, we would need different widget types

        // Just verify the element still exists
        assert_eq!(pipeline.element_registry().len(), 1);
    }

    #[test]
    fn test_pipeline_syncs_focused_element_to_build_owner() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(Text::new("Hello")));

        // Set focus on the root element
        let root_id = pipeline.element_registry().root().unwrap();
        pipeline.set_focus(Some(root_id));

        // After update, BuildOwner should have the focused element
        pipeline.update(Box::new(Text::new("Hello")));
        assert_eq!(pipeline.build_owner().focused_element(), Some(root_id));
    }
}
