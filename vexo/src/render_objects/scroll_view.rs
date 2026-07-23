//! ScrollViewRenderObject - manages scroll offset and viewport clipping.

use std::any::Any;
use std::cell::Cell;

use crate::core::{Bounds, Logical, Point, Size};
use crate::id::RenderObjectKey;
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey, Overflow};
use crate::render::RenderCommand;
use crate::render_object::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject,
};

pub struct ScrollViewRenderObject {
    child: Option<RenderObjectKey>,
    scroll_offset: Cell<f32>,
    content_size: Size<Logical>,
    viewport_size: Size<Logical>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl ScrollViewRenderObject {
    pub fn new() -> Self {
        Self {
            child: None,
            scroll_offset: Cell::new(0.0),
            content_size: Size::zero(),
            viewport_size: Size::zero(),
            computed_bounds: None,
            layout_node: None,
            child_layout_node: None,
        }
    }

    pub fn set_scroll_offset(&self, offset: f32) {
        self.scroll_offset.set(offset);
    }

    pub fn scroll_offset_value(&self) -> f32 {
        self.scroll_offset.get()
    }

    pub fn content_size(&self) -> Size<Logical> {
        self.content_size
    }

    pub fn viewport_size(&self) -> Size<Logical> {
        self.viewport_size
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_size.height - self.viewport_size.height).max(0.0)
    }
}

impl Default for ScrollViewRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for ScrollViewRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        self.child_layout_node = child_nodes.first().copied();

        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .overflow_x(Overflow::Hidden)
            .overflow_y(Overflow::Scroll);

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
                self.viewport_size = Size::new(computed.bounds.width(), computed.bounds.height());
            }
        }

        if let Some(child_node) = self.child_layout_node {
            if let Some(child_computed) = ctx.engine_ref().get_layout(child_node) {
                self.content_size = Size::new(
                    child_computed.bounds.width(),
                    child_computed.bounds.height(),
                );
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        self.computed_bounds
            .map_or(false, |b| b.contains(&position))
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(c) => std::slice::from_ref(c),
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
        self.layout_node
    }
    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn scroll_offset(&self) -> Option<Point<Logical>> {
        Some(Point::new(0.0, -self.scroll_offset.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let ro = ScrollViewRenderObject::new();
        assert_eq!(ro.scroll_offset_value(), 0.0);
        assert_eq!(ro.max_scroll(), 0.0);
    }

    #[test]
    fn test_set_scroll_offset_via_cell() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(42.0);
        assert_eq!(ro.scroll_offset_value(), 42.0);
    }

    #[test]
    fn test_scroll_offset_trait_method() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(100.0);
        let offset = ro.scroll_offset().unwrap();
        assert_eq!(offset.x, 0.0);
        assert_eq!(offset.y, -100.0);
    }

    #[test]
    fn test_hit_test_transform_is_none() {
        let ro = ScrollViewRenderObject::new();
        ro.set_scroll_offset(50.0);
        // ScrollView uses scroll_offset for child pointer adjustment, not hit_test_transform.
        // hit_test_transform would break the is_inside check by shifting local coords.
        assert!(ro.hit_test_transform().is_none());
    }

    #[test]
    fn test_clip_bounds_returns_computed_bounds() {
        let mut ro = ScrollViewRenderObject::new();
        assert!(ro.clip_bounds().is_none());
        ro.computed_bounds = Some(Bounds::from_xywh(10.0, 20.0, 200.0, 100.0));
        assert!(ro.clip_bounds().is_some());
    }
}
