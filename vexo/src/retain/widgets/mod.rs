//! Widget definitions for the retain-mode system.
//!
//! Widgets are immutable configuration objects that describe "what should exist"
//! in the UI. They are cheap to create, rebuilt each frame, and contain no state.

mod background;
mod border;
mod button;
mod container;
mod corner_radius;
mod decorated_container;
mod gesture_detector;
mod text;

use std::any::Any;

use super::element::Element;
use super::key::WidgetKey;
use super::RenderObject;

pub use background::Background;
pub use border::Border;
pub use button::Button;
pub use container::{Column, Row};
pub use corner_radius::CornerRadius;
pub use gesture_detector::GestureDetector;
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
/// # Type Parameter
///
/// The `M` type parameter is the message type emitted by interactive widgets.
/// Non-interactive widgets (Text, Container, etc.) use `M = ()`.
/// Interactive widgets (Button) are generic over the application's message type.
///
/// # Implementing Clone
///
/// All widgets should implement `Clone`. This is not enforced at the trait level
/// to allow the trait to be dyn-compatible (usable as `&dyn Widget<M>`).
/// Use the `clone_box` method to clone a boxed widget trait object.
pub trait Widget<M: Clone + Send + 'static>: Any {
    /// Optional key for identity across frames.
    ///
    /// Widgets with matching keys and types can update each other in place,
    /// preserving associated element state.
    ///
    /// Returns `WidgetKey::Local(Key)` for local keys (match within parent's children)
    /// or `WidgetKey::Global(GlobalKey)` for global keys (match anywhere in tree).
    fn key(&self) -> Option<WidgetKey> {
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
    fn clone_box(&self) -> Box<dyn Widget<M>>;

    /// Check if this widget can update an existing element.
    ///
    /// Default implementation checks type and key match.
    /// Two widgets can update each other if:
    /// 1. They have the same type (TypeId)
    /// 2. They have matching keys (both None or both Some with equal values)
    fn can_update(&self, other: &dyn Widget<M>) -> bool {
        Any::type_id(self) == Any::type_id(other) && self.key() == other.key()
    }

    /// Get as Any for downcasting.
    ///
    /// This enables downcasting to the concrete widget type for type-specific operations.
    fn as_any(&self) -> &dyn Any;

    /// Get the child widget, if this is a modifier widget.
    ///
    /// Returns None for leaf widgets and multi-child containers.
    /// Returns Some(child) for single-child modifier widgets like Background, Padding, Border.
    fn child(&self) -> Option<&dyn Widget<M>> {
        None
    }

    /// Get the children widgets for container widgets.
    ///
    /// Returns an empty slice for leaf widgets and single-child modifiers.
    /// Returns the children for multi-child containers like Column, Row.
    fn children(&self) -> &[Box<dyn Widget<M>>] {
        &[]
    }

    /// Update an existing render object with new properties from this widget.
    ///
    /// Called during Element::update() when a widget is updated in place.
    /// The widget should update the render object's mutable properties
    /// (e.g., text content, colors, sizes) to reflect the new configuration.
    ///
    /// Default implementation does nothing (for widgets with no updatable properties
    /// like GestureDetector, or containers that don't have visual properties).
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn update_render_object(&self, render_object: &mut dyn RenderObject) {
    ///     if let Some(text_ro) = render_object.as_any_mut().downcast_mut::<TextRenderObject>() {
    ///         text_ro.set_content(&self.content);
    ///     }
    /// }
    /// ```
    fn update_render_object(&self, _render_object: &mut dyn RenderObject) {
        // Default: no-op for widgets without updatable properties
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::element::{Element, ElementRegistry};
    use crate::retain::key::{Key, WidgetKey};
    use crate::retain::{LayoutContext, RenderObject};
    use crate::layout::TaffyLayoutEngine;
    use crate::core::Logical;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    struct TestWidget {
        key: Option<WidgetKey>,
    }

    impl TestWidget {
        fn new(key: Option<&str>) -> Self {
            Self {
                key: key.map(|s| WidgetKey::Local(Key::new(s))),
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

    impl Widget<()> for TestWidget {
        fn key(&self) -> Option<WidgetKey> {
            self.key.clone()
        }

        fn create_element(&self) -> Box<dyn Element> {
            Box::new(TestElement)
        }

        fn create_render_object(&self) -> Box<dyn RenderObject> {
            Box::new(TestRenderObject { layout_node: None })
        }

        fn clone_box(&self) -> Box<dyn Widget<()>> {
            Box::new(self.clone())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    struct TestElement;

    impl Element for TestElement {
        fn mount(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn update(&mut self, _new_widget: Box<dyn std::any::Any>, _context: &mut crate::retain::ElementContext) {}
        fn unmount(&mut self, _context: &mut crate::retain::ElementContext) {}
        fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
        fn render_object(&self) -> Option<crate::retain::RenderObjectId> {
            None
        }
        fn widget_key(&self) -> Option<WidgetKey> {
            None
        }
        fn can_update(&self, _widget: &dyn std::any::Any) -> bool {
            true
        }
    }

    struct TestRenderObject {
        layout_node: Option<crate::layout::LayoutNodeId>,
    }

    impl RenderObject for TestRenderObject {
        fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[crate::layout::LayoutNodeId]) -> crate::retain::LayoutResult {
            let node = ctx.engine().create_leaf(&crate::layout::Layout::default());
            self.layout_node = Some(node);
            crate::retain::LayoutResult {
                node,
                size: crate::core::Size::new(100.0, 50.0),
            }
        }

        fn apply_layout(&mut self, _ctx: &LayoutContext) {
            // Test implementation
        }

        fn paint(&self, _ctx: &mut crate::retain::PaintContext) -> Vec<crate::render::RenderCommand> {
            vec![]
        }

        fn hit_test(&self, _position: crate::core::Point<Logical>, _ctx: &crate::retain::HitTestContext) -> bool {
            true
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_widget_key() {
        let widget = TestWidget::new(Some("test"));
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("test"))));
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
        let widget: Text<()> = Text::new("Hello");
        let mut render_object = widget.create_render_object();

        // Should be able to layout the render object
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        let result = render_object.layout(&mut ctx, &[]);

        // Should have created a layout node (node ID is valid)
        // Just verify no panic during layout
        let _ = result;
    }
}
