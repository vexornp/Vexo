//! Opacity widget - applies an alpha multiplier to a child subtree.
//!
//! This widget applies an opacity value to its child subtree. The opacity
//! is paint-only — layout is unaffected, so the child occupies its original
//! space regardless of the opacity.
//!
//! This matches Flutter's `Opacity` widget design.

use std::any::Any;

use crate::elements::{OpacityElement, RenderObjectElement};
use crate::render_objects::OpacityRenderObject;
use crate::{Element, RenderObject, UpdateResult, Widget, WidgetKey};

/// A widget that applies an opacity multiplier to its child.
///
/// The opacity is paint-only — layout is unaffected. The child occupies
/// its original layout space regardless of the opacity applied.
///
/// # Example
///
/// ```ignore
/// // Make a text element semi-transparent
/// Opacity::new(Text::new("Faded"), 0.5)
///
/// // Using the trait modifier
/// Text::new("Faded").opacity(0.5)
/// ```
pub struct Opacity {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    opacity: f32,
}

impl Opacity {
    /// Create a new opacity widget.
    ///
    /// The opacity value is clamped to [0.0, 1.0]:
    /// - 0.0 = fully transparent
    /// - 1.0 = fully opaque
    pub fn new(child: impl Widget + 'static, opacity: f32) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            opacity: opacity.clamp(0.0, 1.0),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the opacity value.
    pub fn opacity_value(&self) -> f32 {
        self.opacity
    }
}

impl Clone for Opacity {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            opacity: self.opacity,
        }
    }
}

impl Widget for Opacity {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = OpacityElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(OpacityRenderObject::new(self.opacity))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(opacity_ro) = render_object
            .as_any_mut()
            .downcast_mut::<OpacityRenderObject>()
        {
            if opacity_ro.set_opacity(self.opacity) {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
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
    use crate::render_objects::OpacityRenderObject;
    use crate::Text;

    #[test]
    fn test_opacity_creation() {
        let w = Opacity::new(Text::new("Hello"), 0.5);
        assert_eq!(w.opacity_value(), 0.5);
    }

    #[test]
    fn test_opacity_clamping() {
        let w = Opacity::new(Text::new("Hello"), 1.5);
        assert_eq!(w.opacity_value(), 1.0);
        let w2 = Opacity::new(Text::new("Hello"), -0.5);
        assert_eq!(w2.opacity_value(), 0.0);
    }

    #[test]
    fn test_opacity_render_object_creation() {
        let w = Opacity::new(Text::new("Hello"), 0.5);
        let ro = w.create_render_object();
        assert!(ro.as_any().downcast_ref::<OpacityRenderObject>().is_some());
        assert_eq!(
            ro.as_any()
                .downcast_ref::<OpacityRenderObject>()
                .unwrap()
                .opacity(),
            Some(0.5)
        );
    }

    #[test]
    fn test_opacity_update_render_object() {
        let w1 = Opacity::new(Text::new("Hello"), 0.5);
        let w2 = Opacity::new(Text::new("Hello"), 0.7);
        let mut ro = OpacityRenderObject::new(0.5);

        let result = w1.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);

        let result = w2.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT));
    }
}
