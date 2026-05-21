//! Focus and FocusScope widget definitions for the retain-mode system.
//!
//! These are single-child wrapper widgets that create focus nodes/scopes in
//! the FocusManager's focus tree when their elements mount.
//!
//! - `Focus` — wraps a child and registers a focusable node; supports autofocus
//! - `FocusScope` — wraps a child and registers a focus scope (grouping boundary)

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
        Box::new(ContainerElement::new())
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

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        // Focus is a proxy — delegate to child
        self.child.update_render_object(render_object)
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// FocusScope Widget
// ============================================================================

/// A widget that wraps a child and creates a focus scope boundary.
///
/// When the corresponding element mounts, a focus scope node is registered
/// in the FocusManager's focus tree. Scopes group focusable nodes and
/// maintain their own "focused child" memory for scope-aware traversal.
///
/// FocusScope is a proxy widget — it delegates rendering entirely to its child.
pub struct FocusScope {
    child: Box<dyn Widget>,
}

impl FocusScope {
    /// Create a new FocusScope widget wrapping the given child.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
        }
    }
}

impl Clone for FocusScope {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
        }
    }
}

impl Widget for FocusScope {
    fn key(&self) -> Option<WidgetKey> {
        self.child.key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ContainerElement::new())
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        self.child.create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        // Must both be FocusScope widgets, then delegate to child comparison
        other.as_any().downcast_ref::<FocusScope>()
            .map_or(false, |other_scope| {
                self.child.can_update(&*other_scope.child)
            })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        // FocusScope is a proxy — delegate to child
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
    fn test_focus_can_update_different_type() {
        let focus = Focus::new(Text::new("Hello"));
        let scope = FocusScope::new(Text::new("Hello"));
        assert!(!focus.can_update(&scope));
    }

    #[test]
    fn test_focus_clone() {
        let focus = Focus::new(Text::new("Hello")).autofocus(true);
        let cloned = focus.clone();
        assert!(cloned.autofocus);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_focus_scope_new() {
        let scope = FocusScope::new(Text::new("Hello"));
        assert!(scope.child().is_some());
    }

    #[test]
    fn test_focus_scope_key_delegates_to_child() {
        let scope = FocusScope::new(
            Text::new("Hello").with_key("scope-key")
        );
        assert!(scope.key().is_some());
    }

    #[test]
    fn test_focus_scope_child_returns_child() {
        let scope = FocusScope::new(Text::new("Hello"));
        let child = scope.child().unwrap();
        assert!(child.as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_scope_can_update_same_type() {
        let s1 = FocusScope::new(Text::new("Hello"));
        let s2 = FocusScope::new(Text::new("World"));
        assert!(s1.can_update(&s2));
    }

    #[test]
    fn test_focus_scope_can_update_different_type() {
        let scope = FocusScope::new(Text::new("Hello"));
        let focus = Focus::new(Text::new("Hello"));
        assert!(!scope.can_update(&focus));
    }

    #[test]
    fn test_focus_scope_clone() {
        let scope = FocusScope::new(Text::new("Hello"));
        let cloned = scope.clone();
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_focus_create_element() {
        let focus = Focus::new(Text::new("Hello"));
        let _element = focus.create_element();
    }

    #[test]
    fn test_focus_scope_create_element() {
        let scope = FocusScope::new(Text::new("Hello"));
        let _element = scope.create_element();
    }
}
