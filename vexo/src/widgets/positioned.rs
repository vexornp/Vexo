//! Positioned widget — absolutely positions its child within a Stack.
//!
//! A `Positioned` widget must be a descendant of a `Stack` (which provides the
//! `position: Relative` containing block). The insets (`top`/`right`/`bottom`/`left`)
//! position the child relative to the Stack's edges.
//!
//! This matches Flutter's `Positioned` widget.

use std::any::Any;

use crate::elements::{PositionedElement, RenderObjectElement};
use crate::render_objects::{PositionedInsets, PositionedRenderObject};
use crate::{Element, RenderObject, UpdateResult, Widget, WidgetKey};

/// A widget that absolutely positions its child within a Stack.
///
/// # Example
///
/// ```ignore
/// Stack::new()
///     .push(Positioned::new(Text::new("Top-Left")).top(10.0).left(10.0))
///     .push(Positioned::new(Text::new("Bottom-Right")).bottom(10.0).right(10.0))
/// ```
pub struct Positioned {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    insets: PositionedInsets,
}

impl Positioned {
    /// Create a new positioned widget with no insets.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            insets: PositionedInsets::new(),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the top inset.
    pub fn top(mut self, value: f32) -> Self {
        self.insets.top = Some(value);
        self
    }

    /// Set the right inset.
    pub fn right(mut self, value: f32) -> Self {
        self.insets.right = Some(value);
        self
    }

    /// Set the bottom inset.
    pub fn bottom(mut self, value: f32) -> Self {
        self.insets.bottom = Some(value);
        self
    }

    /// Set the left inset.
    pub fn left(mut self, value: f32) -> Self {
        self.insets.left = Some(value);
        self
    }
}

impl Clone for Positioned {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            insets: self.insets,
        }
    }
}

impl Widget for Positioned {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = PositionedElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(PositionedRenderObject::new(self.insets))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<PositionedRenderObject>()
        {
            if ro.set_insets(self.insets) {
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
    use crate::Text;

    #[test]
    fn test_positioned_creation() {
        let w = Positioned::new(Text::new("Hello")).top(10.0).left(20.0);
        assert_eq!(w.insets.top, Some(10.0));
        assert_eq!(w.insets.left, Some(20.0));
        assert_eq!(w.insets.right, None);
    }

    #[test]
    fn test_positioned_render_object_creation() {
        let w = Positioned::new(Text::new("Hello")).top(10.0);
        let ro = w.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<PositionedRenderObject>()
            .is_some());
    }

    #[test]
    fn test_positioned_update_render_object_no_change() {
        let w = Positioned::new(Text::new("Hello")).top(10.0);
        let mut ro = PositionedRenderObject::new(PositionedInsets::new().top(10.0));
        let result = w.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);
    }

    #[test]
    fn test_positioned_update_render_object_change() {
        let w = Positioned::new(Text::new("Hello")).top(20.0);
        let mut ro = PositionedRenderObject::new(PositionedInsets::new().top(10.0));
        let result = w.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::LAYOUT));
    }
}
