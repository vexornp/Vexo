//! Render object for Spacer — a leaf that claims a share of free space.
//!
//! `layout()` creates a Taffy leaf with `Layout::default().flex_grow(1.0)`.
//! `paint()` emits nothing, `hit_test()` returns false, `children()` is empty.
//! Direction-agnostic: the parent's `flex_direction` decides which axis the
//! spacer grows along, which is why the layout uses `Layout::default()` (not
//! `Layout::row()` / `Layout::column()`).

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

pub struct SpacerRenderObject {
    owned_node: Option<LayoutNodeKey>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl SpacerRenderObject {
    pub fn new() -> Self {
        Self {
            owned_node: None,
            computed_bounds: None,
        }
    }
}

impl Default for SpacerRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for SpacerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let spacer_layout = Layout::default().flex_grow(1.0);
        let node = match self.owned_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &spacer_layout);
                ctx.engine().set_children(existing, &[]);
                existing
            }
            None => {
                let node = ctx.engine().create_container(&spacer_layout, &[]);
                self.owned_node = Some(node);
                node
            }
        };
        LayoutResult {
            node,
            size: Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.owned_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        false
    }

    fn children(&self) -> &[RenderObjectKey] {
        &[]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.owned_node
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
    fn spacer_layout_creates_node() {
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }

    #[test]
    fn spacer_layout_node_uses_flex_grow_one() {
        // Behavioral assertion: when placed in a 200px-wide row with an 80px
        // fixed sibling, the spacer absorbs the leftover 120px.
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let spacer_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]).node
        };

        let bubble_node = engine.create_leaf(&Layout::default().width(80.0).height(20.0));
        let row = engine.create_container(
            &Layout::row().width(200.0).height(20.0),
            &[spacer_node, bubble_node],
        );

        engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

        let spacer_layout = engine.get_layout(spacer_node).expect("spacer has layout");
        assert_eq!(spacer_layout.x(), 0.0);
        assert_eq!(spacer_layout.width(), 120.0);

        let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");
        assert_eq!(bubble_layout.x(), 120.0);
        assert_eq!(bubble_layout.width(), 80.0);
    }

    #[test]
    fn spacer_paint_is_empty() {
        let ro = SpacerRenderObject::new();
        let mut commands: Vec<crate::render::RenderCommand> = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        assert!(ro.paint(&mut ctx).is_empty());
    }

    #[test]
    fn spacer_hit_test_returns_false() {
        let ro = SpacerRenderObject::new();
        assert!(!ro.hit_test(Point::new(0.0, 0.0), &HitTestContext::mock()));
    }

    #[test]
    fn spacer_children_is_empty() {
        let ro = SpacerRenderObject::new();
        assert_eq!(ro.children(), &[] as &[RenderObjectKey]);
    }

    #[test]
    fn spacer_apply_layout_populates_bounds() {
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let spacer_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]).node
        };

        // Compute against a 100x50 box so the spacer fills it.
        // The pipeline calls `set_root_size` before `compute` (layouter.rs);
        // a root node with `flex_grow(1.0)` + auto size is 0x0 without it.
        engine.set_root_size(spacer_node);
        engine.compute(spacer_node, Size::new(100.0, 50.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("apply_layout populates bounds");
        assert_eq!(bounds.width(), 100.0);
        assert_eq!(bounds.height(), 50.0);
    }
}
