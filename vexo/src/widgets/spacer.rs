//! Spacer widget — a leaf that claims a share of the parent's free space.
//!
//! Drop-in replacement for `MultiChild::empty(Layout::default().flex_grow(1.0))`
//! when used as a flexible spacer inside a `row!` / `column!`. Paints nothing,
//! hits nothing, has no children. Backed by `SpacerRenderObject`.
//!
//! See `docs/superpowers/specs/2026-08-03-spacer-widget-design.md`.

use std::any::Any;

use crate::elements::LeafElement;
use crate::key::WidgetKey;
use crate::render_objects::SpacerRenderObject;
use crate::{Element, RenderObject, UpdateResult, Widget};

pub struct Spacer {
    key: Option<WidgetKey>,
}

impl Spacer {
    pub fn new() -> Self {
        Self { key: None }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Spacer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
        }
    }
}

impl Widget for Spacer {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(SpacerRenderObject::new())
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;

    #[test]
    fn spacer_new_creates_spacer_render_object() {
        let w = Spacer::new();
        let ro = w.create_render_object();
        assert!(ro.as_any().downcast_ref::<SpacerRenderObject>().is_some());
    }

    #[test]
    fn spacer_update_render_object_returns_none() {
        let w = Spacer::new();
        let mut ro = SpacerRenderObject::new();
        let result = w.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);
    }

    #[test]
    fn spacer_with_key_round_trips() {
        let w = Spacer::new().with_key("my-spacer");
        assert_eq!(w.key(), Some(WidgetKey::Local(Key::new("my-spacer"))));
    }

    #[test]
    fn spacer_default_is_same_as_new() {
        let _w1 = Spacer::new();
        let _w2 = Spacer::default();
        // If this compiles, `Default` is wired up correctly.
    }

    #[test]
    fn spacer_create_element_is_leaf() {
        let w = Spacer::new();
        let _elem = w.create_element();
        // No assertion on internals — LeafElement is opaque from the widget
        // module. The test confirms `create_element` does not panic.
    }
}
