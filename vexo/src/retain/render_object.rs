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
use crate::layout::{LayoutEngine, LayoutNodeId};
use crate::render::RenderCommand;

use super::id::{ElementId, RenderObjectId};

// ============================================================================
// LAYOUT RESULT
// ============================================================================

/// Result of a RenderObject's layout operation.
///
/// Contains the Taffy node ID and computed size.
#[derive(Debug)]
pub struct LayoutResult {
    /// The Taffy node ID for this render object.
    pub node: LayoutNodeId,
    /// The computed size (available after Taffy computation).
    pub size: Size<crate::core::Logical>,
}

// ============================================================================
// LAYOUT CONTEXT
// ============================================================================

/// Context passed to RenderObject.layout().
///
/// Provides access to the layout engine and font system for text measurement.
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
    font_system: &'a mut glyphon::FontSystem,
}

impl<'a> LayoutContext<'a> {
    /// Create a new layout context.
    pub fn new(engine: &'a mut dyn LayoutEngine, font_system: &'a mut glyphon::FontSystem) -> Self {
        Self { engine, font_system }
    }

    /// Get the layout engine (mutable for creating nodes).
    pub fn engine(&mut self) -> &mut dyn LayoutEngine {
        self.engine
    }

    /// Get the layout engine (immutable for reading computed layouts).
    pub fn engine_ref(&self) -> &dyn LayoutEngine {
        self.engine
    }

    /// Get the font system.
    pub fn font_system(&mut self) -> &mut glyphon::FontSystem {
        self.font_system
    }
}

// ============================================================================
// PAINT CONTEXT
// ============================================================================

/// Context passed to RenderObject.paint().
///
/// Provides access to the render command list and the absolute position
/// where this render object should paint.
///
/// # Coordinate System
///
/// The paint context uses **absolute coordinates** (relative to the window origin).
/// Render objects should paint at the position returned by `absolute_position()`.
///
/// # How Position is Calculated
///
/// 1. The pipeline's `paint_recursive` traverses the render object tree
/// 2. For each render object, it calculates the absolute position by:
///    - Starting from the parent's absolute position
///    - Adding this object's position within its parent (from computed_bounds)
/// 3. This absolute position is passed to the render object via `set_absolute_position`
///
/// # Render Object Responsibility
///
/// Render objects should:
/// - Use `absolute_position()` to get where they should paint
/// - Use `computed_bounds()` only for size information (width, height)
/// - NOT add bounds.left/top to the position - that's already included
///
/// # Example
///
/// ```ignore
/// fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
///     let bounds = self.computed_bounds?;
///     let pos = ctx.absolute_position(); // Already includes bounds.position()
///
///     // Create absolute bounds at the correct position
///     let absolute_bounds = Bounds::new(
///         pos.x, pos.y,
///         pos.x + bounds.width(), pos.y + bounds.height(),
///     );
///
///     vec![RenderCommand::rect(absolute_bounds, self.color)]
/// }
/// ```
pub struct PaintContext<'a> {
    /// The absolute position where this render object should paint.
    /// This is the top-left corner in window coordinates.
    absolute_position: crate::core::Position<crate::core::Logical, crate::core::Absolute>,
    commands: &'a mut Vec<RenderCommand>,
}

impl<'a> PaintContext<'a> {
    /// Create a new paint context starting at the origin.
    pub fn new(commands: &'a mut Vec<RenderCommand>) -> Self {
        Self {
            absolute_position: crate::core::Position::zero(),
            commands,
        }
    }

    /// Push a render command.
    pub fn push_command(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    /// Get the absolute position where this render object should paint.
    ///
    /// This is the top-left corner of this render object in window coordinates.
    /// The position already includes the render object's position within its parent.
    pub fn absolute_position(&self) -> crate::core::Position<crate::core::Logical, crate::core::Absolute> {
        self.absolute_position
    }

    /// Set the absolute position (used by pipeline during traversal).
    pub fn set_absolute_position(&mut self, position: crate::core::Position<crate::core::Logical, crate::core::Absolute>) {
        self.absolute_position = position;
    }

    // Legacy alias for backwards compatibility during migration
    #[deprecated(note = "Use absolute_position() instead for clarity")]
    pub fn offset(&self) -> crate::core::Point<crate::core::Logical> {
        self.absolute_position.to_point()
    }

    // Legacy alias for backwards compatibility during migration
    #[deprecated(note = "Use set_absolute_position() instead for clarity")]
    pub fn set_offset(&mut self, offset: crate::core::Point<crate::core::Logical>) {
        self.absolute_position = crate::core::Position::new(offset.x, offset.y);
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
/// The `layout` method is called with a `LayoutContext` that provides access
/// to the layout engine. The render object creates Taffy node(s) and returns
/// a `LayoutResult` containing the node ID.
///
/// The `apply_layout` method is called after Taffy::compute() to read back
/// computed bounds from the engine.
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
pub trait RenderObject {
    /// Perform layout with the layout engine, creating Taffy node(s).
    ///
    /// This method creates the Taffy node for this render object.
    /// The pipeline handles calling this method on children first (bottom-up),
    /// then passes child node IDs to the parent.
    ///
    /// - For leaf nodes (Text): `child_nodes` is empty, create a leaf node
    /// - For modifiers (Background, Border): `child_nodes` has one element, pass through
    /// - For containers (Column, Row): `child_nodes` has multiple elements, create container
    ///
    /// Returns a LayoutResult containing the node ID and size.
    /// The render object should store the node ID for later use in apply_layout().
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult;

    /// Apply computed layout from Taffy.
    ///
    /// Called after Taffy::compute() to read back computed bounds.
    /// The render object should read its layout from the engine and update computed_bounds.
    fn apply_layout(&mut self, ctx: &LayoutContext);

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

    /// Set a child render object ID.
    ///
    /// Only relevant for modifier render objects (e.g., Background, Padding, Border).
    /// Default implementation does nothing. This enables linking the render tree
    /// so that paint_recursive() can traverse to children.
    fn set_child_id(&mut self, _child: RenderObjectId) {
        // Default: no-op (leaf nodes and multi-children containers don't use this)
    }

    /// Add a child render object ID.
    ///
    /// Only relevant for container render objects (e.g., Column, Row).
    /// Default implementation does nothing. This enables linking the render tree
    /// so that paint_recursive() can traverse to children.
    fn add_child(&mut self, _child: RenderObjectId) {
        // Default: no-op (leaf nodes and single-child modifiers don't use this)
    }

    /// Get the layout node ID (for pipeline to use).
    ///
    /// Returns the Taffy node ID that was created during layout().
    /// Used by the pipeline to call engine.compute() on the root node.
    fn layout_node(&self) -> Option<LayoutNodeId> {
        None
    }

    /// Get the computed bounds after layout.
    ///
    /// Returns `None` if layout has not been applied yet.
    /// Used by event handling to determine element bounds.
    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        None
    }
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

    /// Set the child render object for a parent.
    ///
    /// This is used by modifier elements to link their render object
    /// to their child's render object for tree traversal.
    pub fn set_child(&mut self, parent: RenderObjectId, child: RenderObjectId) {
        if let Some(obj) = self.objects.get_mut(&parent) {
            obj.set_child_id(child);
        }
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
    use crate::core::{Bounds, Logical, Point};

    /// Mock render object for testing registry operations.
    /// Note: layout() and apply_layout() use unimplemented!() as they're never called
    /// in registry tests. Full layout tests require a mock LayoutEngine.
    struct MockRenderObject {
        layout_count: std::cell::Cell<usize>,
    }

    impl RenderObject for MockRenderObject {
        fn layout(&mut self, _ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeId]) -> LayoutResult {
            self.layout_count.set(self.layout_count.get() + 1);
            // Return a dummy result for registry testing
            unimplemented!("MockRenderObject::layout requires a real LayoutEngine")
        }

        fn apply_layout(&mut self, _ctx: &LayoutContext) {
            unimplemented!("MockRenderObject::apply_layout requires a real LayoutEngine")
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

    #[test]
    fn test_registry_set_child() {
        // Create a mock render object that supports set_child_id
        struct MockParentObject {
            child: Option<RenderObjectId>,
        }

        impl RenderObject for MockParentObject {
            fn layout(&mut self, _ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeId]) -> LayoutResult {
                unimplemented!("MockParentObject::layout requires a real LayoutEngine")
            }

            fn apply_layout(&mut self, _ctx: &LayoutContext) {
                unimplemented!("MockParentObject::apply_layout requires a real LayoutEngine")
            }

            fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
                vec![]
            }

            fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
                true
            }

            fn children(&self) -> &[RenderObjectId] {
                static EMPTY: &[RenderObjectId] = &[];
                match &self.child {
                    Some(child) => std::slice::from_ref(child),
                    None => EMPTY,
                }
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }

            fn set_child_id(&mut self, child: RenderObjectId) {
                self.child = Some(child);
            }
        }

        let mut registry = RenderObjectRegistry::new();
        let element_id = ElementId::new();

        let parent = Box::new(MockParentObject { child: None });
        let parent_id = registry.create(parent, element_id);

        // Initially no children
        let parent_obj = registry.get(parent_id).unwrap();
        assert_eq!(parent_obj.children().len(), 0);

        // Set a child
        let child_id = RenderObjectId::new();
        registry.set_child(parent_id, child_id);

        // Now the parent should have the child
        let parent_obj = registry.get(parent_id).unwrap();
        let children = parent_obj.children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], child_id);
    }
}
