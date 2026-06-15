//! ScrollView widget - a scrollable container for content that overflows.

use std::any::Any;

use crate::elements::ScrollViewElement;
use crate::elements::RenderObjectElement;
use crate::element::Element;
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::Widget;
use crate::UpdateResult;

pub struct ScrollView {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { key: None, child: Box::new(child) }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for ScrollView {
    fn clone(&self) -> Self {
        Self { key: self.key.clone(), child: self.child.clone_boxed() }
    }
}

impl Widget for ScrollView {
    fn key(&self) -> Option<WidgetKey> { self.key.clone() }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ScrollViewElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ScrollViewRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any { self }

    fn child(&self) -> Option<&dyn Widget> { Some(self.child.as_ref()) }

    fn can_update(&self, other: &dyn Widget) -> bool {
        other.as_any().downcast_ref::<ScrollView>().is_some()
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        // Always request paint because the scroll offset may have changed
        // via Cell::set in apply_scroll_offset, which bypasses the normal
        // dirty-tracking path. Returning PAINT ensures that when
        // mark_needs_build triggers a rebuild, the render object is
        // marked as needing paint so the painter re-traverses it.
        UpdateResult::PAINT
    }

    fn clone_boxed(&self) -> Box<dyn Widget> { Box::new(self.clone()) }
}
