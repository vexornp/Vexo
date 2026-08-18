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

use slotmap::SlotMap;

use crate::core::{Point, Size};
use crate::input::MouseTrackerAnnotation;
use crate::layout::{LayoutEngine, LayoutNodeKey};
use crate::render::RenderCommand;

use super::id::{ElementKey, RenderObjectKey};

// ============================================================================
// LAYOUT RESULT
// ============================================================================

/// Result of a RenderObject's layout operation.
///
/// Contains the Taffy node ID and computed size.
#[derive(Debug)]
pub struct LayoutResult {
    /// The Taffy node ID for this render object.
    pub node: LayoutNodeKey,
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
        Self {
            engine,
            font_system,
        }
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
    pub fn absolute_position(
        &self,
    ) -> crate::core::Position<crate::core::Logical, crate::core::Absolute> {
        self.absolute_position
    }

    /// Set the absolute position (used by pipeline during traversal).
    pub fn set_absolute_position(
        &mut self,
        position: crate::core::Position<crate::core::Logical, crate::core::Absolute>,
    ) {
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
    /// - For containers (Flex): `child_nodes` has multiple elements, create container
    ///
    /// Returns a LayoutResult containing the node ID and size.
    /// The render object should store the node ID for later use in apply_layout().
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult;

    /// Apply computed layout from Taffy.
    ///
    /// Called after Taffy::compute() to read back computed bounds.
    /// The render object should read its layout from the engine and update computed_bounds.
    fn apply_layout(&mut self, ctx: &mut LayoutContext);

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
    fn children(&self) -> &[RenderObjectKey] {
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
    fn set_child_id(&mut self, _child: RenderObjectKey) {
        // Default: no-op (leaf nodes and multi-children containers don't use this)
    }

    /// Add a child render object ID.
    ///
    /// Only relevant for container render objects (e.g., Flex).
    /// Default implementation does nothing. This enables linking the render tree
    /// so that paint_recursive() can traverse to children.
    fn add_child(&mut self, _child: RenderObjectKey) {
        // Default: no-op (leaf nodes and single-child modifiers don't use this)
    }

    /// Replace an existing child render object with a new one at the same position.
    ///
    /// Called during element replacement (`replace_element`) when a child element
    /// changes type (e.g., Column → Text). The old child's render object has been
    /// removed from the registry; this method swaps the stale key for the new key
    /// in the parent's children list, preserving position so layout order is correct.
    ///
    /// Default implementation appends the new child (fallback for render objects
    /// that don't track ordered children). Container render objects override this
    /// to replace in-place.
    fn replace_child(&mut self, _old: RenderObjectKey, _new: RenderObjectKey) {
        // Default: no-op (leaf nodes and single-child modifiers don't use this)
    }

    /// Remove a child render object reference.
    ///
    /// Called when a child element unmounts. The parent RO must drop its
    /// reference to the child's `RenderObjectKey` and invalidate any cached
    /// child layout node, so the next layout pass doesn't try to read a
    /// removed RO or use a stale Taffy node.
    ///
    /// Default implementation does nothing (leaf nodes have no children).
    fn remove_child(&mut self, _child: RenderObjectKey) {
        // Default: no-op (leaf nodes don't have children)
    }

    /// Clear all children.
    ///
    /// Only relevant for container render objects (e.g., Flex).
    /// Default implementation does nothing.
    fn clear_children(&mut self) {
        // Default: no-op (leaf nodes and single-child modifiers don't use this)
    }

    /// Get the layout node ID (for pipeline to use).
    ///
    /// Returns the Taffy node ID that was created during layout().
    /// Used by the pipeline to call engine.compute() on the root node.
    fn layout_node(&self) -> Option<LayoutNodeKey> {
        None
    }

    /// Get the computed bounds after layout.
    ///
    /// Returns `None` if layout has not been applied yet.
    /// Used by event handling to determine element bounds.
    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        None
    }

    /// Get the paint transform for this render object, if any.
    ///
    /// The pipeline uses this to wrap children's paint commands with
    /// `PushTransform`/`PopTransform`. This is necessary because children
    /// are painted separately from the parent's `paint()` method.
    fn paint_transform(&self) -> Option<crate::core::AffineTransform> {
        None
    }

    /// Get the transform to apply for hit testing, if any.
    ///
    /// When present, the inverse transform is applied to the pointer position
    /// before testing children. This allows rotated/scaled objects to receive
    /// hit events in their transformed coordinate space.
    fn hit_test_transform(&self) -> Option<crate::core::AffineTransform> {
        None
    }

    /// Get the clip bounds for this render object's children, if any.
    ///
    /// When present, the painter emits `PushClip`/`PopClip` around
    /// this object's children. The bounds should be in the object's
    /// local coordinate space (the painter converts to absolute).
    fn clip_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        None
    }

    /// Get the corner radius for this render object's clip, if any.
    ///
    /// When present (and > 0.0), the painter emits `PushClipRRect`/
    /// `PopClipRRect` around this object's children instead of the
    /// plain `PushClip`/`PopClip`. The radius is applied as an SDF
    /// mask in the fragment shader on top of the rectangular scissor
    /// clip from `clip_bounds()`.
    ///
    /// Return `None` (the default) for plain rectangular clipping.
    /// Return `Some(r)` only when `r > 0.0`.
    fn clip_corner_radius(&self) -> Option<f32> {
        None
    }

    /// Get the scroll offset for this render object's children, if any.
    ///
    /// When present, the painter emits `PushOffset`/`PopOffset` around
    /// this object's children. The offset is in the object's local
    /// coordinate space.
    fn scroll_offset(&self) -> Option<crate::core::Point<crate::core::Logical>> {
        None
    }

    /// Get the opacity for this render object, if any.
    ///
    /// When present, the painter emits `PushOpacity`/`PopOpacity` around
    /// this object's children. The opacity value (0.0..1.0) is multiplied
    /// into the alpha of all descendant colors.
    fn opacity(&self) -> Option<f32> {
        None
    }

    /// Whether this render object is a layout pass-through.
    ///
    /// Pass-through ROs (`Opacity`, `Transform`, `Offstage`-onstage) do NOT
    /// own a Taffy node. Their `layout_node()` returns the child's node, so
    /// the layouter links the grandparent directly to the grandchild.
    /// `is_pass_through() == true` tells the registry to skip orphan-node
    /// cleanup on removal (the child owns the node).
    ///
    /// Default: `false` (normal ROs own their Taffy node).
    fn is_pass_through(&self) -> bool {
        false
    }

    /// Get the image data that needs registration in the atlas, if any.
    ///
    /// Returns `Some(&ImageData)` when this render object has image data
    /// but has not yet been assigned an `ImageKey` by the pipeline.
    /// The pipeline calls this during the image registration pass and
    /// then calls `set_image_key()` with the resulting key.
    fn needs_image_registration(&self) -> Option<&crate::image_data::ImageData> {
        None
    }

    /// Set the atlas key for this render object's image.
    ///
    /// Called by the pipeline after registering the image data with the
    /// backend. The render object stores the key for use during paint()
    /// to emit `RenderCommand::Image`.
    fn set_image_key(&mut self, _key: crate::image_atlas::ImageKey) {}

    /// Get the atlas key currently assigned to this render object's image, if any.
    ///
    /// The registry calls this during `remove()` to collect keys that must be
    /// returned to the backend via `unregister_image`. Without this, popping a
    /// route that contains images leaks their atlas slots forever and the
    /// 2048x2048 atlas fills up after a few dozen push/pop cycles on iOS.
    fn image_key(&self) -> Option<crate::image_atlas::ImageKey> {
        None
    }
}

// ============================================================================
// RENDER OBJECT REGISTRY
// ============================================================================

/// Per-render-object metadata stored alongside the object itself.
///
/// Co-locating `object`, `owner`, and `cursor_annotation` in one entry
/// keeps identity and per-object side data in sync structurally: `remove`
/// drops a single slot and all three fields die atomically, so there is
/// no parallel set of SecondaryMaps that can drift out of sync.
struct RenderObjectEntry {
    object: Box<dyn RenderObject>,
    /// The element that owns this render object (cross-tree link).
    owner: ElementKey,
    /// Cursor annotation, present only for MouseRegion render objects.
    /// `None` for the vast majority of entries — equivalent to the old
    /// SecondaryMap's implicit-absent behavior.
    cursor_annotation: Option<MouseTrackerAnnotation>,
}

/// Registry that manages render objects using generational keys.
///
/// Uses SlotMap for primary storage (provides ABA protection via generational
/// indices). The cross-tree element mapping and per-object cursor annotation
/// are co-located with each object in `RenderObjectEntry`.
pub struct RenderObjectRegistry {
    objects: SlotMap<RenderObjectKey, RenderObjectEntry>,
    root: Option<RenderObjectKey>,
    /// Layout node keys orphaned by removed render objects.
    /// Drained during layout to remove nodes from the Taffy engine.
    orphaned_layout_nodes: Vec<LayoutNodeKey>,
    /// Image atlas keys orphaned by removed render objects.
    /// Drained by the pipeline's image pass to call `unregister_image` on the
    /// backend, returning the slot to the atlas free list.
    orphaned_image_keys: Vec<crate::image_atlas::ImageKey>,
}

impl RenderObjectRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            objects: SlotMap::with_key(),
            root: None,
            orphaned_layout_nodes: Vec::new(),
            orphaned_image_keys: Vec::new(),
        }
    }

    /// Create a render object and return its key.
    ///
    /// The render object is associated with the given element ID.
    /// This association is used during reconciliation to find render objects
    /// that correspond to elements.
    pub fn create(&mut self, object: Box<dyn RenderObject>, owner: ElementKey) -> RenderObjectKey {
        self.objects.insert(RenderObjectEntry {
            object,
            owner,
            cursor_annotation: None,
        })
    }

    /// Get a render object by key.
    ///
    /// Returns None if the key is stale (element was removed).
    pub fn get(&self, key: RenderObjectKey) -> Option<&dyn RenderObject> {
        self.objects.get(key).map(|e| e.object.as_ref())
    }

    /// Get a mutable render object by key.
    ///
    /// Returns None if the key is stale (element was removed).
    pub fn get_mut(&mut self, key: RenderObjectKey) -> Option<&mut Box<dyn RenderObject>> {
        self.objects.get_mut(key).map(|e| &mut e.object)
    }

    /// Remove a render object by key.
    ///
    /// After removal, the key becomes stale — any future access returns None.
    /// This provides ABA protection: a new render object at the same slot
    /// will have a different generation.
    ///
    /// The render object's layout node key (if any) is collected for later
    /// cleanup during the layout pass.
    pub fn remove(&mut self, key: RenderObjectKey) {
        if let Some(entry) = self.objects.get(key) {
            let obj = &entry.object;
            if !obj.is_pass_through() {
                if let Some(node) = obj.layout_node() {
                    self.orphaned_layout_nodes.push(node);
                }
            }
            // Collect the atlas key so the pipeline can return the slot to the
            // backend's image atlas. Without this, removing an image render
            // object (e.g. during iOS pop) leaks the atlas slot permanently.
            if let Some(img_key) = obj.image_key() {
                self.orphaned_image_keys.push(img_key);
            }
        }
        // Single removal frees object, owner, and cursor_annotation atomically.
        self.objects.remove(key);
    }

    /// Set the root render object.
    pub fn set_root(&mut self, key: RenderObjectKey) {
        self.root = Some(key);
    }

    /// Get the root render object key.
    pub fn root(&self) -> Option<RenderObjectKey> {
        self.root
    }

    /// Get the element that owns a render object.
    pub fn element_for(&self, key: RenderObjectKey) -> Option<ElementKey> {
        self.objects.get(key).map(|e| e.owner)
    }

    /// Get the render object owned by an element.
    pub fn render_object_for_element(&self, element_key: ElementKey) -> Option<RenderObjectKey> {
        for (ro_key, entry) in &self.objects {
            if entry.owner == element_key {
                return Some(ro_key);
            }
        }
        None
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Get the number of render objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Iterate over all render object keys.
    pub fn keys(&self) -> impl Iterator<Item = RenderObjectKey> + '_ {
        self.objects.keys()
    }

    /// Iterate mutably over all render objects.
    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (RenderObjectKey, &mut Box<dyn RenderObject>)> {
        self.objects.iter_mut().map(|(k, e)| (k, &mut e.object))
    }

    /// Clear all render objects.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.orphaned_layout_nodes.clear();
        self.orphaned_image_keys.clear();
        self.root = None;
    }

    /// Drain all orphaned layout node keys for cleanup.
    ///
    /// Called by the layouter during layout to remove orphaned nodes
    /// from the Taffy engine.
    pub fn drain_orphaned_layout_nodes(&mut self) -> Vec<LayoutNodeKey> {
        std::mem::take(&mut self.orphaned_layout_nodes)
    }

    /// Drain all orphaned image atlas keys for cleanup.
    ///
    /// Called by the pipeline's image pass to call `unregister_image` on the
    /// backend, returning each slot to the atlas free list. Pair this with
    /// `register_images` so a single frame reclaims removed slots before
    /// carving new ones.
    pub fn drain_orphaned_image_keys(&mut self) -> Vec<crate::image_atlas::ImageKey> {
        std::mem::take(&mut self.orphaned_image_keys)
    }

    /// Set the child render object for a parent.
    ///
    /// Calls both `set_child_id` and `add_child` on the parent so this works
    /// for single-child render objects (which override `set_child_id`) and
    /// multi-child containers (which override `add_child`). The non-overridden
    /// method is a no-op by default, so calling both is safe. This mirrors
    /// `RenderObjectElement::insert_child_render_object`.
    pub fn set_child(&mut self, parent: RenderObjectKey, child: RenderObjectKey) {
        if let Some(entry) = self.objects.get_mut(parent) {
            entry.object.set_child_id(child);
            entry.object.add_child(child);
        }
    }

    /// Set a cursor annotation on a render object.
    ///
    /// MouseRegion elements call this during mount to register their
    /// annotation (cursor intent + enter/exit callbacks).
    pub fn set_cursor_annotation(
        &mut self,
        key: RenderObjectKey,
        annotation: MouseTrackerAnnotation,
    ) {
        if let Some(entry) = self.objects.get_mut(key) {
            entry.cursor_annotation = Some(annotation);
        }
    }

    /// Get the cursor annotation for a render object.
    ///
    /// Returns None if no annotation was registered (most render objects
    /// have no annotation — only MouseRegion render objects carry one).
    pub fn cursor_annotation(&self, key: RenderObjectKey) -> Option<&MouseTrackerAnnotation> {
        self.objects
            .get(key)
            .and_then(|e| e.cursor_annotation.as_ref())
    }

    /// Get the cursor annotation for an element.
    ///
    /// Used by hover dispatch to look up on_exit callbacks for elements
    /// leaving hover (which are no longer in the hit path).
    pub fn cursor_annotation_for_element(
        &self,
        element_key: ElementKey,
    ) -> Option<&MouseTrackerAnnotation> {
        for (_ro_key, entry) in &self.objects {
            if entry.owner == element_key {
                return entry.cursor_annotation.as_ref();
            }
        }
        None
    }

    /// Remove the cursor annotation for a render object.
    ///
    /// MouseRegion elements call this during unmount.
    pub fn remove_cursor_annotation(&mut self, key: RenderObjectKey) {
        if let Some(entry) = self.objects.get_mut(key) {
            entry.cursor_annotation = None;
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
        fn layout(
            &mut self,
            _ctx: &mut LayoutContext,
            _child_nodes: &[LayoutNodeKey],
        ) -> LayoutResult {
            self.layout_count.set(self.layout_count.get() + 1);
            // Return a dummy result for registry testing
            unimplemented!("MockRenderObject::layout requires a real LayoutEngine")
        }

        fn apply_layout(&mut self, _ctx: &mut LayoutContext) {
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
    fn test_registry_create() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        assert!(registry.get(id).is_some());
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

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
        let element_id = make_element_key();

        let obj = Box::new(MockRenderObject {
            layout_count: std::cell::Cell::new(0),
        });
        let id = registry.create(obj, element_id);

        assert_eq!(registry.element_for(id), Some(element_id));
    }

    #[test]
    fn test_registry_root() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

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

        let (element_id1, element_id2) = make_two_element_keys();

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
        let element_id = make_element_key();

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
        use crate::core::Position;
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);

        assert_eq!(ctx.absolute_position().x, 0.0);
        assert_eq!(ctx.absolute_position().y, 0.0);

        ctx.set_absolute_position(Position::new(10.0, 20.0));
        assert_eq!(ctx.absolute_position().x, 10.0);
        assert_eq!(ctx.absolute_position().y, 20.0);

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
    fn test_render_object_opacity_default() {
        struct TestRO;
        impl RenderObject for TestRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
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
        let ro = TestRO;
        assert!(ro.opacity().is_none());
    }

    #[test]
    fn test_render_object_clip_corner_radius_default_none() {
        struct TestRO;
        impl RenderObject for TestRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
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
        let ro = TestRO;
        assert!(
            ro.clip_corner_radius().is_none(),
            "clip_corner_radius() must default to None"
        );
    }

    #[test]
    fn test_render_object_is_pass_through_default() {
        struct TestRO;
        impl RenderObject for TestRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
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
        let ro = TestRO;
        assert!(!ro.is_pass_through());
    }

    #[test]
    fn test_registry_set_child() {
        // Create a mock render object that supports set_child_id
        struct MockParentObject {
            child: Option<RenderObjectKey>,
        }

        impl RenderObject for MockParentObject {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!("MockParentObject::layout requires a real LayoutEngine")
            }

            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {
                unimplemented!("MockParentObject::apply_layout requires a real LayoutEngine")
            }

            fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
                vec![]
            }

            fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
                true
            }

            fn children(&self) -> &[RenderObjectKey] {
                static EMPTY: &[RenderObjectKey] = &[];
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

            fn set_child_id(&mut self, child: RenderObjectKey) {
                self.child = Some(child);
            }
        }

        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        let parent = Box::new(MockParentObject { child: None });
        let parent_id = registry.create(parent, element_id);

        // Initially no children
        let parent_obj = registry.get(parent_id).unwrap();
        assert_eq!(parent_obj.children().len(), 0);

        // Set a child - create a dummy key via a temporary SlotMap
        let mut dummy_sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child_id = dummy_sm.insert(());

        registry.set_child(parent_id, child_id);

        // Now the parent should have the child
        let parent_obj = registry.get(parent_id).unwrap();
        let children = parent_obj.children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], child_id);
    }

    #[test]
    fn test_registry_remove_skips_passthrough_cleanup() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        struct MockPassthroughRO;
        impl RenderObject for MockPassthroughRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
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
            fn is_pass_through(&self) -> bool {
                true
            }
            fn layout_node(&self) -> Option<LayoutNodeKey> {
                let mut sm: slotmap::SlotMap<LayoutNodeKey, ()> = slotmap::SlotMap::with_key();
                Some(sm.insert(()))
            }
        }

        let obj = Box::new(MockPassthroughRO);
        let id = registry.create(obj, element_id);
        registry.remove(id);

        let orphaned = registry.drain_orphaned_layout_nodes();
        assert!(
            orphaned.is_empty(),
            "pass-through RO removal must not orphan the borrowed child node"
        );
    }

    #[test]
    fn test_registry_remove_collects_normal_ro_node() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        struct MockOwnerRO {
            node: Option<LayoutNodeKey>,
        }
        impl RenderObject for MockOwnerRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
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
            fn layout_node(&self) -> Option<LayoutNodeKey> {
                self.node
            }
        }

        let mut node_sm: slotmap::SlotMap<LayoutNodeKey, ()> = slotmap::SlotMap::with_key();
        let owned_node = node_sm.insert(());
        let obj = Box::new(MockOwnerRO {
            node: Some(owned_node),
        });
        let id = registry.create(obj, element_id);
        registry.remove(id);

        let orphaned = registry.drain_orphaned_layout_nodes();
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0], owned_node);
    }
}
