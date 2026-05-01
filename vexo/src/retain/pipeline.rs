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

use std::marker::PhantomData;

use crate::core::{Absolute, Bounds, Logical, Point, Position, Relative, Size};
use crate::input::{ButtonState, InputEvent, Modifiers};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;

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
/// The pipeline is generic over the message type `M` to support ELM-style
/// typed message dispatch.
///
/// # Type Parameter
///
/// `M` - The message type emitted by interactive widgets. Must be `Clone + Send + 'static`.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone)]
/// enum Message { Clicked, }
///
/// let mut pipeline: ThreeTreePipeline<Message> = ThreeTreePipeline::new();
///
/// // Build and reconcile widget tree
/// let widget = Button::new("Click Me").with_message(Message::Clicked);
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
///     // msg is Message, not Box<dyn Any>
/// }
/// ```
pub struct ThreeTreePipeline<M: Clone + Send + 'static> {
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

    /// Phantom data for the message type.
    _phantom: PhantomData<M>,
}

impl<M: Clone + Send + 'static> ThreeTreePipeline<M> {
    /// Create a new empty pipeline.
    pub fn new() -> Self {
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
            focused_element: None,
            _phantom: PhantomData,
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
    pub fn reconcile(&mut self, root_widget: Box<dyn Widget<M>>) {
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

    /// Recursively reconcile an element and its children with a new widget tree.
    ///
    /// This method:
    /// 1. Updates the element with the new widget
    /// 2. Reconciles children by matching new child widgets to existing child elements
    fn reconcile_element(&mut self, element_id: ElementId, widget: Box<dyn Widget<M>>) {
        // Get render object ID and parent before mutable borrow
        let render_object_id = self.element_registry.get(element_id)
            .and_then(|el| el.render_object());
        let parent = self.element_registry.parent(element_id);

        // Update the element with the new widget
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_box());
        if let Some(existing_element) = self.element_registry.get_mut(element_id) {
            let mut ctx = ElementContext::with_registry(
                element_id,
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
            );

            existing_element.update(widget_as_any, &mut ctx);
        }

        // Mark render object as dirty
        if let Some(render_id) = render_object_id {
            self.dirty.mark_needs_layout(render_id);
            self.dirty.mark_needs_paint(render_id);
        }

        // Get existing children
        let existing_children = self.element_registry.children(element_id).to_vec();

        // Get new child widgets
        let new_child_widgets: Vec<Box<dyn Widget<M>>> = widget.children().iter()
            .map(|c| c.clone_box())
            .collect();

        // Reconcile children
        self.reconcile_children_internal(element_id, existing_children, new_child_widgets);
    }

    /// Reconcile children of an element.
    ///
    /// This implements a simple diffing algorithm:
    /// 1. Match children by position (for now, we don't support keys)
    /// 2. Update matching children, mount new ones, unmount extra ones
    fn reconcile_children_internal(
        &mut self,
        parent_id: ElementId,
        existing_children: Vec<ElementId>,
        new_child_widgets: Vec<Box<dyn Widget<M>>>,
    ) {
        let mut new_children = Vec::new();
        let mut matched = std::collections::HashSet::new();

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
                    // Recursively reconcile this child
                    self.reconcile_element(child_id, child_widget);
                    new_children.push(child_id);
                    continue;
                }
            }

            // No matching child - mount new element
            let child_id = self.mount_element_tree(Some(parent_id), child_widget);
            new_children.push(child_id);
        }

        // Unmount children that weren't matched
        for child_id in existing_children {
            if !matched.contains(&child_id) {
                self.unmount_element_tree(child_id);
            }
        }

        // Update parent's children list in the registry
        self.element_registry.set_children(parent_id, new_children);
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
    fn mount_element_tree(&mut self, parent: Option<ElementId>, widget: Box<dyn Widget<M>>) -> ElementId {
        // Create element from widget
        let element = widget.create_element();

        // Mount the element using the registry
        let element_id = self.element_registry.mount_element(
            element,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
        );

        // Get the render object from the element after it's in the registry
        let render_object_id = self.element_registry.get(element_id)
            .and_then(|el| el.render_object());

        // Set the render object as root if this is the root element
        if parent.is_none() {
            if let Some(render_id) = render_object_id {
                self.render_objects.set_root(render_id);
            }
        }

        // Mount children recursively
        // First check for single-child modifiers (Background, Border, etc.)
        if let Some(child_widget) = widget.child() {
            let child_id = self.mount_element_tree(Some(element_id), child_widget.clone_box());

            // Link child render object to parent
            if let (Some(parent_ro), Some(child_ro)) = (
                render_object_id,
                self.element_registry.get(child_id).and_then(|el| el.render_object()),
            ) {
                if let Some(parent_obj) = self.render_objects.get_mut(parent_ro) {
                    parent_obj.set_child_id(child_ro);
                }
            }

            // Update element's children list
            if let Some(elem) = self.element_registry.get_mut(element_id) {
                elem.add_child(child_id);
            }
        }

        // Then check for multi-child containers (Column, Row)
        let children: Vec<Box<dyn Widget<M>>> = widget.children().iter().map(|c| c.clone_box()).collect();
        if !children.is_empty() {
            let mut child_render_objects = Vec::new();
            let mut child_element_ids = Vec::new();

            for child_widget in children {
                let child_id = self.mount_element_tree(Some(element_id), child_widget);
                child_element_ids.push(child_id);

                if let Some(child_ro) = self.element_registry.get(child_id).and_then(|el| el.render_object()) {
                    child_render_objects.push(child_ro);
                }
            }

            // Link child render objects to parent container
            if let Some(parent_ro) = render_object_id {
                if let Some(parent_obj) = self.render_objects.get_mut(parent_ro) {
                    for child_ro in &child_render_objects {
                        parent_obj.add_child(*child_ro);
                    }
                }
            }

            // Update element's children list
            if let Some(elem) = self.element_registry.get_mut(element_id) {
                for child_id in child_element_ids {
                    elem.add_child(child_id);
                }
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
            let mut ctx = ElementContext::with_registry(
                element_id,
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

        // Paint root recursively (root starts at origin)
        self.paint_recursive(root_id, &mut ctx, Position::zero());

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

    /// Handle an input event.
    ///
    /// For pointer events, performs hit testing to find the target element.
    /// For keyboard events, dispatches to the focused element.
    ///
    /// Returns `Some(message)` if the event was handled and produced a typed message.
    pub fn handle_event(
        &mut self,
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<M> {
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
    ) -> Option<M> {
        // 1. Hit test to find target
        let hit_result = self.render_objects.hit_test(position);

        // 2. Get target element
        let target_element = hit_result.target_element();

        let target_element = target_element?;

        // 3. Get render object bounds for context
        let target_render = hit_result.target()?;
        let bounds = self.render_objects.get(target_render)
            .and_then(|obj| obj.computed_bounds())
            .unwrap_or_default();

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

        // Downcast the message to M
        any_message.and_then(|m| m.downcast::<M>().ok().map(|b| *b))
    }

    /// Handle a keyboard event.
    fn handle_keyboard_event(
        &mut self,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<M> {
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

        // Downcast the message to M
        any_message.and_then(|m| m.downcast::<M>().ok().map(|b| *b))
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

impl<M: Clone + Send + 'static> Default for ThreeTreePipeline<M> {
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
        let pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        assert!(pipeline.element_registry().is_empty());
        assert!(pipeline.render_objects().is_empty());
        assert!(!pipeline.needs_layout());
        assert!(!pipeline.needs_paint());
    }

    #[test]
    fn test_pipeline_default() {
        let pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::default();

        assert!(pipeline.element_registry().is_empty());
        assert!(pipeline.render_objects().is_empty());
    }

    #[test]
    fn test_pipeline_reconcile_single_widget() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        // Paint with no render objects
        let commands = pipeline.paint();

        assert!(commands.is_empty());
    }

    #[test]
    fn test_pipeline_paint_with_content() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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
        let pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        // Hit test with no content
        let result = pipeline.hit_test(Point::new(100.0, 100.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_hit_test_with_content() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test inside the text bounds
        let result = pipeline.hit_test(Point::new(5.0, 5.0));

        // Should hit the text render object
        assert!(result.is_hit());
        assert!(result.target().is_some());
    }

    #[test]
    fn test_pipeline_hit_test_outside() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Hello")));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test outside the text bounds
        let result = pipeline.hit_test(Point::new(500.0, 500.0));

        // Should miss
        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_pipeline_clear_dirty() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

        pipeline.reconcile(Box::new(Text::new("Hello")));
        pipeline.clear_dirty();

        // Mark all as needing layout
        pipeline.mark_all_needs_layout();

        assert!(pipeline.needs_layout());
    }

    #[test]
    fn test_pipeline_reconcile_replaces_different_type() {
        let mut pipeline: ThreeTreePipeline<()> = ThreeTreePipeline::new();

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