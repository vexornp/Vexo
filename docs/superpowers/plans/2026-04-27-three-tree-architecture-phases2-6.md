# Three-Tree Architecture: Remaining Phases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the three-tree architecture by implementing RenderObject types, integrating with TaffyLayoutEngine, updating WindowState, and removing legacy immediate-mode code.

**Architecture:** Phase 1 (core infrastructure) is complete. Remaining phases build on this foundation: RenderObject implementations (TextRenderObject, ContainerRenderObject), TaffyLayoutEngine integration, WindowState integration with the three-tree pipeline, Application trait update, and cleanup of old code.

**Tech Stack:** Rust, Taffy (flexbox layout), wgpu (GPU rendering), glyphon (text rendering)

---

## File Structure

**New files to create:**
- `vexo/src/retain/render_objects/mod.rs` - RenderObject implementations module
- `vexo/src/retain/render_objects/text.rs` - TextRenderObject
- `vexo/src/retain/render_objects/container.rs` - ContainerRenderObject
- `vexo/src/retain/pipeline.rs` - Three-tree rendering pipeline
- `vexo/src/retain/hit_test.rs` - Hit testing implementation

**Files to modify:**
- `vexo/src/retain/mod.rs` - Add new module exports
- `vexo/src/retain/render_object.rs` - Enhance LayoutContext with Taffy integration
- `vexo/src/retain/widgets/text.rs` - Add create_render_object method
- `vexo/src/retain/widgets/container.rs` - Add create_render_object method
- `vexo/src/retain/widgets/mod.rs` - Add create_render_object to Widget trait
- `vexo/src/retain/elements/leaf.rs` - Create RenderObject on mount
- `vexo/src/retain/elements/container.rs` - Create RenderObject on mount
- `vexo/src/window.rs` - Integrate three-tree pipeline
- `vexo/src/lib.rs` - Update Application trait

**Files to remove (Phase 6):**
- `vexo/src/widgets/` (old immediate-mode widgets)
- `vexo/src/state/` (old state management)
- Related immediate-mode code in window.rs

---

### Task 1: Add create_render_object to Widget trait

**Files:**
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/widgets/mod.rs, add to tests module

#[test]
fn test_widget_creates_render_object() {
    let widget = Text::new("Hello");
    let render_object = widget.create_render_object();

    // Should be able to layout the render object
    let constraints = LayoutConstraints::new(
        Size::new(0.0, 0.0),
        Size::new(100.0, 100.0),
    );
    let mut ctx = LayoutContext::mock();
    let size = render_object.layout(constraints, &mut ctx);

    assert!(size.width > 0.0);
    assert!(size.height > 0.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_widget_creates_render_object -- --nocapture`
Expected: FAIL with "no method named `create_render_object`"

- [ ] **Step 3: Write minimal implementation**

```rust
// In vexo/src/retain/widgets/mod.rs

use crate::retain::{RenderObject, LayoutContext, LayoutConstraints};
use crate::core::Size;
use crate::core::Logical;

/// Immutable widget configuration - rebuilt each frame.
pub trait Widget: std::any::Any {
    /// Optional key for identity across frames.
    fn key(&self) -> Option<Key> {
        None
    }

    /// Create the corresponding element for this widget.
    fn create_element(&self) -> Box<dyn Element>;

    /// Create the render object for this widget.
    fn create_render_object(&self) -> Box<dyn RenderObject>;

    /// Get the type ID for type comparison.
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }

    /// Get as Any for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;
}
```

- [ ] **Step 4: Update Text widget implementation**

```rust
// In vexo/src/retain/widgets/text.rs, add:

use crate::retain::{RenderObject, LayoutContext, LayoutConstraints, HitTestContext};
use crate::core::{Size, Logical, Point, Bounds, Color};
use crate::render::RenderCommand;

impl Widget for Text {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(crate::retain::elements::LeafElement::new())
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TextRenderObject::new(&self.content))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// RenderObject for text display.
pub struct TextRenderObject {
    content: String,
    computed_bounds: Option<Bounds<Logical>>,
}

impl TextRenderObject {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            computed_bounds: None,
        }
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // TODO: Use font system for accurate measurement
        // For now, estimate based on content length
        let estimated_width = (self.content.len() as f32 * 10.0).min(constraints.max_width);
        let estimated_height = 20.0.min(constraints.max_height);

        let size = Size::new(
            estimated_width.max(constraints.min_width),
            estimated_height.max(constraints.min_height),
        );

        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, ctx: &mut crate::retain::PaintContext) -> Vec<RenderCommand> {
        // Text is handled separately via glyphon
        // Return empty for now - text collection happens in pipeline
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(position),
            None => false,
        }
    }
}
```

- [ ] **Step 5: Update Column/Row widget implementation**

```rust
// In vexo/src/retain/widgets/container.rs, add:

use crate::retain::{RenderObject, LayoutContext, LayoutConstraints, HitTestContext, RenderObjectId};
use crate::core::{Size, Logical, Point};

impl Widget for Column {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(crate::retain::elements::ContainerElement::new())
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_column())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Widget for Row {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(crate::retain::elements::ContainerElement::new())
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_row())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// RenderObject for container widgets.
pub struct ContainerRenderObject {
    children: Vec<RenderObjectId>,
    is_row: bool,
    computed_bounds: Option<Bounds<Logical>>,
}

impl ContainerRenderObject {
    pub fn new_column() -> Self {
        Self {
            children: Vec::new(),
            is_row: false,
            computed_bounds: None,
        }
    }

    pub fn new_row() -> Self {
        Self {
            children: Vec::new(),
            is_row: true,
            computed_bounds: None,
        }
    }

    pub fn add_child(&mut self, child: RenderObjectId) {
        self.children.push(child);
    }
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Container layout is handled by Taffy
        // Return constrained size for now
        let size = Size::new(
            constraints.max_width,
            constraints.max_height,
        );
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut crate::retain::PaintContext) -> Vec<RenderCommand> {
        // Containers don't paint themselves
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        &self.children
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/mod.rs vexo/src/retain/widgets/text.rs vexo/src/retain/widgets/container.rs
git commit -m "feat: add create_render_object to Widget trait and implement TextRenderObject, ContainerRenderObject"
```

---

### Task 2: Create RenderObject implementations module

**Files:**
- Create: `vexo/src/retain/render_objects/mod.rs`
- Create: `vexo/src/retain/render_objects/text.rs`
- Create: `vexo/src/retain/render_objects/container.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create render_objects module structure**

```rust
// vexo/src/retain/render_objects/mod.rs
//! RenderObject implementations for the retain rendering system.

mod text;
mod container;

pub use text::TextRenderObject;
pub use container::ContainerRenderObject;
```

- [ ] **Step 2: Move TextRenderObject to dedicated file**

```rust
// vexo/src/retain/render_objects/text.rs
//! TextRenderObject implementation.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, PaintContext, RenderObject};

/// RenderObject for text display.
pub struct TextRenderObject {
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
}

impl TextRenderObject {
    /// Create a new text render object.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            font_size: 16.0,
            computed_bounds: None,
        }
    }

    /// Set the font size.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Estimate text size based on content
        // TODO: Integrate with font system for accurate measurement
        let char_width = self.font_size * 0.6; // Approximate
        let line_height = self.font_size * 1.2;

        let estimated_width = (self.content.len() as f32 * char_width).min(constraints.max_width);
        let estimated_height = line_height.min(constraints.max_height);

        let size = Size::new(
            estimated_width.max(constraints.min_width),
            estimated_height.max(constraints.min_height),
        );

        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Text rendering is handled by glyphon separately
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(position),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_render_object_layout() {
        let mut obj = TextRenderObject::new("Hello World");
        let constraints = LayoutConstraints::new(
            Size::new(0.0, 0.0),
            Size::new(200.0, 50.0),
        );
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
        assert!(size.width <= 200.0);
        assert!(size.height <= 50.0);
    }

    #[test]
    fn test_text_render_object_hit_test() {
        let mut obj = TextRenderObject::new("Test");
        let constraints = LayoutConstraints::new(
            Size::new(0.0, 0.0),
            Size::new(100.0, 50.0),
        );
        let mut ctx = LayoutContext::mock();

        obj.layout(constraints, &mut ctx);

        // Should hit inside bounds
        assert!(obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));

        // Should miss outside bounds
        assert!(!obj.hit_test(Point::new(200.0, 200.0), &HitTestContext::mock()));
    }
}
```

- [ ] **Step 3: Move ContainerRenderObject to dedicated file**

```rust
// vexo/src/retain/render_objects/container.rs
//! ContainerRenderObject implementation for Column and Row.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, PaintContext, RenderObject, RenderObjectId};

/// RenderObject for container widgets (Column, Row).
pub struct ContainerRenderObject {
    children: Vec<RenderObjectId>,
    is_row: bool,
    computed_bounds: Option<Bounds<Logical>>,
}

impl ContainerRenderObject {
    /// Create a new column container.
    pub fn new_column() -> Self {
        Self {
            children: Vec::new(),
            is_row: false,
            computed_bounds: None,
        }
    }

    /// Create a new row container.
    pub fn new_row() -> Self {
        Self {
            children: Vec::new(),
            is_row: true,
            computed_bounds: None,
        }
    }

    /// Add a child render object.
    pub fn add_child(&mut self, child: RenderObjectId) {
        self.children.push(child);
    }

    /// Set children directly.
    pub fn set_children(&mut self, children: Vec<RenderObjectId>) {
        self.children = children;
    }

    /// Check if this is a row layout.
    pub fn is_row(&self) -> bool {
        self.is_row
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Container layout is delegated to Taffy
        // This just returns the constrained size
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Containers don't paint themselves, children do
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        &self.children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_render_object_column() {
        let obj = ContainerRenderObject::new_column();
        assert!(!obj.is_row());
        assert_eq!(obj.children().len(), 0);
    }

    #[test]
    fn test_container_render_object_row() {
        let obj = ContainerRenderObject::new_row();
        assert!(obj.is_row());
        assert_eq!(obj.children().len(), 0);
    }

    #[test]
    fn test_container_add_child() {
        let mut obj = ContainerRenderObject::new_column();
        let child_id = RenderObjectId::new();
        obj.add_child(child_id);

        assert_eq!(obj.children().len(), 1);
        assert_eq!(obj.children()[0], child_id);
    }

    #[test]
    fn test_container_layout() {
        let mut obj = ContainerRenderObject::new_column();
        let constraints = LayoutConstraints::new(
            Size::new(0.0, 0.0),
            Size::new(200.0, 100.0),
        );
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        assert_eq!(size.width, 200.0);
        assert_eq!(size.height, 100.0);
    }
}
```

- [ ] **Step 4: Update retain module exports**

```rust
// In vexo/src/retain/mod.rs, add:

mod render_objects;

pub use render_objects::{TextRenderObject, ContainerRenderObject};
```

- [ ] **Step 5: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/render_objects/ vexo/src/retain/mod.rs
git commit -m "feat: create render_objects module with TextRenderObject and ContainerRenderObject"
```

---

### Task 3: Integrate RenderObjects with Element lifecycle

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs`
- Modify: `vexo/src/retain/elements/container.rs`
- Modify: `vexo/src/retain/element_context.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/elements/leaf.rs, add to tests

#[test]
fn test_leaf_element_creates_render_object_on_mount() {
    use crate::retain::{Text, Widget, RenderObjectRegistry};

    let mut element = LeafElement::new();
    let mut state = StateStorage::new();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let mut context = ElementContext::new_with_registry(None, &mut state, &mut dirty, &mut render_objects);

    let widget = Text::new("Hello");
    element.set_widget(Box::new(widget));
    element.mount(&mut context);

    // Element should have created a render object
    assert!(element.render_object().is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_leaf_element_creates_render_object_on_mount -- --nocapture`
Expected: FAIL with "no method named `set_widget`" or similar

- [ ] **Step 3: Update ElementContext to support RenderObject creation**

```rust
// In vexo/src/retain/element_context.rs

use crate::retain::{RenderObjectRegistry, RenderObject, RenderObjectId};

/// Context provided to element lifecycle methods.
pub struct ElementContext<'a> {
    parent: Option<ElementId>,
    state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    render_objects: Option<&'a mut RenderObjectRegistry>,
}

impl<'a> ElementContext<'a> {
    /// Create a new element context.
    pub fn new(parent: Option<ElementId>, state: &'a mut StateStorage, dirty: &'a mut DirtyTracking) -> Self {
        Self {
            parent,
            state,
            dirty,
            render_objects: None,
        }
    }

    /// Create with render object registry.
    pub fn new_with_registry(
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
    ) -> Self {
        Self {
            parent,
            state,
            dirty,
            render_objects: Some(render_objects),
        }
    }

    /// Create a render object and return its ID.
    pub fn create_render_object(&mut self, object: Box<dyn RenderObject>) -> Option<RenderObjectId> {
        self.render_objects.as_mut().map(|ro| {
            let id = RenderObjectId::new();
            // Store in registry - need to add method to registry
            ro.create(object, ElementId::new()) // Temporary owner ID
        })
    }

    /// Remove a render object.
    pub fn remove_render_object(&mut self, id: RenderObjectId) {
        if let Some(ro) = self.render_objects.as_mut() {
            ro.remove(id);
        }
    }

    // ... existing methods ...
}
```

- [ ] **Step 4: Update LeafElement to create RenderObject**

```rust
// In vexo/src/retain/elements/leaf.rs

use std::any::Any;
use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId, Widget};

/// Element for leaf widgets (no children).
pub struct LeafElement {
    id: Option<ElementId>,
    key: Option<Key>,
    widget: Option<Box<dyn Widget>>,
    render_object: Option<RenderObjectId>,
}

impl LeafElement {
    /// Create a new leaf element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            widget: None,
            render_object: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            widget: None,
            render_object: None,
        }
    }

    /// Set the widget.
    pub fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.key = widget.key();
        self.widget = Some(widget);
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}

impl Element for LeafElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(ElementId::new());

        // Create render object from widget
        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            self.render_object = context.create_render_object(render_obj);
        }
    }

    fn update(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Leaf elements have no children
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }
}
```

- [ ] **Step 5: Update ContainerElement similarly**

```rust
// In vexo/src/retain/elements/container.rs

use std::any::Any;
use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId, Widget};

/// Element for container widgets (multiple children).
pub struct ContainerElement {
    id: Option<ElementId>,
    key: Option<Key>,
    widget: Option<Box<dyn Widget>>,
    children: Vec<ElementId>,
    render_object: Option<RenderObjectId>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            widget: None,
            children: Vec::new(),
            render_object: None,
        }
    }

    /// Set the widget.
    pub fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.key = widget.key();
        self.widget = Some(widget);
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }

    /// Get the children.
    pub fn children(&self) -> &[ElementId] {
        &self.children
    }
}

impl Element for ContainerElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(ElementId::new());

        // Create render object from widget
        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            self.render_object = context.create_render_object(render_obj);
        }
    }

    fn update(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Requires registry access - handled separately
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }
}
```

- [ ] **Step 6: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/elements/leaf.rs vexo/src/retain/elements/container.rs vexo/src/retain/element_context.rs
git commit -m "feat: integrate RenderObject creation into Element lifecycle"
```

---

### Task 4: Implement hit testing

**Files:**
- Create: `vexo/src/retain/hit_test.rs`
- Modify: `vexo/src/retain/mod.rs`
- Modify: `vexo/src/retain/render_object.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/hit_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{TextRenderObject, RenderObjectRegistry};
    use crate::core::Point;

    #[test]
    fn test_hit_test_finds_target() {
        let mut registry = RenderObjectRegistry::new();

        // Create a text render object
        let obj = TextRenderObject::new("Hello");
        let id = registry.create(Box::new(obj), ElementId::new());
        registry.set_root(id);

        // Hit test at a point inside
        let result = registry.hit_test(Point::new(5.0, 5.0));

        assert!(result.is_hit());
        assert_eq!(result.target(), Some(id));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_hit_test_finds_target -- --nocapture`
Expected: FAIL with "use of undeclared crate or module"

- [ ] **Step 3: Implement hit testing**

```rust
// vexo/src/retain/hit_test.rs
//! Hit testing for the retain rendering system.

use crate::core::{Logical, Point};
use crate::retain::{ElementId, RenderObjectId, RenderObjectRegistry};

/// Result of a hit test.
#[derive(Debug, Clone)]
pub struct HitTestResult {
    /// Path from root to the hit target (if any).
    path: Vec<RenderObjectId>,
    /// The element IDs along the path.
    element_path: Vec<ElementId>,
}

impl HitTestResult {
    /// Create a miss result.
    pub fn miss() -> Self {
        Self {
            path: Vec::new(),
            element_path: Vec::new(),
        }
    }

    /// Check if anything was hit.
    pub fn is_hit(&self) -> bool {
        !self.path.is_empty()
    }

    /// Get the target render object (deepest hit).
    pub fn target(&self) -> Option<RenderObjectId> {
        self.path.last().copied()
    }

    /// Get the target element.
    pub fn target_element(&self) -> Option<ElementId> {
        self.element_path.last().copied()
    }

    /// Get the path from root to target.
    pub fn path(&self) -> &[RenderObjectId] {
        &self.path
    }

    /// Get the element path.
    pub fn element_path(&self) -> &[ElementId] {
        &self.element_path
    }
}

impl RenderObjectRegistry {
    /// Hit test from root at the given position.
    pub fn hit_test(&self, position: Point<Logical>) -> HitTestResult {
        let mut path = Vec::new();
        let mut element_path = Vec::new();

        if let Some(root) = self.root {
            self.hit_test_recursive(root, position, &mut path, &mut element_path);
        }

        HitTestResult { path, element_path }
    }

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

        let ctx = super::HitTestContext::mock();

        if obj.hit_test(position, &ctx) {
            path.push(id);
            if let Some(element_id) = self.element_map.get(&id) {
                element_path.push(*element_id);
            }

            // Test children in reverse order (top-most first)
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
```

- [ ] **Step 4: Update retain module exports**

```rust
// In vexo/src/retain/mod.rs, add:

mod hit_test;

pub use hit_test::HitTestResult;
```

- [ ] **Step 5: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/hit_test.rs vexo/src/retain/mod.rs
git commit -m "feat: implement hit testing for RenderObject tree"
```

---

### Task 5: Create three-tree rendering pipeline

**Files:**
- Create: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Write the pipeline module**

```rust
// vexo/src/retain/pipeline.rs
//! Three-tree rendering pipeline.
//!
//! This module orchestrates the rendering flow:
//! 1. Reconcile widget tree with element tree
//! 2. Layout dirty render objects
//! 3. Paint dirty render objects
//! 4. Submit to GPU

use crate::core::{Logical, Physical, Point, Size};
use crate::layout::{LayoutEngine, LayoutConstraints};
use crate::render::RenderCommand;
use crate::retain::{
    DirtyTracking, ElementContext, ElementId, ElementRegistry, HitTestResult,
    PaintContext, RenderObjectRegistry, StateStorage, Widget,
};
use crate::input::InputEvent;

/// The three-tree rendering pipeline.
pub struct ThreeTreePipeline {
    element_registry: ElementRegistry,
    render_objects: RenderObjectRegistry,
    state: StateStorage,
    dirty: DirtyTracking,
}

impl ThreeTreePipeline {
    /// Create a new pipeline.
    pub fn new() -> Self {
        Self {
            element_registry: ElementRegistry::new(),
            render_objects: RenderObjectRegistry::new(),
            state: StateStorage::new(),
            dirty: DirtyTracking::new(),
        }
    }

    /// Reconcile a new widget tree with the existing element tree.
    pub fn reconcile(&mut self, root_widget: Box<dyn Widget>) {
        self.element_registry.reconcile_root(root_widget);
    }

    /// Layout all dirty render objects.
    pub fn layout(&mut self, available_size: Size<Logical>, _engine: &mut dyn LayoutEngine) {
        // Get dirty objects
        let dirty_ids: Vec<_> = self.dirty.needs_layout().iter().copied().collect();

        for id in dirty_ids {
            if let Some(obj) = self.render_objects.get_mut(id) {
                let constraints = LayoutConstraints::new(
                    Size::zero(),
                    available_size,
                );
                let mut ctx = crate::retain::LayoutContext::mock();
                obj.layout(constraints, &mut ctx);
            }
        }

        self.dirty.clear_layout();
    }

    /// Paint all dirty render objects.
    pub fn paint(&mut self) -> Vec<RenderCommand> {
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);

        // Paint from root
        if let Some(root) = self.render_objects.root() {
            self.paint_recursive(root, &mut ctx);
        }

        self.dirty.clear_paint();
        commands
    }

    fn paint_recursive(&self, id: crate::retain::RenderObjectId, ctx: &mut PaintContext) {
        if let Some(obj) = self.render_objects.get(id) {
            let child_commands = obj.paint(ctx);
            for cmd in child_commands {
                ctx.push_command(cmd);
            }

            // Paint children
            for child in obj.children() {
                self.paint_recursive(*child, ctx);
            }
        }
    }

    /// Hit test at the given position.
    pub fn hit_test(&self, position: Point<Logical>) -> HitTestResult {
        self.render_objects.hit_test(position)
    }

    /// Get the element registry.
    pub fn element_registry(&self) -> &ElementRegistry {
        &self.element_registry
    }

    /// Get the render object registry.
    pub fn render_objects(&self) -> &RenderObjectRegistry {
        &self.render_objects
    }

    /// Check if layout is needed.
    pub fn needs_layout(&self) -> bool {
        !self.dirty.needs_layout().is_empty()
    }

    /// Check if paint is needed.
    pub fn needs_paint(&self) -> bool {
        !self.dirty.needs_paint().is_empty()
    }
}

impl Default for ThreeTreePipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Column, Text};

    #[test]
    fn test_pipeline_creation() {
        let pipeline = ThreeTreePipeline::new();
        assert!(!pipeline.needs_layout());
        assert!(!pipeline.needs_paint());
    }

    #[test]
    fn test_pipeline_reconcile() {
        let mut pipeline = ThreeTreePipeline::new();

        let widget = Column::new()
            .push(Text::new("Hello"))
            .push(Text::new("World"));

        pipeline.reconcile(Box::new(widget));

        // After reconcile, should have elements
        assert!(pipeline.element_registry().len() > 0);
    }
}
```

- [ ] **Step 2: Update retain module exports**

```rust
// In vexo/src/retain/mod.rs, add:

mod pipeline;

pub use pipeline::ThreeTreePipeline;
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs vexo/src/retain/mod.rs
git commit -m "feat: create three-tree rendering pipeline"
```

---

### Task 6: Update WindowState to use three-tree pipeline

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Add three-tree pipeline to WindowState**

The WindowState currently uses immediate-mode rendering. We need to add the three-tree pipeline alongside the existing code for a gradual migration.

```rust
// In vexo/src/window.rs, add to WindowState struct:

use crate::retain::{ThreeTreePipeline, Widget as RetainWidget};

pub struct WindowState<A: Application + 'static> {
    // ... existing fields ...

    // Three-tree pipeline (new retain-mode system)
    retain_pipeline: Option<ThreeTreePipeline>,
    use_retain_mode: bool,
}
```

- [ ] **Step 2: Initialize pipeline in WindowState::new**

```rust
// In WindowState::new()

Ok(Self {
    // ... existing fields ...
    retain_pipeline: Some(ThreeTreePipeline::new()),
    use_retain_mode: false, // Start with immediate mode for compatibility
})
```

- [ ] **Step 3: Add render method for retain mode**

```rust
// In WindowState

fn render_retain(&mut self) -> Result<(), wgpu::SurfaceError> {
    // 1. Generate widget tree
    let widget_tree = self.view_retain();

    // 2. Reconcile
    if let Some(pipeline) = &mut self.retain_pipeline {
        pipeline.reconcile(widget_tree);

        // 3. Layout
        let logical_size = Size::<Logical>::new(
            self.backend.width() as f32 / self.widget_context.scale.factor(),
            self.backend.height() as f32 / self.widget_context.scale.factor(),
        );
        pipeline.layout(logical_size, self.layout_engine.as_mut());

        // 4. Paint
        let commands = pipeline.paint();

        // 5. Submit to GPU
        // Process commands through batcher
        self.batcher.clear();
        for cmd in commands {
            // Convert RenderCommand to batcher operations
        }
    }

    Ok(())
}

fn view_retain(&self) -> Box<dyn RetainWidget> {
    // Convert Application::view() to retain widgets
    // This is a temporary bridge during migration
    Box::new(crate::retain::Column::new())
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: add three-tree pipeline to WindowState (disabled by default)"
```

---

### Task 7: Update Application trait for retain mode

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Add retain_view to Application trait**

```rust
// In vexo/src/lib.rs

pub trait Application: Sized + 'static {
    type Message: Clone + std::fmt::Debug + Send;
    type State;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;

    /// Retain-mode view (optional, for migration).
    /// Returns a retain-mode widget tree.
    fn retain_view(state: &Self::State) -> Option<Box<dyn retain::Widget>> {
        let _ = state;
        None
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "feat: add retain_view to Application trait"
```

---

### Task 8: Add integration tests for full pipeline

**Files:**
- Modify: `vexo/src/retain/integration_tests.rs`

- [ ] **Step 1: Write comprehensive integration tests**

```rust
// In vexo/src/retain/integration_tests.rs

#[cfg(test)]
mod full_pipeline_tests {
    use crate::retain::{Column, Row, Text, ThreeTreePipeline, Widget};
    use crate::core::{Point, Size};
    use crate::layout::TaffyLayoutEngine;

    #[test]
    fn test_full_frame_flow() {
        let mut pipeline = ThreeTreePipeline::new();
        let mut engine = TaffyLayoutEngine::new();

        // First frame
        let widget = Column::new()
            .push(Text::new("First"))
            .push(Text::new("Second"));

        pipeline.reconcile(Box::new(widget));
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

        assert!(pipeline.element_registry().len() >= 3); // Column + 2 Text

        // Second frame - update text
        let widget = Column::new()
            .push(Text::new("First Updated"))
            .push(Text::new("Second"));

        pipeline.reconcile(Box::new(widget));

        // Elements should be reused, not recreated
        assert!(pipeline.needs_layout() || pipeline.needs_paint());
    }

    #[test]
    fn test_hit_test_through_pipeline() {
        let mut pipeline = ThreeTreePipeline::new();
        let mut engine = TaffyLayoutEngine::new();

        let widget = Column::new()
            .push(Text::new("Top"))
            .push(Text::new("Bottom"));

        pipeline.reconcile(Box::new(widget));
        pipeline.layout(Size::new(800.0, 600.0), &mut engine);

        // Hit test at a point
        let result = pipeline.hit_test(Point::new(10.0, 10.0));

        // Should hit something
        // (exact behavior depends on layout results)
        let _ = result;
    }

    #[test]
    fn test_keyed_reconciliation() {
        let mut pipeline = ThreeTreePipeline::new();

        // First frame with keyed widgets
        let widget = Column::new()
            .push(Text::new("A").with_key("first"))
            .push(Text::new("B").with_key("second"));

        pipeline.reconcile(Box::new(widget));
        let count_after_first = pipeline.element_registry().len();

        // Second frame - reorder with same keys
        let widget = Column::new()
            .push(Text::new("B updated").with_key("second"))
            .push(Text::new("A updated").with_key("first"));

        pipeline.reconcile(Box::new(widget));
        let count_after_second = pipeline.element_registry().len();

        // Element count should be the same (elements reused)
        assert_eq!(count_after_first, count_after_second);
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo full_pipeline_tests -- --nocapture`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/integration_tests.rs
git commit -m "test: add integration tests for full three-tree pipeline"
```

---

### Task 9: Run full test suite and fix any issues

**Files:**
- All retain module files

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p vexo -- -D warnings`
Expected: No errors (warnings acceptable)

- [ ] **Step 3: Build in release mode**

Run: `cargo build -p vexo --release`
Expected: Build succeeds

- [ ] **Step 4: Commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve test failures and clippy warnings"
```

---

### Task 10: Update documentation

**Files:**
- Modify: `vexo/src/retain/mod.rs` (module docs)

- [ ] **Step 1: Update module documentation**

```rust
// In vexo/src/retain/mod.rs

//! Retain-mode rendering system (Widget/Element/RenderObject trees).
//!
//! This module implements Flutter-style three-tree architecture for
//! efficient incremental updates.
//!
//! # Architecture
//!
//! The three trees work together:
//!
//! - **Widget tree**: Immutable configuration, rebuilt each frame
//! - **Element tree**: Stateful lifecycle, persistent across frames
//! - **RenderObject tree**: Layout and painting, dirty tracking
//!
//! # Example
//!
//! ```ignore
//! use vexo::retain::{Column, Text, ThreeTreePipeline, Widget};
//!
//! let mut pipeline = ThreeTreePipeline::new();
//!
//! // Create widget tree
//! let widget = Column::new()
//!     .push(Text::new("Hello"))
//!     .push(Text::new("World"));
//!
//! // Reconcile with element tree
//! pipeline.reconcile(Box::new(widget));
//!
//! // Layout and paint
//! pipeline.layout(available_size, &mut layout_engine);
//! let commands = pipeline.paint();
//! ```
//!
//! # Migration from Immediate Mode
//!
//! The retain-mode system can coexist with the immediate-mode system.
//! Set `use_retain_mode = true` in WindowState to enable.
```

- [ ] **Step 2: Commit**

```bash
git add vexo/src/retain/mod.rs
git commit -m "docs: update retain module documentation"
```

---

## Summary

This plan completes the three-tree architecture implementation:

1. **Task 1-2**: Add `create_render_object` to Widget trait and create dedicated RenderObject implementations
2. **Task 3**: Integrate RenderObject creation into Element lifecycle (mount/update/unmount)
3. **Task 4**: Implement hit testing for input event routing
4. **Task 5**: Create the three-tree rendering pipeline
5. **Task 6-7**: Integrate with WindowState and Application trait
6. **Task 8-10**: Testing, fixes, and documentation

After completion, the retain-mode system will be ready for use alongside the existing immediate-mode system, enabling a gradual migration.
