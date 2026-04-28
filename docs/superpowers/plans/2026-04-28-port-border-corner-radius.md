# Port Border and CornerRadius Modifiers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port Border and CornerRadius modifier widgets to retain-mode, following the Background pattern.

**Architecture:** Each modifier uses ModifierElement for element tree and implements Widget::child() and RenderObject with set_child_id()/children() for render tree linking.

**Tech Stack:** Rust, vexo retain module (Widget, Element, RenderObject traits), Color, RenderCommand

---

## File Structure

**Files to create:**
- `vexo/src/retain/widgets/border.rs` - Border widget + BorderRenderObject
- `vexo/src/retain/widgets/corner_radius.rs` - CornerRadius widget + CornerRadiusRenderObject

**Files to modify:**
- `vexo/src/retain/widgets/mod.rs` - Add Border, CornerRadius exports
- `vexo/src/retain/mod.rs` - Export Border, CornerRadius

---

### Task 1: Create Border widget with BorderRenderObject

**Files:**
- Create: `vexo/src/retain/widgets/border.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/widgets/border.rs, add at the end:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Widget, Key};
    use crate::core::Color;

    #[test]
    fn test_border_widget_creation() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0);
        
        assert!(border.key().is_none());
    }

    #[test]
    fn test_border_widget_with_key() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0)
            .with_key("my-border");
        
        assert_eq!(border.key(), Some(Key::new("my-border")));
    }

    #[test]
    fn test_border_creates_render_object() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0);
        
        let mut ro = border.create_render_object();
        
        // Must layout first to set computed_bounds
        let constraints = crate::layout::LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..Default::default()
        };
        let mut layout_ctx = crate::retain::LayoutContext::mock();
        ro.layout(constraints, &mut layout_ctx);
        
        // Should be able to paint
        let mut commands = Vec::new();
        let mut ctx = crate::retain::PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        
        // Border should return a rect_with_border command
        assert_eq!(cmds.len(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_border_widget_creation -- --nocapture`
Expected: FAIL with "use of undeclared crate or module"

- [ ] **Step 3: Write the Border widget implementation**

```rust
// vexo/src/retain/widgets/border.rs
//! Border modifier widget - draws a colored border around a child.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// Border modifier - draws a colored border around a child widget.
pub struct Border {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
    width: f32,
}

impl Border {
    /// Create a new border modifier.
    pub fn new(child: Box<dyn Widget>, color: Color, width: f32) -> Self {
        Self {
            key: None,
            child,
            color,
            width,
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

    /// Get the border color.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Get the border width.
    pub fn width(&self) -> f32 {
        self.width
    }
}

impl Widget for Border {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(BorderRenderObject::new(self.color, self.width))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            color: self.color,
            width: self.width,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for Border - draws a colored border.
pub struct BorderRenderObject {
    color: Color,
    width: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl BorderRenderObject {
    /// Create a new border render object.
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            child: None,
            computed_bounds: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for BorderRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Border takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => vec![RenderCommand::rect_with_border(
                *bounds,
                Color::TRANSPARENT,
                self.color,
                self.width,
            )],
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }
}
```

- [ ] **Step 4: Update widgets/mod.rs to export Border**

```rust
// In vexo/src/retain/widgets/mod.rs, add:

mod border;

pub use border::Border;
```

- [ ] **Step 5: Update retain/mod.rs to export Border**

```rust
// In vexo/src/retain/mod.rs, update the widgets re-export line:

pub use widgets::{Widget, Text, Column, Row, Background, Border};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo test_border -- --nocapture`
Expected: All 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/border.rs vexo/src/retain/widgets/mod.rs vexo/src/retain/mod.rs
git commit -m "feat: add Border widget to retain-mode"
```

---

### Task 2: Create CornerRadius widget with CornerRadiusRenderObject

**Files:**
- Create: `vexo/src/retain/widgets/corner_radius.rs`

- [ ] **Step 1: Write the failing test**

```rust
// In vexo/src/retain/widgets/corner_radius.rs, add at the end:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Widget, Key};
    use crate::core::Color;

    #[test]
    fn test_corner_radius_widget_creation() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0);
        
        assert!(cr.key().is_none());
    }

    #[test]
    fn test_corner_radius_widget_with_key() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0)
            .with_key("my-corners");
        
        assert_eq!(cr.key(), Some(Key::new("my-corners")));
    }

    #[test]
    fn test_corner_radius_creates_render_object() {
        let child = Box::new(crate::retain::Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0);
        
        let mut ro = cr.create_render_object();
        
        // Must layout first to set computed_bounds
        let constraints = crate::layout::LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..Default::default()
        };
        let mut layout_ctx = crate::retain::LayoutContext::mock();
        ro.layout(constraints, &mut layout_ctx);
        
        // Should be able to paint
        let mut commands = Vec::new();
        let mut ctx = crate::retain::PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        
        // CornerRadius should return push/pop commands
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], crate::render::RenderCommand::PushCornerRadius { .. }));
        assert!(matches!(cmds[1], crate::render::RenderCommand::PopCornerRadius));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_corner_radius_widget_creation -- --nocapture`
Expected: FAIL with "use of undeclared crate or module"

- [ ] **Step 3: Write the CornerRadius widget implementation**

```rust
// vexo/src/retain/widgets/corner_radius.rs
//! CornerRadius modifier widget - applies rounded corners to a child.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// CornerRadius modifier - applies rounded corners to a child widget.
pub struct CornerRadius {
    key: Option<Key>,
    child: Box<dyn Widget>,
    radius: f32,
}

impl CornerRadius {
    /// Create a new corner radius modifier.
    pub fn new(child: Box<dyn Widget>, radius: f32) -> Self {
        Self {
            key: None,
            child,
            radius,
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

    /// Get the corner radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl Widget for CornerRadius {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(CornerRadiusRenderObject::new(self.radius))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            radius: self.radius,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for CornerRadius - applies rounded corners.
pub struct CornerRadiusRenderObject {
    radius: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl CornerRadiusRenderObject {
    /// Create a new corner radius render object.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            child: None,
            computed_bounds: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for CornerRadiusRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // CornerRadius takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Return push/pop commands for corner radius
        vec![
            RenderCommand::PushCornerRadius { radius: self.radius },
            RenderCommand::PopCornerRadius,
        ]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }
}
```

- [ ] **Step 4: Update widgets/mod.rs to export CornerRadius**

```rust
// In vexo/src/retain/widgets/mod.rs, add:

mod corner_radius;

pub use corner_radius::CornerRadius;
```

- [ ] **Step 5: Update retain/mod.rs to export CornerRadius**

```rust
// In vexo/src/retain/mod.rs, update the widgets re-export line:

pub use widgets::{Widget, Text, Column, Row, Background, Border, CornerRadius};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo test_corner_radius -- --nocapture`
Expected: All 3 tests PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/corner_radius.rs vexo/src/retain/widgets/mod.rs vexo/src/retain/mod.rs
git commit -m "feat: add CornerRadius widget to retain-mode"
```

---

### Task 3: Add integration tests for Border and CornerRadius

**Files:**
- Modify: `vexo/src/retain/e2e_test.rs`

- [ ] **Step 1: Add integration tests**

```rust
// In vexo/src/retain/e2e_test.rs, add:

#[test]
fn test_border_widget_in_pipeline() {
    use crate::retain::{Border, Text, Widget};
    use crate::core::Color;

    // Create a widget tree with Border
    let child = Box::new(Text::new("Hello"));
    let border = Border::new(child, Color::BLACK, 2.0);
    
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(border));
    
    // Should have created elements
    assert!(pipeline.element_registry().len() >= 1);
    assert!(pipeline.render_objects().len() >= 1);
    
    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine);
    
    // Paint
    let commands = pipeline.paint();
    
    // Border should produce a rect_with_border command
    assert!(commands.len() >= 1, "Border should produce at least one command");
}

#[test]
fn test_corner_radius_widget_in_pipeline() {
    use crate::retain::{CornerRadius, Text, Widget};

    // Create a widget tree with CornerRadius
    let child = Box::new(Text::new("Hello"));
    let cr = CornerRadius::new(child, 10.0);
    
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(cr));
    
    // Should have created elements
    assert!(pipeline.element_registry().len() >= 1);
    assert!(pipeline.render_objects().len() >= 1);
    
    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine);
    
    // Paint
    let commands = pipeline.paint();
    
    // CornerRadius should produce push/pop commands
    assert!(commands.len() >= 2, "CornerRadius should produce at least two commands");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo test_border_widget_in_pipeline test_corner_radius_widget_in_pipeline -- --nocapture`
Expected: Both tests PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/e2e_test.rs
git commit -m "test: add integration tests for Border and CornerRadius widgets"
```

---

## Summary

This plan ports Border and CornerRadius modifier widgets to retain-mode:

1. **Task 1**: Create Border widget with BorderRenderObject
2. **Task 2**: Create CornerRadius widget with CornerRadiusRenderObject
3. **Task 3**: Add integration tests for both

After completion, both modifiers can be used in retain-mode widget trees:

```rust
let child = Box::new(Text::new("Hello"));
let border = Border::new(child, Color::BLACK, 2.0);
pipeline.reconcile(Box::new(border));

let child2 = Box::new(Text::new("World"));
let cr = CornerRadius::new(child2, 10.0);
pipeline.reconcile(Box::new(cr));
```
