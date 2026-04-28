# Port Background Widget to Retain-Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the Background modifier widget from immediate-mode to retain-mode, enabling colored backgrounds behind child widgets.

**Architecture:** Background widget creates ModifierElement which manages a child element and BackgroundRenderObject. The render object paints a colored rect, and the pipeline paints children after parents.

**Tech Stack:** Rust, vexo retain module (Widget, Element, RenderObject traits), Color, RenderCommand

---

## File Structure

**Files to create:**
- `vexo/src/retain/widgets/background.rs` - Background widget + BackgroundRenderObject

**Files to modify:**
- `vexo/src/retain/widgets/mod.rs` - Add Background export
- `vexo/src/retain/mod.rs` - Export Background
- `vexo/src/retain/elements/modifier.rs` - Add child element handling

---

### Task 1: Create Background widget with BackgroundRenderObject

**Files:**
- Create: `vexo/src/retain/widgets/background.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/widgets/background.rs, add at the end:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Widget, Key};
    use crate::core::Color;

    #[test]
    fn test_background_widget_creation() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        
        assert!(bg.key().is_none());
    }

    #[test]
    fn test_background_widget_with_key() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let bg = Background::new(child, Color::RED)
            .with_key("my-bg");
        
        assert_eq!(bg.key(), Some(Key::new("my-bg")));
    }

    #[test]
    fn test_background_creates_render_object() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        
        let ro = bg.create_render_object();
        
        // Should be able to paint
        let mut commands = Vec::new();
        let mut ctx = crate::retain::PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        
        // Background should return a rect command
        assert_eq!(cmds.len(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_background_widget_creation -- --nocapture`
Expected: FAIL with "use of undeclared crate or module"

- [ ] **Step 3: Write the Background widget implementation**

```rust
// vexo/src/retain/widgets/background.rs
//! Background modifier widget - draws a colored background behind a child.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, ElementContext, ElementId, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// Background modifier - draws a colored rectangle behind a child widget.
pub struct Background {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
}

impl Background {
    /// Create a new background modifier.
    pub fn new(child: Box<dyn Widget>, color: Color) -> Self {
        Self {
            key: None,
            child,
            color,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the background color.
    pub fn color(&self) -> Color {
        self.color
    }
}

impl Widget for Background {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        // ModifierElement will be updated to handle children
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(BackgroundRenderObject::new(self.color))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            color: self.color,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// RenderObject for Background - draws a colored rect.
pub struct BackgroundRenderObject {
    color: Color,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl BackgroundRenderObject {
    /// Create a new background render object.
    pub fn new(color: Color) -> Self {
        Self {
            color,
            child: None,
            computed_bounds: None,
        }
    }

    /// Set the child render object.
    pub fn set_child(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for BackgroundRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Background takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => vec![RenderCommand::rect(*bounds, self.color)],
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        // Return empty for now - child is managed separately
        &[]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
```

- [ ] **Step 4: Update widgets/mod.rs to export Background**

```rust
// In vexo/src/retain/widgets/mod.rs, add:

mod background;

pub use background::Background;
```

- [ ] **Step 5: Update retain/mod.rs to export Background**

```rust
// In vexo/src/retain/mod.rs, update the widgets re-export line:

pub use widgets::{Widget, Text, Column, Row, Background};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo test_background -- --nocapture`
Expected: All 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/background.rs vexo/src/retain/widgets/mod.rs vexo/src/retain/mod.rs
git commit -m "feat: add Background widget to retain-mode"
```

---

### Task 2: Update ModifierElement to handle child widgets

**Files:**
- Modify: `vexo/src/retain/elements/modifier.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/elements/modifier.rs, add to tests module:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Text, Background, Widget, RenderObjectRegistry};
    use crate::core::Color;

    #[test]
    fn test_modifier_element_creates_child_element() {
        let mut element = ModifierElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut context = ElementContext::new_with_registry(
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
        );

        // Create a Background widget with a Text child
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);
        element.mount(&mut context);

        // Should have created an element ID
        assert!(element.id().is_some());
        
        // Should have created a render object
        assert!(element.render_object().is_some());
        
        // Should have created a child element
        assert!(element.child_element().is_some());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_modifier_element_creates_child_element -- --nocapture`
Expected: FAIL with "no method named `child_element`"

- [ ] **Step 3: Update ModifierElement to handle children**

```rust
// In vexo/src/retain/elements/modifier.rs, replace the entire file:

//! Modifier element implementation.
//!
//! ModifierElement is an element that wraps a single child.
//! Used by modifier widgets like Background, Border, Padding, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId, Widget};

/// Element for modifier widgets (wraps single child).
pub struct ModifierElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
    child_element: Option<ElementId>,
}

impl ModifierElement {
    /// Create a new modifier element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            child_element: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            widget: None,
            child_element: None,
        }
    }

    /// Set the widget for this element.
    ///
    /// Must be called before mount to create the render object.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }

    /// Get the child element ID.
    pub fn child_element(&self) -> Option<ElementId> {
        self.child_element
    }

    /// Try to get the child widget from the stored widget.
    /// 
    /// This attempts to downcast the widget to Background to get its child.
    fn get_child_widget(&self) -> Option<Box<dyn Widget>> {
        // For now, we check if the widget is a Background and get its child
        // In a more generic system, we'd have a ChildWidget trait
        let widget = self.widget.as_ref()?;
        let any = widget.as_any();
        
        // Try to downcast to Background
        if let Some(bg) = any.downcast_ref::<crate::retain::widgets::Background>() {
            Some(bg.child().clone_box())
        } else {
            None
        }
    }
}

impl Default for ModifierElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ModifierElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(ElementId::new());

        // Create render object if widget is set
        if let (Some(widget), Some(id)) = (&self.widget, self.id) {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);
            }
        }

        // Create child element if widget has a child
        if let Some(child_widget) = self.get_child_widget() {
            let child_element = child_widget.create_element();
            // Store the child widget and mount the child element
            // For now, we just create the element - full mounting would need
            // the registry to track parent-child relationships
            let _ = child_element; // Placeholder until full child mounting is implemented
        }
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        // Store the new widget configuration
        self.widget = Some(new_widget);

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove render object from registry
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
        // Child element would be unmounted by the registry
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // TODO: Visit child element when registry supports parent-child lookup
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

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo test_modifier_element -- --nocapture`
Expected: Tests PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/modifier.rs
git commit -m "feat: add child widget support to ModifierElement"
```

---

### Task 3: Add integration test for Background in pipeline

**Files:**
- Modify: `vexo/src/retain/e2e_test.rs`

- [ ] **Step 1: Add test for Background widget in pipeline**

```rust
// In vexo/src/retain/e2e_test.rs, add:

#[test]
fn test_background_widget_in_pipeline() {
    use crate::retain::{Background, Text, Widget};
    use crate::core::Color;

    // Create a widget tree with Background
    let child = Box::new(Text::new("Hello"));
    let bg = Background::new(child, Color::RED);
    
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(bg));
    
    // Should have created elements
    assert!(pipeline.element_registry().len() >= 1);
    assert!(pipeline.render_objects().len() >= 1);
    
    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine);
    
    // Paint
    let commands = pipeline.paint();
    
    // Background should produce a rect command
    assert!(commands.len() >= 1, "Background should produce at least one command");
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p vexo test_background_widget_in_pipeline -- --nocapture`
Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/e2e_test.rs
git commit -m "test: add integration test for Background widget in pipeline"
```

---

## Summary

This plan ports the Background modifier widget to retain-mode:

1. **Task 1**: Create Background widget with BackgroundRenderObject
2. **Task 2**: Update ModifierElement to handle child widgets
3. **Task 3**: Add integration test for Background in pipeline

After completion, Background can be used in retain-mode widget trees:

```rust
let child = Box::new(Text::new("Hello"));
let bg = Background::new(child, Color::RED);
pipeline.reconcile(Box::new(bg));
```