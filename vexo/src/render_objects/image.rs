//! ImageRenderObject implementation.

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position};
use crate::image_atlas::ImageKey;
use crate::image_data::ImageData;
use crate::layout::{Dimension, Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};

/// RenderObject for image display.
///
/// This render object handles layout and hit testing for image content.
/// Image rendering is handled by the GPU pipeline via `RenderCommand::Image`.
///
/// # Image Registration
///
/// On first paint, the render object signals that it needs image registration
/// via `needs_image_registration()`. The pipeline calls `register_images()`
/// which uploads the pixel data to the GPU atlas and calls `set_image_key()`
/// with the resulting key. Subsequent paint calls use this key.
pub struct ImageRenderObject {
    image_data: ImageData,
    image_key: Option<ImageKey>,
    style: Style,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl ImageRenderObject {
    /// Create a new image render object.
    pub fn new(image_data: &ImageData, style: Style, layout: Layout) -> Self {
        Self {
            image_data: image_data.clone(),
            image_key: None,
            style,
            layout,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Set the image data.
    ///
    /// Returns true if the data changed. When data changes, the image_key
    /// is reset to None so the pipeline re-registers the image.
    pub fn set_image_data(&mut self, image_data: &ImageData) -> bool {
        if self.image_data.pixels != image_data.pixels
            || self.image_data.width != image_data.width
            || self.image_data.height != image_data.height
        {
            self.image_data = image_data.clone();
            self.image_key = None;
            true
        } else {
            false
        }
    }

    /// Set the style configuration.
    ///
    /// Returns true if the style changed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    /// Set the layout configuration.
    ///
    /// Returns true if the layout changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }
}

impl RenderObject for ImageRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Use intrinsic image dimensions as the natural size.
        // If the widget hasn't specified explicit width/height,
        // set them from the image data so the layout engine sizes
        // the node to the image's natural dimensions.
        let mut effective_layout = self.layout.clone();

        if effective_layout.width.is_none() {
            effective_layout.width = Some(Dimension::Length(self.image_data.width as f32));
        }
        if effective_layout.height.is_none() {
            effective_layout.height = Some(Dimension::Length(self.image_data.height as f32));
        }

        let effective_layout = effective_layout.flex_shrink(0.0);

        match self.layout_node {
            Some(existing) => {
                // Incremental: update style on existing node
                ctx.engine().set_style(existing, &effective_layout);
                LayoutResult {
                    node: existing,
                    size: crate::core::Size::new(0.0, 0.0),
                }
            }
            None => {
                // First frame: create new node
                let node = ctx.engine().create_leaf(&effective_layout);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: crate::core::Size::new(0.0, 0.0),
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
        match &self.computed_bounds {
            Some(bounds) => {
                let mut commands = Vec::new();
                let pos: Position<Logical, Absolute> = ctx.absolute_position();

                let absolute_bounds = Bounds::new(
                    pos.x,
                    pos.y,
                    pos.x + bounds.width(),
                    pos.y + bounds.height(),
                );

                // 1. Push corner radius if set (affects all subsequent rects)
                if let Some(ref cr) = self.style.corner_radius {
                    commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
                }

                // 2. Draw background first (behind image)
                if let Some(bg_color) = self.style.background {
                    commands.push(RenderCommand::rect(absolute_bounds, bg_color));
                }

                // 3. Draw border on top (after background)
                if let Some(ref border) = self.style.border {
                    commands.push(RenderCommand::rect_with_border(
                        absolute_bounds,
                        Color::TRANSPARENT,
                        border.color,
                        border.width,
                    ));
                }

                // 4. Pop corner radius
                if self.style.corner_radius.is_some() {
                    commands.push(RenderCommand::PopCornerRadius);
                }

                // 5. Draw image if key is set (registered in atlas)
                if let Some(key) = self.image_key {
                    let corner_radius = self
                        .style
                        .corner_radius
                        .as_ref()
                        .map_or(0.0, |cr| cr.radius);
                    // Inset image bounds by border width so it renders inside the border ring
                    let bw = self.style.border.as_ref().map_or(0.0, |b| b.width);
                    let image_bounds = Bounds::new(
                        absolute_bounds.left + bw,
                        absolute_bounds.top + bw,
                        absolute_bounds.right - bw,
                        absolute_bounds.bottom - bw,
                    );
                    if image_bounds.width() > 0.0 && image_bounds.height() > 0.0 {
                        commands.push(RenderCommand::Image {
                            bounds: image_bounds,
                            image_key: key,
                            corner_radius: (corner_radius - bw).max(0.0),
                        });
                    }
                }

                commands
            }
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
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

    fn needs_image_registration(&self) -> Option<&ImageData> {
        if self.image_key.is_none() {
            Some(&self.image_data)
        } else {
            None
        }
    }

    fn set_image_key(&mut self, key: ImageKey) {
        self.image_key = Some(key);
    }

    fn image_key(&self) -> Option<ImageKey> {
        self.image_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, TaffyLayoutEngine};

    fn make_test_image_data() -> ImageData {
        ImageData {
            pixels: vec![255, 0, 0, 255], // 1x1 red pixel
            width: 1,
            height: 1,
        }
    }

    fn make_larger_image_data() -> ImageData {
        ImageData {
            pixels: vec![0; 4 * 100 * 50], // 100x50 transparent pixels
            width: 100,
            height: 50,
        }
    }

    #[test]
    fn test_image_render_object_new() {
        let data = make_test_image_data();
        let obj = ImageRenderObject::new(&data, Style::default(), Layout::default());
        assert!(obj.computed_bounds().is_none());
        assert!(obj.image_key.is_none());
    }

    #[test]
    fn test_image_render_object_layout_creates_node() {
        let data = make_larger_image_data();
        let mut obj = ImageRenderObject::new(&data, Style::default(), Layout::default());
        let mut engine = TaffyLayoutEngine::new();
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        let mut font_system = glyphon::FontSystem::new_with_fonts([binary]);
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);

        assert!(obj.layout_node.is_some());
        assert_eq!(obj.layout_node, Some(result.node));
    }

    #[test]
    fn test_image_render_object_hit_test_no_layout() {
        let data = make_test_image_data();
        let obj = ImageRenderObject::new(&data, Style::default(), Layout::default());

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_image_render_object_paint_no_layout() {
        let data = make_test_image_data();
        let obj = ImageRenderObject::new(&data, Style::default(), Layout::default());

        // Paint returns empty without layout (computed_bounds is None)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_image_render_object_paint_no_key() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data, Style::default(), Layout::default());
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        // No image_key set, so no Image command emitted

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        // Should have no commands (no background, no border, no image key)
        assert!(result.is_empty());
    }

    #[test]
    fn test_image_render_object_paint_with_key() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data, Style::default(), Layout::default());
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        obj.image_key = Some(42);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        // Should have exactly one Image command
        assert_eq!(result.len(), 1);
        match &result[0] {
            RenderCommand::Image { image_key, .. } => {
                assert_eq!(*image_key, 42);
            }
            _ => panic!("Expected Image command"),
        }
    }

    #[test]
    fn test_image_render_object_paint_with_style() {
        let data = make_test_image_data();
        let style = crate::Style::new().background(crate::core::Color::RED);
        let mut obj = ImageRenderObject::new(&data, style, Layout::default());
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        obj.image_key = Some(1);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        // Should have background rect + image command
        assert!(
            result.len() >= 2,
            "expected at least 2 commands, got {}",
            result.len()
        );
    }

    #[test]
    fn test_image_render_object_set_image_data_change_detection() {
        let data1 = make_test_image_data();
        let data2 = make_larger_image_data();
        let mut obj = ImageRenderObject::new(&data1, Style::default(), Layout::default());

        // Set a key first
        obj.image_key = Some(1);

        // Changing data should reset image_key
        assert!(obj.set_image_data(&data2));
        assert!(
            obj.image_key.is_none(),
            "image_key should be reset on data change"
        );

        // Same data should not report change
        let data2_dup = data2.clone();
        assert!(!obj.set_image_data(&data2_dup));
    }

    #[test]
    fn test_image_render_object_set_style_change_detection() {
        let data = make_test_image_data();
        let style1 = crate::Style::new().background(crate::core::Color::RED);
        let style2 = crate::Style::new().background(crate::core::Color::BLUE);
        let style2_dup = style2.clone();
        let mut obj = ImageRenderObject::new(&data, style1, Layout::default());
        assert!(obj.set_style(style2));
        assert!(!obj.set_style(style2_dup));
    }

    #[test]
    fn test_image_render_object_set_layout_change_detection() {
        let data = make_test_image_data();
        let layout1 = Layout::default().padding(8.0);
        let layout2 = Layout::default().padding(16.0);
        let layout2_dup = layout2.clone();
        let mut obj = ImageRenderObject::new(&data, Style::default(), layout1);
        assert!(obj.set_layout(layout2));
        assert!(!obj.set_layout(layout2_dup));
    }

    #[test]
    fn test_image_render_object_needs_image_registration() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data, Style::default(), Layout::default());

        // No key set, should need registration
        assert!(obj.needs_image_registration().is_some());

        // After setting key, should not need registration
        obj.set_image_key(42);
        assert!(obj.needs_image_registration().is_none());
    }

    #[test]
    fn test_image_render_object_clip_bounds() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data, Style::default(), Layout::default());
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        // No clip by default
        assert!(obj.clip_bounds().is_none());

        // With clip
        obj.style.clip = true;
        assert!(obj.clip_bounds().is_some());
    }

    #[test]
    fn test_image_render_object_border_insets_image() {
        let data = make_test_image_data();
        let style = crate::Style::new().border(crate::core::Color::BLUE, 3.0);
        let mut obj = ImageRenderObject::new(&data, style, Layout::default());
        obj.computed_bounds = Some(Bounds::new(0.0, 0.0, 100.0, 50.0));
        obj.image_key = Some(1);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        let image_cmd = result.iter().find_map(|c| match c {
            RenderCommand::Image { bounds, .. } => Some(*bounds),
            _ => None,
        });
        let img_bounds = image_cmd.expect("should have Image command");
        // Image bounds should be inset by border_width (3.0)
        assert_eq!(img_bounds.left, 3.0);
        assert_eq!(img_bounds.top, 3.0);
        assert_eq!(img_bounds.right, 97.0);
        assert_eq!(img_bounds.bottom, 47.0);
    }
}
