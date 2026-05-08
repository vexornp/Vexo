# Convert Retain Mode Widgets to Callback-Based System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the generic message type parameter `M` from all retain mode widgets, replacing typed messages with Flutter-style callbacks and futures-signals for reactive state.

**Architecture:** The retain mode Widget trait becomes non-generic. Button stores `on_press: Option<Box<dyn FnMut()>>` instead of `message: M`. Elements handle callbacks internally. The `clone_box()` method is removed since callbacks are moved to elements.

**Tech Stack:** Rust, futures-signals crate for reactive state

---

## File Structure

### Files to Modify
- `vexo/Cargo.toml` - Add futures-signals dependency
- `vexo/src/retain/widgets/mod.rs` - Remove `M` from Widget trait, remove `clone_box()`
- `vexo/src/retain/widgets/button.rs` - Replace `message: M` with `on_press: Box<dyn FnMut()>`
- `vexo/src/retain/widgets/text.rs` - Remove `M` and `PhantomData<M>`
- `vexo/src/retain/widgets/container.rs` - Remove `M` from Column, Row
- `vexo/src/retain/widgets/decorated_container.rs` - Remove `M` from DecoratedContainer
- `vexo/src/retain/element.rs` - Update signatures (if needed)
- `vexo/src/retain/elements/leaf.rs` - Remove `M` from LeafElement
- `vexo/src/retain/elements/container.rs` - Remove `M` from ContainerElement
- `vexo/src/retain/pipeline.rs` - Update widget usage
- `vexo/src/retain/integration_tests.rs` - Remove `()` type parameters
- `vexo/src/retain/e2e_test.rs` - Remove `()` type parameters

### New Files to Create
- `vexo/src/reactive/mod.rs` - Re-export futures_signals types

---

## Task 1: Add futures-signals Dependency

**Files:**
- Modify: `vexo/Cargo.toml`

- [ ] **Step 1: Add futures-signals to Cargo.toml**

Add to the dependencies section:

```toml
futures-signals = "0.3"
```

- [ ] **Step 2: Verify dependency resolves**

Run: `cargo check -p vexo`
Expected: Compiles successfully (dependency downloaded)

- [ ] **Step 3: Commit**

```bash
git add vexo/Cargo.toml
git commit -m "feat: add futures-signals dependency for reactive state"
```

---

## Task 2: Create Reactive Module

**Files:**
- Create: `vexo/src/reactive/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create reactive module directory and file**

```bash
mkdir -p vexo/src/reactive
```

Create `vexo/src/reactive/mod.rs`:

```rust
//! Reactive state primitives.
//!
//! Re-exports from futures-signals for use throughout the framework.

pub use futures_signals::signal::{Mutable, ReadOnlyMutable, Signal, SignalCloned};
```

- [ ] **Step 2: Add reactive module to lib.rs**

In `vexo/src/lib.rs`, add the module declaration:

```rust
pub mod reactive;
```

- [ ] **Step 3: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add vexo/src/reactive/mod.rs vexo/src/lib.rs
git commit -m "feat: add reactive module with futures-signals re-exports"
```

---

## Task 3: Update Widget Trait

**Files:**
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Remove M parameter from Widget trait**

Replace the trait definition (lines 44-141) with:

```rust
/// Immutable widget configuration - rebuilt each frame.
///
/// Widgets describe "what should exist" in the UI. They are:
/// - Cheap to create (no expensive operations in constructors)
/// - Immutable (no internal state that changes)
///
/// The widget tree is the first tree in the three-tree architecture:
/// Widget (configuration) -> Element (state) -> RenderObject (layout/paint)
pub trait Widget: Any {
    /// Optional key for identity across frames.
    fn key(&self) -> Option<WidgetKey> {
        None
    }

    /// Create the corresponding element for this widget.
    fn create_element(&self) -> Box<dyn Element>;

    /// Create the render object for this widget.
    fn create_render_object(&self) -> Box<dyn RenderObject>;

    /// Check if this widget can update an existing element.
    fn can_update(&self, other: &dyn Widget) -> bool {
        Any::type_id(self) == Any::type_id(other) && self.key() == other.key()
    }

    /// Get as Any for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Get the child widget, if this is a modifier widget.
    fn child(&self) -> Option<&dyn Widget> {
        None
    }

    /// Get the children widgets for container widgets.
    fn children(&self) -> &[Box<dyn Widget>] {
        &[]
    }

    /// Update an existing render object with new properties from this widget.
    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 2: Update test widget implementation**

In the tests module (lines 143-291), update `TestWidget` to not use `M`:

```rust
impl Widget for TestWidget {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TestElement)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TestRenderObject { layout_node: None })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

Remove `clone_box` method from the impl.

- [ ] **Step 3: Update test_widget_creates_render_object test**

Change line 278 from:
```rust
let widget: Text<()> = Text::new("Hello");
```
to:
```rust
let widget = Text::new("Hello");
```

- [ ] **Step 4: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected - they still use `M`)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/widgets/mod.rs
git commit -m "refactor: remove M parameter from Widget trait"
```

---

## Task 4: Update Text Widget

**Files:**
- Modify: `vexo/src/retain/widgets/text.rs`

- [ ] **Step 1: Remove M parameter from Text struct**

Replace the struct definition (lines 15-19) with:

```rust
/// Text widget - displays a string.
pub struct Text {
    key: Option<WidgetKey>,
    content: String,
}
```

- [ ] **Step 2: Update impl blocks**

Replace all impl blocks with:

```rust
impl Text {
    /// Create a new text widget.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            key: None,
            content: content.into(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl Clone for Text {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
        }
    }
}

impl Widget for Text {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TextRenderObject::new(&self.content))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(text_ro) = render_object.as_any_mut().downcast_mut::<TextRenderObject>() {
            if text_ro.set_content(&self.content) {
                UpdateResult::LAYOUT | UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
}
```

- [ ] **Step 3: Update tests**

Replace the tests module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_widget_creation() {
        let widget = Text::new("Hello");
        assert_eq!(widget.content(), "Hello");
    }

    #[test]
    fn test_text_widget_with_key() {
        let widget = Text::new("Hello").with_key("greeting");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("greeting"))));
    }

    #[test]
    fn test_text_widget_with_global_key() {
        let global_key = GlobalKey::new();
        let widget = Text::new("Hello").with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_text_widget_clone() {
        let widget = Text::new("Hello").with_key("greeting");
        let cloned = widget.clone();

        assert_eq!(widget.content(), cloned.content());
        assert_eq!(widget.key(), cloned.key());
    }
}
```

- [ ] **Step 4: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/widgets/text.rs
git commit -m "refactor: remove M parameter from Text widget"
```

---

## Task 5: Update Container Widgets (Column, Row)

**Files:**
- Modify: `vexo/src/retain/widgets/container.rs`

- [ ] **Step 1: Remove M parameter from Column struct**

Replace Column struct and impls (lines 11-93) with:

```rust
/// Column widget - arranges children vertically.
pub struct Column {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
}

impl Column {
    /// Create a new empty column.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Column {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.as_any().downcast_ref::<Column>().cloned()).collect(),
        }
    }
}

impl Widget for Column {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_column())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE
    }
}
```

- [ ] **Step 2: Remove M parameter from Row struct**

Replace Row struct and impls (lines 96-178) with:

```rust
/// Row widget - arranges children horizontally.
pub struct Row {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
}

impl Row {
    /// Create a new empty row.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Row {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.as_any().downcast_ref::<Row>().cloned()).collect(),
        }
    }
}

impl Widget for Row {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_row())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE
    }
}
```

- [ ] **Step 3: Update tests**

Replace tests module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Text;

    #[test]
    fn test_column_creation() {
        let column = Column::new()
            .push(Text::new("First"))
            .push(Text::new("Second"));

        assert_eq!(column.children().len(), 2);
    }

    #[test]
    fn test_column_with_key() {
        let column = Column::new()
            .with_key("my-column")
            .push(Text::new("Hello"));

        assert_eq!(column.key(), Some(WidgetKey::Local(Key::new("my-column"))));
    }

    #[test]
    fn test_column_with_global_key() {
        let global_key = GlobalKey::new();
        let column = Column::new()
            .with_key(global_key.clone())
            .push(Text::new("Hello"));

        assert_eq!(column.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_row_creation() {
        let row = Row::new()
            .push(Text::new("Left"))
            .push(Text::new("Right"));

        assert_eq!(row.children().len(), 2);
    }
}
```

- [ ] **Step 4: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/widgets/container.rs
git commit -m "refactor: remove M parameter from Column and Row widgets"
```

---

## Task 6: Update Button Widget with Callback

**Files:**
- Modify: `vexo/src/retain/widgets/button.rs`

- [ ] **Step 1: Remove M parameter, add callback**

Replace Button struct (lines 41-46) with:

```rust
/// Button widget - clickable button with a label.
///
/// When clicked, calls the `on_press` callback if set.
pub struct Button {
    key: Option<WidgetKey>,
    label: String,
    /// Callback invoked when button is pressed.
    on_press: Option<Box<dyn FnMut()>>,
}
```

- [ ] **Step 2: Update Button impl**

Replace Button impl (lines 48-76) with:

```rust
impl Button {
    /// Create a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: None,
            label: label.into(),
            on_press: None,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the callback for press events.
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Box::new(callback));
        self
    }

    /// Get the button label.
    pub fn label(&self) -> &str {
        &self.label
    }
}
```

- [ ] **Step 3: Update Clone impl**

Replace Clone impl (lines 78-86) with:

```rust
impl Clone for Button {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            label: self.label.clone(),
            // Note: callbacks are not cloned - they are moved to the element
            on_press: None,
        }
    }
}
```

- [ ] **Step 4: Update Widget impl**

Replace Widget impl (lines 88-124) with:

```rust
impl Widget for Button {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ButtonElement::new(self.label.clone(), self.on_press.take());
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ButtonRenderObject::new(&self.label))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(button_ro) = render_object.as_any_mut().downcast_mut::<ButtonRenderObject>() {
            if button_ro.set_label(&self.label) {
                UpdateResult::LAYOUT | UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
}
```

- [ ] **Step 5: Update ButtonElement**

Replace ButtonElement struct (lines 135-142) with:

```rust
/// Element for Button widget - handles click events.
pub struct ButtonElement {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
    label: String,
    on_press: Option<Box<dyn FnMut()>>,
}
```

- [ ] **Step 6: Update ButtonElement impl**

Replace ButtonElement impl (lines 144-168) with:

```rust
impl ButtonElement {
    /// Create a new button element.
    pub fn new(label: impl Into<String>, on_press: Option<Box<dyn FnMut()>>) -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            label: label.into(),
            on_press,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(Box::new(widget.clone()));
        self.key = widget.key();
    }

    /// Get the element ID.
    #[allow(dead_code)]
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}
```

- [ ] **Step 7: Update Element impl for ButtonElement**

Replace Element impl (lines 170-262) with:

```rust
impl Element for ButtonElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(context.element_id);

        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);

                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self.widget.as_ref().unwrap().update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerButton { state, .. } => {
                if *state == ButtonState::Pressed {
                    if context.is_pointer_inside() {
                        // Call the callback if set
                        if let Some(on_press) = &mut self.on_press {
                            on_press();
                        }
                        return Some(Box::new(()));
                    }
                }
            }
            _ => {}
        }
        None
    }
}
```

- [ ] **Step 8: Update tests**

Replace tests module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_widget_creation() {
        let widget = Button::new("Click Me");
        assert_eq!(widget.label(), "Click Me");
    }

    #[test]
    fn test_button_widget_with_key() {
        let widget = Button::new("Click Me").with_key("my-button");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("my-button"))));
    }

    #[test]
    fn test_button_widget_with_global_key() {
        let global_key = GlobalKey::new();
        let widget = Button::new("Click Me").with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_button_widget_with_callback() {
        use std::cell::Cell;
        use std::rc::Rc;

        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let widget = Button::new("Click Me").on_press(move || {
            called_clone.set(true);
        });

        assert!(widget.on_press.is_some());
    }

    #[test]
    fn test_button_render_object_layout() {
        use crate::layout::TaffyLayoutEngine;
        use std::sync::Arc;

        let mut obj = ButtonRenderObject::new("Click Me");
        let mut engine = TaffyLayoutEngine::new();
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        let mut font_system = glyphon::FontSystem::new_with_fonts([binary]);
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);
        assert!(obj.layout_node.is_some());
        let _ = result;
    }
}
```

- [ ] **Step 9: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected)

- [ ] **Step 10: Commit**

```bash
git add vexo/src/retain/widgets/button.rs
git commit -m "refactor: replace message M with callback in Button widget"
```

---

## Task 7: Update DecoratedContainer Widget

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs`

- [ ] **Step 1: Remove M parameter from DecoratedContainerElement**

Replace struct (lines 183-189) with:

```rust
pub struct DecoratedContainerElement {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
    child_element: Option<ElementId>,
}
```

- [ ] **Step 2: Update DecoratedContainerElement impl**

Replace impl (lines 191-223) with:

```rust
impl DecoratedContainerElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            child_element: None,
        }
    }

    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(Box::new(widget.clone()));
        self.key = widget.key();
    }

    pub fn id(&self) -> Option<ElementId> {
        self.id
    }

    pub fn child_element(&self) -> Option<ElementId> {
        self.child_element
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}
```

- [ ] **Step 3: Update Element impl for DecoratedContainerElement**

Replace Element impl (lines 231-369) - remove all `M` type parameters and `clone_box()` calls.

- [ ] **Step 4: Remove M parameter from DecoratedContainer**

Replace struct (lines 397-401) with:

```rust
pub struct DecoratedContainer {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    style: Style,
}
```

- [ ] **Step 5: Update DecoratedContainer impl**

Replace impls (lines 403-488) - remove all `M` type parameters and `clone_box()` calls.

- [ ] **Step 6: Update tests**

Replace all `DecoratedContainer<()>` with `DecoratedContainer` in tests.

- [ ] **Step 7: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected)

- [ ] **Step 8: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "refactor: remove M parameter from DecoratedContainer widget"
```

---

## Task 8: Update Element Implementations

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs`
- Modify: `vexo/src/retain/elements/container.rs`

- [ ] **Step 1: Update LeafElement**

Remove `M` parameter from `LeafElement<M>` struct and all impls.

- [ ] **Step 2: Update ContainerElement**

Remove `M` parameter from `ContainerElement<M>` struct and all impls.

- [ ] **Step 3: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in other files (expected)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/elements/leaf.rs vexo/src/retain/elements/container.rs
git commit -m "refactor: remove M parameter from Element implementations"
```

---

## Task 9: Update Pipeline

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

- [ ] **Step 1: Update pipeline to use non-generic Widget**

Remove all `M` type parameters from pipeline functions.

- [ ] **Step 2: Verify module compiles**

Run: `cargo check -p vexo`
Expected: Compilation errors in test files (expected)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "refactor: update pipeline for non-generic Widget"
```

---

## Task 10: Update Integration Tests

**Files:**
- Modify: `vexo/src/retain/integration_tests.rs`
- Modify: `vexo/src/retain/e2e_test.rs`

- [ ] **Step 1: Remove () type parameters in integration_tests.rs**

Replace all `Column<()>`, `Text<()>`, etc. with `Column`, `Text`, etc.

- [ ] **Step 2: Remove () type parameters in e2e_test.rs**

Replace all `Column<()>`, `Text<()>`, etc. with `Column`, `Text`, etc.

- [ ] **Step 3: Verify tests compile**

Run: `cargo check -p vexo`
Expected: Compiles successfully

- [ ] **Step 4: Run tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/integration_tests.rs vexo/src/retain/e2e_test.rs
git commit -m "test: update retain mode tests for non-generic Widget"
```

---

## Task 11: Update Sample App

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Update sample app to use callbacks**

Update the sample app to use the new callback-based API with signals.

- [ ] **Step 2: Verify sample app compiles**

Run: `cargo check -p shared_app`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "refactor: update sample app to use callback-based widgets"
```

---

## Task 12: Final Verification

- [ ] **Step 1: Run full build**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 2: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 3: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Application runs without errors

- [ ] **Step 4: Manual testing**

Verify:
- Button clicks trigger callbacks
- Container widgets render correctly
- DecoratedContainer applies styles correctly

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete callback-based widget system for retain mode"
```
