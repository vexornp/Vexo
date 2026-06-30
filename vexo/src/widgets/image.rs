//! Image widget - displays an image from ImageData.

use crate::element::Element;
use crate::elements::LeafElement;
use crate::image_data::{ImageData, ImageDataError};
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::modifier_methods;
use crate::render_object::RenderObject;
use crate::render_objects::ImageRenderObject;
use crate::style::Style;
use crate::update_result::UpdateResult;
use crate::widgets::Widget;

/// Image widget - displays an image from ImageData.
pub struct Image {
    key: Option<WidgetKey>,
    image_data: ImageData,
    style: Style,
    layout: Layout,
}

impl Image {
    /// Create a new image widget from ImageData.
    pub fn new(image_data: ImageData) -> Self {
        Self {
            key: None,
            image_data,
            style: Style::default(),
            layout: Layout::default(),
        }
    }

    /// Create a new image widget from raw bytes (e.g., PNG, JPEG).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageDataError> {
        let image_data = ImageData::from_bytes(bytes)?;
        Ok(Self::new(image_data))
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the image data.
    pub fn image_data(&self) -> &ImageData {
        &self.image_data
    }

    modifier_methods!();
}

impl Clone for Image {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            image_data: self.image_data.clone(),
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Image {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ImageRenderObject::new(
            &self.image_data,
            self.style.clone(),
            self.layout.clone(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<ImageRenderObject>()
        {
            let mut result = UpdateResult::NONE;
            if ro.set_image_data(&self.image_data) {
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
    use super::super::{GlobalKey, Key};
    use super::*;

    fn make_test_image_data() -> ImageData {
        ImageData {
            pixels: vec![255, 0, 0, 255], // 1x1 red pixel
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn test_image_widget_creation() {
        let data = make_test_image_data();
        let widget = Image::new(data);
        assert_eq!(widget.image_data().width, 1);
        assert_eq!(widget.image_data().height, 1);
    }

    #[test]
    fn test_image_widget_with_key() {
        let data = make_test_image_data();
        let widget = Image::new(data).with_key("my_image");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("my_image"))));
    }

    #[test]
    fn test_image_widget_with_global_key() {
        let data = make_test_image_data();
        let global_key = GlobalKey::new();
        let widget = Image::new(data).with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_image_widget_clone() {
        let data = make_test_image_data();
        let widget = Image::new(data).with_key("img");
        let cloned = widget.clone();

        assert_eq!(widget.image_data().width, cloned.image_data().width);
        assert_eq!(widget.key(), cloned.key());
    }

    #[test]
    fn test_image_modifier_background_returns_self() {
        let data = make_test_image_data();
        let w = Image::new(data).background(crate::core::Color::RED);
        assert_eq!(w.style.background, Some(crate::core::Color::RED));
        assert_eq!(w.image_data().width, 1);
    }

    #[test]
    fn test_image_modifier_padding_returns_self() {
        let data = make_test_image_data();
        let w = Image::new(data).padding(8.0);
        assert!(w.layout.padding.is_some());
        assert_eq!(w.image_data().width, 1);
    }

    #[test]
    fn test_image_modifier_chain_preserves_all() {
        let data = make_test_image_data();
        let w = Image::new(data)
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
        assert_eq!(w.image_data().width, 1);
    }
}
