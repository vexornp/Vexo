//! Container widgets - Column and Row for layout.
//!
//! Column arranges children vertically, Row arranges them horizontally.

use super::{Element, Key, Widget};
use super::super::{RenderObject, RenderObjectId};
use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{LayoutContext, PaintContext, HitTestContext};

/// Column widget - arranges children vertically.
pub struct Column {
    key: Option<Key>,
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
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
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

/// Row widget - arranges children horizontally.
pub struct Row {
    key: Option<Key>,
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
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
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
        // Container layout is handled by Taffy
        // Return constrained size for now
        let size = Size::new(
            constraints.max_width,
            constraints.max_height,
        );
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Containers don't paint themselves
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
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

        assert_eq!(column.key(), Some(Key::new("my-column")));
    }

    #[test]
    fn test_row_creation() {
        let row = Row::new()
            .push(Text::new("Left"))
            .push(Text::new("Right"));

        assert_eq!(row.children().len(), 2);
    }
}
