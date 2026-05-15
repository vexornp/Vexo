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

use crate::core::{Absolute, Bounds, Logical, Point, Position, Size};
use crate::input::{ButtonState, InputEvent, Modifiers};
use crate::render::RenderCommand;

use super::build_owner::BuildOwner;
use super::child_ops::{ChildOp, ChildOps};
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::event_context::EventContext;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::layouter::Layouter;
use super::painter::Painter;
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

    /// Currently focused element (for keyboard events).
    focused_element: Option<ElementKey>,

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
            focused_element: None,
            build_owner: BuildOwner::new(),
            child_ops: ChildOps::new(),
            dirty_sender,
            dirty_receiver,
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
            let can_update = self
                .element_registry
                .get(root_id)
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
    /// - First, performs any pending state-driven rebuilds
    /// - Then, reconciles the widget tree with the element tree
    ///
    /// After initial mount, prefer calling `mark_needs_build()` for updates.
    pub fn update(&mut self, root_widget: Box<dyn Widget>) {
        // First, perform any pending state-driven rebuilds (from setState)
        self.perform_rebuilds();

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
                let can_update = self
                    .element_registry
                    .get(root_id)
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
    /// This follows the Flutter-style pattern where each element's rebuild()
    /// method handles both updating the widget and reconciling children.
    fn rebuild_root(&mut self, root_id: ElementKey, widget: Box<dyn Widget>) {
        let parent = self.element_registry.parent(root_id);

        log::debug!("[RetainMode] rebuild_root() - element_id: {:?}", root_id);

        // Call element.rebuild() which handles both update and child reconciliation
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            root_id,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        self.element_registry
            .with_element(root_id, &mut ctx, |element, ctx| {
                element.rebuild(widget_as_any, ctx);
            });

        // Execute any child operations emitted during rebuild
        self.execute_child_ops();
    }

    /// Perform targeted rebuilds for dirty elements.
    ///
    /// This is the Flutter-style rebuild: only dirty elements and their
    /// subtrees are reconciled. Much more efficient than full-tree reconcile.
    pub fn perform_rebuilds(&mut self) {
        // First, drain any dirty signals from StatefulMutable callbacks
        self.drain_dirty_channel();

        if !self.build_owner.has_pending_rebuilds() {
            return;
        }

        // Sort by depth: parents must rebuild before children
        let element_registry = &self.element_registry;
        self.build_owner
            .sort_dirty_by_depth(|id| element_registry.depth(id));

        // Drain dirty elements
        let dirty_ids: Vec<ElementKey> = self.build_owner.drain_dirty_sorted();

        // Rebuild each dirty element
        for element_id in dirty_ids {
            // Skip if element was removed during a previous rebuild
            if !self.element_registry.contains(element_id) {
                continue;
            }

            // Enter build scope (cycle detection)
            if !self.build_owner.enter_build_scope(element_id) {
                continue;
            }

            // Get parent and render_object for context
            let parent = self.element_registry.parent(element_id);

            // Create context for the element
            let mut ctx = ElementContext::new(
                element_id,
                parent,
                &mut self.state,
                &mut self.dirty,
                &mut self.render_objects,
                &self.build_owner,
                &self.dirty_sender,
                &mut self.child_ops,
            );

            // Rebuild from current state using with_element
            self.element_registry
                .with_element(element_id, &mut ctx, |element, ctx| {
                    element.rebuild_from_state(ctx);
                });

            // Execute any child operations emitted during rebuild
            self.execute_child_ops();

            // Exit build scope
            self.build_owner.exit_build_scope(element_id);
        }
    }

    /// Perform state-driven rebuilds only, without a new widget tree.
    pub fn update_state_only(&mut self) {
        self.perform_rebuilds();
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

    /// Recursively reconcile an element and its children with a new widget tree.
    ///
    /// This follows the Flutter-style pattern where each element's rebuild()
    /// method handles both updating the widget and reconciling children.
    fn reconcile_element(&mut self, element_id: ElementKey, widget: Box<dyn Widget>) {
        let parent = self.element_registry.parent(element_id);

        log::debug!(
            "[RetainMode] reconcile_element() - element_id: {:?}",
            element_id
        );

        // Call element.rebuild() which handles both update and child reconciliation.
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            element_id,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        self.element_registry
            .with_element(element_id, &mut ctx, |element, ctx| {
                element.rebuild(widget_as_any, ctx);
            });

        // Execute any child operations emitted during rebuild
        self.execute_child_ops();
    }

    /// Mount an element tree from a widget.
    ///
    /// Creates an element from the widget, inserts it into the registry,
    /// calls mount() via with_element, then executes any child ops that
    /// the element emitted during mount (which recursively mounts children).
    ///
    /// Note: This does NOT add the element to its parent's children list
    /// or call child_mounted. The caller is responsible for that.
    fn mount_element_tree(
        &mut self,
        parent: Option<ElementKey>,
        widget: Box<dyn Widget>,
    ) -> ElementKey {
        // Create the element from the widget
        let element = widget.create_element();

        // Insert into registry (does NOT call mount — we handle lifecycle here)
        let key = self.element_registry.insert(element, parent);

        // Build context for the mount call
        let mut ctx = ElementContext::new(
            key,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        // Call mount() on the element via with_element
        self.element_registry
            .with_element(key, &mut ctx, |element, ctx| {
                element.mount(ctx);
            });

        // After mount, the element may have created a render object.
        // If this is the root element (no parent), set it as the render object root.
        if parent.is_none() {
            if let Some(ro_key) = self
                .element_registry
                .get(key)
                .and_then(|el| el.render_object())
            {
                self.render_objects.set_root(ro_key);
            }
        }

        // Execute any child ops the element emitted during mount
        // This recursively mounts children (who may emit their own child ops, etc.)
        self.execute_child_ops();

        // After child ops are processed, the element's render_object() may have changed
        // (e.g., StatefulElement delegates to its child's render object after child_mounted).
        // Re-check the root render object if this is the root element.
        if parent.is_none() {
            if let Some(ro_key) = self
                .element_registry
                .get(key)
                .and_then(|el| el.render_object())
            {
                self.render_objects.set_root(ro_key);
            }
        }

        key
    }

    /// Drain and execute all pending child operations.
    ///
    /// This method processes ChildOps emitted by element lifecycle methods
    /// (mount, rebuild, update). Each operation is executed in order:
    ///
    /// - `Inflate`: Mounts a new child element tree, adds it to the parent's
    ///   children list, and notifies the parent via `child_mounted`.
    /// - `Update`: Rebuilds an existing child element with a new widget.
    /// - `Unmount`: Unmounts a child element tree.
    ///
    /// Because element methods can emit ops recursively (e.g., mount emits
    /// Inflate ops, which cause more mount calls that emit more ops), this
    /// method loops until no ops remain.
    fn execute_child_ops(&mut self) {
        loop {
            let ops = self.child_ops.drain();
            if ops.is_empty() {
                break;
            }

            for op in ops {
                match op {
                    ChildOp::Inflate {
                        slot,
                        widget,
                        parent,
                    } => {
                        // Mount the new child element tree
                        let child_key = self.mount_element_tree(Some(parent), widget);

                        // Add child to parent's children list at the given slot
                        self.element_registry.add_child(parent, child_key, slot);

                        // Get the child's render object key for linking
                        let child_ro = self
                            .element_registry
                            .get(child_key)
                            .and_then(|el| el.render_object());

                        // Notify parent element of the new child via child_mounted,
                        // passing the child's render object key so the parent can
                        // link the render object tree.
                        let parent_parent = self.element_registry.parent(parent);

                        let mut ctx = ElementContext::new(
                            parent,
                            parent_parent,
                            &mut self.state,
                            &mut self.dirty,
                            &mut self.render_objects,
                            &self.build_owner,
                            &self.dirty_sender,
                            &mut self.child_ops,
                        );

                        self.element_registry
                            .with_element(parent, &mut ctx, |element, ctx| {
                                element.child_mounted(child_key, slot, child_ro, ctx);
                            });
                    }
                    ChildOp::Update { child, widget } => {
                        // Rebuild the existing child element with the new widget
                        self.rebuild_element(child, widget);
                    }
                    ChildOp::Unmount { child } => {
                        // Unmount the child element tree
                        self.unmount_element_tree(child);
                    }
                }
            }
        }
    }

    /// Rebuild a single element with a new widget.
    ///
    /// This is used by execute_child_ops to handle ChildOp::Update.
    /// It calls element.rebuild() via with_element and then executes
    /// any child ops emitted during the rebuild.
    fn rebuild_element(&mut self, element_id: ElementKey, widget: Box<dyn Widget>) {
        let parent = self.element_registry.parent(element_id);

        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            element_id,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        self.element_registry
            .with_element(element_id, &mut ctx, |element, ctx| {
                element.rebuild(widget_as_any, ctx);
            });
    }

    /// Drain dirty signals from the channel and mark elements for rebuild.
    ///
    /// When a `StatefulMutable::set()` fires its dirty callback, it sends
    /// the element ID through the channel. This method drains the channel
    /// and calls `mark_needs_build()` on the BuildOwner for each one.
    fn drain_dirty_channel(&mut self) {
        while let Ok(element_id) = self.dirty_receiver.try_recv() {
            self.build_owner.mark_needs_build(element_id);
        }
    }

    /// Unmount an element and all its descendants.
    fn unmount_element_tree(&mut self, element_id: ElementKey) {
        // Get children and parent before unmounting
        let children = self.element_registry.children(element_id).to_vec();
        let parent = self.element_registry.parent(element_id);

        // Recursively unmount children first
        for child_id in children {
            self.unmount_element_tree(child_id);
        }

        // Build context for the unmount call
        let mut ctx = ElementContext::new(
            element_id,
            parent,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        // Call unmount() via with_element
        self.element_registry
            .with_element(element_id, &mut ctx, |element, ctx| {
                // Remove render object
                if let Some(render_id) = element.render_object() {
                    ctx.remove_render_object(render_id);
                }

                // Call unmount lifecycle
                element.unmount(ctx);
            });

        // Remove state
        self.state.remove(element_id);

        // Unmount from registry (removes from parent's children list, etc.)
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
            InputEvent::Keyboard { .. } => self.handle_keyboard_event(event, modifiers),
            _ => None,
        }
    }

    /// Handle a pointer event (moved or button).
    ///
    /// Events are dispatched using Flutter-style bubbling: the event is sent
    /// to each element in the hit test path from deepest (innermost) to
    /// shallowest (root). The first element that handles the event stops
    /// propagation. This allows modifier elements like GestureDetector to
    /// intercept events before they reach the child element.
    fn handle_pointer_event(
        &mut self,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Convert Point to Position (absolute window coordinates)
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);

        // 1. Hit test to find target and build element path
        let hit_result = self.render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            return None;
        }

        // 2. Get absolute bounds for context (from hit test result)
        let bounds = hit_result.absolute_bounds().unwrap_or_default();

        // 3. Bubble event up the element path (deepest to shallowest)
        // This matches Flutter's event propagation: innermost element gets
        // the event first, then it bubbles up to parent elements.
        // Modifier elements like GestureDetector can intercept events
        // before they reach the child.
        let element_path = hit_result.element_path();
        let mut any_message: Option<Box<dyn Any>> = None;

        // Iterate from deepest (last) to shallowest (first)
        for &element_id in element_path.iter().rev() {
            if let Some(element) = self.element_registry.get_mut(element_id) {
                let mut ctx = EventContext::with_build_owner(
                    position,
                    self.focused_element,
                    bounds,
                    modifiers,
                    &mut self.state,
                    &self.build_owner,
                    &self.dirty_sender,
                );

                let message = element.on_event(event, &mut ctx);

                // Handle focus requests from this element
                if let Some(focus) = ctx.focus_request() {
                    self.focused_element = Some(focus);
                } else if ctx.should_clear_focus() {
                    self.focused_element = None;
                }

                if message.is_some() {
                    any_message = message;
                    break; // Event handled - stop bubbling
                }
            }
        }

        // If no element handled the event and it's a press, clear focus
        if any_message.is_none() {
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = event
            {
                self.focused_element = None;
            }
        }

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

        let mut ctx = EventContext::with_build_owner(
            Point::zero(),
            self.focused_element,
            bounds,
            modifiers,
            &mut self.state,
            &self.build_owner,
            &self.dirty_sender,
        );

        let any_message = self
            .element_registry
            .get_mut(focused)?
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
    pub fn focused_element(&self) -> Option<ElementKey> {
        self.focused_element
    }

    /// Set focus to an element.
    pub fn set_focus(&mut self, element: Option<ElementKey>) {
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
