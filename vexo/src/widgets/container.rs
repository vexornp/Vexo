//! Container widgets - Column and Row for layout.
//!
//! Column arranges children vertically, Row arranges them horizontally.

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::layout::{AlignItems, FlexDirection, Layout};
use super::super::render_objects::ContainerRenderObject;
use super::super::{RenderObject, UpdateResult};

/// Default layout for a Column: vertical flex with stretch alignment.
fn column_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch)
}

/// Default layout for a Row: horizontal flex with stretch alignment.
fn row_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch)
}

/// Column widget - arranges children vertically.
pub struct Column {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl Column {
    /// Create a new empty column.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: column_layout(),
        }
    }

    /// Set the key for this widget.
    ///
    /// Accepts both local keys (strings) and global keys.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Set the layout properties for this column.
    ///
    /// Overrides the default column layout. Use this to customize
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

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Column {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            // Clone each child widget using clone_boxed() method
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Column {
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
        if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<ContainerRenderObject>() {
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

/// Row widget - arranges children horizontally.
pub struct Row {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl Row {
    /// Create a new empty row.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: row_layout(),
        }
    }

    /// Set the key for this widget.
    ///
    /// Accepts both local keys (strings) and global keys.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Set the layout properties for this row.
    ///
    /// Overrides the default row layout. Use this to customize
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

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Row {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Row {
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
        if let Some(container_ro) = render_object.as_any_mut().downcast_mut::<ContainerRenderObject>() {
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
    use super::super::Text;
    use super::super::{Key, GlobalKey};

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