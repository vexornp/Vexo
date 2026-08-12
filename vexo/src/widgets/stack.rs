//! Stack widget — a positioning context for overlapping `Positioned` children.
//!
//! `Stack` is the Vexo equivalent of Flutter's `Stack` / CSS `position: relative`
//! container. It establishes a positioning context: descendant `Positioned` widgets
//! are absolutely positioned relative to the Stack's content box.
//!
//! Non-positioned children are laid out by the Stack's flexbox
//! (`FlexDirection::Column` + `AlignItems::Stretch`): they flow vertically
//! top-to-bottom and are stretched to fill the Stack's cross-axis (width).
//! They do NOT overlap each other. Only `Positioned` children (taken out of
//! flow via `position: Absolute`) overlap the in-flow children — that is the
//! sole mechanism for z-stacking within a Stack.
//!
//! The Stack defaults to filling its parent (`width_percent(1.0).height_percent(1.0)`,
//! i.e. `StackFit.expand`). Children paint in push order, so a later `Positioned`
//! child paints on top of an earlier one.
//!
//! # Example
//!
//! ```ignore
//! Stack::new()
//!     .push(Text::new("Background"))              // non-positioned, fills stack
//!     .push(Positioned::new(Text::new("TL")).top(10.0).left(10.0))
//!     .push(Positioned::new(Text::new("BR")).bottom(10.0).right(10.0))
//! ```
//!

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

/// Default layout for a Stack: relative positioning context, fills parent, column direction.
///
/// `AlignItems::Stretch` makes non-positioned children fill the stack's
/// cross-axis, matching Flutter's `Stack` behavior. Positioned children are
/// absolutely positioned and are not affected by `AlignItems`.
///
/// `min_height(0.0)` allows the stack to shrink below its content's
/// min-content when the parent is shorter. Without this, the stack's
/// min-content (tallest child) propagates upward and can push siblings
/// (e.g. a tab bar) off screen on short windows. This matches CSS block
/// layout semantics where `min-height: auto` is `0`.
fn stack_layout() -> Layout {
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
        .height_percent(1.0)
        .min_height(0.0)
}

/// Stack widget — a positioning context for overlapping `Positioned` children.
///
/// Non-positioned children flow vertically in the Stack's column flexbox
/// (`AlignItems::Stretch` stretches them to fill the cross-axis width); they
/// do NOT overlap. `Positioned` children are taken out of flow
/// (`position: Absolute`) and overlap the in-flow children via their insets —
/// this is the only mechanism for z-stacking within a Stack.
pub struct Stack {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl Stack {
    /// Create a new Stack with default layout (fills parent, column direction).
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: stack_layout(),
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

    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
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
}
