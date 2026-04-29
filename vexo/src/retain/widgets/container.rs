//! Container widgets - Column and Row for layout.
//!
//! Column arranges children vertically, Row arranges them horizontally.

use super::{Element, Key, Widget};
use super::super::RenderObject;
use super::super::render_objects::ContainerRenderObject;

/// Column widget - arranges children vertically.
pub struct Column<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}

impl<M: Clone + Send + 'static> Column<M> {
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
    pub fn push(mut self, child: impl Widget<M> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}

impl<M: Clone + Send + 'static> Default for Column<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Clone + Send + 'static> Clone for Column<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_box()).collect(),
        }
    }
}

impl<M: Clone + Send + 'static> Widget<M> for Column<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::<M>::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_column())
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}

/// Row widget - arranges children horizontally.
pub struct Row<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}

impl<M: Clone + Send + 'static> Row<M> {
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
    pub fn push(mut self, child: impl Widget<M> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget<M>>] {
        &self.children
    }
}

impl<M: Clone + Send + 'static> Default for Row<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Clone + Send + 'static> Clone for Row<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_box()).collect(),
        }
    }
}

impl<M: Clone + Send + 'static> Widget<M> for Row<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ContainerElement::<M>::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_row())
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget<M>>] {
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
