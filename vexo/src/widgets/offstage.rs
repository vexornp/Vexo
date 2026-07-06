//! Offstage widget — hides a child subtree while keeping it mounted.
//!
//! When `offstage` is true, the child is not laid out, painted, or hit-tested,
//! but its element (and all associated state: `ComponentState`, focus,
//! `TextEditingController`, etc.) is preserved. Toggling `offstage` back to
//! false restores the child with its state intact.
//!
//! This matches Flutter's `Offstage` widget. It is the key primitive enabling
//! navigation stacks to preserve intermediate page state.

use std::any::Any;

use crate::elements::{OffstageElement, RenderObjectElement};
use crate::render_objects::OffstageRenderObject;
use crate::{Element, RenderObject, UpdateResult, Widget, WidgetKey};

/// A widget that hides its child while keeping it mounted.
///
/// When `offstage` is true:
/// - The child takes zero size (not laid out by Taffy)
/// - The child is not painted
/// - The child is not hit-tested (does not receive pointer events)
/// - The child's element and state are preserved
///
/// # Example
///
/// ```ignore
/// Offstage::new(Text::new("Hidden"), true)
///
/// // Using the builder
/// Offstage::new(my_page).offstage(is_hidden)
/// ```
pub struct Offstage {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    offstage: bool,
}

impl Offstage {
    /// Create a new offstage widget.
    ///
    /// `offstage == true` hides the child; `false` shows it.
    pub fn new(child: impl Widget + 'static, offstage: bool) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            offstage,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set whether the child is offstage.
    pub fn offstage(mut self, offstage: bool) -> Self {
        self.offstage = offstage;
        self
    }

    /// Get the offstage flag.
    pub fn is_offstage(&self) -> bool {
        self.offstage
    }
}

impl Clone for Offstage {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            offstage: self.offstage,
        }
    }
}

impl Widget for Offstage {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = OffstageElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(OffstageRenderObject::new(self.offstage))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<OffstageRenderObject>()
        {
            if ro.set_offstage(self.offstage) {
                UpdateResult::LAYOUT
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
    use crate::Text;

    #[test]
    fn test_offstage_creation() {
        let w = Offstage::new(Text::new("Hidden"), true);
        assert!(w.is_offstage());
    }

    #[test]
    fn test_offstage_builder() {
        let w = Offstage::new(Text::new("Visible"), false).offstage(true);
        assert!(w.is_offstage());
    }

    #[test]
    fn test_offstage_render_object_creation() {
        let w = Offstage::new(Text::new("Hello"), true);
        let ro = w.create_render_object();
        let offstage_ro = ro.as_any().downcast_ref::<OffstageRenderObject>().unwrap();
        assert!(offstage_ro.is_offstage());
    }

    #[test]
    fn test_offstage_update_render_object_no_change() {
        let w = Offstage::new(Text::new("Hello"), true);
        let mut ro = OffstageRenderObject::new(true);
        let result = w.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);
    }

    #[test]
    fn test_offstage_update_render_object_change() {
        let w = Offstage::new(Text::new("Hello"), false);
        let mut ro = OffstageRenderObject::new(true);
        let result = w.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::LAYOUT));
        assert!(!ro.is_offstage());
    }
}
