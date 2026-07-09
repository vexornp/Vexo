//! Widget definitions for the retain-mode system.
//!
//! Widgets are immutable configuration objects that describe "what should exist"
//! in the UI. They are cheap to create, rebuilt each frame, and contain no state.

mod container;
mod decorated_container;
mod gesture_detector;
mod grid;
mod image;
mod indexed_stack;
mod mouse_region;
mod offstage;
mod opacity;
mod positioned;
mod safe_area;
pub(crate) mod scroll_view;
mod stack;
mod text;
mod text_edit;
mod text_edit_content;
pub(crate) mod transform;
mod transitions;
mod with_layout;

use std::any::Any;

use super::element::Element;
use super::key::WidgetKey;
use super::RenderObject;
use super::UpdateResult;

// Public API - leaf and container widgets
pub use super::{GlobalKey, Key};
pub use container::{ChildPush, Column, Flex, Row};
pub use grid::Grid;
pub use image::Image;
pub use safe_area::SafeArea;
pub use scroll_view::ScrollView;
pub use text::Text;
pub use text_edit::{TextEdit, TextEditState, TextEditingController};

// Crate-internal modifier widgets (not part of public API)
use crate::core::Color;
use crate::input::MouseCursor;
use crate::layout::Layout;
pub use decorated_container::DecoratedContainer;
pub(crate) use gesture_detector::GestureDetector;
pub use indexed_stack::IndexedStack;
pub(crate) use mouse_region::MouseRegion;
pub use offstage::Offstage;
pub use opacity::Opacity;
pub use positioned::Positioned;
pub use stack::Stack;
pub(crate) use text_edit_content::TextEditContent;
pub use transform::Transform;
pub use transitions::{FadeTransition, SlideDirection, SlideTransition};
pub use with_layout::WithLayout;

/// Immutable widget configuration - rebuilt each frame.
///
/// Widgets describe "what should exist" in the UI. They are:
/// - Cheap to create (no expensive operations in constructors)
/// - Immutable (no internal state that changes)
///
/// The widget tree is the first tree in the three-tree architecture:
/// Widget (configuration) -> Element (state) -> RenderObject (layout/paint)
///
/// Note: Widget does not require `Clone` as a supertrait because that would make
/// the trait not object-safe. Instead, the `clone_boxed()` method provides a way
/// to clone widgets through trait objects.
pub trait Widget: Any {
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

    /// Get the child widget, if this is a modifier widget.
    ///
    /// Returns None for leaf widgets and multi-child containers.
    /// Returns Some(child) for single-child modifier widgets like Background, Padding, Border.
    fn child(&self) -> Option<&dyn Widget> {
        None
    }

    /// Get the children widgets for container widgets.
    ///
    /// Returns an empty slice for leaf widgets and single-child modifiers.
    /// Returns the children for multi-child containers like Flex.
    fn children(&self) -> &[Box<dyn Widget>] {
        &[]
    }

    /// Update an existing render object with new properties from this widget.
    ///
    /// Called during Element::update() when a widget is updated in place.
    /// The widget should update the render object's mutable properties
    /// (e.g., text content, colors, sizes) to reflect the new configuration.
    ///
    /// # Returns
    ///
    /// An `UpdateResult` indicating what changed:
    /// - `UpdateResult::NONE` if nothing changed (no dirty marking)
    /// - `UpdateResult::LAYOUT` if only layout-affecting properties changed
    /// - `UpdateResult::PAINT` if only visual properties changed
    /// - `UpdateResult::ALL` if both types changed
    ///
    /// The default implementation returns `UpdateResult::ALL` for backward
    /// compatibility. Widgets that want to optimize should override this
    /// method and implement property comparison.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    ///     if let Some(text_ro) = render_object.as_any_mut().downcast_mut::<TextRenderObject>() {
    ///         if text_ro.set_content(&self.content) {
    ///             UpdateResult::LAYOUT
    ///         } else {
    ///             UpdateResult::NONE
    ///         }
    ///     } else {
    ///         UpdateResult::NONE
    ///     }
    /// }
    /// ```
    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        // Default: assume nothing changed. Widgets with mutable properties
        // must override this and return LAYOUT/PAINT when their properties differ.
        UpdateResult::NONE
    }

    /// Clone this widget into a boxed trait object.
    ///
    /// This method is necessary because `Box<dyn Widget>` cannot be cloned directly
    /// even when `Widget: Clone`. Each widget implementation must provide this method
    /// to enable cloning of widget trees stored as trait objects.
    fn clone_boxed(&self) -> Box<dyn Widget>;

    /// Wrap this widget with layout properties.
    ///
    /// The Vexo equivalent of inline styles on a child element in CSS.
    fn with_layout(self, layout: Layout) -> WithLayout
    where
        Self: Sized + 'static,
    {
        WithLayout::new(self, layout)
    }

    /// Box this widget into a `Box<dyn Widget>`.
    fn boxed(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }

    // Decoration modifiers (fallback: wrap in DecoratedContainer)

    fn background(self, color: Color) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).background(color))
    }

    fn border(self, color: Color, width: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).border(color, width))
    }

    fn corner_radius(self, radius: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).corner_radius(radius))
    }

    fn clip(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).clip())
    }

    // Layout modifiers (fallback: wrap in WithLayout)

    fn padding(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().padding(value)))
    }

    fn margin(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().margin(value)))
    }

    fn width(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().width(value)))
    }

    fn height(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().height(value)))
    }

    fn flex_grow(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().flex_grow(value)))
    }

    fn align_self(self, value: crate::layout::AlignSelf) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().align_self(value)))
    }

    fn absolute(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(WithLayout::new(self, Layout::default().absolute()))
    }

    // Behavioral modifiers (always wrap)

    fn on_press(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(GestureDetector::new(self).on_press(callback))
    }

    fn on_release(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(GestureDetector::new(self).on_release(callback))
    }

    fn cursor(self, cursor: MouseCursor) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(MouseRegion::new(self).cursor(cursor))
    }

    fn on_enter(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(MouseRegion::new(self).on_enter(callback))
    }

    fn on_exit(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(MouseRegion::new(self).on_exit(callback))
    }

    // Transform modifiers (always wrap)

    fn translate(self, dx: f32, dy: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(Transform::translate(self, dx, dy))
    }

    fn rotate(self, radians: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(Transform::rotate(self, radians))
    }

    fn scale(self, sx: f32, sy: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(Transform::scale(self, sx, sy))
    }

    fn opacity(self, value: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(Opacity::new(self, value))
    }
}

/// Delegate Widget implementation for `Box<dyn Widget>`.
///
/// This enables modifier chaining on boxed widgets: after a behavioral or
/// transform modifier returns `Box<dyn Widget>`, further trait methods can
/// still be called because `Box<dyn Widget>` itself implements `Widget`.
impl Widget for Box<dyn Widget> {
    fn key(&self) -> Option<WidgetKey> {
        (**self).key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        (**self).create_element()
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        (**self).create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        (**self).can_update(other)
    }

    fn as_any(&self) -> &dyn Any {
        (**self).as_any()
    }

    fn child(&self) -> Option<&dyn Widget> {
        (**self).child()
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        (**self).children()
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        (**self).update_render_object(render_object)
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        (**self).clone_boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Logical;
    use crate::element::Element;
    use crate::input::SystemCursorKind;
    use crate::key::{Key, WidgetKey};
    use crate::layout::TaffyLayoutEngine;
    use crate::{LayoutContext, RenderObject};
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

    impl Widget for TestWidget {
        fn key(&self) -> Option<WidgetKey> {
            self.key.clone()
        }

        fn create_element(&self) -> Box<dyn Element> {
            Box::new(TestElement {
                focus_attachment: None,
            })
        }

        fn create_render_object(&self) -> Box<dyn RenderObject> {
            Box::new(TestRenderObject { layout_node: None })
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn clone_boxed(&self) -> Box<dyn Widget> {
            Box::new(self.clone())
        }
    }

    struct TestElement {
        focus_attachment: Option<crate::focus::attachment::FocusAttachment>,
    }

    impl Element for TestElement {
        fn mount(&mut self, _context: &mut crate::ElementContext) {}
        fn update(
            &mut self,
            _new_widget: Box<dyn std::any::Any>,
            _context: &mut crate::ElementContext,
        ) {
        }
        fn unmount(&mut self, _context: &mut crate::ElementContext) {}
        fn render_object(&self) -> Option<crate::RenderObjectKey> {
            None
        }
        fn widget_key(&self) -> Option<WidgetKey> {
            None
        }
        fn can_update(&self, _widget: &dyn std::any::Any) -> bool {
            true
        }
        fn focus_attachment(&self) -> &Option<crate::focus::attachment::FocusAttachment> {
            &self.focus_attachment
        }
        fn focus_attachment_mut(
            &mut self,
        ) -> &mut Option<crate::focus::attachment::FocusAttachment> {
            &mut self.focus_attachment
        }
    }

    struct TestRenderObject {
        layout_node: Option<crate::layout::LayoutNodeKey>,
    }

    impl RenderObject for TestRenderObject {
        fn layout(
            &mut self,
            ctx: &mut LayoutContext,
            _child_nodes: &[crate::layout::LayoutNodeKey],
        ) -> crate::LayoutResult {
            let node = ctx.engine().create_leaf(&crate::layout::Layout::default());
            self.layout_node = Some(node);
            crate::LayoutResult {
                node,
                size: crate::core::Size::new(100.0, 50.0),
            }
        }

        fn apply_layout(&mut self, _ctx: &mut LayoutContext) {
            // Test implementation
        }

        fn paint(&self, _ctx: &mut crate::PaintContext) -> Vec<crate::render::RenderCommand> {
            vec![]
        }

        fn hit_test(
            &self,
            _position: crate::core::Point<Logical>,
            _ctx: &crate::HitTestContext,
        ) -> bool {
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
        let widget = Text::new("Hello");
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

    #[test]
    fn test_widget_trait_on_press_wraps() {
        let widget = Text::new("Click").on_press(|| {});
        assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
    }

    #[test]
    fn test_widget_trait_cursor_wraps() {
        let widget = Text::new("Hover").cursor(MouseCursor::System(SystemCursorKind::Pointer));
        assert!(widget.as_any().downcast_ref::<MouseRegion>().is_some());
    }

    #[test]
    fn test_widget_trait_translate_wraps() {
        let widget = Text::new("Shift").translate(10.0, 20.0);
        assert!(widget.as_any().downcast_ref::<Transform>().is_some());
    }

    #[test]
    fn test_widget_trait_on_press_chain() {
        let widget = Text::new("Click")
            .background(Color::RED)
            .padding(8.0)
            .on_press(|| {});
        // Text with style/layout set, then wrapped in GestureDetector
        assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
    }

    #[test]
    fn test_widget_trait_boxed() {
        let widget = Text::new("Hello").boxed();
        assert!(widget.as_any().downcast_ref::<Text>().is_some());
    }
}
