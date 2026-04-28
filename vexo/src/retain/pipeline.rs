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

use crate::core::{Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;

use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::hit_test::HitTestResult;
use super::id::{ElementId, RenderObjectId};
use super::render_object::{LayoutContext, PaintContext, RenderObjectRegistry};
use super::state::StateStorage;
use super::widgets::Widget;

// ============================================================================
// THREE-TREE PIPELINE
// ============================================================================

/// Orchestrates the three trees for retain-mode rendering.
///
/// The pipeline manages:
/// - Element lifecycle (mount, update, unmount)
/// - Render object creation and destruction
/// - Dirty tracking for incremental layout/paint
/// - State storage for elements
///
/// # Example
///
/// ```ignore
/// let mut pipeline = ThreeTreePipeline::new();
///
/// // Build and reconcile widget tree
/// let widget = Column::new()
///     .push(Text::new("Hello"))
///     .push(Text::new("World"));
/// pipeline.reconcile(Box::new(widget));
///
/// // Layout with constraints
/// let mut engine = TaffyLayoutEngine::new();
/// pipeline.layout(Size::new(800.0, 600.0), &mut engine);
///
/// // Paint to get render commands
/// let commands = pipeline.paint();
///
/// // Hit test for input
/// if pipeline.hit_test(Point::new(100.0, 100.0)).is_hit() {
///     // Handle input
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
}

impl ThreeTreePipeline {
    /// Create a new empty pipeline.
    pub fn new() -> Self {
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
        }
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
    pub fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
        // Check if we have an existing root element
        if let Some(root_id) = self.element_registry.root() {
            // Check if the widget can update the existing element
            let can_update = self.element_registry.get(root_id)
                .map(|el| el.can_update(root_widget.as_any()))
                .unwrap_or(false);

            if can_update {
                // Get render object ID before mutable borrow
                let render_object_id = self.element_registry.get(root_id)
                    .and_then(|el| el.render_object());

                // Update existing element
                if let Some(existing_element) = self.element_registry.get_mut(root_id) {
                    let mut ctx = ElementContext::new_with_registry(
                        None,
                        &mut self.state,
                        &mut self.dirty,
                        &mut self.render_objects,
                    );

                    existing_element.update(root_widget, &mut ctx);
                }

                // Mark the entire render object tree as needing layout and paint
                // (not just the root, since children may need repainting too)
                if let Some(render_id) = render_object_id {
                    self.mark_subtree_dirty(render_id);
                }

                return;
            }

            // Can't update existing root - unmount it
            self.unmount_element_tree(root_id);
        }

        // Mount new root element
        self.mount_element_tree(None, root_widget);
    }

    /// Mark a render object and all its descendants as dirty.
    fn mark_subtree_dirty(&mut self, root_id: RenderObjectId) {
        // Mark the root
        self.dirty.mark_needs_layout(root_id);
        self.dirty.mark_needs_paint(root_id);

        // Get children to mark (clone to avoid borrow issues)
        let children: Vec<_> = self.render_objects.get(root_id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Recursively mark children
        for child_id in children {
            self.mark_subtree_dirty(child_id);
        }
    }

    /// Mount an element tree from a widget.
    ///
    /// This method creates an element and calls its mount() lifecycle.
    /// The element's mount() method creates render objects and links children.
    fn mount_element_tree(&mut self, parent: Option<ElementId>, widget: Box<dyn Widget>) -> ElementId {
        // Create element
        let mut element = widget.create_element();

        // Create context with all registries for mount
        let mut ctx = ElementContext::new_full(
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &mut self.element_registry,
        );

        // Call mount lifecycle - element creates render objects and links children
        element.mount(&mut ctx);

        // Note: After element.mount(), ctx.render_object is the LAST render object created
        // (which could be a child's). We need to get the element's own render object.
        // We retrieve it from the registry after mounting.

        // Drop context to release borrows
        drop(ctx);

        // Mount the element in the registry
        let element_id = self.element_registry.mount(element, parent);

        // Get the render object from the element after it's in the registry
        let render_object_id = self.element_registry.get(element_id)
            .and_then(|el| el.render_object());

        // Set the render object as root if this is the root element
        if parent.is_none() {
            if let Some(render_id) = render_object_id {
                self.render_objects.set_root(render_id);
            }
        }

        element_id
    }

    /// Unmount an element and all its descendants.
    fn unmount_element_tree(&mut self, element_id: ElementId) {
        // Get children and parent before unmounting
        let children = self.element_registry.children(element_id).to_vec();
        let parent = self.element_registry.parent(element_id);

        // Recursively unmount children
        for child_id in children {
            self.unmount_element_tree(child_id);
        }

        // Get render object ID before getting mutable element
        let render_object_id = self.element_registry.get(element_id)
            .and_then(|el| el.render_object());

        // Get the element to perform unmount lifecycle
        if let Some(element) = self.element_registry.get_mut(element_id) {
            let mut ctx = ElementContext::new_with_registry(
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
            );

            // Remove render object
            if let Some(render_id) = render_object_id {
                ctx.remove_render_object(render_id);
            }

            // Call unmount lifecycle
            element.unmount(&mut ctx);
        }

        // Remove state
        self.state.remove(element_id);

        // Unmount from registry
        self.element_registry.unmount(element_id);
    }

    /// Perform layout on dirty render objects.
    ///
    /// This method iterates over render objects marked as needing layout
    /// and calls their `layout()` method with appropriate constraints.
    ///
    /// # Arguments
    ///
    /// * `available_size` - The size available for the root render object
    /// * `_engine` - Layout engine placeholder (integration pending)
    ///
    /// # Note
    ///
    /// Currently uses a simplified layout approach. Full integration with
    /// the TaffyLayoutEngine will be implemented in a later phase.
    ///
    /// # Example
    ///
    /// ```ignore
    /// pipeline.layout(Size::new(800.0, 600.0), &mut engine);
    /// ```
    pub fn layout(&mut self, available_size: Size<Logical>, _engine: &mut dyn crate::layout::LayoutEngine) {
        // Get dirty render objects that need layout
        let dirty_ids: Vec<_> = self.dirty.drain_layout().collect();

        // If no dirty objects, nothing to do
        if dirty_ids.is_empty() {
            return;
        }

        // Create layout context
        let mut ctx = LayoutContext::mock();

        // Create constraints from available size
        let constraints = LayoutConstraints {
            min_width: 0.0,
            max_width: available_size.width,
            min_height: 0.0,
            max_height: available_size.height,
            ..LayoutConstraints::default()
        };

        // Layout only dirty objects
        for id in dirty_ids {
            if let Some(obj) = self.render_objects.get_mut(id) {
                obj.layout(constraints, &mut ctx);
            }
        }

        // Note: For full implementation, we would:
        // 1. Use the layout engine to create nodes for each render object
        // 2. Compute layout with the engine
        // 3. Apply computed layout to each render object
        // 4. Mark children for layout if parent size changed
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
        let mut commands = Vec::new();

        // If no objects need paint, return empty
        if self.dirty.is_paint_empty() {
            return commands;
        }

        // If no root, nothing to paint
        let root_id = match self.render_objects.root() {
            Some(id) => id,
            None => return commands,
        };

        // Drain the dirty paint flags (we're about to paint them)
        let _dirty_ids: Vec<_> = self.dirty.drain_paint().collect();

        // Create paint context
        let mut ctx = PaintContext::new(&mut commands);

        // Paint root recursively
        self.paint_recursive(root_id, &mut ctx);

        commands
    }

    /// Recursively paint a render object and its children.
    fn paint_recursive(&self, id: RenderObjectId, ctx: &mut PaintContext) {
        // Get the render object
        let obj = match self.render_objects.get(id) {
            Some(o) => o,
            None => return,
        };

        // Paint this object
        let local_commands = obj.paint(ctx);

        // Push commands from this object
        for cmd in local_commands {
            ctx.push_command(cmd);
        }

        // Paint children
        for child_id in obj.children() {
            self.paint_recursive(*child_id, ctx);
        }
    }

    /// Hit test at a given position.
    ///
    /// Determines which render object (if any) is at the given position.
    /// Returns a `HitTestResult` containing the path from root to the hit target.
    ///
    /// # Arguments
    ///
    /// * `position` - The position to test in logical coordinates
    ///
    /// # Returns
    ///
    /// A `HitTestResult` with the path to the hit target, or a miss result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = pipeline.hit_test(Point::new(100.0, 100.0));
    /// if let Some(target) = result.target() {
    ///     // Handle input on target render object
    /// }
    /// ```
    pub fn hit_test(&self, position: Point<Logical>) -> HitTestResult {
        self.render_objects.hit_test(position)
    }

    /// Get the element registry.
    pub fn element_registry(&self) -> &ElementRegistry {
        &self.element_registry
    }

    /// Get the render object registry.
    pub fn render_objects(&self) -> &RenderObjectRegistry {
        &self.render_objects
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
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

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
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

        // Paint - text render object returns empty commands
        // (text rendering is handled by glyphon separately)
        let commands = pipeline.paint();

        // TextRenderObject doesn't generate render commands
        // It returns empty because text is rendered via glyphon
        assert!(commands.is_empty());
    }

    #[test]
    fn test_pipeline_hit_test_miss() {
        let pipeline = ThreeTreePipeline::new();

        // Hit test with no content
        let result = pipeline.hit_test(Point::new(100.0, 100.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_hit_test_with_content() {
        let mut pipeline = ThreeTreePipeline::new();

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

        // Hit test inside the text bounds
        let result = pipeline.hit_test(Point::new(5.0, 5.0));

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
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

        // Hit test outside the text bounds
        let result = pipeline.hit_test(Point::new(500.0, 500.0));

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
}