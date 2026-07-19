//! ContainerRenderObject implementation for Flex, Grid, and DecoratedContainer.

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

/// RenderObject for container widgets (Flex, Grid, DecoratedContainer).
///
/// Container render objects hold references to child render objects,
/// participate in layout, and optionally paint decorations (background,
/// border, corner radius, clip).
///
/// # Example
///
/// ```ignore
/// use vexo::render_objects::ContainerRenderObject;
/// use vexo::layout::{Layout, FlexDirection, AlignItems};
/// use vexo::style::Style;
/// use vexo::core::Color;
///
/// let layout = Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch);
/// let style = Style::new().background(Color::RED).border(Color::BLACK, 2.0);
/// let mut container = ContainerRenderObject::new_with_style(layout, style);
/// container.add_child(child_id);
/// ```
pub struct ContainerRenderObject {
    children: Vec<RenderObjectKey>,
    layout: Layout,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl ContainerRenderObject {
    /// Create a new container with the given layout and default style.
    pub fn new(layout: Layout) -> Self {
        Self::new_with_style(layout, Style::default())
    }

    /// Create a new container with the given layout and style.
    pub fn new_with_style(layout: Layout, style: Style) -> Self {
        Self {
            children: Vec::new(),
            layout,
            style,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Add a child render object.
    pub fn add_child(&mut self, child: RenderObjectKey) {
        self.children.push(child);
    }

    /// Set children directly.
    pub fn set_children(&mut self, children: Vec<RenderObjectKey>) {
        self.children = children;
    }

    /// Set a single child render object (for single-child modifier widgets).
    pub fn set_child_id(&mut self, child: RenderObjectKey) {
        self.children = vec![child];
    }

    /// Set the layout, returning true if it changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }

    /// Set the style, returning true if it changed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Set the computed bounds directly (for testing).
    #[cfg(test)]
    pub fn set_computed_bounds(&mut self, bounds: Option<Bounds<Logical>>) {
        self.computed_bounds = bounds;
    }

    /// Clear all children.
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    /// Get the number of children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        match self.layout_node {
            Some(existing) => {
                // Incremental: update existing node
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                // First frame: create new node
                let node = ctx.engine().create_container(&self.layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        // Container reads computed bounds from Taffy
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

        let base_corner_radius = self
            .style
            .corner_radius
            .as_ref()
            .map(|cr| cr.radius)
            .unwrap_or(0.0);

        // 1. Emit shadows BEFORE fill/border (shadows draw behind everything).
        // Shadows bypass PushCornerRadius context — each shadow Rect carries its
        // own corner_radius field (computed as base + spread).
        // Shadows also bypass style.clip's PushClip — clipping the shadow to the
        // very shape casting it would make it invisible.
        for shadow in &self.style.shadows {
            if shadow.color.a == 0.0 {
                continue;
            }
            let blur = shadow.blur_radius.max(0.0);
            let pad = blur + shadow.spread_radius;
            let shadow_bounds = Bounds::new(
                absolute_bounds.left + shadow.offset.x - pad,
                absolute_bounds.top + shadow.offset.y - pad,
                absolute_bounds.right + shadow.offset.x + pad,
                absolute_bounds.bottom + shadow.offset.y + pad,
            );
            let shadow_corner_radius = (base_corner_radius + shadow.spread_radius).max(0.0);
            commands.push(RenderCommand::Rect {
                bounds: shadow_bounds,
                fill: shadow.color,
                stroke: None,
                corner_radius: shadow_corner_radius,
                shadow_color: shadow.color.to_array(),
                shadow_blur: blur,
            });
        }

        // 2. Push corner radius if set (affects fill/border only, NOT shadows)
        if let Some(ref cr) = self.style.corner_radius {
            commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
        }

        // 3. Draw background first (behind child)
        if let Some(bg_color) = self.style.background {
            commands.push(RenderCommand::rect(absolute_bounds, bg_color));
        }

        // 4. Draw border on top (after background)
        if let Some(ref border) = self.style.border {
            commands.push(RenderCommand::rect_with_border(
                absolute_bounds,
                Color::TRANSPARENT,
                border.color,
                border.width,
            ));
        }

        // 5. Pop corner radius
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
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
    use crate::core::Point;
    use crate::layout::{AlignItems, FlexDirection, Layout, LayoutEngine, TaffyLayoutEngine};
    use crate::style::BoxShadow;

    fn column_layout() -> Layout {
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
    }

    fn row_layout() -> Layout {
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .align(AlignItems::Stretch)
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_container_render_object_column() {
        let obj = ContainerRenderObject::new(column_layout());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
        assert!(obj.computed_bounds().is_none());
    }

    #[test]
    fn test_container_render_object_row() {
        let obj = ContainerRenderObject::new(row_layout());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_add_child() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child_id = sm.insert(());
        obj.add_child(child_id);

        assert_eq!(obj.children().len(), 1);
        assert_eq!(obj.child_count(), 1);
        assert_eq!(obj.children()[0], child_id);
    }

    #[test]
    fn test_container_set_children() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child1 = sm.insert(());
        let child2 = sm.insert(());

        obj.set_children(vec![child1, child2]);

        assert_eq!(obj.children().len(), 2);
        assert_eq!(obj.children()[0], child1);
        assert_eq!(obj.children()[1], child2);
    }

    #[test]
    fn test_container_clear_children() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        obj.add_child(sm.insert(()));
        obj.add_child(sm.insert(()));
        assert_eq!(obj.child_count(), 2);

        obj.clear_children();

        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_layout_creates_node() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);

        // Should have created a layout node
        assert!(obj.layout_node.is_some());
        assert_eq!(obj.layout_node, Some(result.node));
    }

    #[test]
    fn test_container_apply_layout() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create node
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _result = obj.layout(&mut ctx, &[]);
        }

        // Compute layout
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 100.0), &mut font_system);

        // Apply layout should read computed bounds
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        // After apply_layout, computed_bounds should be set (though may be zero
        // since the node isn't part of the computed tree properly)
        // The key thing is it doesn't crash
    }

    #[test]
    fn test_container_hit_test_no_layout() {
        let obj = ContainerRenderObject::new(column_layout());

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_container_paint_no_style() {
        let obj = ContainerRenderObject::new(column_layout());

        // Paint returns empty when no style decorations
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_container_paint_no_bounds() {
        let style = Style::new().background(Color::RED);
        let obj = ContainerRenderObject::new_with_style(column_layout(), style);

        // Paint returns empty when no computed bounds
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_container_paint_with_background() {
        let style = Style::new().background(Color::RED);
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Should have 1 command (background)
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_container_paint_with_border() {
        let style = Style::new().border(Color::BLACK, 2.0);
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Should have 1 command (border)
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_container_paint_with_background_and_border() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Should have 2 commands (background + border)
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_container_paint_with_corner_radius() {
        let style = Style::new().background(Color::RED).corner_radius(8.0);
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Should have 3 commands (push radius + background + pop radius)
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_container_paint_empty_style() {
        let style = Style::new(); // No decorations
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Should have 0 commands (no decorations)
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn test_container_set_style_change_detection() {
        let mut obj = ContainerRenderObject::new(column_layout());

        // Setting the same default style should return false (no change)
        assert!(!obj.set_style(Style::default()));

        // Setting a different style should return true (changed)
        let style = Style::new().background(Color::RED);
        assert!(obj.set_style(style.clone()));

        // Setting the same style again should return false
        assert!(!obj.set_style(style));
    }

    #[test]
    fn test_container_clip_bounds_no_clip() {
        let obj = ContainerRenderObject::new(column_layout());
        // Default style has clip = false
        assert!(obj.clip_bounds().is_none());
    }

    #[test]
    fn test_container_clip_bounds_with_clip_no_bounds() {
        let style = Style::new().clip();
        let obj = ContainerRenderObject::new_with_style(column_layout(), style);
        // clip is true but no computed bounds
        assert!(obj.clip_bounds().is_none());
    }

    #[test]
    fn test_container_clip_bounds_with_clip_and_bounds() {
        let style = Style::new().clip();
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        // clip is true and bounds exist
        assert_eq!(
            obj.clip_bounds(),
            Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0))
        );
    }

    #[test]
    fn test_container_children_trait() {
        let mut obj = ContainerRenderObject::new(row_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child1 = sm.insert(());
        let child2 = sm.insert(());

        obj.add_child(child1);
        obj.add_child(child2);

        // Test the RenderObject::children() trait method
        let children = RenderObject::children(&obj);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }

    #[test]
    fn test_container_set_layout_change_detection() {
        let mut obj = ContainerRenderObject::new(column_layout());

        // Setting the same layout should return false (no change)
        assert!(!obj.set_layout(column_layout()));

        // Setting a different layout should return true (changed)
        assert!(obj.set_layout(row_layout()));

        // Setting the same new layout again should return false
        assert!(!obj.set_layout(row_layout()));
    }

    #[test]
    fn test_container_set_child_id() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();

        // Add two children via add_child
        let child1 = sm.insert(());
        let child2 = sm.insert(());
        obj.add_child(child1);
        obj.add_child(child2);
        assert_eq!(obj.child_count(), 2);

        // set_child_id replaces all children with a single child
        let child3 = sm.insert(());
        obj.set_child_id(child3);
        assert_eq!(obj.child_count(), 1);
        assert_eq!(obj.children()[0], child3);
    }

    #[test]
    fn test_container_paint_with_single_shadow() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // 1 shadow Rect + 1 background Rect
        assert_eq!(cmds.len(), 2);

        // First command is the shadow
        match &cmds[0] {
            RenderCommand::Rect {
                shadow_color,
                shadow_blur,
                bounds,
                corner_radius,
                ..
            } => {
                assert_eq!(*shadow_color, Color::BLACK.to_array());
                assert_eq!(*shadow_blur, 8.0);
                // pad = blur(8) + spread(0) = 8; shadow rect grown by 8 on each side
                assert_eq!(bounds.width(), 100.0 + 2.0 * 8.0);
                assert_eq!(bounds.height(), 50.0 + 2.0 * 8.0);
                assert_eq!(*corner_radius, 0.0); // base corner_radius is 0
            }
            _ => panic!("Expected shadow Rect as first command"),
        }
    }

    #[test]
    fn test_container_paint_with_multiple_shadows() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0))
            .shadow(BoxShadow::new(Color::RED).blur(4.0))
            .shadow(BoxShadow::new(Color::BLUE).blur(12.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // 3 shadow Rects + 1 background Rect
        assert_eq!(cmds.len(), 4);

        // Verify shadows are in list order (first = back)
        match &cmds[0] {
            RenderCommand::Rect { shadow_color, .. } => {
                assert_eq!(*shadow_color, Color::BLACK.to_array());
            }
            _ => panic!("Expected first shadow"),
        }
        match &cmds[1] {
            RenderCommand::Rect { shadow_color, .. } => {
                assert_eq!(*shadow_color, Color::RED.to_array());
            }
            _ => panic!("Expected second shadow"),
        }
        match &cmds[2] {
            RenderCommand::Rect { shadow_color, .. } => {
                assert_eq!(*shadow_color, Color::BLUE.to_array());
            }
            _ => panic!("Expected third shadow"),
        }
    }

    #[test]
    fn test_container_paint_shadow_respects_offset() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).offset(10.0, 20.0).blur(4.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        match &cmds[0] {
            RenderCommand::Rect { bounds, .. } => {
                // pad = 4; shadow left = base.left(0) + offset.x(10) - pad(4) = 6
                // shadow top  = base.top(0)  + offset.y(20) - pad(4) = 16
                assert_eq!(bounds.left, 6.0);
                assert_eq!(bounds.top, 16.0);
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_respects_blur_and_spread() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(12.0).spread(4.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        match &cmds[0] {
            RenderCommand::Rect { bounds, .. } => {
                // pad = blur(12) + spread(4) = 16
                assert_eq!(bounds.width(), 100.0 + 2.0 * 16.0);
                assert_eq!(bounds.height(), 50.0 + 2.0 * 16.0);
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_with_corner_radius() {
        let style = Style::new()
            .background(Color::WHITE)
            .corner_radius(8.0)
            .shadow(BoxShadow::new(Color::BLACK).spread(4.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Shadow corner_radius = base(8) + spread(4) = 12
        match &cmds[0] {
            RenderCommand::Rect { corner_radius, .. } => {
                assert_eq!(*corner_radius, 12.0);
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_skips_transparent_color() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::TRANSPARENT).blur(8.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Shadow is transparent → skipped → only background Rect emitted
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_container_paint_shadow_negative_blur_clamped() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(-5.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        match &cmds[0] {
            RenderCommand::Rect { shadow_blur, .. } => {
                assert_eq!(*shadow_blur, 0.0);
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_zero_blur_sharp() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(0.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Shadow with blur=0 is still emitted (sharp shadow is valid)
        assert_eq!(cmds.len(), 2);
        match &cmds[0] {
            RenderCommand::Rect { shadow_blur, .. } => {
                assert_eq!(*shadow_blur, 0.0);
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_extends_beyond_bounds() {
        let style = Style::new()
            .background(Color::WHITE)
            .clip()
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Shadow Rect must extend beyond computed_bounds (clip doesn't clip own shadow).
        match &cmds[0] {
            RenderCommand::Rect { bounds, .. } => {
                assert!(bounds.left < 0.0, "Shadow should extend left of bounds");
                assert!(bounds.top < 0.0, "Shadow should extend above bounds");
                assert!(bounds.right > 100.0, "Shadow should extend right of bounds");
                assert!(bounds.bottom > 50.0, "Shadow should extend below bounds");
            }
            _ => panic!("Expected shadow Rect"),
        }
    }

    #[test]
    fn test_container_paint_shadow_no_corner_radius_context() {
        // Shadows must NOT be wrapped in PushCornerRadius/PopCornerRadius.
        // Each shadow Rect carries its own corner_radius field.
        let style = Style::new()
            .corner_radius(8.0)
            .shadow(BoxShadow::new(Color::BLACK).blur(4.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // Find the shadow Rect.
        let shadow_idx = cmds.iter().position(
            |c| matches!(c, RenderCommand::Rect { shadow_color, .. } if shadow_color[3] > 0.0),
        );
        assert!(shadow_idx.is_some());

        // Verify NO PushCornerRadius appears before the shadow.
        for cmd in cmds.iter().take(shadow_idx.unwrap()) {
            assert!(
                !matches!(cmd, RenderCommand::PushCornerRadius { .. }),
                "Shadow must not be wrapped in PushCornerRadius"
            );
        }
    }

    #[test]
    fn test_container_paint_shadows_do_not_affect_hit_test() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(20.0).spread(20.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        // Point is inside shadow area but outside computed_bounds
        // Shadow extends 40px (blur+spread) beyond bounds in all directions
        let outside_point = Point::new(-10.0, -10.0);
        assert!(!obj.hit_test(outside_point, &HitTestContext::mock()));

        // Point inside bounds still hits
        let inside_point = Point::new(50.0, 25.0);
        assert!(obj.hit_test(inside_point, &HitTestContext::mock()));
    }

    #[test]
    fn test_container_set_style_detects_shadow_change() {
        let mut obj = ContainerRenderObject::new(column_layout());

        let style1 = Style::new().shadow(BoxShadow::new(Color::BLACK));
        obj.set_style(style1);

        let style2 = Style::new()
            .shadow(BoxShadow::new(Color::BLACK))
            .shadow(BoxShadow::new(Color::RED));
        assert!(
            obj.set_style(style2),
            "Adding a shadow should trigger style change"
        );
    }

    #[test]
    fn test_container_set_style_same_shadows_no_change() {
        let mut obj = ContainerRenderObject::new(column_layout());

        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        obj.set_style(style.clone());

        assert!(
            !obj.set_style(style),
            "Setting same style should not trigger change"
        );
    }

    #[test]
    fn test_container_paint_shadow_no_background_still_emits_shadow() {
        let style = Style::new().shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // No background → only shadow emitted
        assert_eq!(cmds.len(), 1);
        assert!(
            matches!(&cmds[0], RenderCommand::Rect { shadow_color, .. } if shadow_color[3] > 0.0)
        );
    }

    #[test]
    fn test_container_paint_no_shadows_unchanged_behavior() {
        // Regression: existing behavior with empty shadows must be unchanged.
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);
        let mut obj = ContainerRenderObject::new_with_style(column_layout(), style);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = obj.paint(&mut ctx);

        // 2 commands: background + border (same as existing test_container_paint_with_background_and_border)
        assert_eq!(cmds.len(), 2);
    }
}
