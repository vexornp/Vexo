//! SafeArea widget — insets its child away from the device's unsafe regions.
//!
//! On mobile (iOS) the OS reports per-edge safe-area insets covering the
//! status bar / notch / home indicator. `SafeArea` reads those insets live
//! during layout (via [`LayoutContext::safe_area_source`]) and pads its child
//! so content stays within the safe region. On desktop the insets are always
//! zero, so `SafeArea` is a transparent pass-through.
//!
//! This mirrors Flutter's `SafeArea` widget: opt out per side, and enforce a
//! `minimum` inset floor.
//!
//! # Design notes
//!
//! Insets are resolved at *layout* time, not *render* time, because safe-area
//! values can change at runtime (device rotation) without a widget rebuild.
//! [`WindowState`](crate::window::WindowState) writes the live insets into a
//! shared [`SafeAreaSource`](crate::core::SafeAreaSource) each frame; when they
//! change it marks the tree dirty, so this render object's `layout()` re-runs
//! and picks up the new padding. The render object also sets `flex_grow(1.0)`
//! so it fills its parent (matching Flutter's `SafeArea` filling its
//! constraints rather than hugging its child).

use std::any::Any;

use crate::core::{Bounds, Logical};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::{AlignItems, EdgeInsets, FlexDirection, Layout, LayoutNodeKey};
use crate::render_objects::ContainerRenderObject;
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// SAFE AREA RENDER OBJECT
// ============================================================================

/// Render object backing [`SafeArea`].
///
/// Delegates layout/paint/hit-testing to an internal [`ContainerRenderObject`]
/// whose `Layout` is recomputed each layout pass from the live safe-area
/// insets (read off the [`LayoutContext`]) combined with this object's
/// per-side enable flags and `minimum` floor.
pub struct SafeAreaRenderObject {
    inner: ContainerRenderObject,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
    minimum: EdgeInsets,
}

impl SafeAreaRenderObject {
    /// Create a new safe-area render object with the given per-side config.
    pub fn new(top: bool, right: bool, bottom: bool, left: bool, minimum: EdgeInsets) -> Self {
        Self {
            inner: ContainerRenderObject::new(Layout::default()),
            top,
            right,
            bottom,
            left,
            minimum,
        }
    }

    /// Effective per-side padding (logical pixels) for the given insets.
    ///
    /// A side contributes its inset only when enabled, floored to `minimum`.
    /// Disabled sides contribute zero.
    fn effective_padding(&self, insets: EdgeInsets) -> (f32, f32, f32, f32) {
        // (left, right, top, bottom) — matches Layout::padding_each order.
        let left = if self.left {
            insets.left.max(self.minimum.left)
        } else {
            0.0
        };
        let right = if self.right {
            insets.right.max(self.minimum.right)
        } else {
            0.0
        };
        let top = if self.top {
            insets.top.max(self.minimum.top)
        } else {
            0.0
        };
        let bottom = if self.bottom {
            insets.bottom.max(self.minimum.bottom)
        } else {
            0.0
        };
        (left, right, top, bottom)
    }

    /// Build the Taffy layout for the given insets.
    ///
    /// `flex_grow(1.0)` makes the safe area fill its parent's main axis;
    /// `AlignItems::Stretch` (via the flex column) fills the cross axis.
    /// `min_height(0.0)` allows the safe area to shrink below its content's
    /// min-content when the parent is shorter (e.g. a scrollable page inside
    /// a TabBarView on a short window). Without this, the content's
    /// min-content propagates upward and can push siblings off screen.
    fn layout_for(&self, insets: EdgeInsets) -> Layout {
        let (left, right, top, bottom) = self.effective_padding(insets);
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(left, right, top, bottom)
    }

    /// Update the per-side config, returning `true` if anything changed.
    fn set_config(
        &mut self,
        top: bool,
        right: bool,
        bottom: bool,
        left: bool,
        minimum: EdgeInsets,
    ) -> bool {
        let changed = self.top != top
            || self.right != right
            || self.bottom != bottom
            || self.left != left
            || self.minimum != minimum;
        self.top = top;
        self.right = right;
        self.bottom = bottom;
        self.left = left;
        self.minimum = minimum;
        changed
    }
}

impl RenderObject for SafeAreaRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Resolve live insets here, every layout pass, so runtime changes
        // (rotation) take effect without a widget rebuild.
        let insets = ctx.safe_area_source().get();
        let layout = self.layout_for(insets);
        // `set_layout` is infallible here because we always recompute from
        // live insets; the returned `changed` flag isn't needed since this
        // render object is only laid out when dirty.
        self.inner.set_layout(layout);
        self.inner.layout(ctx, child_nodes)
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        self.inner.apply_layout(ctx);
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        self.inner.paint(ctx)
    }

    fn hit_test(&self, position: crate::core::Point<Logical>, ctx: &HitTestContext) -> bool {
        self.inner.hit_test(position, ctx)
    }

    fn children(&self) -> &[RenderObjectKey] {
        self.inner.children()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn add_child(&mut self, child: RenderObjectKey) {
        self.inner.add_child(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        self.inner.replace_child(old, new);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.inner.layout_node()
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.inner.computed_bounds()
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        self.inner.clip_bounds()
    }
}

// ============================================================================
// SAFE AREA ELEMENT
// ============================================================================

/// Element for the [`SafeArea`] widget.
///
/// Single-child element: mounts/updates/unmounts one child and reconciles it
/// via `update_child`. Mirrors `WithLayoutElement` — the only widget-specific
/// behavior lives in the render object (dynamic padding).
pub struct SafeAreaElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl SafeAreaElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for SafeAreaElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for SafeAreaElement {
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

impl Element for SafeAreaElement {
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

        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
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

    fn can_update(&self, widget: &dyn Any) -> bool {
        self.widget
            .as_ref()
            .map(|old| old.as_any().type_id() == widget.type_id())
            .unwrap_or(false)
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
// SAFE AREA WIDGET
// ============================================================================

/// A widget that insets its child by the device's safe-area insets.
///
/// On mobile this keeps content clear of the status bar, notch, and home
/// indicator. On desktop the insets are zero, so this is a transparent
/// pass-through. Per-side opt-out is supported via the builder methods, and a
/// `minimum` floor can be enforced.
///
/// # Example
///
/// ```ignore
/// use vexo::{SafeArea, Text};
///
/// // Inset on all sides (default)
/// SafeArea::new(Text::new("Hello"))
///
/// // Only avoid the top (status bar); let content extend to other edges
/// SafeArea::new(Text::new("Hello")).bottom(false).left(false).right(false)
/// ```
pub struct SafeArea {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
    minimum: EdgeInsets,
}

impl SafeArea {
    /// Create a `SafeArea` that insets its child on all sides by default.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            top: true,
            right: true,
            bottom: true,
            left: true,
            minimum: EdgeInsets::default(),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Whether to inset the top edge (default `true`).
    pub fn top(mut self, enabled: bool) -> Self {
        self.top = enabled;
        self
    }

    /// Whether to inset the right edge (default `true`).
    pub fn right(mut self, enabled: bool) -> Self {
        self.right = enabled;
        self
    }

    /// Whether to inset the bottom edge (default `true`).
    pub fn bottom(mut self, enabled: bool) -> Self {
        self.bottom = enabled;
        self
    }

    /// Whether to inset the left edge (default `true`).
    pub fn left(mut self, enabled: bool) -> Self {
        self.left = enabled;
        self
    }

    /// Set the minimum insets (logical pixels). Each enabled side's effective
    /// inset is `max(device_inset, minimum_side)`; default zero.
    pub fn minimum(mut self, minimum: EdgeInsets) -> Self {
        self.minimum = minimum;
        self
    }
}

impl Clone for SafeArea {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            left: self.left,
            minimum: self.minimum,
        }
    }
}

impl Widget for SafeArea {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = SafeAreaElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(SafeAreaRenderObject::new(
            self.top,
            self.right,
            self.bottom,
            self.left,
            self.minimum,
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
            .downcast_mut::<SafeAreaRenderObject>()
        {
            if ro.set_config(self.top, self.right, self.bottom, self.left, self.minimum) {
                // Config change alters the computed padding → needs layout.
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
    use crate::core::SafeAreaSource;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};
    use crate::Text;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_safe_area_defaults_all_sides_enabled() {
        let w = SafeArea::new(Text::new("Hi"));
        assert!(w.top && w.right && w.bottom && w.left);
        assert_eq!(w.minimum, EdgeInsets::default());
    }

    #[test]
    fn test_safe_area_per_side_opt_out() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .bottom(false)
            .left(false)
            .right(false);
        assert!(!w.top && !w.right && !w.bottom && !w.left);
    }

    #[test]
    fn test_safe_area_minimum() {
        let m = EdgeInsets {
            left: 5.0,
            right: 5.0,
            top: 10.0,
            bottom: 10.0,
        };
        let w = SafeArea::new(Text::new("Hi")).minimum(m);
        assert_eq!(w.minimum, m);
    }

    #[test]
    fn test_safe_area_clone_preserves_config() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .minimum(EdgeInsets {
                left: 1.0,
                right: 2.0,
                top: 3.0,
                bottom: 4.0,
            });
        let cloned = w.clone();
        assert_eq!(cloned.top, false);
        assert_eq!(cloned.minimum, w.minimum);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_safe_area_render_object_effective_padding_all_sides() {
        let ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        // (left, right, top, bottom)
        assert_eq!(ro.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn test_safe_area_render_object_effective_padding_opt_out() {
        // Constructor order is (top, right, bottom, left). Disable top & left.
        let ro = SafeAreaRenderObject::new(false, true, true, false, EdgeInsets::default());
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        // top & left disabled → 0
        assert_eq!(ro.effective_padding(insets), (0.0, 20.0, 0.0, 40.0));
    }

    #[test]
    fn test_safe_area_render_object_minimum_floor() {
        let min = EdgeInsets {
            left: 50.0,
            right: 50.0,
            top: 50.0,
            bottom: 50.0,
        };
        let ro = SafeAreaRenderObject::new(true, true, true, true, min);
        // Device inset smaller than minimum → minimum wins
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(ro.effective_padding(insets), (50.0, 50.0, 50.0, 50.0));
    }

    #[test]
    fn test_safe_area_render_object_minimum_no_floor_when_larger() {
        let min = EdgeInsets {
            left: 5.0,
            right: 5.0,
            top: 5.0,
            bottom: 5.0,
        };
        let ro = SafeAreaRenderObject::new(true, true, true, true, min);
        // Device inset larger than minimum → device inset wins
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        assert_eq!(ro.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn test_safe_area_render_object_set_config_detects_change() {
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());
        // Same config → no change
        assert!(!ro.set_config(true, true, true, true, EdgeInsets::default()));
        // Disable top → change
        assert!(ro.set_config(false, true, true, true, EdgeInsets::default()));
        // New minimum → change
        assert!(ro.set_config(
            false,
            true,
            true,
            true,
            EdgeInsets {
                left: 1.0,
                right: 1.0,
                top: 1.0,
                bottom: 1.0
            }
        ));
    }

    #[test]
    fn test_safe_area_layout_uses_live_insets() {
        // With non-zero insets, the render object's Taffy node should be
        // created with padding. We verify layout() runs without panic and
        // produces a layout node.
        let source = SafeAreaSource::new(44.0, 0.0, 44.0, 34.0);
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ctx.set_safe_area_source(source);

        let result = ro.layout(&mut ctx, &[]);
        // A layout node must have been created.
        assert!(ro.layout_node().is_some());
        let _ = result;
    }

    #[test]
    fn test_safe_area_layout_with_zero_insets_is_passthrough() {
        // Desktop case: zero insets → zero padding. Layout still works.
        let source = SafeAreaSource::default();
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ctx.set_safe_area_source(source);

        let _ = ro.layout(&mut ctx, &[]);
        assert!(ro.layout_node().is_some());
    }

    #[test]
    fn test_safe_area_update_render_object_no_change() {
        let w = SafeArea::new(Text::new("Hi"));
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());
        // Same config → NONE
        assert_eq!(w.update_render_object(&mut ro), UpdateResult::NONE);
    }

    #[test]
    fn test_safe_area_update_render_object_config_change() {
        let w = SafeArea::new(Text::new("Hi")).top(false);
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());
        // top changed true→false → LAYOUT
        assert!(w
            .update_render_object(&mut ro)
            .contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_safe_area_widget_child_and_key() {
        let w = SafeArea::new(Text::new("Hi")).with_key("safe");
        assert!(w.child().is_some());
        assert_eq!(
            w.key(),
            Some(WidgetKey::Local(crate::key::Key::new("safe")))
        );
    }
}
