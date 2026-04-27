//! RenderObject trait and registry for the retain rendering system.
//!
//! RenderObjects are persistent objects that handle layout and painting.
//! They form the third tree in the three-tree architecture (Widget/Element/RenderObject).
//!
//! # Key Concepts
//!
//! - **RenderObject trait**: Defines layout(), paint(), hit_test() methods
//! - **RenderObjectRegistry**: Manages all render objects and their relationship to elements
//! - **LayoutContext, PaintContext, HitTestContext**: Provide context during operations
//!
//! # Lifetime
//!
//! RenderObjects persist across frames and are only updated when marked dirty.
//! They are created during element inflation and destroyed during element unmounting.

use std::collections::HashMap;

use crate::core::{Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;

use super::id::{ElementId, RenderObjectId};

// ============================================================================
// LAYOUT CONTEXT
// ============================================================================

/// Context passed to RenderObject.layout().
///
/// Provides access to the layout engine and parent constraints.
pub struct LayoutContext<'a> {
    // Placeholder - will integrate with TaffyLayoutEngine later
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> LayoutContext<'a> {
    /// Create a mock layout context for testing.
    pub fn mock() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

// ============================================================================
// PAINT CONTEXT
// ============================================================================

/// Context passed to RenderObject.paint().
///
/// Provides access to the render command list and current offset.
pub struct PaintContext<'a> {
    offset: Point<crate::core::Logical>,
    commands: &'a mut Vec<RenderCommand>,
}

impl<'a> PaintContext<'a> {
    /// Create a new paint context.
    pub fn new(commands: &'a mut Vec<RenderCommand>) -> Self {
        Self {
            offset: Point::zero(),
            commands,
        }
    }

    /// Push a render command.
    pub fn push_command(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    /// Get the current offset.
    pub fn offset(&self) -> Point<crate::core::Logical> {
        self.offset
    }

    /// Set the offset.
    pub fn set_offset(&mut self, offset: Point<crate::core::Logical>) {
        self.offset = offset;
    }
}

// ============================================================================
// HIT TEST CONTEXT
// ============================================================================

/// Context passed to RenderObject.hit_test().
///
/// Provides information needed for hit testing.
pub struct HitTestContext {
    // Placeholder for hit test context
}

impl HitTestContext {
    /// Create a mock hit test context.
    pub fn mock() -> Self {
        Self {}
    }
}

// ============================================================================
// RENDER OBJECT TRAIT
// ============================================================================

/// Persistent render object for layout and painting.
///
/// RenderObjects form the third tree in the three-tree architecture.
/// They persist across frames and are only updated when marked dirty.
///
/// # Layout
///
/// The `layout` method is called with constraints and should return the
/// computed size. It can mutate internal state (e.g., caching layout results).
///
/// # Paint
///
/// The `paint` method is called after layout and should generate render
/// commands. It must not mutate state - paint is purely for output.
///
/// # Hit Test
///
/// The `hit_test` method determines if a pointer event should be handled
/// by this render object.
///
/// # Example
///
/// ```ignore
/// struct ColoredRect {
///     color: Color,
///     computed_size: Size<Logical>,
/// }
///
/// impl RenderObject for ColoredRect {
///     fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
///         // Use the constraints to determine size
///         Size::new(
///             constraints.min_width.max(constraints.max_width.min(100.0)),
///             constraints.min_height.max(constraints.max_height.min(50.0)),
///         )
///     }
///
///     fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
///         vec![RenderCommand::rect(
///             Bounds::from_xywh(0.0, 0.0, self.computed_size.width, self.computed_size.height),
///             self.color,
///         )]
///     }
///
///     fn hit_test(&self, position: Point, _ctx: &HitTestContext) -> bool {
///         position.x >= 0.0 && position.x < self.computed_size.width
///             && position.y >= 0.0 && position.y < self.computed_size.height
///     }
/// }
/// ```
pub trait RenderObject {
    /// Perform layout with given constraints, return computed size.
    ///
    /// This method can mutate internal state (e.g., caching computed layout).
    fn layout(&mut self, constraints: LayoutConstraints, ctx: &mut LayoutContext) -> Size<crate::core::Logical>;

    /// Generate paint commands.
    ///
    /// This method should not mutate state - it's purely for generating output.
    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand>;

    /// Hit test for pointer events.
    ///
    /// Returns true if the given position should be handled by this render object.
    fn hit_test(&self, position: Point<crate::core::Logical>, ctx: &HitTestContext) -> bool;

    /// Get children (for container render objects).
    ///
    /// Default implementation returns empty slice (leaf nodes).
    fn children(&self) -> &[RenderObjectId] {
        &[]
    }

    /// Get as Any for downcasting.
    ///
    /// This enables runtime type inspection for container-specific operations.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get as Any for mutable downcasting.
    ///
    /// This enables runtime type inspection for container-specific operations.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ============================================================================
// RENDER OBJECT REGISTRY
// ============================================================================

/// Registry for render objects, keyed by ID.
///
/// The registry owns all render objects and maintains the relationship
/// between render objects and their owning elements.
///
/// # Thread Safety
///
/// The registry is not thread-safe. It should only be accessed from the
/// main thread where rendering occurs.
///
/// # Example
///
/// ```ignore
/// let mut registry = RenderObjectRegistry::new();
///
/// // Create a render object
/// let element_id = ElementId::new();
/// let obj = Box::new(MyRenderObject::new());
/// let obj_id = registry.create(obj, element_id);
///
/// // Access the render object
/// if let Some(obj) = registry.get(obj_id) {
///     // Use the render object
/// }
///
/// // Remove when the element is unmounted
/// registry.remove(obj_id);
/// ```
pub struct RenderObjectRegistry {
    objects: HashMap<RenderObjectId, Box<dyn RenderObject>>,
    element_map: HashMap<RenderObjectId, ElementId>,
    root: Option<RenderObjectId>,
}

impl RenderObjectRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
            element_map: HashMap::new(),
            root: None,
        }
    }

    /// Create a render object and return its ID.
    ///
    /// The render object is associated with the given element ID.
    /// This association is used during reconciliation to find render objects
    /// that correspond to elements.
    pub fn create(&mut self, object: Box<dyn RenderObject>, owner: ElementId) -> RenderObjectId {
        let id = RenderObjectId::new();
        self.objects.insert(id, object);
        self.element_map.insert(id, owner);
        id
    }

    /// Get a render object by ID.
    ///
    /// Returns None if the ID is not valid.
    pub fn get(&self, id: RenderObjectId) -> Option<&dyn RenderObject> {
        self.objects.get(&id).map(|b| b.as_ref())
    }

    /// Get a mutable render object by ID.
    ///
    /// Returns None if the ID is not valid.
    pub fn get_mut(&mut self, id: RenderObjectId) -> Option<&mut Box<dyn RenderObject>> {
        self.objects.get_mut(&id)
    }

    /// Remove a render object by ID.
    ///
    /// Does nothing if the ID is not valid.
    pub fn remove(&mut self, id: RenderObjectId) {
        self.objects.remove(&id);
        self.element_map.remove(&id);
    }

    /// Set the root render object.
    ///
    /// The root is the top-level render object that contains all others.
    pub fn set_root(&mut self, id: RenderObjectId) {
        self.root = Some(id);
    }

    /// Get the root render object ID.
    ///
    /// Returns None if no root has been set.
    pub fn root(&self) -> Option<RenderObjectId> {
        self.root
    }

    /// Get the element that owns a render object.
    ///
    /// Returns None if the ID is not valid.
    pub fn element_for(&self, id: RenderObjectId) -> Option<ElementId> {
        self.element_map.get(&id).copied()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Get the number of render objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Clear all render objects.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.element_map.clear();
        self.root = None;
    }
}

impl Default for RenderObjectRegistry {
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
    use crate::core::{Bounds, Logical, Point, Size};
    use crate::layout::LayoutConstraints;

    /// Mock render object for testing.
    struct MockRenderObject {
        layout_count: std::cell::Cell<usize>,
    }

    impl RenderObject for MockRenderObject {
        fn layout(&mut self, _constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
            self.layout_count.set(self.layout_count.get() + 1);
            Size::new(100.0, 50.0)
        }

        fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
            vec![]
        }

        fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
            true
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_registry_create() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        assert!(registry.get(id).is_some());
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        registry.remove(id);

        assert!(registry.get(id).is_none());
    }

    #[test]
    fn test_registry_element_for() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        assert_eq!(registry.element_for(id), Some(element_id));
    }

    #[test]
    fn test_registry_root() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        assert!(registry.root().is_none());

        registry.set_root(id);
        assert_eq!(registry.root(), Some(id));
    }

    #[test]
    fn test_registry_len() {
        let mut registry = RenderObjectRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        let element_id1 = ElementId::new();
        let element_id2 = ElementId::new();

        let obj1 = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let obj2 = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });

        registry.create(obj1, element_id1);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.create(obj2, element_id2);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);
        registry.set_root(id);

        registry.clear();

        assert!(registry.is_empty());
        assert!(registry.root().is_none());
    }

    #[test]
    fn test_layout_context_mock() {
        let ctx = LayoutContext::mock();
        // Just verify it can be created
        let _ = ctx;
    }

    #[test]
    fn test_paint_context() {
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);

        assert_eq!(ctx.offset().x, 0.0);
        assert_eq!(ctx.offset().y, 0.0);

        ctx.set_offset(Point::new(10.0, 20.0));
        assert_eq!(ctx.offset().x, 10.0);
        assert_eq!(ctx.offset().y, 20.0);

        ctx.push_command(RenderCommand::rect(
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            crate::core::Color::RED,
        ));
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn test_hit_test_context_mock() {
        let ctx = HitTestContext::mock();
        // Just verify it can be created
        let _ = ctx;
    }
}
