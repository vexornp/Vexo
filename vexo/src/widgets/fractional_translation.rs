//! FractionalTranslation widget — translates a child by a fraction of its
//! own laid-out size.
//!
//! Unlike `Transform::translate`, which offsets by a fixed number of pixels,
//! `FractionalTranslation` offsets by `fraction * computed_size`. The fraction
//! is resolved at paint time against the render object's `computed_bounds`,
//! which layout populates each frame. This means the translation always tracks
//! the actual rendered size, with no read-back, `GlobalKey` lookup, or
//! one-frame staleness.
//!
//! This matches Flutter's `FractionalTranslation` widget, which is the
//! primitive underlying `SlideTransition` (used by `CupertinoPageTransition`
//! and `MaterialPageRoute` for iOS-style page slides). The slide distance is
//! expressed as `1.0` (one full page width) rather than a pixel count, so the
//! same transition code is correct at any window size.
//!
//! Layout is pass-through (the fraction does NOT affect layout). The child
//! occupies its original space; only painting (and optionally hit-testing) is
//! shifted.

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
// FractionalTranslationRenderObject
// ============================================================================

/// Render object for FractionalTranslation — offsets its child by a fraction
/// of its own laid-out size.
///
/// Layout is pass-through (fraction does NOT affect layout). The fractional
/// offset is applied via `paint_transform()` so the painter wraps children's
/// commands with `PushTransform`/`PopTransform`. The translation in pixels is
/// computed at paint time from `computed_bounds`, so it always reflects the
/// current laid-out size.
pub struct FractionalTranslationRenderObject {
    offset: (f32, f32),
    transform_hit_tests: bool,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl FractionalTranslationRenderObject {
    /// Create a new fractional translation render object.
    pub fn new(offset: (f32, f32), transform_hit_tests: bool) -> Self {
        Self {
            offset,
            transform_hit_tests,
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the fractional offset. Returns true if it changed.
    pub fn set_offset(&mut self, offset: (f32, f32)) -> bool {
        if self.offset != offset {
            self.offset = offset;
            true
        } else {
            false
        }
    }

    /// Set whether hit tests use the transform.
    pub fn set_transform_hit_tests(&mut self, value: bool) {
        self.transform_hit_tests = value;
    }

    /// Get the current fractional offset.
    #[allow(dead_code)]
    pub fn offset(&self) -> (f32, f32) {
        self.offset
    }

    /// Compute the absolute pixel translation from the fractional offset and
    /// the current `computed_bounds`. Returns `None` if layout hasn't run yet
    /// (no bounds available) — in that case the paint transform is identity.
    fn absolute_translation(&self) -> Option<(f32, f32)> {
        let bounds = self.computed_bounds?;
        Some((
            self.offset.0 * bounds.width(),
            self.offset.1 * bounds.height(),
        ))
    }
}

impl RenderObject for FractionalTranslationRenderObject {
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             FractionalTranslation always has a child per its constructor",
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
        // For the render object itself, check untransformed bounds.
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
        let (dx, dy) = self.absolute_translation()?;
        // A zero translation is identity — return None so the painter skips
        // the PushTransform/PopTransform pair entirely.
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        Some(AffineTransform::translation(dx, dy))
    }

    fn hit_test_transform(&self) -> Option<AffineTransform> {
        if self.transform_hit_tests {
            self.absolute_translation()
                .map(|(dx, dy)| AffineTransform::translation(dx, dy))
        } else {
            None
        }
    }
}

// ============================================================================
// FractionalTranslationElement
// ============================================================================

/// Element for FractionalTranslation widget.
///
/// Manages a single child element and updates the render object when the
/// fractional offset changes. Mirrors `TransformElement`.
pub struct FractionalTranslationElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl FractionalTranslationElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for FractionalTranslationElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for FractionalTranslationElement {
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

impl Element for FractionalTranslationElement {
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
// FractionalTranslation Widget
// ============================================================================

/// A widget that translates its child by a fraction of the child's own
/// laid-out size.
///
/// The fractional offset `(fx, fy)` is resolved at paint time against the
/// render object's `computed_bounds`: the absolute pixel translation is
/// `(fx * width, fy * height)`. Layout is unaffected — the child still
/// occupies its original space; only painting (and optionally hit-testing) is
/// shifted.
///
/// This is the Vexo equivalent of Flutter's `FractionalTranslation` and is the
/// primitive backing iOS-style page slide transitions: a push transition
/// slides the incoming page in by `1.0` page-widths (from off-screen right to
/// in-place), without the framework needing to know the pixel width.
///
/// # Example
///
/// ```ignore
/// // Slide a page one full width to the right (off-screen).
/// FractionalTranslation::new(page, 1.0, 0.0)
///
/// // Slide a page 30% of its width to the left.
/// FractionalTranslation::new(page, -0.3, 0.0)
/// ```
pub struct FractionalTranslation {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    offset: (f32, f32),
    transform_hit_tests: bool,
}

impl FractionalTranslation {
    /// Create a new fractional translation widget.
    ///
    /// `fx` and `fy` are fractions of the child's laid-out width/height.
    /// `1.0` means "one full size"; `0.0` means no offset.
    pub fn new(child: impl Widget + 'static, fx: f32, fy: f32) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            offset: (fx, fy),
            transform_hit_tests: true,
        }
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

    /// Get the fractional offset.
    pub fn offset(&self) -> (f32, f32) {
        self.offset
    }
}

impl Clone for FractionalTranslation {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            offset: self.offset,
            transform_hit_tests: self.transform_hit_tests,
        }
    }
}

impl Widget for FractionalTranslation {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = FractionalTranslationElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(FractionalTranslationRenderObject::new(
            self.offset,
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
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<FractionalTranslationRenderObject>()
        {
            let offset_changed = ro.set_offset(self.offset);
            let old_hit_tests = ro.transform_hit_tests;
            ro.set_transform_hit_tests(self.transform_hit_tests);

            if offset_changed || old_hit_tests != self.transform_hit_tests {
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
    use crate::core::Size;
    use crate::key::Key;
    use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
    use crate::{LayoutContext, LayoutResult, Text};

    #[test]
    fn test_fractional_translation_creation() {
        let w = FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0);
        assert!(w.key().is_none());
        assert_eq!(w.offset(), (1.0, 0.0));
    }

    #[test]
    fn test_fractional_translation_with_key() {
        let w = FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0).with_key("my-ft");
        assert_eq!(w.key(), Some(WidgetKey::Local(Key::new("my-ft"))));
    }

    #[test]
    fn test_fractional_translation_render_object_creation() {
        let w = FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0);
        let ro = w.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<FractionalTranslationRenderObject>()
            .is_some());
    }

    #[test]
    fn test_fractional_translation_set_offset() {
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        assert!(!ro.set_offset((1.0, 0.0))); // same = no change
        assert!(ro.set_offset((0.5, 0.0))); // different = change
    }

    #[test]
    fn test_fractional_translation_paint_transform_is_none_before_layout() {
        // Before layout, computed_bounds is None → paint_transform is None.
        let ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        assert!(ro.paint_transform().is_none());
    }

    #[test]
    fn test_fractional_translation_paint_transform_zero_offset_is_none() {
        let mut ro = FractionalTranslationRenderObject::new((0.0, 0.0), true);
        // Simulate layout populating bounds.
        ro.computed_bounds = Some(Bounds::new(0.0, 0.0, 100.0, 50.0));
        // Zero fraction → zero translation → None (identity).
        assert!(ro.paint_transform().is_none());
    }

    #[test]
    fn test_fractional_translation_paint_transform_scales_with_bounds() {
        // The core invariant: the absolute translation tracks the laid-out
        // size, not a stored constant. Same fraction, different bounds →
        // different pixel translation.
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);

        // 60px wide → translate by 60.
        ro.computed_bounds = Some(Bounds::new(0.0, 0.0, 60.0, 25.0));
        let t60 = ro.paint_transform().expect("should have a transform");
        assert_eq!(t60.e, 60.0);
        assert_eq!(t60.f, 0.0);

        // 100px wide → translate by 100 (NOT 60).
        ro.computed_bounds = Some(Bounds::new(0.0, 0.0, 100.0, 25.0));
        let t100 = ro.paint_transform().expect("should have a transform");
        assert_eq!(t100.e, 100.0);
        assert_eq!(t100.f, 0.0);
    }

    #[test]
    fn test_fractional_translation_paint_transform_fractional_offset() {
        let mut ro = FractionalTranslationRenderObject::new((-0.3, 0.5), true);
        ro.computed_bounds = Some(Bounds::new(0.0, 0.0, 200.0, 40.0));
        let t = ro.paint_transform().expect("should have a transform");
        assert!((t.e - (-60.0)).abs() < 1e-3); // -0.3 * 200
        assert!((t.f - 20.0).abs() < 1e-3); // 0.5 * 40
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_fractional_translation_apply_layout_reads_child_bounds() {
        // Mirrors test_transform_apply_layout_reads_child_bounds. Proves the
        // RO reads the child's laid-out bounds in apply_layout, and the
        // resulting paint_transform uses them.
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);
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

        // With fraction (1.0, 0.0) and width 60, the translation is 60px.
        let t = ro.paint_transform().expect("should have a transform");
        assert_eq!(t.e, 60.0);
        assert_eq!(t.f, 0.0);
    }

    #[test]
    fn test_fractional_translation_is_pass_through() {
        let ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        assert!(ro.is_pass_through());
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_fractional_translation_layout_no_child_panics() {
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }

    #[test]
    fn test_fractional_translation_update_render_object() {
        let widget1 = FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0);
        let widget2 = FractionalTranslation::new(Text::new("Hi"), 0.5, 0.0);
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);

        let result = widget1.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE); // Same offset

        let result = widget2.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Different offset
    }

    #[test]
    fn test_fractional_translation_hit_tests_flag_change() {
        let widget_with =
            FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0).transform_hit_tests(true);
        let widget_without =
            FractionalTranslation::new(Text::new("Hi"), 1.0, 0.0).transform_hit_tests(false);
        let mut ro = FractionalTranslationRenderObject::new((1.0, 0.0), true);

        let result = widget_without.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Flag changed

        let result = widget_with.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::PAINT)); // Flag changed back
    }

    #[test]
    fn test_fractional_translation_hit_test_transform() {
        let ro_with = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        let ro_without = FractionalTranslationRenderObject::new((1.0, 0.0), false);

        // Without bounds, hit_test_transform is None regardless of flag.
        assert!(ro_with.hit_test_transform().is_none());
        assert!(ro_without.hit_test_transform().is_none());

        // With bounds, the flag controls whether the transform is applied.
        let mut ro_with_bounds = FractionalTranslationRenderObject::new((1.0, 0.0), true);
        ro_with_bounds.computed_bounds = Some(Bounds::new(0.0, 0.0, 100.0, 50.0));
        assert!(ro_with_bounds.hit_test_transform().is_some());

        let mut ro_without_bounds = FractionalTranslationRenderObject::new((1.0, 0.0), false);
        ro_without_bounds.computed_bounds = Some(Bounds::new(0.0, 0.0, 100.0, 50.0));
        assert!(ro_without_bounds.hit_test_transform().is_none());
    }
}
