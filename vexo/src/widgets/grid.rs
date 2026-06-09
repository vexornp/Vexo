//! Grid widget - CSS Grid layout container.

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::layout::{Display, Layout};
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
}

impl Grid {
    /// Create a new empty grid.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: grid_layout(),
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
