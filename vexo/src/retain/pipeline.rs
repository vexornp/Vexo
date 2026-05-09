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

use crate::core::{Absolute, Bounds, Logical, Point, Position, Relative, Size};
use crate::input::{ButtonState, InputEvent, Modifiers};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;

use super::build_owner::BuildOwner;
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::event_context::EventContext;
use super::hit_test::HitTestResult;
use super::id::{ElementId, RenderObjectId};
use super::render_object::{LayoutContext, LayoutResult, PaintContext, RenderObjectRegistry};
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

    /// Currently focused element (for keyboard events).
    focused_element: Option<ElementId>,

    /// Build owner for targeted rebuilds.
    build_owner: BuildOwner,

    /// Flag indicating if full reconcile is needed.
    ///
    /// True after initial mount or when root element type changes.
    needs_full_reconcile: bool,
}

impl ThreeTreePipeline {
    /// Create a new empty pipeline.
    pub fn new() -> Self {
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
            focused_element: None,
            build_owner: BuildOwner::new(),
            needs_full_reconcile: true,
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
                // Recursively reconcile the element tree
                self.reconcile_element(root_id, root_widget);
                return;
            }

            // Can't update existing root - unmount it
            self.unmount_element_tree(root_id);
        }

        // Mount new root element
        self.mount_element_tree(None, root_widget);
    }

    /// Reconcile or rebuild based on current state.
    ///
    /// This is the main entry point for frame updates.
    /// - If `needs_full_reconcile` is true, performs full reconcile
    /// - Otherwise, performs targeted rebuilds only
    ///
    /// After initial mount, prefer calling `mark_needs_build()` for updates.
    pub fn update(&mut self, root_widget: Box<dyn Widget>) {
        log::debug!(
            "[RetainMode] update() - elements: {}, render_objects: {}, needs_full_reconcile: {}",
            self.element_registry.len(),
            self.render_objects.len(),
            self.needs_full_reconcile
        );

        if self.needs_full_reconcile || self.element_registry.root().is_none() {
            // Full reconcile needed (initial mount or root type changed)
            log::debug!("[RetainMode] Performing FULL reconcile");
            self.reconcile(root_widget);
            self.needs_full_reconcile = false;
        } else {
            // Check if root can be updated
            if let Some(root_id) = self.element_registry.root() {
                let can_update = self.element_registry.get(root_id)
                    .map(|el| el.can_update(root_widget.as_any()))
                    .unwrap_or(false);

                if can_update {
                    // Targeted rebuild of root
                    log::debug!("[RetainMode] Performing TARGETED rebuild (root can update)");
                    self.rebuild_root(root_id, root_widget);
                } else {
                    // Root type changed, full reconcile
                    log::debug!("[RetainMode] Performing FULL reconcile (root type changed)");
                    self.reconcile(root_widget);
                }
            } else {
                self.reconcile(root_widget);
            }
        }

        log::debug!(
            "[RetainMode] After update - dirty layout: {}, dirty paint: {}",
            self.dirty.layout_count(),
            self.dirty.paint_count()
        );
    }

    /// Rebuild the root element with a new widget.
    ///
    /// This follows the same pattern as reconcile_element():
    /// 1. Update the element with the new widget
    /// 2. Reconcile children separately (to avoid borrow issues)
    fn rebuild_root(&mut self, root_id: ElementId, widget: Box<dyn Widget>) {
        let render_object_id = self.element_registry.get(root_id)
            .and_then(|el| el.render_object());
        let parent = self.element_registry.parent(root_id);

        log::debug!(
            "[RetainMode] rebuild_root() - element_id: {:?}, has_render_object: {}",
            root_id,
            render_object_id.is_some()
        );

        // Step 1: Update the element with the new widget
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());
        if let Some(element) = self.element_registry.get_mut(root_id) {
            let mut ctx = ElementContext::with_registry(
                root_id,
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
            );
            ctx.global_key_registry = Some(self.build_owner.global_keys_mut());

            element.update(widget_as_any, &mut ctx);
        }

        // Step 2: Reconcile children (same pattern as reconcile_element)
        let existing_children = self.element_registry.children(root_id).to_vec();

        // First check for single-child widgets (DecoratedContainer, modifiers, etc.)
        let new_child_widgets: Vec<Box<dyn Widget>> = if let Some(child) = widget.child() {
            vec![child.clone_boxed()]
        } else {
            widget.children().iter()
                .map(|c| c.clone_boxed())
                .collect()
        };

        // Rebuild children recursively
        self.rebuild_children_internal(root_id, existing_children, new_child_widgets);

        // Note: We only mark dirty if the element's render object actually changed
        // The element's update() method should handle this via ElementContext
        // We don't automatically mark dirty here to allow for smarter updates
    }

    /// Rebuild children of an element.
    ///
    /// Similar to reconcile_children_internal but uses rebuild semantics.
    /// Skips elements that manage their own children (like StatefulElement).
    fn rebuild_children_internal(
        &mut self,
        parent_id: ElementId,
        existing_children: Vec<ElementId>,
        new_child_widgets: Vec<Box<dyn Widget>>,
    ) {
        // Check if the parent manages its own children
        // If so, skip reconciliation - the element handles it internally
        let manages_own = self.element_registry.get(parent_id)
            .map(|el| el.manages_own_children())
            .unwrap_or(false);

        if manages_own {
            log::debug!(
                "[RetainMode] rebuild_children_internal - skipping, element {:?} manages own children",
                parent_id
            );
            return;
        }

        let mut new_children = Vec::new();
        let mut matched = std::collections::HashSet::new();

        // Build key map for existing children (local keys only)
        let key_map: std::collections::HashMap<super::key::WidgetKey, ElementId> = existing_children
            .iter()
            .filter_map(|&id| {
                self.element_registry.get(id)
                    .and_then(|el| el.widget_key().map(|k| (k, id)))
            })
            .collect();

        // Match new widgets to existing elements
        for (index, child_widget) in new_child_widgets.into_iter().enumerate() {
            let element_id = match child_widget.key() {
                Some(super::key::WidgetKey::Local(key)) => {
                    // Local key: look up in map
                    if let Some(&id) = key_map.get(&super::key::WidgetKey::Local(key)) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Some(super::key::WidgetKey::Global(_)) => {
                    // Global keys are handled by the pipeline's global registry
                    // For now, fall back to position-based matching
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                None => {
                    // Non-keyed: match by position
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            if let Some(child_id) = element_id {
                // Check if can update
                let can_update = self.element_registry.get(child_id)
                    .map(|el| el.can_update(child_widget.as_any()))
                    .unwrap_or(false);

                if can_update {
                    matched.insert(child_id);
                    // Recursively rebuild this child
                    self.rebuild_root(child_id, child_widget);
                    new_children.push(child_id);
                } else {
                    // Can't update - mount new
                    let child_id = self.mount_element_tree(Some(parent_id), child_widget);
                    new_children.push(child_id);
                }
            } else {
                // No matching child - mount new element
                let child_id = self.mount_element_tree(Some(parent_id), child_widget);
                new_children.push(child_id);
            }
        }

        // Unmount children that weren't matched
        for child_id in existing_children {
            if !matched.contains(&child_id) {
                self.unmount_element_tree(child_id);
            }
        }

        // Update parent's children list
        self.element_registry.set_children(parent_id, new_children);
    }

    /// Perform targeted rebuilds for dirty elements.
    ///
    /// This is the Flutter-style rebuild: only dirty elements and their
    /// subtrees are reconciled. Much more efficient than full-tree reconcile.
    ///
    /// Returns the number of elements rebuilt.
    pub fn perform_rebuilds(&mut self) -> usize {
        if !self.build_owner.has_pending_rebuilds() {
            return 0;
        }

        let mut count = 0;

        // Drain dirty elements and rebuild each
        let dirty_ids: Vec<ElementId> = self.build_owner.drain_dirty().collect();

        for element_id in dirty_ids {
            // Skip if element no longer exists
            if !self.element_registry.contains(element_id) {
                continue;
            }

            // Enter build scope (detect cycles)
            if !self.build_owner.enter_build_scope(element_id) {
                // Cycle detected, skip
                continue;
            }

            // The actual rebuild would need the new widget
            // For now, this is a placeholder that will be filled in
            // when we integrate with the Application's view() method

            self.build_owner.exit_build_scope(element_id);
            count += 1;
        }

        count
    }

    /// Mark an element as needing rebuild.
    ///
    /// This is the entry point for Flutter-style targeted rebuilds.
    /// Elements call this when their state changes (e.g., setState equivalent).
    pub fn mark_needs_build(&mut self, element_id: ElementId) {
        self.build_owner.mark_needs_build(element_id);
    }

    /// Check if there are pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        self.build_owner.has_pending_rebuilds()
    }

    /// Recursively reconcile an element and its children with a new widget tree.
    ///
    /// This method:
    /// 1. Updates the element with the new widget
    /// 2. Reconciles children by matching new child widgets to existing child elements
    fn reconcile_element(&mut self, element_id: ElementId, widget: Box<dyn Widget>) {
        // Get parent before mutable borrow
        let parent = self.element_registry.parent(element_id);

        log::debug!(
            "[RetainMode] reconcile_element() - element_id: {:?}",
            element_id
        );

        // Update the element with the new widget
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());
        if let Some(existing_element) = self.element_registry.get_mut(element_id) {
            let mut ctx = ElementContext::with_registry(
                element_id,
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
            );
            ctx.global_key_registry = Some(self.build_owner.global_keys_mut());

            existing_element.update(widget_as_any, &mut ctx);
        }

        // Note: Dirty marking is handled by the element's update() method via ElementContext

        // Get existing children
        let existing_children = self.element_registry.children(element_id).to_vec();

        // Get new child widgets
        // First check for single-child widgets (DecoratedContainer, modifiers, etc.)
        let new_child_widgets: Vec<Box<dyn Widget>> = if let Some(child) = widget.child() {
            vec![child.clone_boxed()]
        } else {
            widget.children().iter()
                .map(|c| c.clone_boxed())
                .collect()
        };

        // Reconcile children
        self.reconcile_children_internal(element_id, existing_children, new_child_widgets);
    }

    /// Reconcile children of an element.
    ///
    /// This implements a simple diffing algorithm:
    /// 1. Match children by position (for now, we don't support keys)
    /// 2. Update matching children, mount new ones, unmount extra ones
    /// 3. Skip elements that manage their own children (like StatefulElement)
    fn reconcile_children_internal(
        &mut self,
        parent_id: ElementId,
        existing_children: Vec<ElementId>,
        new_child_widgets: Vec<Box<dyn Widget>>,
    ) {
        // Check if the parent manages its own children
        // If so, skip reconciliation - the element handles it internally
        let manages_own = self.element_registry.get(parent_id)
            .map(|el| el.manages_own_children())
            .unwrap_or(false);

        if manages_own {
            log::debug!(
                "[RetainMode] reconcile_children_internal - skipping, element {:?} manages own children",
                parent_id
            );
            return;
        }

        let mut new_children = Vec::new();
        let mut matched = std::collections::HashSet::new();
        let mut reused_count = 0;
        let mut mounted_count = 0;
        let mut unmounted_count = 0;

        // Match by position
        for (index, child_widget) in new_child_widgets.into_iter().enumerate() {
            let existing_child = existing_children.get(index).copied();

            if let Some(child_id) = existing_child {
                // Check if this element can be updated by the new widget
                let can_update = self.element_registry.get(child_id)
                    .map(|el| el.can_update(child_widget.as_any()))
                    .unwrap_or(false);

                if can_update && !matched.contains(&child_id) {
                    matched.insert(child_id);
                    // Recursively reconcile this child (REUSED)
                    reused_count += 1;
                    self.reconcile_element(child_id, child_widget);
                    new_children.push(child_id);
                    continue;
                }
            }

            // No matching child - mount new element (NEW)
            mounted_count += 1;
            let child_id = self.mount_element_tree(Some(parent_id), child_widget);
            new_children.push(child_id);
        }

        // Unmount children that weren't matched (REMOVED)
        for child_id in existing_children {
            if !matched.contains(&child_id) {
                unmounted_count += 1;
                self.unmount_element_tree(child_id);
            }
        }

        // Update parent's children list in the registry
        self.element_registry.set_children(parent_id, new_children);

        log::debug!(
            "[RetainMode] reconcile_children - reused: {}, mounted: {}, unmounted: {}",
            reused_count,
            mounted_count,
            unmounted_count
        );
    }

    /// Mount an element tree from a widget.
    ///
    /// This method delegates to ElementRegistry::inflate_widget() which
    /// creates an element, mounts it, and recursively mounts all children,
    /// linking render objects.
    fn mount_element_tree(&mut self, parent: Option<ElementId>, widget: Box<dyn Widget>) -> ElementId {
        let global_keys = self.build_owner.global_keys_mut();
        self.element_registry.inflate_widget(
            widget,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            Some(global_keys),
        )
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
            let mut ctx = ElementContext::with_registry(
                element_id,
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
            );
            ctx.global_key_registry = Some(self.build_owner.global_keys_mut());

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
        let dirty_layout_count = self.dirty.layout_count();
        let total_objects = self.render_objects.len();

        log::debug!(
            "[RetainMode] layout() - Processing {} dirty objects out of {} total",
            dirty_layout_count,
            total_objects
        );

        // Get the root render object
        let root_id = match self.render_objects.root() {
            Some(id) => id,
            None => return,
        };

        // Phase 1: Build Taffy tree (bottom-up: children first, then parent)
        // The pipeline traverses children first, collects their node IDs,
        // then passes them to the parent's layout method.
        {
            let mut ctx = LayoutContext::new(engine, font_system);
            self.layout_build_recursive(root_id, &mut ctx);
        }

        // Phase 2: Compute layout with Taffy
        if let Some(root_node) = self.get_layout_node(root_id) {
            engine.compute(root_node, available_size, font_system);
        }

        // Phase 3: Apply computed layouts back to render objects
        {
            let ctx = LayoutContext::new(engine, font_system);
            self.apply_layout_recursive(root_id, &ctx);
        }

        // Clear dirty flags
        self.dirty.drain_layout().for_each(drop);

        log::debug!("[RetainMode] layout() complete - dirty flags cleared");
    }

    /// Recursively build Taffy tree (bottom-up: children first).
    fn layout_build_recursive(
        &mut self,
        id: RenderObjectId,
        ctx: &mut LayoutContext,
    ) -> LayoutResult {
        // Get children
        let children: Vec<RenderObjectId> = self.render_objects.get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Layout children first (bottom-up)
        let child_nodes: Vec<LayoutNodeId> = children
            .iter()
            .map(|child_id| self.layout_build_recursive(*child_id, ctx).node)
            .collect();

        // Now layout this object with child nodes
        if let Some(obj) = self.render_objects.get_mut(id) {
            obj.layout(ctx, &child_nodes)
        } else {
            // Fallback: create empty node
            let node = ctx.engine().create_leaf(&Layout::default());
            LayoutResult { node, size: Size::new(0.0, 0.0) }
        }
    }

    /// Get the layout node ID from a render object.
    fn get_layout_node(&self, id: RenderObjectId) -> Option<LayoutNodeId> {
        self.render_objects.get(id).and_then(|obj| obj.layout_node())
    }

    /// Recursively apply computed layouts.
    fn apply_layout_recursive(&mut self, id: RenderObjectId, ctx: &LayoutContext) {
        // Get children first
        let children: Vec<RenderObjectId> = self.render_objects.get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Apply to this object
        if let Some(obj) = self.render_objects.get_mut(id) {
            obj.apply_layout(ctx);
        }

        // Recursively apply to children
        for child_id in children {
            self.apply_layout_recursive(child_id, ctx);
        }
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
        let dirty_paint_count = self.dirty.paint_count();
        let total_objects = self.render_objects.len();

        let mut commands = Vec::new();

        // If no root, nothing to paint
        let root_id = match self.render_objects.root() {
            Some(id) => id,
            None => return commands,
        };

        // Check if we need to paint
        // Note: We always need to generate render commands for the GPU,
        // but we log whether this is a full paint or just reusing cached state.
        if self.dirty.is_paint_empty() {
            log::debug!(
                "[RetainMode] paint() - No changes, regenerating commands for {} objects",
                total_objects
            );
        } else {
            log::debug!(
                "[RetainMode] paint() - Processing {} dirty objects out of {} total",
                dirty_paint_count,
                total_objects
            );
        }

        // Drain the dirty paint flags (we're about to paint them)
        let _dirty_ids: Vec<_> = self.dirty.drain_paint().collect();

        // Create paint context
        let mut ctx = PaintContext::new(&mut commands);

        // Paint root recursively (root starts at origin)
        self.paint_recursive(root_id, &mut ctx, Position::zero());

        log::debug!(
            "[RetainMode] paint() complete - generated {} render commands",
            commands.len()
        );

        commands
    }

    /// Recursively paint a render object and its children.
    fn paint_recursive(&self, id: RenderObjectId, ctx: &mut PaintContext, parent_absolute_position: Position<Logical, Absolute>) {
        // Get the render object
        let obj = match self.render_objects.get(id) {
            Some(o) => o,
            None => return,
        };

        // Get this object's position relative to its parent (from Taffy layout)
        // For containers, this is their position within the parent container
        // For leafs, this is their position relative to their parent container
        let position_in_parent: Position<Logical, Relative> = obj.computed_bounds()
            .map(|b| Position::new(b.left, b.top))
            .unwrap_or(Position::zero());

        // Calculate absolute position for this object:
        // parent's absolute position + this object's position within parent
        let absolute_position = position_in_parent.to_absolute(parent_absolute_position);

        // Tell the render object where to paint (in absolute coordinates)
        ctx.set_absolute_position(absolute_position);

        // Paint this object
        let local_commands = obj.paint(ctx);

        // Push commands from this object
        for cmd in local_commands {
            ctx.push_command(cmd);
        }

        // Paint children
        // For containers, children's positions are relative to the container's origin (0, 0),
        // not relative to the container's position in its parent.
        // So we pass the container's absolute position as the parent position for children.
        // This means: child_absolute = container_absolute + child_position_in_container
        for child_id in obj.children() {
            self.paint_recursive(*child_id, ctx, absolute_position);
        }
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
        self.render_objects.hit_test(position)
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
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerMoved { position } => {
                self.handle_pointer_event(*position, event, modifiers)
            }
            InputEvent::PointerButton { position, .. } => {
                self.handle_pointer_event(*position, event, modifiers)
            }
            InputEvent::Keyboard { .. } => {
                self.handle_keyboard_event(event, modifiers)
            }
            _ => None,
        }
    }

    /// Handle a pointer event (moved or button).
    fn handle_pointer_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Convert Point to Position (absolute window coordinates)
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);

        // 1. Hit test to find target
        let hit_result = self.render_objects.hit_test(absolute_position);

        // 2. Get target element
        let target_element = hit_result.target_element();

        let target_element = target_element?;

        // 3. Get absolute bounds for context (from hit test result)
        let bounds = hit_result.absolute_bounds().unwrap_or_default();

        // 4. Create event context
        let mut ctx = EventContext::new(
            position,
            self.focused_element,
            bounds,
            modifiers,
            &mut self.state,
        );

        // 5. Dispatch to element
        let any_message = self.element_registry.get_mut(target_element)?
            .on_event(event, &mut ctx);

        // 6. Handle focus requests
        if let Some(focus) = ctx.focus_request() {
            self.focused_element = Some(focus);
        } else if ctx.should_clear_focus() {
            self.focused_element = None;
        } else if any_message.is_none() {
            // If event not handled and it's a press, clear focus
            if let InputEvent::PointerButton { state: ButtonState::Pressed, .. } = event {
                self.focused_element = None;
            }
        }

        // Return the message directly (already Box<dyn Any>)
        any_message
    }

    /// Handle a keyboard event.
    fn handle_keyboard_event(
        &mut self,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Get focused element
        let focused = self.focused_element?;

        // Bounds not critical for keyboard events
        let bounds = Bounds::default();

        let mut ctx = EventContext::new(
            Point::zero(),
            self.focused_element,
            bounds,
            modifiers,
            &mut self.state,
        );

        let any_message = self.element_registry.get_mut(focused)?
            .on_event(event, &mut ctx);

        // Handle focus requests
        if let Some(focus) = ctx.focus_request() {
            self.focused_element = Some(focus);
        } else if ctx.should_clear_focus() {
            self.focused_element = None;
        }

        // Return the message directly (already Box<dyn Any>)
        any_message
    }

    /// Get the currently focused element.
    pub fn focused_element(&self) -> Option<ElementId> {
        self.focused_element
    }

    /// Set focus to an element.
    pub fn set_focus(&mut self, element: Option<ElementId>) {
        self.focused_element = element;
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
}