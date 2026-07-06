//! IndexedStack widget — shows one child at a time while keeping all children mounted.
//!
//! `IndexedStack` holds N children but only displays the child at `index`. The other
//! children are kept mounted (via `Offstage`) so their state — `ComponentState`, focus,
//! `TextEditingController`, animations — is preserved when switching between them.
//!
//! This is the key primitive for navigation stacks: each page is a child, and the
//! `index` points to the top of the stack. Pushing increments `index` (new child
//! inflated, previous children stay mounted offstage); popping decrements `index`
//! (the formerly-top child is unmounted, the new top becomes visible).
//!
//! This matches Flutter's `IndexedStack` widget.
//!
//! # Example
//!
//! ```ignore
//! IndexedStack::new(1)
//!     .push(Text::new("Page 0"))   // offstage
//!     .push(Text::new("Page 1"))   // visible (index == 1)
//!     .push(Text::new("Page 2"))   // offstage
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
use super::{Element, Offstage, Widget};
use crate::layout_builder_methods;
use crate::style::Style;

/// Default layout for an IndexedStack: fills parent, column direction, start-aligned.
/// Matches `Stack`'s defaults so the visible child fills the stack.
fn indexed_stack_layout() -> Layout {
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Start)
        .width_percent(1.0)
        .height_percent(1.0)
}

/// A widget that shows one child at a time while keeping all children mounted.
///
/// All children stay in the element tree (state preserved); only the child at
/// `index` is visible. The others are wrapped in `Offstage(offstage: true)`.
///
/// Internally, each pushed child is immediately wrapped in an `Offstage` widget
/// (offstage = position != index). The wrapped children are stored and exposed
/// via `Widget::children()`, so the element tree reconciles them positionally:
/// when `index` changes, `OffstageElement`s are updated in place (same type),
/// only their `offstage` flag flips — preserving the underlying page elements
/// and their state.
pub struct IndexedStack {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    index: usize,
    layout: Layout,
    style: Style,
}

impl IndexedStack {
    /// Create a new IndexedStack showing the child at `index`.
    pub fn new(index: usize) -> Self {
        Self {
            key: None,
            children: Vec::new(),
            index,
            layout: indexed_stack_layout(),
            style: Style::default(),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    ///
    /// The child is wrapped in `Offstage` immediately: offstage if its position
    /// != `index`, onstage otherwise.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        let i = self.children.len();
        let offstage = i != self.index;
        let wrapped = Offstage::new(child, offstage).boxed();
        self.children.push(wrapped);
        self
    }

    /// The currently visible child index.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl IndexedStack {
    layout_builder_methods!();

    pub fn background(mut self, color: crate::core::Color) -> Self {
        self.style = self.style.background(color);
        self
    }

    pub fn border(mut self, color: crate::core::Color, width: f32) -> Self {
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

impl Default for IndexedStack {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clone for IndexedStack {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            index: self.index,
            layout: self.layout.clone(),
            style: self.style.clone(),
        }
    }
}

impl Widget for IndexedStack {
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
    use super::*;
    use crate::Text;

    #[test]
    fn test_indexed_stack_creation() {
        let s = IndexedStack::new(1)
            .push(Text::new("A"))
            .push(Text::new("B"))
            .push(Text::new("C"));
        assert_eq!(s.index(), 1);
        assert_eq!(s.children.len(), 3);
    }

    #[test]
    fn test_indexed_stack_children_are_offstage_wrapped() {
        let s = IndexedStack::new(1)
            .push(Text::new("A"))
            .push(Text::new("B"))
            .push(Text::new("C"));

        // Each child should be an Offstage widget
        assert_eq!(s.children.len(), 3);
        for (i, child) in s.children.iter().enumerate() {
            let offstage = child.as_any().downcast_ref::<Offstage>();
            assert!(offstage.is_some(), "child {} should be Offstage", i);
            let offstage = offstage.unwrap();
            assert_eq!(
                offstage.is_offstage(),
                i != 1,
                "child {} offstage flag wrong (expected {}, got {})",
                i,
                i != 1,
                offstage.is_offstage()
            );
        }
    }

    #[test]
    fn test_indexed_stack_clone_preserves_wrappers() {
        let s = IndexedStack::new(0)
            .push(Text::new("A"))
            .push(Text::new("B"));
        let cloned = s.clone();
        assert_eq!(cloned.children.len(), 2);
        assert_eq!(cloned.index(), 0);
        // Cloned children should still be Offstage
        assert!(cloned.children[0]
            .as_any()
            .downcast_ref::<Offstage>()
            .is_some());
    }
}
