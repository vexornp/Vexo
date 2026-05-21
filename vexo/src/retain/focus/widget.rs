//! Focus and FocusScope widgets for the focus system.
//!
//! These widgets integrate their children into the focus tree:
//!
//! - **Focus**: Marks a subtree as focusable. On pointer press, requests focus
//!   for this node via FocusManager. Supports autofocus, can_request_focus,
//!   and skip_traversal configuration.
//!
//! - **FocusScope**: Creates a focus scope boundary. Scopes group focus nodes
//!   and control tab traversal order via a TraversalPolicy.
//!
//! Both widgets use ProxyRenderObject (invisible, pass-through) and have
//! single-child element types that manage focus node lifecycle.

use std::any::Any;

use crate::retain::{
    Element, RenderObject, Widget, WidgetKey,
    ProxyRenderObject,
};
use crate::retain::focus::TraversalPolicy;

// ============================================================================
// FOCUS WIDGET
// ============================================================================

/// Widget that makes its child focusable.
///
/// Focus wraps a child widget and creates a focus node in the FocusManager
/// when mounted. On pointer press inside bounds, it requests focus for
/// this node (user_initiated = true).
///
/// # Configuration
///
/// - `autofocus`: If true, requests focus immediately when mounted
/// - `can_request_focus`: If false, this node cannot receive focus
/// - `skip_traversal`: If true, this node is skipped during tab traversal
///
/// # Example
///
/// ```ignore
/// Focus::new(Box::new(Text::new("Focusable text")))
///     .autofocus(true)
///     .with_key("my-focus")
/// ```
pub struct Focus {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    autofocus: bool,
    can_request_focus: bool,
    skip_traversal: bool,
}

impl Focus {
    /// Create a new Focus widget wrapping a child.
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            key: None,
            child,
            autofocus: false,
            can_request_focus: true,
            skip_traversal: false,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set autofocus. If true, this node requests focus when mounted.
    pub fn autofocus(mut self, value: bool) -> Self {
        self.autofocus = value;
        self
    }

    /// Set can_request_focus. If false, this node cannot receive focus.
    pub fn can_request_focus(mut self, value: bool) -> Self {
        self.can_request_focus = value;
        self
    }

    /// Set skip_traversal. If true, this node is skipped during tab traversal.
    pub fn skip_traversal(mut self, value: bool) -> Self {
        self.skip_traversal = value;
        self
    }

    /// Get the autofocus value.
    pub fn autofocus_value(&self) -> bool {
        self.autofocus
    }

    /// Get the can_request_focus value.
    pub fn can_request_focus_value(&self) -> bool {
        self.can_request_focus
    }

    /// Get the skip_traversal value.
    pub fn skip_traversal_value(&self) -> bool {
        self.skip_traversal
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }
}

impl Clone for Focus {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            autofocus: self.autofocus,
            can_request_focus: self.can_request_focus,
            skip_traversal: self.skip_traversal,
        }
    }
}

impl Widget for Focus {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = super::element::FocusElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ProxyRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        if let Some(other_focus) = other.as_any().downcast_ref::<Focus>() {
            self.key == other_focus.key
        } else {
            false
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// FOCUS SCOPE WIDGET
// ============================================================================

/// Widget that creates a focus scope boundary.
///
/// FocusScope wraps a child widget and creates a scope node in the
/// FocusManager when mounted. Scopes group focus nodes and control
/// tab traversal order via a TraversalPolicy.
///
/// Unlike Focus, FocusScope does not handle pointer events - it only
/// provides scope structure for focus traversal.
///
/// # Example
///
/// ```ignore
/// FocusScope::new(Box::new(
///     Column::new()
///         .push(Focus::new(Box::new(Text::new("Field 1"))))
///         .push(Focus::new(Box::new(Text::new("Field 2"))))
/// ))
/// .policy(TraversalPolicy::WidgetOrder)
/// .with_key("form-scope")
/// ```
pub struct FocusScope {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    traversal_policy: TraversalPolicy,
}

impl FocusScope {
    /// Create a new FocusScope widget wrapping a child.
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            key: None,
            child,
            traversal_policy: TraversalPolicy::WidgetOrder,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the traversal policy for this scope.
    pub fn policy(mut self, policy: TraversalPolicy) -> Self {
        self.traversal_policy = policy;
        self
    }

    /// Get the traversal policy value.
    pub fn traversal_policy_value(&self) -> &TraversalPolicy {
        &self.traversal_policy
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }
}

impl Clone for FocusScope {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            traversal_policy: self.traversal_policy.clone(),
        }
    }
}

impl Widget for FocusScope {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = super::element::FocusScopeElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ProxyRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        if let Some(other_scope) = other.as_any().downcast_ref::<FocusScope>() {
            self.key == other_scope.key
        } else {
            false
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Text, Key, GlobalKey};

    #[test]
    fn test_focus_creation() {
        let focus = Focus::new(Box::new(Text::new("Hello")));
        assert!(focus.key().is_none());
        assert!(!focus.autofocus_value());
        assert!(focus.can_request_focus_value());
        assert!(!focus.skip_traversal_value());
    }

    #[test]
    fn test_focus_with_key() {
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .with_key("my-focus");
        assert_eq!(focus.key(), Some(WidgetKey::Local(Key::new("my-focus"))));
    }

    #[test]
    fn test_focus_with_global_key() {
        let global_key = GlobalKey::new();
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .with_key(global_key.clone());
        assert_eq!(focus.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_focus_autofocus() {
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .autofocus(true);
        assert!(focus.autofocus_value());
    }

    #[test]
    fn test_focus_can_request_focus() {
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .can_request_focus(false);
        assert!(!focus.can_request_focus_value());
    }

    #[test]
    fn test_focus_skip_traversal() {
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .skip_traversal(true);
        assert!(focus.skip_traversal_value());
    }

    #[test]
    fn test_focus_can_update_same_key() {
        let f1 = Focus::new(Box::new(Text::new("Hello"))).with_key("a");
        let f2 = Focus::new(Box::new(Text::new("World"))).with_key("a");
        assert!(f1.can_update(&f2));
    }

    #[test]
    fn test_focus_cannot_update_different_key() {
        let f1 = Focus::new(Box::new(Text::new("Hello"))).with_key("a");
        let f2 = Focus::new(Box::new(Text::new("World"))).with_key("b");
        assert!(!f1.can_update(&f2));
    }

    #[test]
    fn test_focus_cannot_update_different_type() {
        let focus = Focus::new(Box::new(Text::new("Hello")));
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        assert!(!focus.can_update(&scope));
    }

    #[test]
    fn test_focus_clone() {
        let focus = Focus::new(Box::new(Text::new("Hello")))
            .autofocus(true)
            .with_key("test");
        let cloned = focus.clone();
        assert_eq!(cloned.key(), Some(WidgetKey::Local(Key::new("test"))));
        assert!(cloned.autofocus_value());
    }

    #[test]
    fn test_focus_child() {
        let focus = Focus::new(Box::new(Text::new("Hello")));
        assert!(focus.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_scope_creation() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        assert!(scope.key().is_none());
        assert_eq!(*scope.traversal_policy_value(), TraversalPolicy::WidgetOrder);
    }

    #[test]
    fn test_focus_scope_with_key() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")))
            .with_key("my-scope");
        assert_eq!(scope.key(), Some(WidgetKey::Local(Key::new("my-scope"))));
    }

    #[test]
    fn test_focus_scope_policy() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")))
            .policy(TraversalPolicy::ReadingOrder);
        assert_eq!(*scope.traversal_policy_value(), TraversalPolicy::ReadingOrder);
    }

    #[test]
    fn test_focus_scope_can_update_same_key() {
        let s1 = FocusScope::new(Box::new(Text::new("Hello"))).with_key("a");
        let s2 = FocusScope::new(Box::new(Text::new("World"))).with_key("a");
        assert!(s1.can_update(&s2));
    }

    #[test]
    fn test_focus_scope_cannot_update_different_key() {
        let s1 = FocusScope::new(Box::new(Text::new("Hello"))).with_key("a");
        let s2 = FocusScope::new(Box::new(Text::new("World"))).with_key("b");
        assert!(!s1.can_update(&s2));
    }

    #[test]
    fn test_focus_scope_cannot_update_different_type() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        let focus = Focus::new(Box::new(Text::new("Hello")));
        assert!(!scope.can_update(&focus));
    }

    #[test]
    fn test_focus_scope_clone() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")))
            .policy(TraversalPolicy::ReadingOrder)
            .with_key("test");
        let cloned = scope.clone();
        assert_eq!(cloned.key(), Some(WidgetKey::Local(Key::new("test"))));
        assert_eq!(*cloned.traversal_policy_value(), TraversalPolicy::ReadingOrder);
    }

    #[test]
    fn test_focus_scope_child() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        assert!(scope.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_create_element() {
        let focus = Focus::new(Box::new(Text::new("Hello")));
        let _element = focus.create_element();
    }

    #[test]
    fn test_focus_create_render_object() {
        let focus = Focus::new(Box::new(Text::new("Hello")));
        let _ro = focus.create_render_object();
    }

    #[test]
    fn test_focus_scope_create_element() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        let _element = scope.create_element();
    }

    #[test]
    fn test_focus_scope_create_render_object() {
        let scope = FocusScope::new(Box::new(Text::new("Hello")));
        let _ro = scope.create_render_object();
    }
}
