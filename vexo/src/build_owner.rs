//! Build owner for tracking dirty elements and driving targeted rebuilds.
//!
//! Inspired by Flutter's BuildOwner, this module tracks which elements
//! need to rebuild and performs those rebuilds efficiently.
//!
//! # Depth Ordering
//!
//! Dirty elements are sorted by tree depth before rebuild so that parents
//! always rebuild before children. This is a critical Flutter invariant:
//! a parent's rebuild may change which children exist, so children must
//! not rebuild until the parent has reconciled its subtree.
//!
//! # Interior Mutability
//!
//! The dirty tracking uses `RefCell` for interior mutability. This allows
//! the `mark_needs_build()` method to be called from within callbacks
//! (e.g., `Signal::set()` dirty callbacks) that fire during
//! event handling, when the pipeline has a mutable borrow on itself.
//! Using `RefCell` avoids aliasing UB that would occur with raw pointers.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;

use super::global_key_registry::GlobalKeyRegistry;
use super::id::ElementKey;
use crate::core::KeyboardInsetSource;
use crate::core::SafeAreaSource;

/// Tracks dirty elements and drives targeted rebuilds.
///
/// In Flutter, when `setState()` is called, only that element's subtree
/// rebuilds. The BuildOwner tracks which elements are dirty and ensures
/// only those elements (and their descendants) are reconciled.
///
/// Dirty elements are stored in a `Vec` for ordering (sorted by depth
/// before rebuild) and a `HashSet` for O(1) membership checks.
///
/// # Interior Mutability
///
/// The `dirty_elements` and `dirty_set` are stored in `RefCell` to allow
/// `mark_needs_build()` to be called from within event callbacks without
/// requiring a mutable reference to the BuildOwner. This is necessary
/// because the pipeline holds a mutable borrow during event handling,
/// and the `Signal` dirty callbacks need to mark elements dirty.
pub struct BuildOwner {
    /// Elements that need rebuild, in insertion order.
    /// Sorted by depth before rebuild via `sort_dirty_by_depth()`.
    /// Wrapped in RefCell for interior mutability.
    dirty_elements: RefCell<Vec<ElementKey>>,

    /// Set for O(1) membership check (kept in sync with dirty_elements).
    /// Wrapped in RefCell for interior mutability.
    dirty_set: RefCell<HashSet<ElementKey>>,

    /// Elements in the current build scope.
    ///
    /// During a rebuild, we track which elements are being built
    /// to detect cycles.
    building: HashSet<ElementKey>,

    /// Global key registry for cross-parent element identity.
    /// Wrapped in RefCell so it can be borrowed mutably via &self,
    /// enabling simultaneous shared borrow of BuildOwner and mutable
    /// borrow of GlobalKeyRegistry.
    global_keys: RefCell<GlobalKeyRegistry>,

    /// The currently focused element, if any.
    /// Wrapped in RefCell for interior mutability so that
    /// `set_focused_element()` can be called from event handlers
    /// that only have `&BuildOwner`.
    focused_element: RefCell<Option<ElementKey>>,

    /// Deferred unfocus request, set by `LifecycleContext::clear_focus()` during
    /// a rebuild (e.g. when `NavigationStackView` observes a pending pop).
    ///
    /// `RenderContext` cannot reach `FocusManager` directly (it only holds a
    /// shared `&BuildOwner`), so the request is stashed here and applied by
    /// the pipeline after `perform_rebuilds()` returns, where it has a free
    /// `&mut FocusManager`. This is what lets a widget dismiss focus (and on
    /// mobile, the software keyboard) at the moment a navigation transition
    /// *starts* rather than waiting for the outgoing page to unmount at the
    /// end of the animation.
    pending_unfocus: RefCell<bool>,

    /// Device safe-area insets (logical pixels), shared with all
    /// [`RenderContext`](crate::stateful_widget::RenderContext)s so widgets
    /// like `SafeArea` can read live values during `Component::render()`.
    ///
    /// Backed by atomics inside [`SafeAreaSource`], so updates from
    /// `WindowState` (each frame) are visible here without additional locking.
    /// Defaults to all-zero (desktop / pre-init), which makes safe-area a no-op
    /// for tests and desktop builds.
    safe_area_source: SafeAreaSource,

    /// Keyboard target inset source (logical pixels), shared with all
    /// [`RenderContext`](crate::stateful_widget::RenderContext)s so
    /// `KeyboardAvoidance` can read live values during `Component::render()`.
    ///
    /// Backed by atomics inside [`KeyboardInsetSource`], so updates from
    /// the iOS keyboard shim are visible here without additional locking.
    /// Defaults to all-zero (desktop / pre-init / keyboard down), making
    /// keyboard avoidance a no-op for tests and desktop builds.
    keyboard_inset_source: KeyboardInsetSource,
}

impl BuildOwner {
    /// Create a new empty build owner.
    pub fn new() -> Self {
        Self {
            dirty_elements: RefCell::new(Vec::new()),
            dirty_set: RefCell::new(HashSet::new()),
            building: HashSet::new(),
            global_keys: RefCell::new(GlobalKeyRegistry::new()),
            focused_element: RefCell::new(None),
            pending_unfocus: RefCell::new(false),
            safe_area_source: SafeAreaSource::default(),
            keyboard_inset_source: KeyboardInsetSource::default(),
        }
    }

    /// Mark an element as needing rebuild.
    ///
    /// The element will be rebuilt during the next `perform_rebuilds()`.
    /// This is equivalent to Flutter's `markNeedsBuild()`.
    ///
    /// Idempotent: calling this multiple times with the same element ID
    /// only adds it once.
    ///
    /// This method uses interior mutability (RefCell), so it can be called
    /// from within callbacks that fire during event handling without
    /// requiring a mutable reference to the BuildOwner.
    pub fn mark_needs_build(&self, element_id: ElementKey) {
        if self.dirty_set.borrow_mut().insert(element_id) {
            self.dirty_elements.borrow_mut().push(element_id);
        }
    }

    /// Check if an element is marked as dirty.
    pub fn is_dirty(&self, element_id: ElementKey) -> bool {
        self.dirty_set.borrow().contains(&element_id)
    }

    /// Check if there are any pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        !self.dirty_set.borrow().is_empty()
    }

    /// Get the number of dirty elements.
    pub fn dirty_count(&self) -> usize {
        self.dirty_set.borrow().len()
    }

    /// Sort dirty elements by tree depth (parents before children).
    ///
    /// Must be called before draining. The `depth` function is provided
    /// by the caller (typically via `ElementRegistry::depth()`).
    ///
    /// This ensures the Flutter invariant that parents rebuild before
    /// children, which is critical because a parent's rebuild may
    /// change which children exist.
    pub fn sort_dirty_by_depth<F>(&self, mut depth: F)
    where
        F: FnMut(ElementKey) -> usize,
    {
        self.dirty_elements
            .borrow_mut()
            .sort_by_key(|id| depth(*id));
    }

    /// Drain dirty elements in depth order.
    ///
    /// Call `sort_dirty_by_depth()` first to ensure parents come
    /// before children. Clears both the vec and the set.
    pub fn drain_dirty_sorted(&mut self) -> Vec<ElementKey> {
        self.dirty_set.borrow_mut().clear();
        self.dirty_elements.borrow_mut().drain(..).collect()
    }

    /// Drain dirty elements, returning them for processing.
    ///
    /// This clears the dirty set and returns all elements that need rebuild.
    /// Elements are returned in insertion order (not depth-sorted).
    /// Prefer `sort_dirty_by_depth()` + `drain_dirty_sorted()` for
    /// correct parent-before-child ordering.
    pub fn drain_dirty(&mut self) -> Vec<ElementKey> {
        self.dirty_set.borrow_mut().clear();
        self.dirty_elements.borrow_mut().drain(..).collect()
    }

    /// Clear all dirty elements without rebuilding.
    pub fn clear_dirty(&mut self) {
        self.dirty_elements.borrow_mut().clear();
        self.dirty_set.borrow_mut().clear();
    }

    /// Enter a build scope for an element.
    ///
    /// Used to detect cycles during rebuild. Returns false if the element
    /// is already being built (cycle detected).
    pub fn enter_build_scope(&mut self, element_id: ElementKey) -> bool {
        if self.building.contains(&element_id) {
            // Cycle detected
            return false;
        }
        self.building.insert(element_id);
        true
    }

    /// Exit a build scope for an element.
    pub fn exit_build_scope(&mut self, element_id: ElementKey) {
        self.building.remove(&element_id);
    }

    /// Check if currently building an element.
    pub fn is_building(&self, element_id: ElementKey) -> bool {
        self.building.contains(&element_id)
    }

    /// Get a reference to the global key registry.
    pub fn global_keys(&self) -> Ref<'_, GlobalKeyRegistry> {
        self.global_keys.borrow()
    }

    /// Get a mutable reference to the global key registry via interior mutability.
    ///
    /// Takes `&self` (not `&mut self`) so that a shared `&BuildOwner` reference
    /// can coexist with a mutable borrow of the global key registry.
    /// This is needed because the pipeline passes `&BuildOwner` to elements
    /// while also needing mutable access to global keys for registration.
    pub fn global_keys_mut(&self) -> RefMut<'_, GlobalKeyRegistry> {
        self.global_keys.borrow_mut()
    }

    /// Get the currently focused element.
    pub fn focused_element(&self) -> Option<ElementKey> {
        *self.focused_element.borrow()
    }

    /// Set the currently focused element.
    pub fn set_focused_element(&self, element: Option<ElementKey>) {
        *self.focused_element.borrow_mut() = element;
    }

    /// Request that the pipeline clear primary focus after the current
    /// rebuild pass.
    ///
    /// Called from `LifecycleContext::clear_focus()` by widgets that need to
    /// dismiss focus while rebuilding (e.g. `NavigationStackView` when a
    /// pop transition starts). The request is deferred because `RenderContext`
    /// only has `&BuildOwner` and cannot touch `FocusManager` directly; the
    /// pipeline drains it in `perform_rebuilds()` via
    /// [`take_unfocus_requested()`](Self::take_unfocus_requested).
    ///
    /// Idempotent: multiple requests within one rebuild cycle collapse to a
    /// single unfocus.
    pub fn request_unfocus(&self) {
        *self.pending_unfocus.borrow_mut() = true;
    }

    /// Take and clear the deferred unfocus request.
    ///
    /// Returns `true` if at least one `request_unfocus()` was made since the
    /// last call. The pipeline calls this after `perform_rebuilds()` and, when
    /// it returns `true`, invokes `FocusManager::unfocus()` (a no-op when
    /// nothing is focused).
    pub fn take_unfocus_requested(&self) -> bool {
        let v = *self.pending_unfocus.borrow();
        *self.pending_unfocus.borrow_mut() = false;
        v
    }

    /// Test-only accessor: returns `true` if `request_unfocus()` has been
    /// called since the last `take_unfocus_requested()`. Used by tests to
    /// assert that a deferred unfocus was scheduled.
    pub fn has_unfocus_request(&self) -> bool {
        *self.pending_unfocus.borrow()
    }

    /// Get a clone of the shared safe-area source.
    ///
    /// Returns a cheaply-clonable handle ([`SafeAreaSource`] is `Arc`-based)
    /// whose [`SafeAreaSource::get()`] always reads the latest insets set by
    /// [`WindowState`](crate::window::WindowState). Used by
    /// [`RenderContext::safe_area()`](crate::stateful_widget::RenderContext::safe_area)
    /// so widgets such as `SafeArea` can resolve insets during render.
    pub fn safe_area_source(&self) -> SafeAreaSource {
        self.safe_area_source.clone()
    }

    /// Replace the safe-area source.
    ///
    /// Called once at window init so the [`BuildOwner`] shares the same
    /// atomics as [`WindowState`](crate::window::WindowState); subsequent
    /// per-frame updates happen via [`SafeAreaSource::set()`] on either clone.
    pub fn set_safe_area_source(&mut self, source: SafeAreaSource) {
        self.safe_area_source = source;
    }

    /// Get a clone of the shared keyboard-inset source.
    ///
    /// Returns a cheaply-clonable handle ([`KeyboardInsetSource`] is `Arc`-based)
    /// whose [`KeyboardInsetSource::get()`] always reads the latest target
    /// written by the iOS keyboard shim. Used by
    /// [`RenderContext::keyboard_inset()`](crate::stateful_widget::RenderContext::keyboard_inset)
    /// so `KeyboardAvoidance` can resolve the target during render.
    pub fn keyboard_inset_source(&self) -> KeyboardInsetSource {
        self.keyboard_inset_source.clone()
    }

    /// Replace the keyboard-inset source.
    ///
    /// Called once at window init so the [`BuildOwner`] shares the same
    /// atomics as [`WindowState`](crate::window::WindowState); subsequent
    /// updates happen via [`KeyboardInsetSource::set_target()`] on either clone.
    pub fn set_keyboard_inset_source(&mut self, source: KeyboardInsetSource) {
        self.keyboard_inset_source = source;
    }
}

impl Default for BuildOwner {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a rebuild operation.
#[derive(Debug, Default)]
pub struct RebuildResult {
    /// Number of elements rebuilt.
    pub elements_rebuilt: usize,

    /// Whether any cycles were detected.
    pub cycles_detected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deferred_unfocus_request_and_take() {
        let bo = BuildOwner::new();
        // Initially nothing is requested.
        assert!(!bo.take_unfocus_requested());

        // A single request is reported once then cleared.
        bo.request_unfocus();
        assert!(bo.take_unfocus_requested());
        assert!(!bo.take_unfocus_requested());
    }

    #[test]
    fn test_deferred_unfocus_idempotent() {
        // Multiple requests within one rebuild cycle collapse to a single
        // unfocus — mirrors how `LifecycleContext::clear_focus()` may be called
        // by several widgets during the same rebuild pass.
        let bo = BuildOwner::new();
        bo.request_unfocus();
        bo.request_unfocus();
        bo.request_unfocus();
        assert!(bo.take_unfocus_requested());
        assert!(!bo.take_unfocus_requested());
    }
}
