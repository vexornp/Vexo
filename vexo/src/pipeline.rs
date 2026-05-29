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
//! let mut pipeline = ThreeTreePipeline::new();
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
use std::sync::mpsc;

use crate::core::{Absolute, Logical, Point, Position, Size};
use crate::mouse_tracker::MouseTracker;
use crate::input::{InputEvent, Modifiers, MouseTrackerAnnotation, SystemCursorKind};
use crate::render::RenderCommand;

use crate::state::CursorBlinkState;
use super::build_owner::BuildOwner;
use super::child_ops::ChildOps;
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::event_handler::EventHandler;
use super::focus::FocusManager;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::layouter::Layouter;
use super::painter::Painter;
use super::reconciler::Reconciler;
use super::render_object::RenderObjectRegistry;
use super::element_state::StateStorage;
use super::widgets::Widget;

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
/// let mut pipeline = ThreeTreePipeline::new();
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

    /// Channel for receiving dirty element signals from StatefulMutable callbacks.
    ///
    /// When a `StatefulMutable::set()` fires its dirty callback, it sends
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
}

impl ThreeTreePipeline {
    /// Create a new empty pipeline.
    pub fn new() -> Self {
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
        }
    }

    /// Sync focused_element to BuildOwner so StatefulWidget::build() can access it.
    fn sync_focus_to_build_owner(&self) {
        self.build_owner.set_focused_element(self.focus_manager.primary_focus_element());
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
    /// let mut pipeline = ThreeTreePipeline::new();
    ///
    /// // Initial widget tree
    /// pipeline.reconcile(Box::new(Text::new("Hello")));
    ///
    /// // Updated widget tree (reconciliation preserves state for matching elements)
    /// pipeline.reconcile(Box::new(Text::new("Hello World")));
    /// ```
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
            &mut self.needs_full_reconcile,
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
            &self.dirty_receiver,
        );
    }

    /// Mark an element as needing rebuild.
    ///
    /// This is the entry point for Flutter-style targeted rebuilds.
    /// Elements call this when their state changes (e.g., setState equivalent).
    pub fn mark_needs_build(&mut self, element_id: ElementKey) {
        self.build_owner.mark_needs_build(element_id);
    }

    /// Check if there are pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        self.build_owner.has_pending_rebuilds()
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
        Layouter::layout(&mut self.render_objects, &mut self.dirty, available_size, engine, font_system);
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

        self.mouse_tracker.dispatch_hover_exit_for(&exit_annotations);
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
                (MouseTracker::resolve_cursor(hit_result.annotations()), hit_result.annotations().to_vec())
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
    ) -> Option<Box<dyn Any>> {
        let result = EventHandler::handle_event(
            &mut self.element_registry,
            &self.render_objects,
            &mut self.state,
            font_system,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.focus_manager,
            position,
            event,
            modifiers,
        );

        // Commit deferred focus changes
        self.focus_manager.apply_focus_changes();

        result
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
            let node_id = self.focus_manager
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
            if let Some(ro_id) = self.element_registry.with_element(focused_el, &mut (), |el, _| el.render_object()).flatten() {
                self.dirty.mark_needs_paint(ro_id);
                Self::mark_subtree_needs_paint(&self.render_objects, ro_id, &mut self.dirty);
            }
        }

        // Mark previously focused subtree (cursor disappears)
        if let Some(prev_el) = self.focus_manager.previous_primary_focus() {
            if let Some(ro_id) = self.element_registry.with_element(prev_el, &mut (), |el, _| el.render_object()).flatten() {
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
    /// in StatefulWidget::build() via BuildContext::is_focused(). A repaint
    /// alone doesn't help because ProxyRenderObject.paint() returns empty
    /// commands — the visual output comes from the child DecoratedContainer
    /// which needs a new widget configuration from build().
    ///
    /// This follows Flutter's model: focus change → setState() →
    /// markNeedsBuild() → build() → reconciliation → markNeedsPaint().
    pub fn mark_focus_needs_build(&mut self) {
        // Mark the currently focused element for rebuild (e.g., gain blue border)
        if let Some(focused_el) = self.focus_manager.primary_focus_element() {
            self.build_owner.mark_needs_build(focused_el);
        }

        // Mark the previously focused element for rebuild (e.g., lose blue border)
        if let Some(prev_el) = self.focus_manager.previous_primary_focus() {
            self.build_owner.mark_needs_build(prev_el);
        }
    }

    /// Clear all dirty tracking.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Mark all render objects as needing layout.
    ///
    /// Useful when the window size changes.
    pub fn mark_all_needs_layout(&mut self) {
        // Mark root
        if let Some(root) = self.render_objects.root() {
            self.dirty.mark_needs_layout(root);
            self.dirty.mark_needs_paint(root);
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
                if let Some(ro_id) = self.element_registry.with_element(focused_el, &mut (), |el, _| el.render_object()).flatten() {
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

        // First pass: set all TextEditRenderObjects to unfocused with current blink state
        for (_, ro) in self.render_objects.iter_mut() {
            if let Some(text_edit_ro) = ro.as_any_mut().downcast_mut::<crate::render_objects::TextEditRenderObject>() {
                text_edit_ro.set_focused(false);
                text_edit_ro.set_cursor_blink_visible(blink_visible);
            }
        }

        // If an element is focused, find its subtree's TextEditRenderObject and set focused=true
        if let Some(focused_key) = focused_element {
            // Get the focused element's render object key
            let focused_ro = self.element_registry.with_element(focused_key, &mut (), |element, _| {
                element.render_object()
            });

            if let Some(ro_key) = focused_ro.flatten() {
                Self::set_cursor_focus_in_subtree(&mut self.render_objects, ro_key, blink_visible);
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
            if let Some(text_edit_ro) = ro.as_any_mut().downcast_mut::<crate::render_objects::TextEditRenderObject>() {
                text_edit_ro.set_focused(true);
                text_edit_ro.set_cursor_blink_visible(blink_visible);
                return;
            }
        }

        // Recurse into children
        let children: Vec<_> = render_objects.get(root)
            .map(|r| r.children().to_vec())
            .unwrap_or_default();
        for child in children {
            Self::set_cursor_focus_in_subtree(render_objects, child, blink_visible);
        }
    }
}

impl Default for ThreeTreePipeline {
    fn default() -> Self {
        Self::new()
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
        let pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

        // Paint with no render objects
        let commands = pipeline.paint();

        assert!(commands.is_empty());
    }

    #[test]
    fn test_pipeline_paint_with_content() {
        let mut pipeline = ThreeTreePipeline::new();

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
        let pipeline = ThreeTreePipeline::new();

        // Hit test with no content
        let result = pipeline.hit_test(Position::new(100.0, 100.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_hit_test_with_content() {
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test outside the text bounds
        let result = pipeline.hit_test(Position::new(500.0, 500.0));

        // Should miss
        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_clear_dirty() {
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();

        pipeline.reconcile(Box::new(Text::new("Hello")));
        pipeline.clear_dirty();

        // Mark all as needing layout
        pipeline.mark_all_needs_layout();

        assert!(pipeline.needs_layout());
    }

    #[test]
    fn test_pipeline_reconcile_replaces_different_type() {
        let mut pipeline = ThreeTreePipeline::new();

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
        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(Text::new("Hello")));

        // Set focus on the root element
        let root_id = pipeline.element_registry().root().unwrap();
        pipeline.set_focus(Some(root_id));

        // After update, BuildOwner should have the focused element
        pipeline.update(Box::new(Text::new("Hello")));
        assert_eq!(pipeline.build_owner().focused_element(), Some(root_id));
    }
}
