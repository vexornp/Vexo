//! Render object for IndexedStack — lays out only the visible child.
//!
//! Matches Flutter's `RenderIndexedStack.performLayout`: only the child at
//! `index` participates in Taffy layout. Offstage children's zero-size leaf
//! nodes (owned by their `OffstageRenderObject`) are NOT linked into this
//! node's Taffy children list, so Taffy's `compute()` never visits them.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

pub struct IndexedStackRenderObject {
    children: Vec<RenderObjectKey>,
    index: usize,
    layout: Layout,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl IndexedStackRenderObject {
    pub fn new(index: usize) -> Self {
        Self::new_with_style(index, indexed_stack_layout(), Style::default())
    }

    pub fn new_with_style(index: usize, layout: Layout, style: Style) -> Self {
        Self {
            children: Vec::new(),
            index,
            layout,
            style,
            computed_bounds: None,
            layout_node: None,
        }
    }

    pub fn set_index(&mut self, index: usize) -> bool {
        if self.index != index {
            self.index = index;
            true
        } else {
            false
        }
    }

    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }

    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

fn indexed_stack_layout() -> Layout {
    use crate::layout::{AlignItems, FlexDirection};
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
        .height_percent(1.0)
}

impl RenderObject for IndexedStackRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let visible_nodes: Vec<LayoutNodeKey> = child_nodes
            .get(self.index)
            .map(|n| vec![*n])
            .unwrap_or_default();

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, &visible_nodes);
                LayoutResult {
                    node: existing,
                    size: crate::core::Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_container(&self.layout, &visible_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: crate::core::Size::zero(),
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

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let mut commands = Vec::new();
        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        let absolute_bounds = Bounds::new(
            pos.x,
            pos.y,
            pos.x + bounds.width(),
            pos.y + bounds.height(),
        );

        if let Some(ref cr) = self.style.corner_radius {
            commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
        }

        if let Some(bg_color) = self.style.background {
            commands.push(RenderCommand::rect(absolute_bounds, bg_color));
        }

        if let Some(ref border) = self.style.border {
            commands.push(RenderCommand::rect_with_border(
                absolute_bounds,
                Color::TRANSPARENT,
                border.color,
                border.width,
            ));
        }

        if self.style.corner_radius.is_some() {
            commands.push(RenderCommand::PopCornerRadius);
        }

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        &self.children
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn add_child(&mut self, child: RenderObjectKey) {
        self.children.push(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if let Some(pos) = self.children.iter().position(|&c| c == old) {
            self.children[pos] = new;
        } else {
            self.children.push(new);
        }
    }

    fn clear_children(&mut self) {
        self.children.clear();
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_indexed_stack_ro_creation() {
        let ro = IndexedStackRenderObject::new(0);
        assert_eq!(ro.index(), 0);
        assert!(ro.layout_node().is_none());
    }

    #[test]
    fn test_indexed_stack_ro_set_index() {
        let mut ro = IndexedStackRenderObject::new(0);
        assert!(ro.set_index(2));
        assert_eq!(ro.index(), 2);
        assert!(!ro.set_index(2));
    }

    #[test]
    fn test_indexed_stack_ro_layout_filters_to_visible_child() {
        let mut ro = IndexedStackRenderObject::new(1);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };
        let child1 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(80.0).height(60.0))
        };
        let child2 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(30.0).height(30.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child0, child1, child2]);

        let stack_node = ro.layout_node().expect("should have a layout node");

        let linked_children = engine.children(stack_node);
        assert_eq!(
            linked_children.len(),
            1,
            "only the visible child (index 1) should be linked"
        );
        assert_eq!(
            linked_children[0], child1,
            "the linked child should be the one at index 1"
        );
    }

    #[test]
    fn test_indexed_stack_ro_layout_index_out_of_bounds_links_nothing() {
        let mut ro = IndexedStackRenderObject::new(5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child0]);

        let stack_node = ro.layout_node().expect("should have a layout node");
        let linked_children = engine.children(stack_node);
        assert!(
            linked_children.is_empty(),
            "index out of bounds should link no children"
        );
    }

    #[test]
    fn test_indexed_stack_ro_index_change_relays_children() {
        let mut ro = IndexedStackRenderObject::new(0);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };
        let child1 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(80.0).height(60.0))
        };

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0, child1]);
        }

        let stack_node = ro.layout_node().unwrap();
        assert_eq!(engine.children(stack_node), vec![child0]);

        ro.set_index(1);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0, child1]);
        }

        assert_eq!(
            engine.children(stack_node),
            vec![child1],
            "after index flip, the visible child should be child1"
        );
    }

    #[test]
    fn test_indexed_stack_ro_apply_layout_reads_bounds() {
        let mut ro = IndexedStackRenderObject::new(0);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(100.0).height(50.0))
        };

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0]);
        }

        let stack_node = ro.layout_node().unwrap();
        engine.compute(stack_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("should have bounds");
        assert_eq!(bounds.width(), 200.0);
        assert_eq!(bounds.height(), 200.0);
    }
}
