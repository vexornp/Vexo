//! Reconciler: reconciliation, lifecycle, and rebuild operations.
//!
//! This module extracts the reconciliation-related logic from `ThreeTreePipeline`
//! into a standalone `Reconciler` struct (zero-sized). All methods are
//! associated functions that take explicit parameters instead of accessing
//! `self` fields on a pipeline.

use std::sync::mpsc;

use super::build_owner::BuildOwner;
use super::child_ops::{ChildOp, ChildOps};
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;
use super::state::StateStorage;
use super::widgets::Widget;

/// Zero-sized struct that serves as a namespace for reconciliation logic.
///
/// All methods are associated functions that take explicit parameters instead
/// of accessing `ThreeTreePipeline` fields. This keeps reconciliation
/// independent of the pipeline struct.
pub struct Reconciler;

impl Reconciler {
    /// Reconcile a new widget tree with the existing element tree.
    ///
    /// This method:
    /// 1. Diffs the new widget tree against existing elements
    /// 2. Mounts new elements for new widgets
    /// 3. Updates existing elements where widgets match
    /// 4. Unmounts elements for removed widgets
    /// 5. Creates/destroys render objects accordingly
    /// 6. Marks affected render objects as dirty
    pub fn reconcile(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
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
        root_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(root_id);

        log::debug!("[RetainMode] rebuild_root() - element_id: {:?}", root_id);

        // Call element.rebuild() which handles both update and child reconciliation
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            root_id,
            parent,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
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
        dirty_receiver: &mpsc::Receiver<ElementKey>,
    ) {
        // First, drain any dirty signals from StatefulMutable callbacks
        Self::drain_dirty_channel(dirty_receiver, build_owner);

        if !build_owner.has_pending_rebuilds() {
            return;
        }

        // Sort by depth: parents must rebuild before children
        build_owner.sort_dirty_by_depth(|id| element_registry.depth(id));

        // Drain dirty elements
        let dirty_ids: Vec<ElementKey> = build_owner.drain_dirty_sorted();

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

            // Create context for the element
            let mut ctx = ElementContext::new(
                element_id,
                parent,
                state,
                dirty,
                render_objects,
                build_owner,
                dirty_sender,
                child_ops,
            );

            // Rebuild from current state using with_element
            element_registry.with_element(element_id, &mut ctx, |element, ctx| {
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
            );

            // Exit build scope
            build_owner.exit_build_scope(element_id);
        }
    }

    /// Perform state-driven rebuilds only, without a new widget tree.
    pub fn update_state_only(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        dirty_receiver: &mpsc::Receiver<ElementKey>,
    ) {
        Self::perform_rebuilds(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            dirty_receiver,
        );
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
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
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
        );
    }

    /// Mount an element tree from a widget.
    ///
    /// Creates an element from the widget, inserts it into the registry,
    /// calls mount() via with_element, then executes any child ops that
    /// the element emitted during mount (which recursively mounts children).
    ///
    /// Note: This does NOT add the element to its parent's children list
    /// or call child_mounted. The caller is responsible for that.
    pub(crate) fn mount_element_tree(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        parent: Option<ElementKey>,
        widget: Box<dyn Widget>,
    ) -> ElementKey {
        // Create the element from the widget
        let element = widget.create_element();

        // Insert into registry (does NOT call mount — we handle lifecycle here)
        let key = element_registry.insert(element, parent);

        // Build context for the mount call
        let mut ctx = ElementContext::new(
            key,
            parent,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
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
        );

        // After child ops are processed, the element's render_object() may have changed
        // (e.g., StatefulElement delegates to its child's render object after child_mounted).
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
                            Some(parent),
                            widget,
                        );

                        // Add child to parent's children list at the given slot
                        element_registry.add_child(parent, child_key, slot);

                        // Get the child's render object key for linking
                        let child_ro = element_registry
                            .get(child_key)
                            .and_then(|el| el.render_object());

                        // Notify parent element of the new child via child_mounted,
                        // passing the child's render object key so the parent can
                        // link the render object tree.
                        let parent_parent = element_registry.parent(parent);

                        let mut ctx = ElementContext::new(
                            parent,
                            parent_parent,
                            state,
                            dirty,
                            render_objects,
                            build_owner,
                            dirty_sender,
                            child_ops,
                        );

                        element_registry.with_element(parent, &mut ctx, |element, ctx| {
                            element.child_mounted(child_key, slot, child_ro, ctx);
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
        element_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(element_id);

        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let mut ctx = ElementContext::new(
            element_id,
            parent,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
        );

        element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild(widget_as_any, ctx);
        });
    }

    /// Drain dirty signals from the channel and mark elements for rebuild.
    ///
    /// When a `StatefulMutable::set()` fires its dirty callback, it sends
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
        element_id: ElementKey,
    ) {
        // Get children and parent before unmounting
        let children = element_registry.children(element_id).to_vec();
        let parent = element_registry.parent(element_id);

        // Recursively unmount children first
        for child_id in children {
            Self::unmount_element_tree(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                child_id,
            );
        }

        // Build context for the unmount call
        let mut ctx = ElementContext::new(
            element_id,
            parent,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
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