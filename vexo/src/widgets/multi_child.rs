//! MultiChild widget — a multi-child container with a user-supplied `Layout`.
//!
//! `MultiChild` is the Vexo replacement for `Flex`/`Column`/`Row`. It holds
//! N children and applies a `Layout` (flexbox, grid, or block) to them.
//! Unlike the old `Flex`, it has no `Style` field — decoration goes on
//! `DecoratedBox`.
//!
//! # Example
//!
//! ```ignore
//! use vexo::{MultiChild, Layout, Text};
//!
//! MultiChild::new(
//!     vec![Text::new("A").boxed(), Text::new("B").boxed()],
//!     Layout::column().gap(16.0),
//! )
//! ```

use super::container::ChildPush;
use super::{Element, Widget};
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::render_objects::ContainerRenderObject;
use crate::{RenderObject, UpdateResult};

/// A multi-child container with a user-supplied `Layout`.
///
/// The replacement for `Flex`/`Column`/`Row`. Pass a `Layout::column()`,
/// `Layout::row()`, `Layout::grid()`, or `Layout::default()` (block) to
/// control how children are arranged. For decoration (background, border,
/// etc.), wrap in `DecoratedBox`.
pub struct MultiChild {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl MultiChild {
    /// Create a new `MultiChild` with the given children and layout.
    pub fn new(children: Vec<Box<dyn Widget>>, layout: Layout) -> Self {
        Self {
            key: None,
            children,
            layout,
        }
    }

    /// Create an empty `MultiChild` with the given layout; add children via `.push()`.
    pub fn empty(layout: Layout) -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Add a child widget.
    ///
    /// Accepts any `impl Widget` or `Option<Box<dyn Widget>>` (for conditional children).
    pub fn push(mut self, child: impl ChildPush + 'static) -> Self {
        child.push_into(&mut self.children);
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Get the layout.
    pub fn layout_ref(&self) -> &Layout {
        &self.layout
    }
}

impl Default for MultiChild {
    fn default() -> Self {
        Self::empty(Layout::default())
    }
}

impl Clone for MultiChild {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for MultiChild {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.layout.clone()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object
            .as_any_mut()
            .downcast_mut::<ContainerRenderObject>()
        {
            if container_ro.set_layout(self.layout.clone()) {
                UpdateResult::LAYOUT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{FlexDirection, Layout};
    use crate::Text;

    #[test]
    fn test_multi_child_new_with_children() {
        let mc = MultiChild::new(
            vec![Text::new("A").boxed(), Text::new("B").boxed()],
            Layout::column(),
        );
        assert_eq!(mc.children().len(), 2);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn test_multi_child_empty_then_push() {
        let mc = MultiChild::empty(Layout::column())
            .push(Text::new("A"))
            .push(Text::new("B"));
        assert_eq!(mc.children().len(), 2);
    }

    #[test]
    fn test_multi_child_with_key() {
        let mc = MultiChild::empty(Layout::column()).with_key("my-mc");
        assert_eq!(mc.key(), Some(WidgetKey::Local(crate::Key::new("my-mc"))));
    }

    #[test]
    fn test_multi_child_with_layout_replaces() {
        let mc = MultiChild::empty(Layout::column()).with_layout(Layout::row().gap(8.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
        assert!(mc.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_clone() {
        let mc = MultiChild::new(vec![Text::new("A").boxed()], Layout::column().gap(16.0));
        let cloned = mc.clone();
        assert_eq!(cloned.children().len(), 1);
        assert!(cloned.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_creates_container_render_object() {
        let mc = MultiChild::empty(Layout::column());
        let ro = mc.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<ContainerRenderObject>()
            .is_some());
    }

    #[test]
    fn test_multi_child_update_render_object_layout_change() {
        let mc1 = MultiChild::empty(Layout::default().padding(10.0));
        let mc2 = MultiChild::empty(Layout::default().padding(20.0));
        let mut ro = ContainerRenderObject::new(Layout::default().padding(10.0));
        assert_eq!(mc1.update_render_object(&mut ro), UpdateResult::NONE);
        assert!(mc2
            .update_render_object(&mut ro)
            .contains(UpdateResult::LAYOUT));
    }
}
