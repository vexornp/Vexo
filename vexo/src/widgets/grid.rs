//! Grid widget - CSS Grid layout container.

use super::{Element, Widget};
use super::super::key::WidgetKey;
use crate::style::Style;
#[allow(unused_imports)]
use crate::core::Color;
#[allow(unused_imports)]
use super::super::layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, Display, EdgeInsets, FlexDirection, FlexWrap,
    GridAutoFlow, GridPlacement, Inset, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
#[allow(unused_imports)]
use super::super::core::{Logical, Size};
use crate::layout_builder_methods;
use super::super::render_objects::ContainerRenderObject;
use super::super::{RenderObject, UpdateResult};

/// Default layout for Grid: display grid.
fn grid_layout() -> Layout {
    let mut layout = Layout::default();
    layout.display = Some(Display::Grid);
    layout
}

/// Grid widget - arranges children in a CSS Grid layout.
///
/// Use `.layout()` to set grid template columns/rows and other grid properties.
/// Use `.with_layout()` on children to set grid column/row placement.
pub struct Grid {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
    style: Style,
}

impl Grid {
    /// Create a new empty grid.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: grid_layout(),
            style: Style::default(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the layout properties for this grid.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
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

impl Grid {
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

    // Grid-specific layout methods (not in the macro since only Grid uses them)
    pub fn columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.columns(sizes);
        self
    }

    pub fn rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.rows(sizes);
        self
    }

    pub fn grid_column(mut self, placement: GridPlacement) -> Self {
        self.layout = self.layout.grid_column(placement);
        self
    }

    pub fn grid_row(mut self, placement: GridPlacement) -> Self {
        self.layout = self.layout.grid_row(placement);
        self
    }

    pub fn grid_auto_flow(mut self, value: GridAutoFlow) -> Self {
        self.layout = self.layout.grid_auto_flow(value);
        self
    }

    pub fn auto_rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.auto_rows(sizes);
        self
    }

    pub fn auto_columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.auto_columns(sizes);
        self
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Grid {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
            style: self.style.clone(),
        }
    }
}

impl Widget for Grid {
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

    #[test]
    fn test_grid_gap_preserves_display() {
        let grid = Grid::new().gap(12.0);
        assert_eq!(grid.layout.display, Some(Display::Grid));
        assert!(grid.layout.gap.is_some());
    }

    #[test]
    fn test_grid_columns_method() {
        let grid = Grid::new()
            .columns(vec![TrackSizing::Auto; 3])
            .gap(8.0);
        assert_eq!(grid.layout.display, Some(Display::Grid));
        assert!(grid.layout.grid_template_columns.is_some());
        assert!(grid.layout.gap.is_some());
    }

    #[test]
    fn test_grid_padding_and_gap() {
        let grid = Grid::new().padding(16.0).gap(4.0);
        assert_eq!(grid.layout.display, Some(Display::Grid));
        assert!(grid.layout.padding.is_some());
        assert!(grid.layout.gap.is_some());
    }

    #[test]
    fn test_grid_modifier_background_returns_self() {
        let w = Grid::new().background(Color::RED);
        assert_eq!(w.style.background, Some(Color::RED));
    }

    #[test]
    fn test_grid_modifier_chain() {
        let w = Grid::new()
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
