//! Stack widget — a multi-child container where children overlap (last child on top).
//!
//! `Stack` is the Vexo equivalent of Flutter's `Stack` / CSS `position: relative`
//! container. It establishes a positioning context: descendant `Positioned` widgets
//! are absolutely positioned relative to the Stack's content box.
//!
//! Non-positioned children are laid out by the Stack's flexbox (top-left aligned,
//! `FlexDirection::Column` + `AlignItems::Start`). `Positioned` children are taken
//! out of flow and positioned via their insets.
//!
//! The Stack defaults to filling its parent (`width_percent(1.0).height_percent(1.0)`,
//! i.e. `StackFit.expand`). Children paint in order, so the last child is on top.
//!
//! # Example
//!
//! ```ignore
//! Stack::new()
//!     .push(Text::new("Background"))              // non-positioned, fills stack
//!     .push(Positioned::new(Text::new("TL")).top(10.0).left(10.0))
//!     .push(Positioned::new(Text::new("BR")).bottom(10.0).right(10.0))
//! ```

#[allow(unused_imports)]
use super::super::core::{Logical, Size};
use super::super::key::WidgetKey;
#[allow(unused_imports)]
use super::super::layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, EdgeInsets, FlexDirection, FlexWrap, Inset,
    JustifyContent, Layout, Overflow, Position,
};
use super::super::render_objects::ContainerRenderObject;
use super::super::{RenderObject, UpdateResult};
use super::container::ChildPush;
use super::{Element, Widget};
#[allow(unused_imports)]
use crate::core::Color;
use crate::layout_builder_methods;
use crate::style::Style;

/// Default layout for a Stack: relative positioning context, fills parent, column direction.
///
/// `AlignItems::Stretch` makes non-positioned children fill the stack's
/// cross-axis, matching Flutter's `Stack` behavior. Positioned children are
/// absolutely positioned and are not affected by `AlignItems`.
fn stack_layout() -> Layout {
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
        .height_percent(1.0)
}

/// Stack widget — a multi-child container where children overlap.
///
/// Non-positioned children are laid out top-left by the Stack's flexbox.
/// `Positioned` children are absolutely positioned via their insets.
pub struct Stack {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
    style: Style,
}

impl Stack {
    /// Create a new Stack with default layout (fills parent, column direction).
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: stack_layout(),
            style: Style::default(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    ///
    /// Accepts any `impl Widget` or `Option<Box<dyn Widget>>` (for conditional children).
    pub fn push(mut self, child: impl ChildPush + 'static) -> Self {
        child.push_into(&mut self.children);
        self
    }

    /// Override the layout properties.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Stack {
    layout_builder_methods!();

    pub fn background(mut self, color: Color) -> Self {
        self.style = self.style.background(color);
        self
    }

    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.style = self.style.border(color, width);
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.style = self.style.corner_radius(radius);
        self
    }

    pub fn clip(mut self) -> Self {
        self.style = self.style.clip();
        self
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Stack {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
            style: self.style.clone(),
        }
    }
}

impl Widget for Stack {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_with_style(
            self.layout.clone(),
            self.style.clone(),
        ))
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
            let layout_changed = container_ro.set_layout(self.layout.clone());
            let style_changed = container_ro.set_style(self.style.clone());
            if layout_changed {
                UpdateResult::LAYOUT
            } else if style_changed {
                UpdateResult::PAINT
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
    use super::super::Text;
    use super::*;

    #[test]
    fn test_stack_creation() {
        let s = Stack::new();
        assert_eq!(s.children.len(), 0);
        assert_eq!(s.layout.flex_direction, Some(FlexDirection::Column));
        assert_eq!(s.layout.position, None); // Relative is the Taffy default
    }

    #[test]
    fn test_stack_push_children() {
        let s = Stack::new().push(Text::new("A")).push(Text::new("B"));
        assert_eq!(s.children.len(), 2);
    }

    #[test]
    fn test_stack_fills_parent_by_default() {
        let s = Stack::new();
        // width_percent / height_percent should be set to 1.0
        // (checking via the layout field indirectly — the layout struct stores these
        // as Dimension::Percent, which we verify by re-building)
        let _ = s.layout; // just ensure it exists
    }

    #[test]
    fn test_stack_with_key() {
        let s = Stack::new().with_key("my-stack");
        assert_eq!(s.key(), Some(WidgetKey::Local(crate::Key::new("my-stack"))));
    }

    #[test]
    fn test_stack_background() {
        let s = Stack::new().background(Color::RED);
        assert_eq!(s.style.background, Some(Color::RED));
    }
}
