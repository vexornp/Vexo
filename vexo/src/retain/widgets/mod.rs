//! Widget definitions for the retain-mode system.
//!
//! Widgets are immutable configuration objects that describe "what should exist"
//! in the UI. They are cheap to create, rebuilt each frame, and contain no state.

mod container;
mod text;

use std::any::Any;

use super::element::Element;
use super::key::Key;

pub use container::{Column, Row};
pub use text::Text;

/// Immutable widget configuration - rebuilt each frame.
///
/// Widgets describe "what should exist" in the UI. They are:
/// - Cheap to create (no expensive operations in constructors)
/// - Immutable (no internal state that changes)
/// - Clonable (implement `Clone` for easy duplication)
///
/// The widget tree is the first tree in the three-tree architecture:
/// Widget (configuration) -> Element (state) -> RenderObject (layout/paint)
///
/// # Implementing Clone
///
/// All widgets should implement `Clone`. This is not enforced at the trait level
/// to allow the trait to be dyn-compatible (usable as `&dyn Widget`).
pub trait Widget: Any {
    /// Optional key for identity across frames.
    ///
    /// Widgets with matching keys and types can update each other in place,
    /// preserving associated element state.
    fn key(&self) -> Option<Key> {
        None
    }

    /// Create the corresponding element for this widget.
    ///
    /// Called when a new widget is mounted (no matching element exists).
    fn create_element(&self) -> Box<dyn Element>;

    /// Check if this widget can update an existing element.
    ///
    /// Default implementation checks type and key match.
    /// Two widgets can update each other if:
    /// 1. They have the same type (TypeId)
    /// 2. They have matching keys (both None or both Some with equal values)
    fn can_update(&self, other: &dyn Widget) -> bool {
        Any::type_id(self) == Any::type_id(other) && self.key() == other.key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::element::Element;
    use crate::retain::key::Key;

    struct TestWidget {
        key: Option<Key>,
    }

    impl TestWidget {
        fn new(key: Option<&str>) -> Self {
            Self {
                key: key.map(|s| Key::new(s)),
            }
        }
    }

    impl Clone for TestWidget {
        fn clone(&self) -> Self {
            Self {
                key: self.key.clone(),
            }
        }
    }

    impl Widget for TestWidget {
        fn key(&self) -> Option<Key> {
            self.key.clone()
        }

        fn create_element(&self) -> Box<dyn Element> {
            Box::new(TestElement)
        }
    }

    struct TestElement;

    impl Element for TestElement {
        fn mount(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn update(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn unmount(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {}
        fn render_object(&self) -> Option<crate::retain::RenderObjectId> {
            None
        }
        fn widget_key(&self) -> Option<Key> {
            None
        }
        fn can_update(&self, _widget: &dyn std::any::Any) -> bool {
            true
        }
    }

    #[test]
    fn test_widget_key() {
        let widget = TestWidget::new(Some("test"));
        assert_eq!(widget.key(), Some(Key::new("test")));
    }

    #[test]
    fn test_widget_can_update_same_type() {
        let w1 = TestWidget::new(Some("test"));
        let w2 = TestWidget::new(Some("test"));

        assert!(w1.can_update(&w2));
    }

    #[test]
    fn test_widget_can_update_different_key() {
        let w1 = TestWidget::new(Some("test1"));
        let w2 = TestWidget::new(Some("test2"));

        assert!(!w1.can_update(&w2));
    }
}
