//! Text widget - displays a string.

use std::marker::PhantomData;

use super::{Element, Key, Widget};
use super::super::RenderObject;
use super::super::render_objects::TextRenderObject;

/// Text widget - displays a string.
///
/// Generic over the message type `M` to fit into widget trees with typed messages.
/// For non-interactive usage, `M = ()` (the default).
pub struct Text<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    content: String,
    _marker: PhantomData<M>,
}

impl<M: Clone + Send + 'static> Text<M> {
    /// Create a new text widget.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            key: None,
            content: content.into(),
            _marker: PhantomData,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl<M: Clone + Send + 'static> Clone for Text<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            _marker: PhantomData,
        }
    }
}

impl<M: Clone + Send + 'static> Widget<M> for Text<M> {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::LeafElement::<M>::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TextRenderObject::new(&self.content))
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) {
        // Downcast to TextRenderObject and update properties
        if let Some(text_ro) = render_object.as_any_mut().downcast_mut::<TextRenderObject>() {
            text_ro.set_content(&self.content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_widget_creation() {
        let widget: Text<()> = Text::new("Hello");
        assert_eq!(widget.content(), "Hello");
    }

    #[test]
    fn test_text_widget_with_key() {
        let widget: Text<()> = Text::new("Hello").with_key("greeting");
        assert_eq!(widget.key(), Some(Key::new("greeting")));
    }

    #[test]
    fn test_text_widget_clone() {
        let widget: Text<()> = Text::new("Hello").with_key("greeting");
        let cloned = widget.clone();

        assert_eq!(widget.content(), cloned.content());
        assert_eq!(widget.key(), cloned.key());
    }

    #[test]
    fn test_text_widget_generic_message() {
        #[derive(Clone, Debug)]
        enum MyMessage {
            Clicked,
        }

        // Text widget can be used with a custom message type
        let widget: Text<MyMessage> = Text::new("Hello");
        assert_eq!(widget.content(), "Hello");
    }
}