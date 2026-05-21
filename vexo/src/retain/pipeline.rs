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
use crate::input::{InputEvent, Modifiers};
use crate::render::RenderCommand;

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
use super::state::StateStorage;
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
        Painter::paint(&self.render_objects, &mut self.dirty)
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
    /// If the element does not yet have a FocusNode in the focus tree,
    /// one is created under the root scope before requesting focus.
    /// Pass `None` to clear focus.
    pub fn set_focus(&mut self, element: Option<ElementKey>) {
        if let Some(element_key) = element {
            // Ensure a FocusNode exists for this element.
            let node_id = if let Some(existing) = self.focus_manager.node_for_element(element_key) {
                existing
            } else {
                self.focus_manager.create_node_with_element(
                    self.focus_manager.root_scope(),
                    element_key,
                )
            };
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

    /// Check if any render objects need layout.
    pub fn needs_layout(&self) -> bool {
        !self.dirty.is_layout_empty()
    }

    /// Check if any render objects need paint.
    pub fn needs_paint(&self) -> bool {
        !self.dirty.is_paint_empty()
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
    use crate::retain::Text;
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
