//! Transform widget - applies a 2D affine transform to a child.
//!
//! This widget applies a rotation, scale, translation, or skew transform
//! to its child subtree. The transform is paint-only — layout is unaffected,
//! so the child occupies its original space regardless of the transform.
//!
//! This matches Flutter's `Transform` widget design.

use std::any::Any;

use crate::core::{AffineTransform, Bounds, Logical, Point};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::LayoutNodeKey;
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// TransformRenderObject
// ============================================================================

/// Render object for Transform — applies an affine transform to its child.
///
/// Layout is pass-through (transform does NOT affect layout).
/// The transform is applied via `paint_transform()` so the painter wraps
/// children's commands with `PushTransform`/`PopTransform`.
pub struct TransformRenderObject {
    transform: AffineTransform,
    transform_hit_tests: bool,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl TransformRenderObject {
    /// Create a new transform render object.
    pub fn new(transform: AffineTransform, transform_hit_tests: bool) -> Self {
        Self {
            transform,
            transform_hit_tests,
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the transform.
    /// Returns true if it changed.
    pub fn set_transform(&mut self, transform: AffineTransform) -> bool {
        if self.transform != transform {
            self.transform = transform;
            true
        } else {
            false
        }
    }

    /// Set whether hit tests use the transform.
    pub fn set_transform_hit_tests(&mut self, value: bool) {
        self.transform_hit_tests = value;
    }

    /// Get the current transform.
    pub fn transform(&self) -> &AffineTransform {
        &self.transform
    }
}

impl RenderObject for TransformRenderObject {
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             Transform always has a child per its constructor",
        );
        self.child_layout_node = Some(child_node);
        LayoutResult {
            node: child_node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(child_node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        // The transform is handled by hit_test_transform() for child hit testing.
        // For the Transform render object itself, check untransformed bounds.
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn paint_transform(&self) -> Option<AffineTransform> {
        // Skip painting for singular transforms (collapsed to a point/line).
        if self.transform.determinant().abs() < 1e-10 {
            return None;
        }
        Some(self.transform)
    }

    fn hit_test_transform(&self) -> Option<AffineTransform> {
        if self.transform_hit_tests {
            Some(self.transform)
        } else {
            None
        }
    }
}

// ============================================================================
// TransformElement
// ============================================================================

/// Element for Transform widget.
///
/// Manages a single child element and updates the render object
/// when the transform changes.
pub struct TransformElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl TransformElement {
    /// Create a new transform element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for TransformElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for TransformElement {
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

impl Element for TransformElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        self.mount_render_object(context);

        if let Some(widget) = &self.widget {
            if let Some(child_widget) = widget.child() {
                context.inflate_child(None, child_widget.clone_boxed());
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
        }
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
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            let old_child = context.children().first().copied();
            if let Some(child_widget) = self.get_child_widget() {
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = old_child {
                context.unmount_child(old_child_key);
            }
        }

        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        child_ro: Option<RenderObjectKey>,
        context: &mut ElementContext,
    ) {
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}

// ============================================================================
// Transform Widget
// ============================================================================

/// A widget that applies a 2D affine transform to its child.
///
/// The transform is paint-only — layout is unaffected. The child occupies
/// its original layout space regardless of the transform applied.
///
/// # Example
///
/// ```ignore
/// // Rotate a card 15 degrees
/// Transform::rotate(
///     DecoratedContainer::new(Text::new("Rotated!")).style(Style::new().background(Color::RED)),
///     15.0 * std::f32::consts::PI / 180.0,
/// )
///
/// // Scale a text element
/// Transform::scale(Text::new("Big!"), 2.0, 2.0)
///
/// // Translate (offset) a child
/// Transform::translate(Text::new("Shifted"), 20.0, 10.0)
/// ```
pub struct Transform {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    transform: AffineTransform,
    transform_hit_tests: bool,
}

impl Transform {
    /// Create a new transform widget with a custom affine transform.
    pub fn new(child: impl crate::Widget + 'static, transform: AffineTransform) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            transform,
            transform_hit_tests: true,
        }
    }

    /// Create a rotation transform.
    pub fn rotate(child: impl crate::Widget + 'static, radians: f32) -> Self {
        Self::new(child, AffineTransform::rotation(radians))
    }

    /// Create a scale transform.
    pub fn scale(child: impl crate::Widget + 'static, sx: f32, sy: f32) -> Self {
        Self::new(child, AffineTransform::scale(sx, sy))
    }

    /// Create a translation transform.
    pub fn translate(child: impl crate::Widget + 'static, dx: f32, dy: f32) -> Self {
        Self::new(child, AffineTransform::translation(dx, dy))
    }

    /// Set whether hit tests use the transform (default: true).
    pub fn transform_hit_tests(mut self, value: bool) -> Self {
        self.transform_hit_tests = value;
        self
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the transform.
    pub fn transform_ref(&self) -> &AffineTransform {
        &self.transform
    }
}

impl Clone for Transform {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            transform: self.transform,
            transform_hit_tests: self.transform_hit_tests,
        }
    }
}

impl Widget for Transform {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = TransformElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TransformRenderObject::new(
            self.transform,
            self.transform_hit_tests,
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(transform_ro) = render_object
            .as_any_mut()
            .downcast_mut::<TransformRenderObject>()
        {
            let transform_changed = transform_ro.set_transform(self.transform);
            let old_hit_tests = transform_ro.transform_hit_tests;
            transform_ro.set_transform_hit_tests(self.transform_hit_tests);

            if transform_changed || old_hit_tests != self.transform_hit_tests {
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
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_transform_creation() {
        let t = Transform::rotate(Text::new("Hello"), 0.5);
        assert!(t.key().is_none());
        assert!(t.transform_ref().is_translation_only() == false);
    }

    #[test]
    fn test_transform_with_key() {
        let t = Transform::scale(Text::new("Hello"), 2.0, 2.0).with_key("my-transform");
        assert_eq!(t.key(), Some(WidgetKey::Local(Key::new("my-transform"))));
    }

    #[test]
    fn test_transform_with_global_key() {
        let global_key = GlobalKey::new();
        let t = Transform::translate(Text::new("Hello"), 10.0, 20.0).with_key(global_key.clone());
        assert_eq!(t.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_transform_render_object_creation() {
        let t = Transform::rotate(Text::new("Hello"), 0.5);
        let ro = t.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<TransformRenderObject>()
            .is_some());
    }

    #[test]
    fn test_transform_render_object_set_transform() {
        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        assert!(!ro.set_transform(AffineTransform::rotation(0.5))); // same = no change
        assert!(ro.set_transform(AffineTransform::rotation(1.0))); // different = change
    }

    #[test]
    fn test_transform_render_object_paint_transform() {
        let ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        assert!(ro.paint_transform().is_some());

        // Singular transform should return None for paint_transform
        let singular = TransformRenderObject::new(AffineTransform::scale(0.0, 0.0), true);
        assert!(singular.paint_transform().is_none());
    }

    #[test]
    fn test_transform_render_object_hit_test_transform() {
        let ro_with = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        assert!(ro_with.hit_test_transform().is_some());

        let ro_without = TransformRenderObject::new(AffineTransform::rotation(0.5), false);
        assert!(ro_without.hit_test_transform().is_none());
    }

    #[test]
    fn test_transform_update_render_object() {
        let widget1 = Transform::rotate(Text::new("Hello"), 0.5);
        let widget2 = Transform::rotate(Text::new("Hello"), 1.0);
        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);

        let result = widget1.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE); // Same transform

        let result = widget2.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Different transform
    }

    #[test]
    fn test_transform_hit_tests_flag_change() {
        let widget_with = Transform::rotate(Text::new("Hello"), 0.5).transform_hit_tests(true);
        let widget_without = Transform::rotate(Text::new("Hello"), 0.5).transform_hit_tests(false);
        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);

        // Changing from true to false should mark paint dirty
        let result = widget_without.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Flag changed

        // Changing from false back to true should also mark paint dirty
        let result = widget_with.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Flag changed back
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_transform_is_pass_through() {
        let ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_transform_layout_stores_child_node() {
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(30.0));

        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }

    #[test]
    fn test_transform_apply_layout_reads_child_bounds() {
        use crate::core::Size;
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(60.0).height(25.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro
            .computed_bounds()
            .expect("apply_layout should set bounds");
        assert_eq!(bounds.width(), 60.0);
        assert_eq!(bounds.height(), 25.0);
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_transform_layout_no_child_panics() {
        use crate::layout::{LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
}
