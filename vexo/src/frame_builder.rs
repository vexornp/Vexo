use crate::core::{AffineTransform, Color, Logical, Point, Stroke};
use crate::image_atlas::ImageKey;
use crate::quad_instance::QuadInstance;

/// A single drawable geometry primitive in paint order.
#[derive(Debug, Clone)]
pub enum DrawOp {
    Quad(QuadInstance),
    Image(ImageRequest),
}

/// Where an op landed in the typed instance buffer, for draw iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpLocation {
    Quad { index: u32 },
    Image { index: u32 },
}

impl OpLocation {
    pub fn kind(&self) -> OpKind {
        match self {
            OpLocation::Quad { .. } => OpKind::Quad,
            OpLocation::Image { .. } => OpKind::Image,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Quad,
    Image,
}

#[derive(Clone)]
pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    /// Optional font family name. When set, the text is shaped against this
    /// family (e.g. an icon font); when `None`, the framework default is used.
    pub font_family: Option<String>,
    /// Maximum width for text wrapping. If None, no wrapping.
    pub max_width: Option<f32>,
    /// Effective clip bounds when this text was added (logical coordinates).
    /// `None` means no clipping (full viewport).
    pub clip_bounds: Option<Bounds>,
}

#[derive(Clone, Debug)]
pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub corner_radius: f32,
    pub transform: [f32; 6],
    pub opacity: f32,
}

pub type Bounds = crate::core::Bounds<Logical>;

pub struct FrameBuilder {
    /// Flat ordered draw list. Each entry is the op plus its effective clip
    /// bounds at add-time. This is the single source of truth for geometry
    /// draw order — quads and images interleave in the order the Painter
    /// emitted them.
    ops: Vec<(DrawOp, Option<Bounds>)>,

    /// Text requests in paint order. Each carries its own `clip_bounds`.
    text_requests: Vec<TextRequest>,

    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
    transform_stack: Vec<AffineTransform>,
    current_transform: AffineTransform,
}

impl Default for FrameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameBuilder {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            text_requests: Vec::new(),
            corner_radius_stack: Vec::new(),
            clip_stack: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: AffineTransform::identity(),
        }
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.text_requests.clear();
        self.corner_radius_stack.clear();
        self.clip_stack.clear();
        self.transform_stack.clear();
        self.current_transform = AffineTransform::identity();
    }

    pub fn quad_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _)| matches!(op, DrawOp::Quad(_)))
            .count()
    }

    pub fn has_quads(&self) -> bool {
        self.ops.iter().any(|(op, _)| matches!(op, DrawOp::Quad(_)))
    }

    pub fn text_count(&self) -> usize {
        self.text_requests.len()
    }

    /// Get all quad instances in paint order (for testing/compat).
    pub fn quad_instances(&self) -> Vec<QuadInstance> {
        self.ops
            .iter()
            .filter_map(|(op, _)| match op {
                DrawOp::Quad(q) => Some(*q),
                _ => None,
            })
            .collect()
    }

    /// Get all text requests in paint order.
    pub fn text_requests(&self) -> &[TextRequest] {
        &self.text_requests
    }

    /// Push a corner radius onto the context stack.
    /// Used by CornerRadius modifier to set radius for child widgets.
    pub fn push_corner_radius(&mut self, radius: f32) {
        self.corner_radius_stack.push(radius);
    }

    /// Pop the corner radius from the context stack.
    /// Called after drawing children to restore previous context.
    pub fn pop_corner_radius(&mut self) {
        self.corner_radius_stack.pop();
    }

    /// Get the current corner radius from the context stack.
    /// Returns 0.0 if no radius is set.
    pub fn current_corner_radius(&self) -> f32 {
        self.corner_radius_stack.last().copied().unwrap_or(0.0)
    }

    /// Push a clipping region onto the stack.
    /// All subsequent commands should be clipped to this region.
    pub fn push_clip(&mut self, bounds: Bounds) {
        self.clip_stack.push(bounds);
    }

    /// Pop the most recent clipping region from the stack.
    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Get the current effective clipping region.
    /// Returns the intersection of all clips on the stack, or None if no clip is set
    /// (or if nested clips have no overlap, meaning content is fully clipped).
    pub fn current_clip(&self) -> Option<Bounds> {
        if self.clip_stack.is_empty() {
            return None;
        }
        let mut result = self.clip_stack[0];
        for bounds in &self.clip_stack[1..] {
            result = match result.intersect(bounds) {
                Some(i) => i,
                None => return None, // Empty intersection = fully clipped
            };
        }
        Some(result)
    }

    /// Push a transform onto the context stack.
    pub fn push_transform(&mut self, transform: AffineTransform) {
        self.transform_stack.push(self.current_transform);
        self.current_transform = self.current_transform * transform;
    }

    /// Pop the transform from the context stack.
    pub fn pop_transform(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.current_transform = prev;
        }
    }

    /// Get the current transform.
    pub fn current_transform(&self) -> AffineTransform {
        self.current_transform
    }

    pub fn add_rect(
        &mut self,
        bounds: Bounds,
        fill: impl Into<Color>,
        stroke: Option<Stroke>,
        corner_radius: f32,
    ) {
        let fill: Color = fill.into();
        let (border_color, border_width) = stroke
            .map(|s| (s.color, s.width))
            .unwrap_or((Color::TRANSPARENT, 0.0));

        // Use explicit radius if > 0, otherwise use context
        let radius = if corner_radius > 0.0 {
            corner_radius
        } else {
            self.current_corner_radius()
        };

        let instance = QuadInstance {
            position: [bounds.left, bounds.top],
            size: [bounds.width(), bounds.height()],
            color: fill.to_array(),
            border_color: border_color.to_array(),
            border_width,
            corner_radius: radius,
            transform: self.current_transform.to_array(),
            _padding: [0.0; 2],
        };

        let clip = self.current_clip();
        self.ops.push((DrawOp::Quad(instance), clip));
    }

    pub fn add_text(
        &mut self,
        content: impl Into<String>,
        position: Point<Logical>,
        size: f32,
        color: impl Into<Color>,
        font_family: Option<String>,
        max_width: Option<f32>,
    ) {
        let color: Color = color.into();
        self.text_requests.push(TextRequest {
            content: content.into(),
            position,
            size,
            color,
            font_family,
            max_width,
            clip_bounds: self.current_clip(),
        });
    }

    pub fn add_image(&mut self, request: ImageRequest) {
        let clip = self.current_clip();
        self.ops.push((DrawOp::Image(request), clip));
    }

    pub fn image_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _)| matches!(op, DrawOp::Image(_)))
            .count()
    }

    /// All geometry ops in paint order, each with its clip bounds.
    pub fn ops(&self) -> &[(DrawOp, Option<Bounds>)] {
        &self.ops
    }

    /// Image requests filtered from `ops`, in paint order.
    pub fn image_requests(&self) -> Vec<ImageRequest> {
        self.ops
            .iter()
            .filter_map(|(op, _)| match op {
                DrawOp::Image(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    /// Compute typed-buffer locations for each op in paint order.
    /// Pure function — no GPU access. Used by upload and unit-tested directly.
    pub fn compute_op_locations(&self) -> Vec<OpLocation> {
        let mut quad_idx = 0u32;
        let mut image_idx = 0u32;
        self.ops
            .iter()
            .map(|(op, _)| match op {
                DrawOp::Quad(_) => {
                    let i = quad_idx;
                    quad_idx += 1;
                    OpLocation::Quad { index: i }
                }
                DrawOp::Image(_) => {
                    let i = image_idx;
                    image_idx += 1;
                    OpLocation::Image { index: i }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AffineTransform;

    #[test]
    fn test_add_image_request() {
        let mut fb = FrameBuilder::new();
        fb.add_image(ImageRequest {
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            image_key: 1,
            corner_radius: 8.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });

        assert_eq!(fb.image_count(), 1);
    }

    #[test]
    fn test_image_request_opacity() {
        let req = ImageRequest {
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            image_key: 1,
            corner_radius: 8.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 0.5,
        };
        assert_eq!(req.opacity, 0.5);
    }

    #[test]
    fn test_flatten_image_requests() {
        let mut fb = FrameBuilder::new();
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [50.0, 50.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });

        fb.push_clip(Bounds::new(0.0, 0.0, 100.0, 100.0));
        fb.add_image(ImageRequest {
            position: [10.0, 10.0],
            size: [30.0, 30.0],
            image_key: 2,
            corner_radius: 4.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.pop_clip();

        let requests = fb.image_requests();
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn test_ops_preserve_paint_order() {
        let mut fb = FrameBuilder::new();
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::RED,
            None,
            0.0,
        );
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [10.0, 10.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::BLUE,
            None,
            0.0,
        );

        let ops = fb.ops();
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0].0, DrawOp::Quad(_)));
        assert!(matches!(ops[1].0, DrawOp::Image(_)));
        assert!(matches!(ops[2].0, DrawOp::Quad(_)));
    }

    #[test]
    fn test_op_carries_clip_bounds() {
        let mut fb = FrameBuilder::new();
        let clip = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
        fb.push_clip(clip);
        fb.add_rect(
            Bounds::from_xywh(10.0, 10.0, 10.0, 10.0),
            Color::RED,
            None,
            0.0,
        );
        fb.add_image(ImageRequest {
            position: [10.0, 10.0],
            size: [10.0, 10.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.pop_clip();
        fb.add_rect(
            Bounds::from_xywh(20.0, 20.0, 10.0, 10.0),
            Color::BLUE,
            None,
            0.0,
        );

        let ops = fb.ops();
        assert_eq!(ops[0].1, Some(clip));
        assert_eq!(ops[1].1, Some(clip));
        assert_eq!(ops[2].1, None);
    }

    #[test]
    fn test_text_request_carries_clip_bounds() {
        let mut fb = FrameBuilder::new();
        let clip = Bounds::from_xywh(0.0, 0.0, 100.0, 100.0);
        fb.push_clip(clip);
        fb.add_text(
            "inside".to_string(),
            Point::new(0.0, 0.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );
        fb.pop_clip();
        fb.add_text(
            "outside".to_string(),
            Point::new(0.0, 0.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );

        let reqs = fb.text_requests();
        assert_eq!(reqs[0].clip_bounds, Some(clip));
        assert_eq!(reqs[1].clip_bounds, None);
    }

    #[test]
    fn test_quad_instances_flatten_preserves_order() {
        let mut fb = FrameBuilder::new();
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 2.0, 2.0),
            Color::BLUE,
            None,
            0.0,
        );

        let quads = fb.quad_instances();
        assert_eq!(quads.len(), 2);
        assert_eq!(quads[0].size, [1.0, 1.0]);
        assert_eq!(quads[1].size, [2.0, 2.0]);
    }

    #[test]
    fn test_image_requests_preserve_order() {
        let mut fb = FrameBuilder::new();
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            image_key: 10,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [2.0, 2.0],
            image_key: 20,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });

        let imgs = fb.image_requests();
        assert_eq!(imgs.len(), 2);
        assert_eq!(imgs[0].image_key, 10);
        assert_eq!(imgs[1].image_key, 20);
    }

    #[test]
    fn test_compute_op_locations_indices() {
        let mut fb = FrameBuilder::new();
        // Sequence: quad, image, quad, quad, image
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            image_key: 2,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });

        let locs = fb.compute_op_locations();
        assert_eq!(locs.len(), 5);
        assert_eq!(locs[0], OpLocation::Quad { index: 0 });
        assert_eq!(locs[1], OpLocation::Image { index: 0 });
        assert_eq!(locs[2], OpLocation::Quad { index: 1 });
        assert_eq!(locs[3], OpLocation::Quad { index: 2 });
        assert_eq!(locs[4], OpLocation::Image { index: 1 });
    }
}
