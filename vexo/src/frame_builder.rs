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
    /// Effective rounded-rect clip stack when this text was added.
    /// Empty slice means no rounded clip active. Applied as SDF masks
    /// in the text fragment shader (future; currently snapshotted but
    /// not yet enforced for text — matches existing text clip behavior).
    pub rclip_snapshot: Vec<RClipEntry>,
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

/// Maximum nesting depth for rounded-rect clips. Pushes beyond this
/// are logged and dropped. The shader uniform array is sized to this.
pub const MAX_RCLIP_DEPTH: usize = 8;

/// A single rounded-rect clip entry: (bounds, radius).
pub type RClipEntry = (Bounds, f32);

pub struct FrameBuilder {
    /// Flat ordered draw list. Each entry is the op plus its effective clip
    /// bounds at add-time. This is the single source of truth for geometry
    /// draw order — quads and images interleave in the order the Painter
    /// emitted them.
    ops: Vec<(DrawOp, Option<Bounds>, Vec<RClipEntry>)>,

    /// Text requests in paint order. Each carries its own `clip_bounds`.
    text_requests: Vec<TextRequest>,

    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
    rclip_stack: Vec<RClipEntry>,
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
            rclip_stack: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: AffineTransform::identity(),
        }
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.text_requests.clear();
        self.corner_radius_stack.clear();
        self.clip_stack.clear();
        self.rclip_stack.clear();
        self.transform_stack.clear();
        self.current_transform = AffineTransform::identity();
    }

    pub fn quad_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _, _)| matches!(op, DrawOp::Quad(_)))
            .count()
    }

    pub fn has_quads(&self) -> bool {
        self.ops
            .iter()
            .any(|(op, _, _)| matches!(op, DrawOp::Quad(_)))
    }

    pub fn text_count(&self) -> usize {
        self.text_requests.len()
    }

    /// Get all quad instances in paint order (for testing/compat).
    pub fn quad_instances(&self) -> Vec<QuadInstance> {
        self.ops
            .iter()
            .filter_map(|(op, _, _)| match op {
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
    ///
    /// Returns the intersection of all clips on the stack:
    /// - `None` when no clip is set (clip_stack empty) → full viewport.
    /// - `Some(bounds)` when clips overlap → the intersection region.
    /// - `Some(degenerate)` when clips don't overlap → fully clipped.
    ///
    /// The degenerate case (zero-area bounds) is distinct from `None`: the
    /// GPU backend interprets `None` as "no clip = full viewport scissor",
    /// so returning `None` for a non-overlapping stack would make fully-
    /// clipped content (e.g. an avatar scrolled under the nav bar) draw
    /// without any clipping. A degenerate bounds makes the GPU backend's
    /// `w == 0 || h == 0` check skip the op instead.
    pub fn current_clip(&self) -> Option<Bounds> {
        if self.clip_stack.is_empty() {
            return None;
        }
        let mut result = self.clip_stack[0];
        for bounds in &self.clip_stack[1..] {
            result = match result.intersect(bounds) {
                Some(i) => i,
                None => {
                    return Some(Bounds::ZERO);
                }
            };
        }
        Some(result)
    }

    /// Push a rounded-rect clip onto the rclip stack.
    /// All subsequent DrawOps snapshot this stack until `pop_rclip`.
    /// Silently drops the push (with a warning log) if `MAX_RCLIP_DEPTH`
    /// is exceeded — the shader uniform array cannot hold more.
    pub fn push_rclip(&mut self, bounds: Bounds, radius: f32) {
        if self.rclip_stack.len() >= MAX_RCLIP_DEPTH {
            log::warn!(
                "[ClipRRect] max depth {} exceeded, dropping rclip push",
                MAX_RCLIP_DEPTH
            );
            return;
        }
        self.rclip_stack.push((bounds, radius));
    }

    /// Pop the most recent rounded-rect clip from the stack.
    pub fn pop_rclip(&mut self) {
        self.rclip_stack.pop();
    }

    /// Get the current active rclip stack as a slice.
    /// Empty slice means no rounded-rect clip is active.
    pub fn current_rclip(&self) -> &[RClipEntry] {
        &self.rclip_stack
    }

    /// Snapshot the current rclip stack for attaching to a DrawOp.
    fn snapshot_rclip(&self) -> Vec<RClipEntry> {
        if self.rclip_stack.is_empty() {
            Vec::new()
        } else {
            self.rclip_stack.clone()
        }
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
            _padding: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            _padding2: [0.0; 3],
        };

        let clip = self.current_clip();
        self.ops
            .push((DrawOp::Quad(instance), clip, self.snapshot_rclip()));
    }

    pub fn add_shadow_rect(
        &mut self,
        bounds: Bounds,
        fill: impl Into<Color>,
        stroke: Option<Stroke>,
        corner_radius: f32,
        shadow_color: [f32; 4],
        shadow_blur: f32,
    ) {
        let fill: Color = fill.into();
        let (border_color, border_width) = stroke
            .map(|s| (s.color, s.width))
            .unwrap_or((Color::TRANSPARENT, 0.0));

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
            _padding: [0.0; 4],
            shadow_color,
            shadow_blur,
            _padding2: [0.0; 3],
        };

        let clip = self.current_clip();
        self.ops
            .push((DrawOp::Quad(instance), clip, self.snapshot_rclip()));
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
            rclip_snapshot: self.snapshot_rclip(),
        });
    }

    pub fn add_image(&mut self, request: ImageRequest) {
        let clip = self.current_clip();
        self.ops
            .push((DrawOp::Image(request), clip, self.snapshot_rclip()));
    }

    pub fn image_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _, _)| matches!(op, DrawOp::Image(_)))
            .count()
    }

    /// All geometry ops in paint order, each with its clip bounds.
    pub fn ops(&self) -> &[(DrawOp, Option<Bounds>, Vec<RClipEntry>)] {
        &self.ops
    }

    /// Image requests filtered from `ops`, in paint order.
    pub fn image_requests(&self) -> Vec<ImageRequest> {
        self.ops
            .iter()
            .filter_map(|(op, _, _)| match op {
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
            .map(|(op, _, _)| match op {
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
    fn test_non_overlapping_clips_produce_degenerate_not_none() {
        // Regression: when two nested clips don't overlap, current_clip()
        // must return Some(degenerate_bounds), NOT None. Returning None
        // means "no clip" (full viewport) to the GPU backend, which causes
        // fully-clipped content (e.g. an avatar scrolled under the nav bar)
        // to be drawn without clipping.
        //
        // The GPU backend skips ops with zero-width/height scissor rects
        // (wgpu_backend.rs execute_render_pass), so a degenerate bounds
        // correctly means "don't draw".
        let mut fb = FrameBuilder::new();
        let outer = Bounds::from_xywh(0.0, 45.0, 800.0, 505.0); // nav content area
        let inner = Bounds::from_xywh(12.0, 569.0, 40.0, 40.0); // avatar below tab bar
        fb.push_clip(outer);
        fb.push_clip(inner);
        let clip = fb.current_clip();
        // Must NOT be None — None means "no clip = full viewport".
        assert!(
            clip.is_some(),
            "non-overlapping clips must return Some(degenerate), not None"
        );
        let c = clip.unwrap();
        assert_eq!(
            c,
            Bounds::ZERO,
            "degenerate clip must equal Bounds::ZERO, got {:?}",
            c
        );
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

    #[test]
    fn test_add_shadow_rect_populates_shadow_fields() {
        let mut fb = FrameBuilder::new();
        let shadow_color = [0.0, 0.0, 0.0, 0.5];
        fb.add_shadow_rect(
            Bounds::from_xywh(10.0, 20.0, 100.0, 50.0),
            Color::TRANSPARENT,
            None,
            8.0,
            shadow_color,
            12.0,
        );

        assert_eq!(fb.quad_count(), 1);
        let quads = fb.quad_instances();
        assert_eq!(quads[0].shadow_color, shadow_color);
        assert_eq!(quads[0].shadow_blur, 12.0);
        assert_eq!(quads[0].corner_radius, 8.0);
    }

    #[test]
    fn test_add_shadow_rect_preserves_paint_order() {
        let mut fb = FrameBuilder::new();
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::RED,
            None,
            0.0,
        );
        fb.add_shadow_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::TRANSPARENT,
            None,
            0.0,
            [0.0, 0.0, 0.0, 0.5],
            8.0,
        );
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 20.0, 20.0),
            Color::BLUE,
            None,
            0.0,
        );

        let ops = fb.ops();
        assert_eq!(ops.len(), 3);
        assert!(matches!(ops[0].0, DrawOp::Quad(_)));
        assert!(matches!(ops[1].0, DrawOp::Quad(_)));
        assert!(matches!(ops[2].0, DrawOp::Quad(_)));
    }

    #[test]
    fn test_add_shadow_rect_respects_transform_stack() {
        let mut fb = FrameBuilder::new();
        let transform = AffineTransform::translation(5.0, 10.0);
        fb.push_transform(transform);
        fb.add_shadow_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::TRANSPARENT,
            None,
            0.0,
            [0.0, 0.0, 0.0, 0.5],
            4.0,
        );
        fb.pop_transform();

        let quads = fb.quad_instances();
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].transform, transform.to_array());
        assert_eq!(quads[0].shadow_blur, 4.0);
    }

    #[test]
    fn test_rclip_stack_push_pop() {
        let mut fb = FrameBuilder::new();
        assert!(fb.current_rclip().is_empty());

        let b1 = Bounds::new(0.0, 0.0, 100.0, 100.0);
        fb.push_rclip(b1, 8.0);
        assert_eq!(fb.current_rclip().len(), 1);
        assert_eq!(fb.current_rclip()[0], (b1, 8.0));

        fb.pop_rclip();
        assert!(fb.current_rclip().is_empty());
    }

    #[test]
    fn test_rclip_stack_depth_cap() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 10.0, 10.0);
        for _ in 0..MAX_RCLIP_DEPTH {
            fb.push_rclip(b, 4.0);
        }
        assert_eq!(fb.current_rclip().len(), MAX_RCLIP_DEPTH);

        // Pushing beyond the cap should log and drop, not panic.
        fb.push_rclip(b, 4.0);
        assert_eq!(
            fb.current_rclip().len(),
            MAX_RCLIP_DEPTH,
            "depth cap must silently drop excess pushes"
        );

        // Pop returns to MAX-1.
        fb.pop_rclip();
        assert_eq!(fb.current_rclip().len(), MAX_RCLIP_DEPTH - 1);
    }

    #[test]
    fn test_rclip_snapshot_on_add_rect() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 50.0, 50.0);

        // Op before rclip: empty snapshot.
        fb.add_rect(b, Color::RED, None, 0.0);
        assert!(fb.ops()[0].2.is_empty());

        // Op inside rclip: snapshot has one entry.
        fb.push_rclip(b, 8.0);
        fb.add_rect(b, Color::RED, None, 0.0);
        assert_eq!(fb.ops()[1].2.len(), 1);
        fb.pop_rclip();

        // Op after rclip: empty snapshot again.
        fb.add_rect(b, Color::RED, None, 0.0);
        assert!(fb.ops()[2].2.is_empty());
    }

    #[test]
    fn test_rclip_snapshot_on_add_image() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 50.0, 50.0);

        fb.push_rclip(b, 12.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [50.0, 50.0],
            image_key: 1,
            corner_radius: 0.0,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
        });
        fb.pop_rclip();

        assert_eq!(fb.ops()[0].2.len(), 1);
        assert_eq!(fb.ops()[0].2[0], (b, 12.0));
    }
}
