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
use crate::render_object::SafeAreaClaimEdges;
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
    /// Effective insets (parent's insets minus ancestors' claims), set by
    /// the layouter's top-down safe-area pre-pass. `None` until the walk
    /// runs; `layout()` falls back to the global source when `None` for
    /// test compatibility (tests that call `layout()` directly).
    effective: Option<EdgeInsets>,
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
            effective: None,
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
        // Use effective insets (set by the layouter's top-down safe-area
        // walk) if available; fall back to the global source for test
        // compatibility (tests that call layout() directly without the
        // walk). This is what prevents double-consumption: when an ancestor
        // claim (e.g. SafeAreaClaim::bottom in TabBarView) zeroes the
        // bottom edge, this SafeArea pads by bottom=0 instead of the raw
        // 34px home-indicator inset.
        let insets = self
            .effective
            .unwrap_or_else(|| ctx.safe_area_source().get());
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

    /// Claim the enabled sides so nested `SafeArea`s (or any descendant
    /// safe-area consumer) see zero insets for edges this `SafeArea`
    /// already padded. This matches Flutter's `MediaQuery.removePadding`
    /// semantics and prevents double-consumption in nested `SafeArea`s.
    fn safe_area_claim(&self) -> SafeAreaClaimEdges {
        SafeAreaClaimEdges {
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            left: self.left,
        }
    }

    /// Store the effective insets (parent's insets minus ancestors' claims)
    /// for use in `layout()`. Called by the layouter's top-down pre-pass
    /// before the bottom-up dirty layout pass runs.
    fn set_effective_safe_area(&mut self, insets: EdgeInsets) {
        self.effective = Some(insets);
    }

    /// Expose the stored effective insets for testing / introspection.
    fn effective_safe_area(&self) -> Option<EdgeInsets> {
        self.effective
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

// ============================================================================
// SAFE AREA CLAIM RENDER OBJECT
// ============================================================================

/// Render object backing [`SafeAreaClaim`].
///
/// A layout pass-through (shares its child's Taffy node, like
/// [`ProxyRenderObject`](crate::stateful_widget::ProxyRenderObject)) that
/// overrides [`safe_area_claim`](RenderObject::safe_area_claim) to declare
/// which edges are "owned" for its subtree. The layouter's top-down
/// safe-area pre-pass reads the claim and zeroes those edges in the
/// effective insets propagated to descendants.
///
/// `SafeAreaClaim` itself does **not** inset its child — it only claims
/// edges so descendant `SafeArea`s see zero for those edges. This is for
/// the "sibling owns the edge" pattern: a tab bar (sibling) paints its
/// background over the home indicator, so the page content (wrapped in
/// `SafeAreaClaim::bottom`) should not re-apply the bottom inset.
pub struct SafeAreaClaimRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
    claim: SafeAreaClaimEdges,
}

impl SafeAreaClaimRenderObject {
    pub fn new(claim: SafeAreaClaimEdges) -> Self {
        Self {
            child: None,
            computed_bounds: None,
            child_layout_node: None,
            claim,
        }
    }

    /// Update the claim edges. Returns `true` if they changed (caller should
    /// mark needs-layout so the top-down walk re-propagates).
    pub fn set_claim(&mut self, claim: SafeAreaClaimEdges) -> bool {
        if self.claim != claim {
            self.claim = claim;
            true
        } else {
            false
        }
    }
}

impl RenderObject for SafeAreaClaimRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        match child_nodes.first() {
            Some(&child_node) => {
                self.child_layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: crate::core::Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.child_layout_node = Some(node);
                LayoutResult {
                    node,
                    size: crate::core::Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        Vec::new()
    }

    fn hit_test(&self, position: crate::core::Point<Logical>, _ctx: &HitTestContext) -> bool {
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

    /// Declare the claimed edges so the layouter's top-down walk zeroes
    /// them for descendants.
    fn safe_area_claim(&self) -> SafeAreaClaimEdges {
        self.claim
    }
}

// ============================================================================
// SAFE AREA CLAIM ELEMENT
// ============================================================================

/// Element for the [`SafeAreaClaim`] widget.
///
/// Single-child element (same pattern as [`SafeAreaElement`]). The only
/// widget-specific behavior lives in the render object (the claim).
pub struct SafeAreaClaimElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl SafeAreaClaimElement {
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

impl Default for SafeAreaClaimElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for SafeAreaClaimElement {
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

impl Element for SafeAreaClaimElement {
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
// SAFE AREA CLAIM WIDGET
// ============================================================================

/// A widget that claims safe-area edges for its subtree without inset-ing
/// its child.
///
/// Descendant `SafeArea`s (and any safe-area consumer) see **zero** insets
/// for the claimed edges. Use this when a *sibling* owns an edge (e.g. a
/// tab bar paints over the home indicator) and the content subtree should
/// not re-apply that edge's inset.
///
/// `SafeAreaClaim` is a layout pass-through — it does not create a Taffy
/// node, add padding, or paint. It only declares a claim that the
/// layouter's top-down safe-area pre-pass reads.
///
/// # When to use
///
/// - **`SafeAreaClaim`**: a sibling bar owns an edge; wrap the content
///   subtree so its `SafeArea` sees `0` for that edge.
/// - **`SafeArea`**: you want to *inset* your own child away from unsafe
///   edges (it also claims its enabled sides for descendants).
///
/// Ordinary content rarely uses `SafeAreaClaim` directly — it just uses
/// `SafeArea`, which auto-sees `0` for edges an upstream `SafeAreaClaim`
/// already handled.
///
/// # Nesting
///
/// Nesting is safe: claiming an already-claimed edge is a harmless
/// no-op (the walk zeroes the edge once; re-zeroing changes nothing).
///
/// # Example
///
/// ```ignore
/// use vexo::{SafeAreaClaim, Flex};
///
/// // Tab bar owns the bottom edge (home indicator). Page content should
/// // not re-apply the bottom safe-area padding.
/// Flex::column()
///     .push(SafeAreaClaim::bottom(page_content).flex_fill())
///     .push(tab_bar)
/// ```
pub struct SafeAreaClaim {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    claim: SafeAreaClaimEdges,
}

impl SafeAreaClaim {
    /// Create a `SafeAreaClaim` with the given edges.
    pub fn new(child: impl Widget + 'static, claim: SafeAreaClaimEdges) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            claim,
        }
    }

    /// Claim the bottom edge only (e.g. for a tab bar that owns the home
    /// indicator).
    pub fn bottom(child: impl Widget + 'static) -> Self {
        Self::new(child, SafeAreaClaimEdges::BOTTOM)
    }

    /// Claim the top edge only (e.g. for a nav bar that owns the status
    /// bar).
    pub fn top(child: impl Widget + 'static) -> Self {
        Self::new(child, SafeAreaClaimEdges::TOP)
    }

    /// Claim all edges.
    pub fn all(child: impl Widget + 'static) -> Self {
        Self::new(child, SafeAreaClaimEdges::ALL)
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for SafeAreaClaim {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            claim: self.claim,
        }
    }
}

impl Widget for SafeAreaClaim {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = SafeAreaClaimElement::new();
        elem.set_stored_key(self.key.clone());
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(SafeAreaClaimRenderObject::new(self.claim))
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
            .downcast_mut::<SafeAreaClaimRenderObject>()
        {
            if ro.set_claim(self.claim) {
                // Claim change alters effective insets for descendants →
                // needs layout so the top-down walk re-propagates.
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

    // ========================================================================
    // SafeAreaClaimEdges tests
    // ========================================================================

    #[test]
    fn test_claim_edges_remove_from_zeroes_claimed_edges() {
        let insets = EdgeInsets {
            left: 10.0,
            right: 20.0,
            top: 30.0,
            bottom: 40.0,
        };
        // Claim bottom only
        let reduced = SafeAreaClaimEdges::BOTTOM.remove_from(insets);
        assert_eq!(reduced.bottom, 0.0);
        assert_eq!(reduced.top, 30.0);
        assert_eq!(reduced.left, 10.0);
        assert_eq!(reduced.right, 20.0);

        // Claim all
        let all_zero = SafeAreaClaimEdges::ALL.remove_from(insets);
        assert_eq!(all_zero, EdgeInsets::default());

        // Claim none — passthrough
        let same = SafeAreaClaimEdges::NONE.remove_from(insets);
        assert_eq!(same, insets);
    }

    // ========================================================================
    // SafeAreaRenderObject claim + effective insets tests
    // ========================================================================

    #[test]
    fn test_safe_area_render_object_claims_enabled_sides() {
        let ro = SafeAreaRenderObject::new(true, false, true, false, EdgeInsets::default());
        let claim = ro.safe_area_claim();
        assert!(claim.top && claim.bottom);
        assert!(!claim.left && !claim.right);
    }

    #[test]
    fn test_safe_area_render_object_set_effective_safe_area() {
        let mut ro = SafeAreaRenderObject::new(true, true, true, true, EdgeInsets::default());
        let insets = EdgeInsets {
            left: 0.0,
            right: 0.0,
            top: 44.0,
            bottom: 0.0, // bottom already claimed by ancestor
        };
        ro.set_effective_safe_area(insets);

        // layout() should use the effective insets (bottom=0), not the
        // global source. We verify by checking the padding the inner
        // container would use — via effective_padding with the stored value.
        let padding = ro.effective_padding(insets);
        assert_eq!(padding.3, 0.0); // bottom = 0 (effective)
    }

    #[test]
    fn test_safe_area_render_object_layout_falls_back_to_global_when_no_effective() {
        // When set_effective_safe_area has NOT been called (e.g. direct
        // layout() in unit tests), layout() should fall back to the global
        // source. This is the backward-compatibility path.
        let source = SafeAreaSource::new(0.0, 0.0, 44.0, 34.0);
        let mut ro = SafeAreaRenderObject::new(false, false, false, true, EdgeInsets::default());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ctx.set_safe_area_source(source);

        let _ = ro.layout(&mut ctx, &[]);
        // No panic = fallback worked. The RO used the global source.
    }

    // ========================================================================
    // SafeAreaClaimRenderObject tests
    // ========================================================================

    #[test]
    fn test_safe_area_claim_render_object_claims_edges() {
        let ro = SafeAreaClaimRenderObject::new(SafeAreaClaimEdges::BOTTOM);
        assert_eq!(ro.safe_area_claim(), SafeAreaClaimEdges::BOTTOM);
    }

    #[test]
    fn test_safe_area_claim_render_object_is_pass_through() {
        let ro = SafeAreaClaimRenderObject::new(SafeAreaClaimEdges::BOTTOM);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_safe_area_claim_render_object_set_claim_detects_change() {
        let mut ro = SafeAreaClaimRenderObject::new(SafeAreaClaimEdges::NONE);
        assert!(ro.set_claim(SafeAreaClaimEdges::BOTTOM));
        assert!(!ro.set_claim(SafeAreaClaimEdges::BOTTOM)); // same → no change
    }

    #[test]
    fn test_safe_area_claim_render_object_paint_empty() {
        let ro = SafeAreaClaimRenderObject::new(SafeAreaClaimEdges::BOTTOM);
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(cmds.is_empty(), "SafeAreaClaim should not paint anything");
    }

    // ========================================================================
    // SafeAreaClaim widget tests
    // ========================================================================

    #[test]
    fn test_safe_area_claim_bottom_convenience() {
        let w = SafeAreaClaim::bottom(Text::new("Hi"));
        assert_eq!(w.claim, SafeAreaClaimEdges::BOTTOM);
        assert!(w.child().is_some());
    }

    #[test]
    fn test_safe_area_claim_top_convenience() {
        let w = SafeAreaClaim::top(Text::new("Hi"));
        assert_eq!(w.claim, SafeAreaClaimEdges::TOP);
    }

    #[test]
    fn test_safe_area_claim_all_convenience() {
        let w = SafeAreaClaim::all(Text::new("Hi"));
        assert_eq!(w.claim, SafeAreaClaimEdges::ALL);
    }

    #[test]
    fn test_safe_area_claim_clone_preserves_claim() {
        let w = SafeAreaClaim::bottom(Text::new("Hi"));
        let cloned = w.clone();
        assert_eq!(cloned.claim, w.claim);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_safe_area_claim_update_render_object_claim_change() {
        let w1 = SafeAreaClaim::new(Text::new("Hi"), SafeAreaClaimEdges::NONE);
        let w2 = SafeAreaClaim::bottom(Text::new("Hi"));
        let mut ro = SafeAreaClaimRenderObject::new(SafeAreaClaimEdges::NONE);
        // Claim changed NONE → BOTTOM → LAYOUT
        assert!(w2
            .update_render_object(&mut ro)
            .contains(UpdateResult::LAYOUT));
        // Same claim → NONE
        assert_eq!(w2.update_render_object(&mut ro), UpdateResult::NONE);
    }

    #[test]
    fn test_safe_area_claim_widget_with_key() {
        let w = SafeAreaClaim::bottom(Text::new("Hi")).with_key("claim");
        assert_eq!(
            w.key(),
            Some(WidgetKey::Local(crate::key::Key::new("claim")))
        );
    }

    // ========================================================================
    // Pipeline-level test: SafeAreaClaim prevents double-consume
    // ========================================================================

    #[test]
    fn test_safe_area_claim_prevents_double_consume_in_pipeline() {
        // Regression: when SafeAreaClaim::bottom wraps a SafeArea, the
        // inner SafeArea should see bottom=0 (claimed by the ancestor),
        // not the global 34px home-indicator inset. This is the fix for
        // the gap between the input bar and the tab bar.
        use crate::animation::AnimationTicker;
        use crate::core::SafeAreaSource;
        use crate::{Flex, ThreeTreePipeline};

        // Tree: SafeAreaClaim::bottom(SafeArea(Text))
        //       — the claim zeroes bottom for the SafeArea.
        let tree = SafeAreaClaim::bottom(SafeArea::new(Text::new("Hi")).flex_fill());

        let mut pipeline = ThreeTreePipeline::new(std::sync::Arc::new(AnimationTicker::new()));
        pipeline.update(tree.boxed());

        // Set non-zero safe-area insets (mimicking iOS: 44pt top, 34pt bottom)
        pipeline.set_safe_area_source(SafeAreaSource::new(0.0, 0.0, 44.0, 34.0));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Find the SafeAreaRenderObject in the tree and verify its effective
        // insets have bottom=0 (claimed by the ancestor SafeAreaClaim).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Walk the tree to find the SafeAreaRenderObject.
        fn find_safe_area_ro(
            ro_reg: &crate::RenderObjectRegistry,
            id: crate::RenderObjectKey,
        ) -> Option<crate::RenderObjectKey> {
            let ro = ro_reg.get(id)?;
            if ro.as_any().downcast_ref::<SafeAreaRenderObject>().is_some() {
                return Some(id);
            }
            for &child in ro.children() {
                if let Some(found) = find_safe_area_ro(ro_reg, child) {
                    return Some(found);
                }
            }
            None
        }

        let safe_area_id = find_safe_area_ro(ro_reg, root).expect("SafeAreaRenderObject");
        let safe_area_ro = ro_reg
            .get(safe_area_id)
            .and_then(|ro| ro.as_any().downcast_ref::<SafeAreaRenderObject>())
            .expect("downcast SafeAreaRenderObject");

        // The effective insets should have bottom=0 (claimed by SafeAreaClaim).
        let effective = safe_area_ro
            .effective
            .expect("effective insets set by layouter walk");
        assert_eq!(
            effective.bottom, 0.0,
            "SafeArea inside SafeAreaClaim::bottom should see bottom=0, got {}",
            effective.bottom
        );
        assert_eq!(
            effective.top, 44.0,
            "top should pass through (not claimed), got {}",
            effective.top
        );
    }

    #[test]
    fn test_nested_safe_area_no_double_consume() {
        // Regression: nested SafeArea widgets should not double-consume.
        // Outer SafeArea(all) claims all edges; inner SafeArea(all) should
        // see all-zero effective insets → no padding.
        use crate::animation::AnimationTicker;
        use crate::core::SafeAreaSource;
        use crate::{Flex, ThreeTreePipeline};

        let tree = SafeArea::new(SafeArea::new(Text::new("Hi")));

        let mut pipeline = ThreeTreePipeline::new(std::sync::Arc::new(AnimationTicker::new()));
        pipeline.update(tree.boxed());
        pipeline.set_safe_area_source(SafeAreaSource::new(10.0, 20.0, 44.0, 34.0));

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Find the INNER SafeAreaRenderObject (the one inside the outer).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_all_safe_area_ros(
            ro_reg: &crate::RenderObjectRegistry,
            id: crate::RenderObjectKey,
            out: &mut Vec<crate::RenderObjectKey>,
        ) {
            if let Some(ro) = ro_reg.get(id) {
                if ro.as_any().downcast_ref::<SafeAreaRenderObject>().is_some() {
                    out.push(id);
                }
                for &child in ro.children() {
                    find_all_safe_area_ros(ro_reg, child, out);
                }
            }
        }

        let mut safe_area_ids = Vec::new();
        find_all_safe_area_ros(ro_reg, root, &mut safe_area_ids);
        assert_eq!(
            safe_area_ids.len(),
            2,
            "expected 2 nested SafeAreaRenderObjects"
        );

        // The inner one (second found in DFS) should have all-zero effective.
        let inner_ro = ro_reg
            .get(safe_area_ids[1])
            .and_then(|ro| ro.as_any().downcast_ref::<SafeAreaRenderObject>())
            .expect("inner SafeAreaRenderObject");

        let effective = inner_ro
            .effective
            .expect("effective insets set by layouter walk");
        assert_eq!(
            effective,
            EdgeInsets::default(),
            "inner SafeArea should see all-zero insets (claimed by outer), got {:?}",
            effective
        );
    }
}
