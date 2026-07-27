//! Reconciler: reconciliation, lifecycle, and rebuild operations.
//!
//! This module extracts the reconciliation-related logic from `ThreeTreePipeline`
//! into a standalone `Reconciler` struct (zero-sized). All methods are
//! associated functions that take explicit parameters instead of accessing
//! `self` fields on a pipeline.

use std::sync::{mpsc, Arc};

use slotmap::SecondaryMap;

use super::build_owner::BuildOwner;
use super::child_ops::{ChildOp, ChildOps};
use super::dirty::DirtyTracking;
use super::element::ElementRegistry;
use super::element_context::ElementContext;
use super::element_state::StateStorage;
use super::focus::{FocusManager, FocusNodeId};
use super::id::ElementKey;
use super::inherited_registry::{InheritedMap, InheritedRegistry};
use super::render_object::RenderObjectRegistry;
use super::widgets::Widget;
use crate::animation::AnimationTicker;

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

    /// Compute the inherited map for an element by looking up its parent's
    /// map in `inherited_maps`. Returns an `Arc` so the caller can hold a
    /// `&InheritedMap` reference without aliasing the `&mut inherited_maps`
    /// that is also passed to `ElementContext::new` as `inherited_map_storage`.
    ///
    /// - For root elements (`parent == None`), returns a fresh empty map.
    /// - If the parent has no entry yet (e.g. mounting under a non-inherited
    ///   element that hasn't populated its slot — which shouldn't normally
    ///   happen, but is defensive), also returns an empty map.
    fn compute_inherited_map(
        parent: Option<ElementKey>,
        inherited_maps: &SecondaryMap<ElementKey, Arc<InheritedMap>>,
    ) -> Arc<InheritedMap> {
        match parent {
            None => Arc::new(InheritedMap::empty()),
            Some(p) => inherited_maps
                .get(p)
                .cloned()
                .unwrap_or_else(|| Arc::new(InheritedMap::empty())),
        }
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
                    inherited_registry,
                    inherited_maps,
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
                inherited_registry,
                inherited_maps,
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
            inherited_registry,
            inherited_maps,
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
        root_widget: Box<dyn Widget>,
    ) {
        // First, perform any pending state-driven rebuilds (from
        // `Signal::set` / dirty-callback invocations)
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
            inherited_registry,
            inherited_maps,
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
                inherited_registry,
                inherited_maps,
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
                        inherited_registry,
                        inherited_maps,
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
                        inherited_registry,
                        inherited_maps,
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
                    inherited_registry,
                    inherited_maps,
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
        root_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        let parent = element_registry.parent(root_id);
        let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, root_id);

        log::debug!("[RetainMode] rebuild_root() - element_id: {:?}", root_id);

        // Call element.rebuild() which handles both update and child reconciliation
        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
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
            &inherited_map_arc,
            inherited_registry,
            inherited_maps,
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
            inherited_registry,
            inherited_maps,
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
            let parent_focus_node_id =
                Self::resolve_parent_focus_node_id(element_registry, element_id);

            // Create context for the element
            let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
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
                &inherited_map_arc,
                inherited_registry,
                inherited_maps,
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
                inherited_registry,
                inherited_maps,
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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

        let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
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
            &inherited_map_arc,
            inherited_registry,
            inherited_maps,
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
            inherited_registry,
            inherited_maps,
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
        let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
        // Store this element's inherited map so its children can resolve it
        // via `compute_inherited_map`. `InheritedElement::mount` overwrites
        // this with its COW'd map (which adds its own type); non-provider
        // elements keep the parent's map unchanged. Insert BEFORE creating
        // `ElementContext` so the `&mut` borrow ends before `ctx` borrows
        // `inherited_maps` as `inherited_map_storage`.
        inherited_maps.insert(key, inherited_map_arc.clone());
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
            &inherited_map_arc,
            inherited_registry,
            inherited_maps,
        );

        // Call mount() on the element via with_element
        element_registry.with_element(key, &mut ctx, |element, ctx| {
            element.mount(ctx);
        });

        // After mount, the element may have created a render object.
        // If this is the root element (no parent), set it as the render object root.
        if parent.is_none() {
            if let Some(ro_key) = element_registry.get(key).and_then(|el| el.render_object()) {
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
            inherited_registry,
            inherited_maps,
        );

        // After child ops are processed, the element's render_object() may have changed
        // (e.g., StatefulElement delegates to its child's render object via child_mounted).
        // Re-check the root render object if this is the root element.
        if parent.is_none() {
            if let Some(ro_key) = element_registry.get(key).and_then(|el| el.render_object()) {
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
                            inherited_registry,
                            inherited_maps,
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

                        let inherited_map_arc =
                            Self::compute_inherited_map(parent_parent, inherited_maps);
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
                            &inherited_map_arc,
                            inherited_registry,
                            inherited_maps,
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
                            inherited_registry,
                            inherited_maps,
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
                            inherited_registry,
                            inherited_maps,
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
    ///
    /// If `can_update()` returns true, calls `element.rebuild()` in place.
    /// If `can_update()` returns false (widget type changed), unmounts the
    /// old element and mounts a new one in its place — matching Flutter's
    /// `updateChild` semantics where a type mismatch triggers replacement.
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
        element_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        // Check can_update before rebuilding. If the widget type changed
        // (e.g. ScrollView → WithLayout), the old element cannot be reused
        // and must be replaced.
        let can_update = element_registry
            .get(element_id)
            .map(|el| el.can_update(widget.as_any()))
            .unwrap_or(false);

        if !can_update {
            Self::replace_element(
                element_registry,
                render_objects,
                state,
                dirty,
                build_owner,
                child_ops,
                dirty_sender,
                focus_manager,
                animation_ticker,
                inherited_registry,
                inherited_maps,
                element_id,
                widget,
            );
            return;
        }

        let parent = element_registry.parent(element_id);
        let parent_focus_node_id = Self::resolve_parent_focus_node_id(element_registry, element_id);

        let widget_as_any: Box<dyn std::any::Any> = Box::new(widget.clone_boxed());

        let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
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
            &inherited_map_arc,
            inherited_registry,
            inherited_maps,
        );

        element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild(widget_as_any, ctx);
        });
    }

    /// Replace an element with a new one built from `widget`.
    ///
    /// Called by `rebuild_element` when `can_update()` returns false.
    /// Mounts the new element, replaces the old element key at the same slot
    /// in the parent's children list, swaps the render object key at the same
    /// position in the parent render object's children list, then unmounts
    /// the old element tree.
    ///
    /// The replacement happens BEFORE unmounting so the slot index remains
    /// valid. If we unmounted first, the old element would be removed from
    /// the parent's children list (shifting subsequent elements left), making
    /// the computed slot index point at the wrong element.
    fn replace_element(
        element_registry: &mut ElementRegistry,
        render_objects: &mut RenderObjectRegistry,
        state: &mut StateStorage,
        dirty: &mut DirtyTracking,
        build_owner: &mut BuildOwner,
        child_ops: &mut ChildOps,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        animation_ticker: &Arc<AnimationTicker>,
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
        element_id: ElementKey,
        widget: Box<dyn Widget>,
    ) {
        // Find the parent and slot.
        let parent = match element_registry.parent(element_id) {
            Some(p) => p,
            None => return,
        };
        let slot = element_registry
            .children(parent)
            .iter()
            .position(|&c| c == element_id);

        log::debug!(
            "[replace_element] replacing element {:?} at slot {:?} under parent {:?}",
            element_id,
            slot,
            parent
        );

        // Capture the old child's render object key for RO replacement.
        let old_child_ro = element_registry
            .get(element_id)
            .and_then(|el| el.render_object());

        // Mount the new element tree with the same parent.
        // mount_element_tree inserts the element into the registry and sets
        // parent_map, but does NOT add it to the parent's children list.
        let new_child_key = Self::mount_element_tree(
            element_registry,
            render_objects,
            state,
            dirty,
            build_owner,
            child_ops,
            dirty_sender,
            focus_manager,
            animation_ticker,
            inherited_registry,
            inherited_maps,
            Some(parent),
            widget,
        );

        // Replace the old element key with the new key at the same slot
        // in the parent's children list. This must happen BEFORE unmounting
        // the old element, otherwise the slot index would be stale (the old
        // element's removal would shift subsequent siblings left).
        if let Some(idx) = slot {
            element_registry.replace_child_at(parent, idx, new_child_key);
        }

        // Replace the old render object key with the new one at the same
        // position in the parent render object's children list. This preserves
        // the correct layout/paint order. Without this, the new RO would be
        // appended at the end (via add_child), causing wrong visual order.
        let new_child_ro = element_registry
            .get(new_child_key)
            .and_then(|el| el.render_object());
        if let (Some(old_ro), Some(new_ro)) = (old_child_ro, new_child_ro) {
            if let Some(parent_ro) = element_registry
                .get(parent)
                .and_then(|el| el.render_object())
            {
                if let Some(parent_obj) = render_objects.get_mut(parent_ro) {
                    parent_obj.replace_child(old_ro, new_ro);
                }
            }
        }

        // Now unmount the old element tree. The old element's key has already
        // been replaced in the parent's children list, so the retain() inside
        // element_registry.unmount() is a no-op — no further shifting occurs.
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
            inherited_registry,
            inherited_maps,
            element_id,
        );

        // Mark the parent's render object for layout so the new child's
        // Taffy node gets linked into the layout tree.
        //
        // Pass-through render objects (DecoratedBox, Offstage, etc.) don't
        // own a Taffy node — they return the child's node directly, so the
        // grandparent is the one that links the grandchild's node into its
        // Taffy children list. When a child is replaced beneath a pass-through
        // RO, the RO needs `layout()` to refresh its `child_layout_node`, AND
        // the nearest non-pass-through ancestor needs `layout()` to re-link
        // its Taffy children with the new node. Walk up through the chain,
        // marking each RO plus the first non-pass-through ancestor.
        let mut current = parent;
        while let Some(ro) = element_registry
            .get(current)
            .and_then(|el| el.render_object())
        {
            dirty.mark_needs_layout(ro);

            let is_pass_through = render_objects
                .get(ro)
                .map(|obj| obj.is_pass_through())
                .unwrap_or(false);

            if !is_pass_through {
                break;
            }

            match element_registry.parent(current) {
                Some(grandparent) => current = grandparent,
                None => break,
            }
        }
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
        inherited_registry: &InheritedRegistry,
        inherited_maps: &mut SecondaryMap<ElementKey, Arc<InheritedMap>>,
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
                inherited_registry,
                inherited_maps,
                *child_id,
            );
        }

        // Build context for the unmount call
        let inherited_map_arc = Self::compute_inherited_map(parent, inherited_maps);
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
            &inherited_map_arc,
            inherited_registry,
            inherited_maps,
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

        // Remove this element's inherited-map entry. `InheritedElement::unmount`
        // already removed its own entry above (no-op here); other elements
        // still have the entry we stored at mount, so clear it now. `ctx` is
        // not used past this point, so NLL allows the `&mut` borrow on
        // `inherited_maps` to resume here.
        inherited_maps.remove(element_id);

        // Remove state
        state.remove(element_id);

        // Unmount from registry (removes from parent's children list, etc.)
        element_registry.unmount(element_id);
    }
}
