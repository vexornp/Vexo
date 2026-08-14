use crate::core::{Bounds, Logical, Point};
use crate::layout::LayoutNodeKey;
use crate::render::RenderCommand;
use crate::render_object::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject,
};

/// Pass-through render object that clips its child to a rounded rectangle.
///
/// Layout is pass-through (borrows child's Taffy node). The clip is
/// applied by the painter via `clip_bounds()` + `clip_corner_radius()`,
/// which emits `PushClipRRect`/`PopClipRRect` around the child's paint
/// commands. The fragment shader multiplies in an SDF mask.
pub struct ClipRRectRenderObject {
    radius: f32,
    child: Option<crate::id::RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl ClipRRectRenderObject {
    pub fn new(radius: f32) -> Self {
        Self {
            radius: radius.max(0.0),
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the corner radius. Returns true if it changed.
    pub fn set_radius(&mut self, radius: f32) -> bool {
        let clamped = radius.max(0.0);
        if self.radius != clamped {
            self.radius = clamped;
            true
        } else {
            false
        }
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl RenderObject for ClipRRectRenderObject {
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             ClipRRect always has a child per its constructor",
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[crate::id::RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn set_child_id(&mut self, child: crate::id::RenderObjectKey) {
        self.child = Some(child);
        self.child_layout_node = None;
    }

    fn replace_child(&mut self, old: crate::id::RenderObjectKey, new: crate::id::RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
            self.child_layout_node = None;
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_corner_radius(&self) -> Option<f32> {
        if self.radius > 0.0 {
            Some(self.radius)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_rrect_ro_is_pass_through() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert!(
            ro.is_pass_through(),
            "ClipRRectRenderObject must be pass-through"
        );
    }

    #[test]
    fn test_clip_rrect_ro_clip_corner_radius_some_when_positive() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert_eq!(ro.clip_corner_radius(), Some(8.0));
    }

    #[test]
    fn test_clip_rrect_ro_clip_corner_radius_none_when_zero() {
        let ro = ClipRRectRenderObject::new(0.0);
        assert_eq!(ro.clip_corner_radius(), None);
    }

    #[test]
    fn test_clip_rrect_ro_set_radius_change_detection() {
        let mut ro = ClipRRectRenderObject::new(8.0);
        assert!(ro.set_radius(12.0));
        assert!(!ro.set_radius(12.0));
        assert!(ro.set_radius(0.0));
        assert!(!ro.set_radius(0.0));
    }

    #[test]
    fn test_clip_rrect_ro_negative_radius_clamped() {
        let ro = ClipRRectRenderObject::new(-5.0);
        assert_eq!(ro.radius(), 0.0);
        assert_eq!(ro.clip_corner_radius(), None);
    }

    #[test]
    fn test_clip_rrect_ro_clip_bounds_none_before_layout() {
        let ro = ClipRRectRenderObject::new(8.0);
        assert!(ro.clip_bounds().is_none());
    }
}
