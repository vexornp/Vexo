//! Focus widget definition for the retain-mode system.
//!
//! `Focus` is a single-child wrapper widget that creates a focus node in
//! the FocusManager's focus tree when its element mounts.

use std::any::Any;

use crate::retain::elements::ContainerElement;
use crate::retain::element::Element;
use crate::retain::key::WidgetKey;
use crate::retain::render_object::RenderObject;
use crate::retain::widgets::Widget;
use crate::retain::UpdateResult;

// ============================================================================
// Focus Widget
// ============================================================================

/// A widget that wraps a child and makes it focusable.
///
/// When the corresponding element mounts, a focus node is registered in the
/// FocusManager's focus tree. If `autofocus` is set, the node requests focus
/// during mount.
///
/// Focus is a proxy widget — it delegates rendering entirely to its child.
pub struct Focus {
    child: Box<dyn Widget>,
    autofocus: bool,
}

impl Focus {
    /// Create a new Focus widget wrapping the given child.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            autofocus: false,
        }
    }

    /// Set whether this focus node should automatically request focus on mount.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }
}

impl Clone for Focus {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            autofocus: self.autofocus,
        }
    }
}

impl Widget for Focus {
    fn key(&self) -> Option<WidgetKey> {
        self.child.key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        self.child.create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        // Must both be Focus widgets, then delegate to child comparison
        other.as_any().downcast_ref::<Focus>()
            .map_or(false, |other_focus| {
                self.child.can_update(&*other_focus.child)
            })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        // ContainerElement::mount() uses children() to inflate child widgets.
        // Return the single child as a slice so it gets inflated.
        std::slice::from_ref(&self.child)
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        // Focus is a proxy — delegate to child
        self.child.update_render_object(render_object)
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::widgets::Text;

    #[test]
    fn test_focus_new() {
        let focus = Focus::new(Text::new("Hello"));
        assert!(!focus.autofocus);
        assert!(focus.child().is_some());
    }

    #[test]
    fn test_focus_autofocus() {
        let focus = Focus::new(Text::new("Hello")).autofocus(true);
        assert!(focus.autofocus);
    }

    #[test]
    fn test_focus_key_delegates_to_child() {
        let focus = Focus::new(
            Text::new("Hello").with_key("my-key")
        );
        assert!(focus.key().is_some());
    }

    #[test]
    fn test_focus_child_returns_child() {
        let focus = Focus::new(Text::new("Hello"));
        let child = focus.child().unwrap();
        assert!(child.as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_can_update_same_type() {
        let f1 = Focus::new(Text::new("Hello"));
        let f2 = Focus::new(Text::new("World"));
        assert!(f1.can_update(&f2));
    }

    #[test]
    fn test_focus_clone() {
        let focus = Focus::new(Text::new("Hello")).autofocus(true);
        let cloned = focus.clone();
        assert!(cloned.autofocus);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_focus_create_element() {
        let focus = Focus::new(Text::new("Hello"));
        let _element = focus.create_element();
    }

    #[test]
    fn test_focus_children_returns_child_as_slice() {
        let focus = Focus::new(Text::new("Hello"));
        let children = focus.children();
        assert_eq!(children.len(), 1);
        assert!(children[0].as_any().downcast_ref::<Text>().is_some());
    }
}
