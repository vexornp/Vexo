use crate::core::{Absolute, Bounds, Logical, Point, Position};
use crate::image_atlas::ImageKey;
use crate::image_data::ImageData;
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
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
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl ImageRenderObject {
    pub fn new(image_data: &ImageData) -> Self {
        Self {
            image_data: image_data.clone(),
            image_key: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

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
}

impl RenderObject for ImageRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Image fills its parent container rather than rendering at intrinsic
        // dimensions. This matches the "Everything is a widget" philosophy:
        // sizing is the parent's responsibility (via WithLayout), and the
        // Image stretches to fill the space it's given.
        //
        // `flex_grow(1.0)` fills the parent's main-axis; the parent's
        // `align_items: Stretch` (WithLayout's default) fills the cross-axis.
        // A standalone Image with no sized parent renders at 0×0 — wrap it
        // in a `WithLayout` with explicit dimensions to display it.
        let effective_layout = Layout::default().flex_grow(1.0).flex_shrink(1.0);

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &effective_layout);
                LayoutResult {
                    node: existing,
                    size: crate::core::Size::new(0.0, 0.0),
                }
            }
            None => {
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

                if let Some(key) = self.image_key {
                    if absolute_bounds.width() > 0.0 && absolute_bounds.height() > 0.0 {
                        commands.push(RenderCommand::Image {
                            bounds: absolute_bounds,
                            image_key: key,
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
            pixels: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        }
    }

    fn make_larger_image_data() -> ImageData {
        ImageData {
            pixels: vec![0; 4 * 100 * 50],
            width: 100,
            height: 50,
        }
    }

    #[test]
    fn test_image_render_object_new() {
        let data = make_test_image_data();
        let obj = ImageRenderObject::new(&data);
        assert!(obj.computed_bounds().is_none());
        assert!(obj.image_key.is_none());
    }

    #[test]
    fn test_image_render_object_layout_creates_node() {
        let data = make_larger_image_data();
        let mut obj = ImageRenderObject::new(&data);
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
        let obj = ImageRenderObject::new(&data);

        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_image_render_object_paint_no_layout() {
        let data = make_test_image_data();
        let obj = ImageRenderObject::new(&data);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_image_render_object_paint_no_key() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_image_render_object_paint_with_key() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data);
        obj.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        obj.image_key = Some(42);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert_eq!(result.len(), 1);
        match &result[0] {
            RenderCommand::Image { image_key, .. } => {
                assert_eq!(*image_key, 42);
            }
            _ => panic!("Expected Image command"),
        }
    }

    #[test]
    fn test_image_render_object_set_image_data_change_detection() {
        let data1 = make_test_image_data();
        let data2 = make_larger_image_data();
        let mut obj = ImageRenderObject::new(&data1);

        obj.image_key = Some(1);

        assert!(obj.set_image_data(&data2));
        assert!(
            obj.image_key.is_none(),
            "image_key should be reset on data change"
        );

        let data2_dup = data2.clone();
        assert!(!obj.set_image_data(&data2_dup));
    }

    #[test]
    fn test_image_render_object_needs_image_registration() {
        let data = make_test_image_data();
        let mut obj = ImageRenderObject::new(&data);

        assert!(obj.needs_image_registration().is_some());

        obj.set_image_key(42);
        assert!(obj.needs_image_registration().is_none());
    }
}
