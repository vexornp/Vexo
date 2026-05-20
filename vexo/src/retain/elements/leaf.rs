//! LeafRenderObjectElement implementation.
//!
//! LeafRenderObjectElement is the simplest element with no children.
//! Used by leaf widgets like Text, Image, etc.
//!
//! This element owns a render object and manages its lifecycle through
//! the RenderObjectElement trait.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementKey, RenderObjectKey, Widget};
use crate::retain::elements::RenderObjectElement;
use crate::retain::key::WidgetKey;

/// Element for leaf widgets (no children).
///
/// This element:
/// - Owns a render object
/// - Has no children
/// - Manages render object lifecycle via RenderObjectElement trait
///
/// # Example
///
/// ```ignore
/// let mut element = LeafRenderObjectElement::new();
/// element.set_widget(&Text::new("Hello"));
/// element.mount(&mut context);
/// ```
pub struct LeafRenderObjectElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
}

impl LeafRenderObjectElement {
    /// Create a new leaf element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<WidgetKey>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            widget: None,
        }
    }

    /// Set the widget for this element.
    ///
    /// Must be called before mount to create the render object.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementKey> {
        self.id
    }
}

impl Default for LeafRenderObjectElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for LeafRenderObjectElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.widget = Some(widget);
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}

// Implement Element trait using RenderObjectElement defaults
impl Element for LeafRenderObjectElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default mount implementation
        self.mount_render_object(context);
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update implementation
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default unmount implementation
        self.unmount_render_object(context);
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::retain::EventContext,
    ) -> Option<Box<dyn Any>> {
        // Leaf elements (like Text) don't handle events by default
        None
    }
}

/// Type alias for backward compatibility.
///
/// New code should use `LeafRenderObjectElement` directly.
pub type LeafElement = LeafRenderObjectElement;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, Text, Key, BuildOwner, ChildOps};
    use crate::retain::focus::FocusManager;

    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn test_leaf_element_mount() {
        let mut element = LeafRenderObjectElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut child_ops = ChildOps::new();
        let mut focus_manager = FocusManager::new();
        let mut context = ElementContext::new(
            make_element_key(),
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
        );

        element.mount(&mut context);

        assert!(element.id().is_some());
    }

    #[test]
    fn test_leaf_element_mount_creates_render_object() {
        let mut element = LeafRenderObjectElement::new();
        let widget = Text::new("Hello");
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut child_ops = ChildOps::new();
        let mut focus_manager = FocusManager::new();
        let mut context = ElementContext::new(
            make_element_key(),
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
        );

        element.mount(&mut context);

        assert!(element.render_object().is_some());

        let ro_id = element.render_object().unwrap();
        assert!(render_objects.get(ro_id).is_some());
    }

    #[test]
    fn test_leaf_element_unmount_removes_render_object() {
        let mut element = LeafRenderObjectElement::new();
        let widget = Text::new("Hello");
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut child_ops = ChildOps::new();
        let mut focus_manager = FocusManager::new();
        let mut context = ElementContext::new(
            make_element_key(),
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
        );

        element.mount(&mut context);
        let ro_id = element.render_object().unwrap();

        element.unmount(&mut context);

        assert!(render_objects.get(ro_id).is_none());
    }

    #[test]
    fn test_leaf_element_unmount() {
        let mut element = LeafRenderObjectElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut child_ops = ChildOps::new();
        let mut focus_manager = FocusManager::new();
        let mut context = ElementContext::new(
            make_element_key(),
            None,
            Vec::new(),
            &mut state,
            &mut dirty,
            &mut render_objects,
            &build_owner,
            &dirty_sender,
            &mut child_ops,
            &mut focus_manager,
        );

        element.mount(&mut context);
        element.unmount(&mut context);
    }

    #[test]
    fn test_leaf_element_with_key() {
        let key = WidgetKey::Local(Key::new("test-key"));
        let element = LeafRenderObjectElement::with_key(Some(key.clone()));

        assert_eq!(element.widget_key(), Some(key));
    }

    #[test]
    fn test_leaf_element_default() {
        let element = LeafRenderObjectElement::default();

        assert!(element.id().is_none());
        assert!(element.widget_key().is_none());
        assert!(element.render_object().is_none());
    }

    #[test]
    fn test_leaf_element_can_update() {
        let element = LeafRenderObjectElement::new();

        assert!(element.can_update(&"any widget" as &dyn Any));
    }

    #[test]
    fn test_backward_compatibility_alias() {
        let element: LeafElement = LeafRenderObjectElement::new();
        assert!(element.id().is_none());
    }
}
