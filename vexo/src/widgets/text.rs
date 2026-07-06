//! Text widget - displays a string.

use super::super::key::WidgetKey;
use super::super::render_objects::TextRenderObject;
use super::super::RenderObject;
use super::super::UpdateResult;
use super::{Element, Widget};
use crate::core::Color;
use crate::layout::Layout;
use crate::modifier_methods;
use crate::style::Style;

/// Text widget - displays a string.
pub struct Text {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    color: Color,
    /// Optional font family name. When set, text is shaped against this
    /// family (e.g. an icon font registered via
    /// [`crate::Application::register_fonts`]); when `None`, the framework
    /// default is used.
    font_family: Option<String>,
    style: Style,
    layout: Layout,
}

impl Text {
    /// Create a new text widget.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            key: None,
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            font_family: None,
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

    /// Set the text color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the font family used to shape this text.
    ///
    /// Pass the family name embedded in the font file (e.g. `"iconfont"`).
    /// When set, the text is shaped against this family only; this is the
    /// primary entry point for rendering icon glyphs:
    ///
    /// ```ignore
    /// Text::new("\u{e001}").with_font_family("iconfont").with_font_size(24.0)
    /// ```
    ///
    /// When `None` (the default), the framework's default font is used.
    pub fn with_font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
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

    /// Get the text color.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Get the font family, if any.
    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    modifier_methods!();
}

impl Clone for Text {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            color: self.color,
            font_family: self.font_family.clone(),
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Text {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(
            TextRenderObject::new(&self.content)
                .with_font_size(self.font_size)
                .with_color(self.color)
                .with_font_family(self.font_family.clone())
                .with_style(self.style.clone())
                .with_layout(self.layout.clone()),
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(text_ro) = render_object
            .as_any_mut()
            .downcast_mut::<TextRenderObject>()
        {
            let mut result = UpdateResult::NONE;
            if text_ro.set_content(&self.content) {
                result |= UpdateResult::LAYOUT;
            }
            if text_ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT;
            }
            if text_ro.set_color(self.color) {
                result |= UpdateResult::PAINT;
            }
            if text_ro.set_font_family(self.font_family.clone()) {
                result |= UpdateResult::LAYOUT;
            }
            if text_ro.set_style(self.style.clone()) {
                result |= UpdateResult::PAINT;
            }
            if text_ro.set_layout(self.layout.clone()) {
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
    use super::super::{GlobalKey, Key};
    use super::*;

    #[test]
    fn test_text_widget_creation() {
        let widget = Text::new("Hello");
        assert_eq!(widget.content(), "Hello");
    }

    #[test]
    fn test_text_widget_with_key() {
        let widget = Text::new("Hello").with_key("greeting");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("greeting"))));
    }

    #[test]
    fn test_text_widget_with_global_key() {
        let global_key = GlobalKey::new();
        let widget = Text::new("Hello").with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_text_widget_clone() {
        let widget = Text::new("Hello").with_key("greeting");
        let cloned = widget.clone();

        assert_eq!(widget.content(), cloned.content());
        assert_eq!(widget.key(), cloned.key());
    }

    #[test]
    fn test_text_modifier_background_returns_self() {
        let w = Text::new("Hello").background(crate::core::Color::RED);
        assert_eq!(w.style.background, Some(crate::core::Color::RED));
        assert_eq!(w.content(), "Hello");
    }

    #[test]
    fn test_text_modifier_padding_returns_self() {
        let w = Text::new("Hello").padding(8.0);
        assert!(w.layout.padding.is_some());
        assert_eq!(w.content(), "Hello");
    }

    #[test]
    fn test_text_modifier_chain_preserves_all() {
        let w = Text::new("Hello")
            .background(crate::core::Color::RED)
            .padding(8.0)
            .margin(4.0)
            .border(crate::core::Color::BLACK, 2.0)
            .corner_radius(8.0)
            .clip();
        assert_eq!(w.style.background, Some(crate::core::Color::RED));
        assert!(w.style.border.is_some());
        assert!(w.style.corner_radius.is_some());
        assert!(w.style.clip);
        assert!(w.layout.padding.is_some());
        assert!(w.layout.margin.is_some());
        assert_eq!(w.content(), "Hello");
    }

    #[test]
    fn test_text_widget_with_font_size() {
        let widget = Text::new("Hello").with_font_size(32.0);
        assert_eq!(widget.font_size(), 32.0);
    }

    #[test]
    fn test_text_widget_default_color_is_black() {
        let widget = Text::new("Hello");
        assert_eq!(widget.color(), crate::core::Color::BLACK);
    }

    #[test]
    fn test_text_widget_with_color() {
        let widget = Text::new("Hello").with_color(crate::core::Color::RED);
        assert_eq!(widget.color(), crate::core::Color::RED);
    }

    #[test]
    fn test_text_widget_clone_preserves_color() {
        let widget = Text::new("Hello").with_color(crate::core::Color::RED);
        let cloned = widget.clone();
        assert_eq!(cloned.color(), crate::core::Color::RED);
    }

    #[test]
    fn test_text_widget_modifier_preserves_color() {
        let w = Text::new("Hello")
            .with_color(crate::core::Color::RED)
            .padding(8.0)
            .background(crate::core::Color::BLUE);
        assert_eq!(w.color(), crate::core::Color::RED);
    }

    #[test]
    fn test_text_widget_update_render_object_color_change() {
        let widget = Text::new("Hello").with_color(crate::core::Color::RED);
        let mut ro = TextRenderObject::new("Hello"); // default BLACK
        let result = widget.update_render_object(&mut ro);
        // Color change is paint-only
        assert!(result.contains(UpdateResult::PAINT));
        assert!(!result.contains(UpdateResult::LAYOUT));
        assert_eq!(ro.color(), crate::core::Color::RED);
    }

    #[test]
    fn test_text_widget_update_render_object_color_no_change() {
        // Both widget and RO default to BLACK
        let widget = Text::new("Hello");
        let mut ro = TextRenderObject::new("Hello");
        ro.set_font_size(24.0); // match widget default
        let result = widget.update_render_object(&mut ro);
        // Color didn't change → no PAINT flag from color
        assert!(!result.contains(UpdateResult::PAINT));
    }

    #[test]
    fn test_text_modifier_preserves_font_size() {
        let w = Text::new("Hello").with_font_size(32.0).padding(8.0);
        assert_eq!(w.font_size(), 32.0);
    }

    #[test]
    fn test_text_widget_with_font_family() {
        let w = Text::new("\u{e001}").with_font_family("iconfont");
        assert_eq!(w.font_family(), Some("iconfont"));
        // default is None
        assert!(Text::new("x").font_family().is_none());
    }

    #[test]
    fn test_text_widget_font_family_preserved_through_clone() {
        let w = Text::new("\u{e001}")
            .with_font_family("iconfont")
            .with_font_size(24.0);
        let cloned = w.clone();
        assert_eq!(cloned.font_family(), Some("iconfont"));
        assert_eq!(cloned.font_size(), 24.0);
    }

    #[test]
    fn test_text_widget_font_family_survives_modifier() {
        let w = Text::new("\u{e001}")
            .with_font_family("iconfont")
            .padding(8.0)
            .background(crate::core::Color::RED);
        assert_eq!(w.font_family(), Some("iconfont"));
    }

    #[test]
    fn test_text_widget_update_render_object_family_change_flags_layout() {
        let widget = Text::new("\u{e001}").with_font_family("iconfont");
        let mut ro = TextRenderObject::new("\u{e001}"); // default family None
        ro.set_font_size(24.0); // match widget default
        let result = widget.update_render_object(&mut ro);
        // family changed → LAYOUT
        assert!(result.contains(UpdateResult::LAYOUT));
        assert_eq!(ro.font_family(), Some("iconfont"));
    }

    #[test]
    fn test_text_widget_update_render_object_family_no_change() {
        let widget = Text::new("\u{e001}").with_font_family("iconfont");
        let mut ro =
            TextRenderObject::new("\u{e001}").with_font_family(Some("iconfont".to_string()));
        ro.set_font_size(24.0);
        let result = widget.update_render_object(&mut ro);
        // family unchanged → no LAYOUT flag from family
        assert!(!result.contains(UpdateResult::LAYOUT));
    }
}
