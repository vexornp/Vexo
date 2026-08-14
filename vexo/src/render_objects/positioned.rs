//! Render object for Positioned — absolutely positions its child within a Stack.
//!
//! Creates a Taffy container node with `position: Absolute` and the given insets
//! (top/right/bottom/left). The child's layout node is the only child of this
//! container. Taffy positions the absolute node relative to the nearest
//! positioned ancestor (the Stack, which is `position: Relative`).
//!
//! If both `left` and `right` are set, the width is determined by the insets.
//! If only one horizontal inset is set, the child uses its intrinsic width.
//! Same for vertical.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{FlexDirection, Layout, LayoutNodeKey, Position};
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

/// Insets for a Positioned widget. `None` means "auto" (not specified).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PositionedInsets {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl PositionedInsets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn top(mut self, v: f32) -> Self {
        self.top = Some(v);
        self
    }
    pub fn right(mut self, v: f32) -> Self {
        self.right = Some(v);
        self
    }
    pub fn bottom(mut self, v: f32) -> Self {
        self.bottom = Some(v);
        self
    }
    pub fn left(mut self, v: f32) -> Self {
        self.left = Some(v);
        self
    }
    pub fn width(mut self, v: f32) -> Self {
        self.width = Some(v);
        self
    }
    pub fn height(mut self, v: f32) -> Self {
        self.height = Some(v);
        self
    }
}

/// Render object for Positioned — absolutely positions its child within a Stack.
pub struct PositionedRenderObject {
    insets: PositionedInsets,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl PositionedRenderObject {
    pub fn new(insets: PositionedInsets) -> Self {
        Self {
            insets,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the insets. Returns true if they changed.
    pub fn set_insets(&mut self, insets: PositionedInsets) -> bool {
        if self.insets != insets {
            self.insets = insets;
            true
        } else {
            false
        }
    }

    fn build_layout(&self) -> Layout {
        let mut layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .position(Position::Absolute);

        if let Some(top) = self.insets.top {
            layout = layout.top(top);
        }
        if let Some(right) = self.insets.right {
            layout = layout.right(right);
        }
        if let Some(bottom) = self.insets.bottom {
            layout = layout.bottom(bottom);
        }
        if let Some(left) = self.insets.left {
            layout = layout.left(left);
        }
        if let Some(width) = self.insets.width {
            layout = layout.width(width);
        }
        if let Some(height) = self.insets.height {
            layout = layout.height(height);
        }

        layout
    }
}

impl RenderObject for PositionedRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let layout = self.build_layout();

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
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
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

    fn remove_child(&mut self, child: RenderObjectKey) {
        if self.child == Some(child) {
            self.child = None;
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_positioned_insets_builder() {
        let insets = PositionedInsets::new().top(10.0).left(20.0);
        assert_eq!(insets.top, Some(10.0));
        assert_eq!(insets.left, Some(20.0));
        assert_eq!(insets.right, None);
        assert_eq!(insets.bottom, None);
    }

    #[test]
    fn test_positioned_set_insets_change_detection() {
        let mut ro = PositionedRenderObject::new(PositionedInsets::new().top(10.0));
        assert!(ro.set_insets(PositionedInsets::new().top(20.0)));
        assert!(!ro.set_insets(PositionedInsets::new().top(20.0)));
    }

    #[test]
    fn test_positioned_layout_creates_node() {
        let mut ro = PositionedRenderObject::new(PositionedInsets::new().top(10.0).left(20.0));
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);
        assert!(ro.layout_node.is_some());
        assert_eq!(ro.layout_node, Some(result.node));
    }

    #[test]
    fn test_positioned_children() {
        let mut ro = PositionedRenderObject::new(PositionedInsets::new());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child = sm.insert(());
        ro.set_child_id(child);
        assert_eq!(ro.children(), &[child]);
    }

    #[test]
    fn test_positioned_replace_child() {
        let mut ro = PositionedRenderObject::new(PositionedInsets::new());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let c1 = sm.insert(());
        let c2 = sm.insert(());
        ro.set_child_id(c1);
        ro.replace_child(c1, c2);
        assert_eq!(ro.children(), &[c2]);
    }
}
