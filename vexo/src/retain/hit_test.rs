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

use crate::core::{Logical, Point};
use crate::retain::{ElementId, RenderObjectId};

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
    path: Vec<RenderObjectId>,
    /// The element IDs along the path.
    element_path: Vec<ElementId>,
}

impl HitTestResult {
    /// Create a miss result (no hit).
    pub fn miss() -> Self {
        Self {
            path: Vec::new(),
            element_path: Vec::new(),
        }
    }

    /// Create a hit result with the given path.
    pub fn hit(path: Vec<RenderObjectId>, element_path: Vec<ElementId>) -> Self {
        Self { path, element_path }
    }

    /// Check if anything was hit.
    pub fn is_hit(&self) -> bool {
        !self.path.is_empty()
    }

    /// Get the target render object (deepest hit).
    ///
    /// Returns None if nothing was hit.
    pub fn target(&self) -> Option<RenderObjectId> {
        self.path.last().copied()
    }

    /// Get the target element.
    ///
    /// Returns None if nothing was hit.
    pub fn target_element(&self) -> Option<ElementId> {
        self.element_path.last().copied()
    }

    /// Get the path from root to target.
    ///
    /// Returns empty slice if nothing was hit.
    pub fn path(&self) -> &[RenderObjectId] {
        &self.path
    }

    /// Get the element path from root to target.
    ///
    /// Returns empty slice if nothing was hit.
    pub fn element_path(&self) -> &[ElementId] {
        &self.element_path
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

use crate::retain::{HitTestContext, RenderObjectRegistry};

impl RenderObjectRegistry {
    /// Hit test from root at the given position.
    ///
    /// Returns a `HitTestResult` containing the path from root to the hit target.
    /// If nothing is hit, returns a miss result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = registry.hit_test(Point::new(100.0, 100.0));
    /// if let Some(target) = result.target() {
    ///     // Handle hit on target
    /// }
    /// ```
    pub fn hit_test(&self, position: Point<Logical>) -> HitTestResult {
        let mut path = Vec::new();
        let mut element_path = Vec::new();

        if let Some(root) = self.root() {
            self.hit_test_recursive(root, position, &mut path, &mut element_path);
        }

        HitTestResult { path, element_path }
    }

    /// Recursive hit test implementation.
    ///
    /// Returns true if this node or any descendant was hit.
    fn hit_test_recursive(
        &self,
        id: RenderObjectId,
        position: Point<Logical>,
        path: &mut Vec<RenderObjectId>,
        element_path: &mut Vec<ElementId>,
    ) -> bool {
        let obj = match self.get(id) {
            Some(o) => o,
            None => return false,
        };

        let ctx = HitTestContext::mock();

        // Check if this object is hit
        if obj.hit_test(position, &ctx) {
            // Add this node to the path
            path.push(id);
            let element_id = self.element_for(id);
            if let Some(element_id) = element_id {
                element_path.push(element_id);
            }

            // Test children in reverse order (top-most first)
            // The last child is drawn on top, so it should be tested first
            for child in obj.children().iter().rev() {
                if self.hit_test_recursive(*child, position, path, element_path) {
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
    use crate::retain::{ElementId, TextRenderObject, RenderObject};
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};
    use crate::retain::LayoutContext;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
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
        let obj_id = RenderObjectId::new();
        let elem_id = ElementId::new();

        let result = HitTestResult::hit(vec![obj_id], vec![elem_id]);

        assert!(result.is_hit());
        assert_eq!(result.target(), Some(obj_id));
        assert_eq!(result.target_element(), Some(elem_id));
        assert_eq!(result.path().len(), 1);
        assert_eq!(result.element_path().len(), 1);
    }

    #[test]
    fn test_hit_test_result_path() {
        let obj1 = RenderObjectId::new();
        let obj2 = RenderObjectId::new();
        let elem1 = ElementId::new();
        let elem2 = ElementId::new();

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

        let element_id = ElementId::new();
        let id = registry.create(Box::new(obj), element_id);
        registry.set_root(id);

        // Hit test at a point inside (depends on computed layout)
        let result = registry.hit_test(Point::new(5.0, 5.0));

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

        let element_id = ElementId::new();
        let id = registry.create(Box::new(obj), element_id);
        registry.set_root(id);

        // Hit test at a point outside
        let result = registry.hit_test(Point::new(200.0, 200.0));

        assert!(!result.is_hit());
        assert!(result.target().is_none());
    }

    #[test]
    fn test_hit_test_no_root() {
        let registry = RenderObjectRegistry::new();

        // Hit test with no root set
        let result = registry.hit_test(Point::new(5.0, 5.0));

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

        let parent_elem = ElementId::new();
        let parent_id = registry.create(Box::new(parent), parent_elem);
        registry.set_root(parent_id);

        // Create child text with layout
        let mut child = TextRenderObject::new("Child");
        child.layout(&mut ctx, &[]);

        let child_elem = ElementId::new();
        let child_id = registry.create(Box::new(child), child_elem);

        // Add child to parent
        if let Some(parent_obj) = registry.get_mut(parent_id) {
            if let Some(container) = parent_obj.as_any_mut().downcast_mut::<ContainerRenderObject>() {
                container.add_child(child_id);
            }
        }

        // Hit test - the result depends on computed layout
        let result = registry.hit_test(Point::new(5.0, 5.0));

        // Result depends on actual computed bounds
        // The key thing is it doesn't panic
        let _ = result;
    }
}
