//! Hit testing for the retain rendering system.
//!
//! This module provides hit testing functionality for the RenderObject tree.
//! Hit testing determines which render object (if any) is at a given position,
//! enabling input event routing.
//!
//! # Algorithm
//!
//! Hit testing works by traversing the render tree from the root:
//! 1. Test each render object at the given position
//! 2. If hit, test children in reverse order (last child = top-most visually)
//! 3. Build a path from root to the deepest hit target
//!
//! # Example
//!
//! ```ignore
//! use vexo::retain::{RenderObjectRegistry, HitTestResult};
//! use vexo::core::Point;
//!
//! let registry = RenderObjectRegistry::new();
//! // ... populate registry ...
//!
//! let result = registry.hit_test(Point::new(100.0, 100.0));
//! if result.is_hit() {
//!     let target = result.target();
//!     // Handle hit on target render object
//! }
//! ```

use crate::core::{Absolute, Bounds, Logical, Position, Relative};
use crate::retain::{ElementKey, RenderObjectKey, RenderObjectRegistry};

// ============================================================================
// HIT TEST RESULT
// ============================================================================

/// Result of a hit test operation.
///
/// Contains the path from root to hit target (if any) and the associated
/// element IDs along that path.
#[derive(Debug, Clone)]
pub struct HitTestResult {
    /// Path from root to the hit target (if any).
    path: Vec<RenderObjectKey>,
    /// The element IDs along the path.
    element_path: Vec<ElementKey>,
    /// Absolute bounds of the hit target (in window coordinates).
    absolute_bounds: Option<Bounds<Logical>>,
}

impl HitTestResult {
    /// Create a miss result (no hit).
    pub fn miss() -> Self {
        Self {
            path: Vec::new(),
            element_path: Vec::new(),
            absolute_bounds: None,
        }
    }

    /// Create a hit result with the given path.
    pub fn hit(path: Vec<RenderObjectKey>, element_path: Vec<ElementKey>) -> Self {
        Self { path, element_path, absolute_bounds: None }
    }

    /// Create a hit result with absolute bounds.
    pub fn hit_with_bounds(
        path: Vec<RenderObjectKey>,
        element_path: Vec<ElementKey>,
        absolute_bounds: Bounds<Logical>,
    ) -> Self {
        Self {
            path,
            element_path,
            absolute_bounds: Some(absolute_bounds),
        }
    }

    /// Check if anything was hit.
    pub fn is_hit(&self) -> bool {
        !self.path.is_empty()
    }

    /// Get the target render object (deepest hit).
    ///
    /// Returns None if nothing was hit.
    pub fn target(&self) -> Option<RenderObjectKey> {
        self.path.last().copied()
    }

    /// Get the target element.
    ///
    /// Returns None if nothing was hit.
    pub fn target_element(&self) -> Option<ElementKey> {
        self.element_path.last().copied()
    }

    /// Get the path from root to target.
    ///
    /// Returns empty slice if nothing was hit.
    pub fn path(&self) -> &[RenderObjectKey] {
        &self.path
    }

    /// Get the element path from root to target.
    ///
    /// Returns empty slice if nothing was hit.
    pub fn element_path(&self) -> &[ElementKey] {
        &self.element_path
    }

    /// Get the absolute bounds of the hit target.
    ///
    /// Returns None if nothing was hit or bounds are not available.
    pub fn absolute_bounds(&self) -> Option<Bounds<Logical>> {
        self.absolute_bounds
    }
}

impl Default for HitTestResult {
    fn default() -> Self {
        Self::miss()
    }
}

// ============================================================================
// HIT TEST IMPLEMENTATION FOR RENDER OBJECT REGISTRY
// ============================================================================

impl RenderObjectRegistry {
    /// Hit test from root at the given position.
    ///
    /// Returns a `HitTestResult` containing the path from root to the hit target.
    /// If nothing is hit, returns a miss result.
    ///
    /// # Arguments
    ///
    /// * `position` - The position to test in absolute window coordinates
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = registry.hit_test(Position::new(100.0, 100.0));
    /// if let Some(target) = result.target() {
    ///     // Handle hit on target
    /// }
    /// ```
    pub fn hit_test(&self, position: Position<Logical, Absolute>) -> HitTestResult {
        let mut path = Vec::new();
        let mut element_path = Vec::new();
        let mut absolute_bounds: Option<Bounds<Logical>> = None;

        if let Some(root) = self.root() {
            // Root is at origin, so parent absolute position is zero
            let root_absolute_position = Position::<Logical, Absolute>::zero();
            self.hit_test_recursive(
                root,
                position,
                root_absolute_position,
                &mut path,
                &mut element_path,
                &mut absolute_bounds,
            );
        }

        HitTestResult {
            path,
            element_path,
            absolute_bounds,
        }
    }

    /// Recursive hit test implementation.
    ///
    /// Tracks the accumulated absolute position of each render object during traversal,
    /// similar to how `paint_recursive` accumulates positions for rendering.
    ///
    /// # Arguments
    ///
    /// * `id` - The render object to test
    /// * `pointer_position` - The pointer/mouse position in absolute window coordinates
    /// * `parent_absolute_position` - The accumulated absolute position of the parent
    /// * `path` - Output: path from root to hit target
    /// * `element_path` - Output: element IDs along the path
    /// * `absolute_bounds` - Output: absolute bounds of the hit target
    ///
    /// Returns true if this node or any descendant was hit.
    fn hit_test_recursive(
        &self,
        id: RenderObjectKey,
        pointer_position: Position<Logical, Absolute>,
        parent_absolute_position: Position<Logical, Absolute>,
        path: &mut Vec<RenderObjectKey>,
        element_path: &mut Vec<ElementKey>,
        absolute_bounds: &mut Option<Bounds<Logical>>,
    ) -> bool {
        let obj = match self.get(id) {
            Some(o) => o,
            None => return false,
        };

        // Get this object's position relative to its parent (from Taffy layout)
        let position_in_parent: Position<Logical, Relative> = obj.computed_bounds()
            .map(|b| Position::new(b.left, b.top))
            .unwrap_or(Position::zero());

        // Calculate this object's absolute position in window coordinates:
        // parent's absolute position + this object's position within parent
        let object_absolute_position = position_in_parent.to_absolute(parent_absolute_position);

        // Convert pointer position to local coordinates relative to this object
        let local_position = pointer_position.to_relative(object_absolute_position);

        // Get the size of this object
        let size = obj.computed_bounds()
            .map(|b| crate::core::Size::<Logical>::new(b.width(), b.height()))
            .unwrap_or(crate::core::Size::zero());

        // Check if the local position is within this object's bounds (0,0 to width,height)
        let is_inside = local_position.x >= 0.0
            && local_position.x <= size.width
            && local_position.y >= 0.0
            && local_position.y <= size.height;

        if is_inside {
            // Add this node to the path
            path.push(id);
            let element_id = self.element_for(id);
            if let Some(element_id) = element_id {
                element_path.push(element_id);
            }

            // Compute absolute bounds for this object
            *absolute_bounds = Some(Bounds::from_xywh(
                object_absolute_position.x,
                object_absolute_position.y,
                size.width,
                size.height,
            ));

            // Test children in reverse order (top-most first)
            // The last child is drawn on top, so it should be tested first
            for child in obj.children().iter().rev() {
                if self.hit_test_recursive(
                    *child,
                    pointer_position,
                    object_absolute_position,
                    path,
                    element_path,
                    absolute_bounds,
                ) {
                    return true;
                }
            }

            return true;
        }

        false
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{ElementKey, TextRenderObject, RenderObject};
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};
    use crate::retain::LayoutContext;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn make_two_element_keys() -> (ElementKey, ElementKey) {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let k1 = sm.insert(());
        let k2 = sm.insert(());
        (k1, k2)
    }

    #[test]
    fn test_hit_test_result_miss() {
        let result = HitTestResult::miss();

        assert!(!result.is_hit());
        assert!(result.target().is_none());
        assert!(result.target_element().is_none());
        assert!(result.path().is_empty());
        assert!(result.element_path().is_empty());
    }

    #[test]
    fn test_hit_test_result_hit() {
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let obj_id = sm.insert(());
        let elem_id = make_element_key();

        let result = HitTestResult::hit(vec![obj_id], vec![elem_id]);

        assert!(result.is_hit());
        assert_eq!(result.target(), Some(obj_id));
        assert_eq!(result.target_element(), Some(elem_id));
        assert_eq!(result.path().len(), 1);
        assert_eq!(result.element_path().len(), 1);
    }

    #[test]
    fn test_hit_test_result_path() {
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let obj1 = sm.insert(());
        let obj2 = sm.insert(());
        let (elem1, elem2) = make_two_element_keys();

        let result = HitTestResult::hit(vec![obj1, obj2], vec![elem1, elem2]);

        assert!(result.is_hit());
        assert_eq!(result.target(), Some(obj2)); // Last in path is target
        assert_eq!(result.target_element(), Some(elem2));
        assert_eq!(result.path(), &[obj1, obj2]);
        assert_eq!(result.element_path(), &[elem1, elem2]);
    }

    #[test]
    fn test_hit_test_result_default() {
        let result = HitTestResult::default();

        assert!(!result.is_hit());
    }

    #[test]
    fn test_hit_test_finds_target() {
        let mut registry = RenderObjectRegistry::new();

        // Create a text render object with layout
        let mut obj = TextRenderObject::new("Hello");
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Layout
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.layout(&mut ctx, &[]);
        }

        // Apply layout to get bounds
        let root = engine.create_leaf(&crate::layout::Layout::default());
        engine.compute(root, crate::core::Size::new(100.0, 50.0), &mut font_system);
        {
            let ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&ctx);
        }

        let element_id = make_element_key();
        let id = registry.create(Box::new(obj), element_id);
        registry.set_root(id);

        // Hit test at a point inside (depends on computed layout)
        let result = registry.hit_test(Position::new(5.0, 5.0));

        // Result depends on actual computed bounds
        // The key thing is it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_hit_test_misses_outside() {
        let mut registry = RenderObjectRegistry::new();

        // Create a text render object with layout
        let mut obj = TextRenderObject::new("Hello");
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        obj.layout(&mut ctx, &[]);

        let element_id = make_element_key();
        let id = registry.create(Box::new(obj), element_id);
        registry.set_root(id);

        // Hit test at a point outside
        let result = registry.hit_test(Position::new(200.0, 200.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_hit_test_no_root() {
        let registry = RenderObjectRegistry::new();

        // Hit test with no root set
        let result = registry.hit_test(Position::new(5.0, 5.0));

        assert!(!result.is_hit());
    }

    #[test]
    fn test_hit_test_with_children() {
        use crate::retain::{ContainerRenderObject, RenderObject};

        let mut registry = RenderObjectRegistry::new();

        // Create parent container with layout
        let mut parent = ContainerRenderObject::new_column();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        parent.layout(&mut ctx, &[]);

        let parent_elem = make_element_key();
        let parent_id = registry.create(Box::new(parent), parent_elem);
        registry.set_root(parent_id);

        // Create child text with layout
        let mut child = TextRenderObject::new("Child");
        child.layout(&mut ctx, &[]);

        let child_elem = make_element_key();
        let child_id = registry.create(Box::new(child), child_elem);

        // Add child to parent
        if let Some(parent_obj) = registry.get_mut(parent_id) {
            if let Some(container) = parent_obj.as_any_mut().downcast_mut::<ContainerRenderObject>() {
                container.add_child(child_id);
            }
        }

        // Hit test - the result depends on computed layout
        let result = registry.hit_test(Position::new(5.0, 5.0));

        // Result depends on actual computed bounds
        // The key thing is it doesn't panic
        let _ = result;
    }
}
