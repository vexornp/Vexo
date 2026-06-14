//! TextEditContent widget - leaf widget that creates a TextEditRenderObject.
//!
//! This is the leaf widget used by TextEdit's StatefulElement::build() method.
//! It carries the text content, font size, editor reference, and cursor blink
//! state, and creates/updates a TextEditRenderObject accordingly.

use std::cell::RefCell;
use std::rc::Rc;

use crate::editor::Editor;
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::modifier_methods;
use crate::render_objects::TextEditRenderObject;
use crate::style::Style;
use crate::elements::LeafRenderObjectElement;
use crate::{Element, RenderObject, UpdateResult, Widget};

/// Leaf widget that creates a TextEditRenderObject.
///
/// This widget is the "inner" widget produced by TextEdit's build method.
/// It carries all the properties needed to configure the render object:
/// text content, font size, editor state, focus state, and cursor blink
/// visibility.
pub struct TextEditContent {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
    style: Style,
    layout: Layout,
}

impl TextEditContent {
    /// Create a new TextEditContent widget.
    pub fn new(content: impl Into<String>, editor: Rc<RefCell<Editor>>) -> Self {
        Self {
            key: None,
            content: content.into(),
            font_size: 24.0,
            editor,
            is_focused: false,
            cursor_blink_visible: false,
            style: Style::default(),
            layout: Layout::default(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the font size.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set the focused state.
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.is_focused = focused;
        self
    }

    /// Set the cursor blink visibility.
    pub fn with_cursor_blink_visible(mut self, visible: bool) -> Self {
        self.cursor_blink_visible = visible;
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Get whether the widget is focused.
    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    /// Get whether the cursor blink is visible.
    pub fn cursor_blink_visible(&self) -> bool {
        self.cursor_blink_visible
    }

    modifier_methods!();
}

impl Clone for TextEditContent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            editor: self.editor.clone(), // Rc shallow clone
            is_focused: self.is_focused,
            cursor_blink_visible: self.cursor_blink_visible,
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for TextEditContent {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = LeafRenderObjectElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut ro = TextEditRenderObject::new(&self.content, self.editor.clone())
            .with_font_size(self.font_size)
            .with_style(self.style.clone())
            .with_layout(self.layout.clone());
        ro.set_focused(self.is_focused);
        ro.set_cursor_blink_visible(self.cursor_blink_visible);
        Box::new(ro)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object.as_any_mut().downcast_mut::<TextEditRenderObject>() {
            let mut result = UpdateResult::NONE;

            if ro.set_content(&self.content) {
                result |= UpdateResult::LAYOUT;
            }
            if ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT;
            }
            if ro.set_focused(self.is_focused) {
                result |= UpdateResult::PAINT;
            }
            if ro.set_cursor_blink_visible(self.cursor_blink_visible) {
                result |= UpdateResult::PAINT;
            }
            if ro.set_style(self.style.clone()) {
                result |= UpdateResult::PAINT;
            }
            if ro.set_layout(self.layout.clone()) {
                result |= UpdateResult::LAYOUT;
            }

            result
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
    use super::super::{Key, GlobalKey};
    use glyphon::Metrics;

    fn create_test_editor() -> Rc<RefCell<Editor>> {
        // Create an editor with an empty buffer - sufficient for widget-level tests.
        // Render-object-level tests that need actual text content are in
        // render_objects/text_edit.rs.
        let metrics = Metrics::new(16.0, 20.0);
        let raw_editor = glyphon::Editor::new(glyphon::Buffer::new_empty(metrics));
        Rc::new(RefCell::new(Editor::new(raw_editor)))
    }

    #[test]
    fn test_text_edit_content_creation() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor);
        assert_eq!(widget.content(), "Hello");
        assert_eq!(widget.font_size(), 24.0);
        assert!(!widget.is_focused());
        assert!(!widget.cursor_blink_visible());
    }

    #[test]
    fn test_text_edit_content_with_key() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_key("editor");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("editor"))));
    }

    #[test]
    fn test_text_edit_content_with_global_key() {
        let editor = create_test_editor();
        let global_key = GlobalKey::new();
        let widget = TextEditContent::new("Hello", editor).with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_text_edit_content_with_font_size() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_font_size(16.0);
        assert_eq!(widget.font_size(), 16.0);
    }

    #[test]
    fn test_text_edit_content_with_focused() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_focused(true);
        assert!(widget.is_focused());
    }

    #[test]
    fn test_text_edit_content_with_cursor_blink_visible() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_cursor_blink_visible(true);
        assert!(widget.cursor_blink_visible());
    }

    #[test]
    fn test_text_edit_content_clone() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_key("editor")
            .with_font_size(16.0)
            .with_focused(true)
            .with_cursor_blink_visible(true);
        let cloned = widget.clone();

        assert_eq!(widget.content(), cloned.content());
        assert_eq!(widget.font_size(), cloned.font_size());
        assert_eq!(widget.is_focused(), cloned.is_focused());
        assert_eq!(widget.cursor_blink_visible(), cloned.cursor_blink_visible());
        assert_eq!(widget.key(), cloned.key());
    }

    #[test]
    fn test_text_edit_content_create_render_object() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor)
            .with_font_size(16.0)
            .with_focused(true)
            .with_cursor_blink_visible(true);
        let ro = widget.create_render_object();

        // Should be able to downcast to TextEditRenderObject
        let any_ro = ro.as_any();
        assert!(any_ro.downcast_ref::<TextEditRenderObject>().is_some());
    }

    #[test]
    fn test_text_edit_content_update_render_object_no_change() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor);
        let mut ro = TextEditRenderObject::new("Hello", create_test_editor());
        ro.set_font_size(24.0);

        let result = widget.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);
    }

    #[test]
    fn test_text_edit_content_update_render_object_content_change() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("World", editor);
        let mut ro = TextEditRenderObject::new("Hello", create_test_editor());

        let result = widget.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_text_edit_content_update_render_object_focus_change() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_focused(true);
        let mut ro = TextEditRenderObject::new("Hello", create_test_editor());
        ro.set_font_size(24.0); // Match widget's default font_size

        let result = widget.update_render_object(&mut ro);
        // Focus change is paint-only
        assert!(result.contains(UpdateResult::PAINT));
        assert!(!result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_text_edit_content_update_render_object_blink_change() {
        let editor = create_test_editor();
        let widget = TextEditContent::new("Hello", editor).with_cursor_blink_visible(true);
        let mut ro = TextEditRenderObject::new("Hello", create_test_editor());
        ro.set_font_size(24.0); // Match widget's default font_size

        let result = widget.update_render_object(&mut ro);
        // Blink change is paint-only
        assert!(result.contains(UpdateResult::PAINT));
        assert!(!result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_text_edit_content_modifier_background_returns_self() {
        let editor = create_test_editor();
        let w = TextEditContent::new("Hello", editor).background(crate::core::Color::RED);
        assert_eq!(w.style.background, Some(crate::core::Color::RED));
        assert_eq!(w.content(), "Hello");
    }

    #[test]
    fn test_text_edit_content_modifier_padding_returns_self() {
        let editor = create_test_editor();
        let w = TextEditContent::new("Hello", editor).padding(8.0);
        assert!(w.layout.padding.is_some());
        assert_eq!(w.content(), "Hello");
    }

    #[test]
    fn test_text_edit_content_modifier_chain_preserves_all() {
        let editor = create_test_editor();
        let w = TextEditContent::new("Hello", editor)
            .background(crate::core::Color::RED)
            .padding(8.0)
            .border(crate::core::Color::BLACK, 2.0)
            .corner_radius(4.0);
        assert_eq!(w.style.background, Some(crate::core::Color::RED));
        assert!(w.style.border.is_some());
        assert!(w.style.corner_radius.is_some());
        assert!(w.layout.padding.is_some());
        assert_eq!(w.content(), "Hello");
    }
}
