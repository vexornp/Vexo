use crate::element::Element;
use crate::elements::LeafElement;
use crate::image_data::{ImageData, ImageDataError};
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::render_objects::ImageRenderObject;
use crate::update_result::UpdateResult;
use crate::widgets::Widget;

pub struct Image {
    key: Option<WidgetKey>,
    image_data: ImageData,
    corner_radius: f32,
}

impl Image {
    pub fn new(image_data: ImageData) -> Self {
        Self {
            key: None,
            image_data,
            corner_radius: 0.0,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageDataError> {
        let image_data = ImageData::from_bytes(bytes)?;
        Ok(Self::new(image_data))
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    pub fn image_data(&self) -> &ImageData {
        &self.image_data
    }

    pub fn corner_radius(&self) -> f32 {
        self.corner_radius
    }
}

impl Clone for Image {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            image_data: self.image_data.clone(),
            corner_radius: self.corner_radius,
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
        Box::new(ImageRenderObject::new(&self.image_data, self.corner_radius))
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
            if ro.set_corner_radius(self.corner_radius) {
                result |= UpdateResult::PAINT;
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
            pixels: vec![255, 0, 0, 255],
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
    fn test_image_widget_with_corner_radius() {
        let data = make_test_image_data();
        let widget = Image::new(data).with_corner_radius(8.0);
        assert_eq!(widget.corner_radius(), 8.0);
    }

    #[test]
    fn test_image_widget_corner_radius_default_zero() {
        let data = make_test_image_data();
        let widget = Image::new(data);
        assert_eq!(widget.corner_radius(), 0.0);
    }

    #[test]
    fn test_image_widget_clone_preserves_corner_radius() {
        let data = make_test_image_data();
        let widget = Image::new(data).with_corner_radius(12.0);
        let cloned = widget.clone();
        assert_eq!(cloned.corner_radius(), 12.0);
    }
}
