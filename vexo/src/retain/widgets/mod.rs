//! Widget definitions for the retain-mode system.
//!
//! Widgets are immutable configuration objects that describe "what should exist"
//! in the UI. They are cheap to create, rebuilt each frame, and contain no state.

mod container;
mod text;

use std::any::Any;

use super::element::Element;
use super::key::Key;
use super::RenderObject;

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
/// Use the `clone_box` method to clone a boxed widget trait object.
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

    /// Create the render object for this widget.
    ///
    /// Render objects handle layout and painting. They persist across frames
    /// and are only updated when marked dirty.
    fn create_render_object(&self) -> Box<dyn RenderObject>;

    /// Clone this widget into a boxed trait object.
    ///
    /// Required for storing widgets in elements. Implementations should
    /// delegate to their `Clone` implementation.
    fn clone_box(&self) -> Box<dyn Widget>;

    /// Check if this widget can update an existing element.
    ///
    /// Default implementation checks type and key match.
    /// Two widgets can update each other if:
    /// 1. They have the same type (TypeId)
    /// 2. They have matching keys (both None or both Some with equal values)
    fn can_update(&self, other: &dyn Widget) -> bool {
        Any::type_id(self) == Any::type_id(other) && self.key() == other.key()
    }

    /// Get as Any for downcasting.
    ///
    /// This enables downcasting to the concrete widget type for type-specific operations.
    fn as_any(&self) -> &dyn Any;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::element::Element;
    use crate::retain::key::Key;
    use crate::retain::{LayoutContext, RenderObject};
    use crate::layout::LayoutConstraints;
    use crate::core::{Size, Logical};

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

        fn create_render_object(&self) -> Box<dyn RenderObject> {
            Box::new(TestRenderObject)
        }

        fn clone_box(&self) -> Box<dyn Widget> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    struct TestElement;

    impl Element for TestElement {
        fn mount(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn update(&mut self, _new_widget: Box<dyn Widget>, _context: &mut crate::retain::ElementContext) {}
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

    struct TestRenderObject;

    impl RenderObject for TestRenderObject {
        fn layout(&mut self, _constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
            Size::new(100.0, 50.0)
        }

        fn paint(&self, _ctx: &mut crate::retain::PaintContext) -> Vec<crate::render::RenderCommand> {
            vec![]
        }

        fn hit_test(&self, _position: crate::core::Point<Logical>, _ctx: &crate::retain::HitTestContext) -> bool {
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

    #[test]
    fn test_widget_creates_render_object() {
        let widget = Text::new("Hello");
        let mut render_object = widget.create_render_object();

        // Should be able to layout the render object
        let constraints = LayoutConstraints {
            min_width: 0.0,
            max_width: 100.0,
            min_height: 0.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();
        let size = render_object.layout(constraints, &mut ctx);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
    }
}
