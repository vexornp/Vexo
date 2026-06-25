//! Reconciler: reconciliation, lifecycle, and rebuild operations.
//!
//! This module extracts the reconciliation-related logic from `ThreeTreePipeline`
//! into a standalone `Reconciler` struct (zero-sized). All methods are
//! associated functions that take explicit parameters instead of accessing
//! `self` fields on a pipeline.

use std::sync::{Arc, mpsc};

use crate::animation::AnimationTicker;
use super::build_owner::BuildOwner;
use super::child_ops::{ChildOp, ChildOps};
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::focus::{FocusManager, FocusNodeId};
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;
use super::element_state::StateStorage;
use super::widgets::Widget;

/// Zero-sized struct that serves as a namespace for reconciliation logic.
///
/// All methods are associated functions that take explicit parameters instead
/// of accessing `ThreeTreePipeline` fields. This keeps reconciliation
/// independent of the pipeline struct.
pub struct Reconciler;

impl Reconciler {
    // -----------------------------------------------------------------------
    // Helper: resolve parent focus node ID
    // -----------------------------------------------------------------------

    /// Walk up the element tree from `element_key` to find the nearest
    /// ancestor that has a focus attachment, and return its focus node ID.
    ///
    /// Returns `None` if no ancestor has a focus attachment (the element
    /// will attach to the root of the focus tree).
    fn resolve_parent_focus_node_id(
        element_registry: &ElementRegistry,
        element_key: ElementKey,
    ) -> Option<FocusNodeId> {
        let mut current = element_registry.parent(element_key);
        while let Some(key) = current {
            if let Some(node_id) = element_registry
                .get(key)
                .and_then(|el| el.focus_attachment().as_ref())
                .map(|att| att.node_id())
            {
                return Some(node_id);
            }
            current = element_registry.parent(key);
        }
        None
    }

    /// Reconcile a new widget tree with the existing element tree.
    ///
    /// This method:
    /// 1. Diffs the new widget tree against existing elements
    /// 2. Mounts new elements for new widgets
    /// 3. Updates existing elements where widgets match
    /// 4. Unmounts elements for removed widgets
    /// 5. Creates/destroys render objects accordingly
    /// 6. Marks affected render objects as dirty
    pub(crate) fn reconcile(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        root_widget: Box<dyn Widget>,
    ) {
        // Check if we have an existing root element
        if let Some(root_id) = element_registry.root() {
            // Check if the widget can update the existing element
            let can_update = element_registry
                .get(root_id)
                .map(|el| el.can_update(root_widget.as_any()))
                .unwrap_or(false);

            if can_update {
                // Recursively reconcile the element tree
                Self::reconcile_element(
                    element_registry,
                    render_objects,
                    state,
                    dirty,
                    build_owner,
                    child_ops,
                    dirty_sender,
                    focus_manager,
                    animation_ticker,
                    root_id,
                    root_widget,
                );
                return;
            }

            // Can't update existing root - unmount it
            Self::unmount_element_tree(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                focus_manager,
                animation_ticker,
                root_id,
            );
        }

        // Mount new root element
        Self::mount_element_tree(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
            None,
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
    pub fn update(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        needs_full_reconcile: &mut bool,
        root_widget: Box<dyn Widget>,
    ) {
        // First, perform any pending state-driven rebuilds (from setState)
        Self::perform_rebuilds(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
            dirty_receiver,
        );

        log::debug!(
            "[RetainMode] update() - elements: {}, render_objects: {}, needs_full_reconcile: {}",
            element_registry.len(),
            render_objects.len(),
            *needs_full_reconcile
        );

        if *needs_full_reconcile || element_registry.root().is_none() {
            // Full reconcile needed (initial mount or root type changed)
            log::debug!("[RetainMode] Performing FULL reconcile");
            Self::reconcile(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                focus_manager,
                animation_ticker,
                root_widget,
            );
            *needs_full_reconcile = false;
        } else {
            // Check if root can be updated
            if let Some(root_id) = element_registry.root() {
                let can_update = element_registry
                    .get(root_id)
                    .map(|el| el.can_update(root_widget.as_any()))
                    .unwrap_or(false);

                if can_update {
                    // Targeted rebuild of root
                    log::debug!("[RetainMode] Performing TARGETED rebuild (root can update)");
                    Self::rebuild_root(
                        element_registry,
                        render_objects,
                        state,
                        dirty,
                        build_owner,
                        child_ops,
                        dirty_sender,
                        focus_manager,
                        animation_ticker,
                        root_id,
                        root_widget,
                    );
                } else {
                    // Root type changed, full reconcile
                    log::debug!("[RetainMode] Performing FULL reconcile (root type changed)");
                    Self::reconcile(
                        element_registry,
                        render_objects,
                        state,
                        dirty,
                        build_owner,
                        child_ops,
                        dirty_sender,
                        focus_manager,
                        animation_ticker,
                        root_widget,
                    );
                }
            } else {
                Self::reconcile(
                    element_registry,
                    render_objects,
                    state,
                    dirty,
                    build_owner,
                    child_ops,
                    dirty_sender,
                    focus_manager,
                    animation_ticker,
                    root_widget,
                );
            }
        }

        log::debug!(
            "[RetainMode] After update - dirty layout: {}, dirty paint: {}",
            dirty.layout_count(),
            dirty.paint_count()
        );
    }

    /// Rebuild the root element with a new widget.
    ///
    /// This follows the Flutter-style pattern where each element's rebuild()
    /// method handles both updating the widget and reconciling children.
    pub(crate) fn rebuild_root(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        root_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(root_id);
        let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, root_id);

        log::debug!("[RetainMode] rebuild_root() - element_id: {:?}", root_id);

        // Call element.rebuild() which handles both update and child reconciliation
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            root_id,
            parent,
            element_registry.children(root_id).to_vec(),
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            parent_focus_node_id,
            animation_ticker.clone(),
        );

        element_registry.with_element(root_id, &mut ctx, |element, ctx| {
            element.rebuild(widget_as_any, ctx);
        });

        // Execute any child operations emitted during rebuild
        Self::execute_child_ops(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
        );
    }

    /// Perform targeted rebuilds for dirty elements.
    ///
    /// This is the Flutter-style rebuild: only dirty elements and their
    /// subtrees are reconciled. Much more efficient than full-tree reconcile.
    pub fn perform_rebuilds(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
    ) {
        // First, drain any dirty signals from Signal callbacks
        Self::drain_dirty_channel(dirty_receiver, build_owner);

        if !build_owner.has_pending_rebuilds() {
            return;
        }

        // Sort by depth: parents must rebuild before children
        build_owner.sort_dirty_by_depth(|id| element_registry.depth(id));

        // Drain dirty elements
        let dirty_ids: Vec<ElementKey> = build_owner.drain_dirty_sorted();

        // Capture the current time once for all animate calls in this cycle.
        let now = std::time::Instant::now();

        // Rebuild each dirty element
        for element_id in dirty_ids {
            // Skip if element was removed during a previous rebuild
            if !element_registry.contains(element_id) {
                continue;
            }

            // Enter build scope (cycle detection)
            if !build_owner.enter_build_scope(element_id) {
                continue;
            }

            // Get parent and render_object for context
            let parent = element_registry.parent(element_id);
            let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, element_id);

            // Create context for the element
            let mut ctx = ElementContext::new(
                element_id,
                parent,
                element_registry.children(element_id).to_vec(),
                state,
                dirty,
                render_objects,
                build_owner,
                dirty_sender,
                child_ops,
                focus_manager,
                parent_focus_node_id,
                animation_ticker.clone(),
            );

            // Animate then rebuild from current state using with_element
            element_registry.with_element(element_id, &mut ctx, |element, ctx| {
                element.animate(now, ctx);
                element.rebuild_from_state(ctx);
            });

            // Execute any child operations emitted during rebuild
            Self::execute_child_ops(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                focus_manager,
                animation_ticker,
            );

            // Exit build scope
            build_owner.exit_build_scope(element_id);
        }
    }

    /// Recursively reconcile an element and its children with a new widget tree.
    ///
    /// This follows the Flutter-style pattern where each element's rebuild()
    /// method handles both updating the widget and reconciling children.
    pub(crate) fn reconcile_element(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        element_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(element_id);

        log::debug!(
            "[RetainMode] reconcile_element() - element_id: {:?}",
            element_id
        );

        // Call element.rebuild() which handles both update and child reconciliation.
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            element_id,
            parent,
            element_registry.children(element_id).to_vec(),
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            None, // parent_focus_node_id not needed during reconcile
            animation_ticker.clone(),
        );

        element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild(widget_as_any, ctx);
        });

        // Execute any child operations emitted during rebuild
        Self::execute_child_ops(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
        );
    }

    /// Mount an element tree from a widget.
    ///
    /// Creates an element from the widget, inserts it into the registry,
    /// calls mount() via with_element, then executes any child ops that
    /// the element emitted during mount (which recursively mounts children).
    ///
    /// Note: This does NOT add the element to its parent's children list
    /// or call child_mounted to link the render object tree.
    /// The caller is responsible for that.
    pub(crate) fn mount_element_tree(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        parent: Option<ElementKey>,
        widget: Box<dyn Widget>,
    ) -> ElementKey {
        // Create the element from the widget
        let element = widget.create_element();

        // Insert into registry (does NOT call mount — we handle lifecycle here)
        let key = element_registry.insert(element, parent);

        // Resolve the parent's focus node ID before creating the context.
        // The parent element (if any) is already mounted and has a focus attachment,
        // because in retain mode, parent.mount() is called before children are mounted.
        let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, key);

        // Build context for the mount call
        let mut ctx = ElementContext::new(
            key,
            parent,
            Vec::new(), // Newly mounted elements have no children yet
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            parent_focus_node_id,
            animation_ticker.clone(),
        );

        // Call mount() on the element via with_element
        element_registry.with_element(key, &mut ctx, |element, ctx| {
            element.mount(ctx);
        });

        // After mount, the element may have created a render object.
        // If this is the root element (no parent), set it as the render object root.
        if parent.is_none() {
            if let Some(ro_key) = element_registry
                .get(key)
                .and_then(|el| el.render_object())
            {
                render_objects.set_root(ro_key);
            }
        }

        // Execute any child ops the element emitted during mount
        // This recursively mounts children (who may emit their own child ops, etc.)
        Self::execute_child_ops(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
        );

        // After child ops are processed, the element's render_object() may have changed
        // (e.g., StatefulElement delegates to its child's render object via child_mounted).
        // Re-check the root render object if this is the root element.
        if parent.is_none() {
            if let Some(ro_key) = element_registry
                .get(key)
                .and_then(|el| el.render_object())
            {
                render_objects.set_root(ro_key);
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
    pub(crate) fn execute_child_ops(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
    ) {
        loop {
            let ops = child_ops.drain();
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
                        let child_key = Self::mount_element_tree(
                            element_registry,
                            render_objects,
                            state,
                            dirty,
                            build_owner,
                            child_ops,
                            dirty_sender,
                            focus_manager,
                            animation_ticker,
                            Some(parent),
                            widget,
                        );

                        // Add child to parent's children list at the given slot
                        element_registry.add_child(parent, child_key, slot);

                        // Get the child's render object key for linking
                        let child_ro = element_registry
                            .get(child_key)
                            .and_then(|el| el.render_object());

                        // Call child_mounted to link the child's render object
                        // into the parent's render object tree.
                        let parent_parent = element_registry.parent(parent);

                        let mut ctx = ElementContext::new(
                            parent,
                            parent_parent,
                            element_registry.children(parent).to_vec(),
                            state,
                            dirty,
                            render_objects,
                            build_owner,
                            dirty_sender,
                            child_ops,
                            focus_manager,
                            None, // parent_focus_node_id not needed during child_mounted
                            animation_ticker.clone(),
                        );

                        element_registry.with_element(parent, &mut ctx, |element, ctx| {
                            element.child_mounted(slot, child_ro, ctx);
                        });
                    }
                    ChildOp::Update { child, widget } => {
                        // Rebuild the existing child element with the new widget
                        Self::rebuild_element(
                            element_registry,
                            render_objects,
                            state,
                            dirty,
                            build_owner,
                            child_ops,
                            dirty_sender,
                            focus_manager,
                            animation_ticker,
                            child,
                            widget,
                        );
                    }
                    ChildOp::Unmount { child } => {
                        // Unmount the child element tree
                        Self::unmount_element_tree(
                            element_registry,
                            render_objects,
                            state,
                            dirty,
                            build_owner,
                            child_ops,
                            dirty_sender,
                            focus_manager,
                            animation_ticker,
                            child,
                        );
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
    pub(crate) fn rebuild_element(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        element_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(element_id);
        let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, element_id);

        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            element_id,
            parent,
            element_registry.children(element_id).to_vec(),
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            parent_focus_node_id,
            animation_ticker.clone(),
        );

        element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild(widget_as_any, ctx);
        });
    }

    /// Drain dirty signals from the channel and mark elements for rebuild.
    ///
    /// When a `Signal::set()` fires its dirty callback, it sends
    /// the element ID through the channel. This method drains the channel
    /// and calls `mark_needs_build()` on the BuildOwner for each one.
    pub(crate) fn drain_dirty_channel(
        dirty_receiver: &mpsc::Receiver<ElementKey>,
        build_owner: &mut BuildOwner,
    ) {
        while let Ok(element_id) = dirty_receiver.try_recv() {
            build_owner.mark_needs_build(element_id);
        }
    }

    /// Unmount an element and all its descendants.
    pub(crate) fn unmount_element_tree(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        element_id: ElementKey,
    ) {
        // Get children and parent before unmounting
        let children = element_registry.children(element_id).to_vec();
        let parent = element_registry.parent(element_id);

        // Recursively unmount children first
        for child_id in &children {
            Self::unmount_element_tree(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                focus_manager,
                animation_ticker,
                *child_id,
            );
        }

        // Build context for the unmount call
        let mut ctx = ElementContext::new(
            element_id,
            parent,
            children,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
            None, // parent_focus_node_id not needed during unmount
            animation_ticker.clone(),
        );

        // Call unmount() via with_element
        element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            // Remove render object
            if let Some(render_id) = element.render_object() {
                ctx.remove_render_object(render_id);
            }

            // Call unmount lifecycle
            element.unmount(ctx);
        });

        // Remove state
        state.remove(element_id);

        // Unmount from registry (removes from parent's children list, etc.)
        element_registry.unmount(element_id);
    }
}
