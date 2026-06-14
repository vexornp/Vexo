//! Flex widget - a flexbox container that arranges children along a direction.

use super::{Element, Widget};
use super::super::key::WidgetKey;
use crate::layout_builder_methods;
use crate::style::Style;
#[allow(unused_imports)]
use crate::core::Color;
#[allow(unused_imports)]
use super::super::layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, EdgeInsets, FlexDirection, FlexWrap,
    Inset, JustifyContent, Layout, Overflow, Position,
};
#[allow(unused_imports)]
use super::super::core::{Logical, Size};
use super::super::render_objects::ContainerRenderObject;
use super::super::{RenderObject, UpdateResult};

/// Default layout for a column: vertical flex with stretch alignment.
fn column_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch)
}

/// Default layout for a row: horizontal flex with stretch alignment.
fn row_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch)
}

/// Flex widget - a flexbox container that arranges children along a direction.
///
/// Use `Flex::column()` for vertical layout, `Flex::row()` for horizontal,
/// or `Flex::new()` which defaults to row.
pub struct Flex {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
    style: Style,
}

impl Flex {
    /// Create a new flex container with row direction (horizontal).
    ///
    /// Equivalent to `Flex::row()`.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: row_layout(),
            style: Style::default(),
        }
    }

    /// Create a flex container with column direction (vertical).
    pub fn column() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: column_layout(),
            style: Style::default(),
        }
    }

    /// Create a flex container with row direction (horizontal).
    pub fn row() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: row_layout(),
            style: Style::default(),
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

    /// Set the layout properties for this flex container.
    ///
    /// Overrides the default layout. Use this to customize
    /// alignment, spacing, padding, and other CSS-like properties.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

impl Flex {
    layout_builder_methods!();

    // Decoration modifier methods (style-based, not layout-based)
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

impl Default for Flex {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Flex {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
            style: self.style.clone(),
        }
    }
}

impl Widget for Flex {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_with_style(self.layout.clone(), self.style.clone()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<ContainerRenderObject>() {
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
    use super::super::Text;
    use super::super::{Key, GlobalKey};

    #[test]
    fn test_flex_column_creation() {
        let col = Flex::column()
            .push(Text::new("First"))
            .push(Text::new("Second"));

        assert_eq!(col.children().len(), 2);
    }

    #[test]
    fn test_flex_row_creation() {
        let row = Flex::row()
            .push(Text::new("Left"))
            .push(Text::new("Right"));

        assert_eq!(row.children().len(), 2);
    }

    #[test]
    fn test_flex_new_is_row() {
        let flex = Flex::new();
        let row = Flex::row();

        // Both should have the same default layout
        assert_eq!(flex.layout, row.layout);
    }

    #[test]
    fn test_flex_with_key() {
        let col = Flex::column()
            .with_key("my-column")
            .push(Text::new("Hello"));

        assert_eq!(col.key(), Some(WidgetKey::Local(Key::new("my-column"))));
    }

    #[test]
    fn test_flex_with_global_key() {
        let global_key = GlobalKey::new();
        let col = Flex::column()
            .with_key(global_key.clone())
            .push(Text::new("Hello"));

        assert_eq!(col.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_flex_gap_preserves_direction() {
        let col = Flex::column().gap(4.0);
        assert_eq!(col.layout.flex_direction, Some(FlexDirection::Column));
        assert!(col.layout.gap.is_some());
    }

    #[test]
    fn test_flex_padding_preserves_direction() {
        let col = Flex::column().padding(8.0);
        assert_eq!(col.layout.flex_direction, Some(FlexDirection::Column));
        assert!(col.layout.padding.is_some());
    }

    #[test]
    fn test_flex_gap_then_align() {
        let col = Flex::column().gap(4.0).align(AlignItems::Center);
        assert_eq!(col.layout.flex_direction, Some(FlexDirection::Column));
        assert!(col.layout.gap.is_some());
        assert_eq!(col.layout.align_items, Some(AlignItems::Center));
    }

    #[test]
    fn test_flex_layout_replaces_everything() {
        let col = Flex::column().layout(Layout::default().gap(4.0));
        // .layout() replaces the entire Layout, so flex_direction is lost
        assert_eq!(col.layout.flex_direction, None);
        assert!(col.layout.gap.is_some());
    }

    #[test]
    fn test_flex_width_height() {
        let row = Flex::row().width(200.0).height(100.0);
        assert_eq!(row.layout.width, Some(Dimension::Length(200.0)));
        assert_eq!(row.layout.height, Some(Dimension::Length(100.0)));
        assert_eq!(row.layout.flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn test_flex_modifier_background_returns_self() {
        let w = Flex::column().background(Color::RED);
        assert_eq!(w.style.background, Some(Color::RED));
    }

    #[test]
    fn test_flex_modifier_chain() {
        let w = Flex::column()
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0)
            .clip()
            .padding(8.0);
        assert_eq!(w.style.background, Some(Color::RED));
        assert!(w.style.border.is_some());
        assert!(w.style.corner_radius.is_some());
        assert!(w.style.clip);
        assert!(w.layout.padding.is_some());
    }
}
