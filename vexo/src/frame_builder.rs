use crate::core::{AffineTransform, Color, Logical, Point, Stroke};
use crate::image_atlas::ImageKey;
use crate::quad_instance::QuadInstance;

/// A single drawable geometry primitive in paint order.
#[derive(Debug, Clone)]
pub enum DrawOp {
    Quad(QuadInstance),
    Image(ImageRequest),
    /// Begin a save-layer group. Ops between Begin/End are rendered
    /// into an offscreen texture and composited as a unit at `opacity`.
    BeginSaveLayer {
        bounds: Bounds,
        opacity: f32,
        z: f32,
    },
    /// End the most recent save-layer group.
    EndSaveLayer,
}

/// Where an op landed in the typed instance buffer, for draw iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpLocation {
    Quad {
        index: u32,
    },
    /// Transparent quad (fill alpha < 1.0) — rendered with depth-write disabled
    /// so it doesn't occlude text rendered in the later text pass.
    TransparentQuad {
        index: u32,
    },
    Image {
        index: u32,
    },
    /// SaveLayer marker (Begin/End) — not drawn directly. The backend
    /// scans for these to delimit offscreen render groups.
    SaveLayerMarker,
}

impl OpLocation {
    pub fn kind(&self) -> OpKind {
        match self {
            OpLocation::Quad { .. } => OpKind::Quad,
            OpLocation::TransparentQuad { .. } => OpKind::TransparentQuad,
            OpLocation::Image { .. } => OpKind::Image,
            OpLocation::SaveLayerMarker => OpKind::SaveLayerMarker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Quad,
    TransparentQuad,
    Image,
    SaveLayerMarker,
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
    /// Depth value for GPU depth testing. Smaller = closer to camera (on top).
    /// Assigned by FrameBuilder in paint order.
    pub z: f32,
}

#[derive(Clone, Debug)]
pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub transform: [f32; 6],
    pub opacity: f32,
    /// Depth value for GPU depth testing. Smaller = closer to camera (on top).
    /// Assigned by FrameBuilder in paint order.
    pub z: f32,
}

pub type Bounds = crate::core::Bounds<Logical>;

/// Maximum nesting depth for rounded-rect clips. Pushes beyond this
/// are logged and dropped. The shader uniform array is sized to this.
pub const MAX_RCLIP_DEPTH: usize = 8;

/// A single rounded-rect clip entry: (bounds, radius).
pub type RClipEntry = (Bounds, f32);

/// In-flight save-layer group state (while between begin/end).
struct SaveLayerFrame {
    bounds: Bounds,
    opacity: f32,
    z: f32,
    text_start: usize,
}

/// A completed save-layer group, ready for backend consumption.
#[derive(Clone)]
pub struct SaveLayerGroup {
    /// The group's bounds in window-absolute logical coords.
    pub bounds: Bounds,
    /// The opacity to apply at composite time.
    pub opacity: f32,
    /// Z-depth for the composite quad (paint-order position).
    pub z: f32,
    /// Text requests belonging to this group.
    pub text_requests: Vec<TextRequest>,
}

pub struct FrameBuilder {
    /// Flat ordered draw list. Each entry is the op plus its effective clip
    /// bounds at add-time. This is the single source of truth for geometry
    /// draw order — quads and images interleave in the order the Painter
    /// emitted them.
    ops: Vec<(DrawOp, Option<Bounds>, Vec<RClipEntry>)>,

    /// Text requests in paint order. Each carries its own `clip_bounds`.
    text_requests: Vec<TextRequest>,

    /// Monotonic counter for z-depth assignment. Incremented on every
    /// add_rect/add_shadow_rect/add_text/add_image call. Earlier paints
    /// get larger z (farther); later paints get smaller z (on top).
    paint_index: u32,

    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
    rclip_stack: Vec<RClipEntry>,
    transform_stack: Vec<AffineTransform>,
    current_transform: AffineTransform,

    /// Stack of active save-layer groups (innermost last).
    save_layer_stack: Vec<SaveLayerFrame>,
    /// Completed save-layer groups (collected at end_save_layer).
    save_layer_groups: Vec<SaveLayerGroup>,
    /// Text requests for currently-active save-layer groups.
    /// Drained into each group's `text_requests` at `end_save_layer`.
    group_text_requests: Vec<TextRequest>,
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
            paint_index: 0,
            corner_radius_stack: Vec::new(),
            clip_stack: Vec::new(),
            rclip_stack: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: AffineTransform::identity(),
            save_layer_stack: Vec::new(),
            save_layer_groups: Vec::new(),
            group_text_requests: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.text_requests.clear();
        self.paint_index = 0;
        self.corner_radius_stack.clear();
        self.clip_stack.clear();
        self.rclip_stack.clear();
        self.transform_stack.clear();
        self.current_transform = AffineTransform::identity();
        self.save_layer_stack.clear();
        self.save_layer_groups.clear();
        self.group_text_requests.clear();
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

    /// Compute a depth value from the current paint index and advance it.
    /// Earlier paints get larger z (farther from camera); later paints get
    /// smaller z (closer, drawn on top). Range: (0.0, 1.0].
    /// At 65536+ paints, z saturates near 0.0 (graceful degradation).
    fn next_z(&mut self) -> f32 {
        let z = 1.0 - self.paint_index as f32 / 65536.0;
        self.paint_index = self.paint_index.wrapping_add(1);
        z
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

        let z = self.next_z();
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
            z,
            _padding2: [0.0; 2],
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

        let z = self.next_z();
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
            z,
            _padding2: [0.0; 2],
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
        let z = self.next_z();
        let request = TextRequest {
            content: content.into(),
            position,
            size,
            color,
            font_family,
            max_width,
            clip_bounds: self.current_clip(),
            rclip_snapshot: self.snapshot_rclip(),
            z,
        };
        if self.save_layer_stack.is_empty() {
            self.text_requests.push(request);
        } else {
            self.group_text_requests.push(request);
        }
    }

    pub fn add_image(&mut self, mut request: ImageRequest) {
        request.z = self.next_z();
        let clip = self.current_clip();
        self.ops
            .push((DrawOp::Image(request), clip, self.snapshot_rclip()));
    }

    /// Begin a save-layer group. Ops added between `begin_save_layer`
    /// and `end_save_layer` are rendered into an offscreen texture and
    /// composited as a unit at `opacity`. The `bounds` determine the
    /// offscreen texture size and the composite quad's position.
    ///
    /// Text requests added while a save-layer group is active are routed
    /// to the group's text list (see `save_layer_groups`), not the
    /// main-pass text list.
    pub fn begin_save_layer(&mut self, bounds: Bounds, opacity: f32) {
        let z = self.next_z();
        let marker = DrawOp::BeginSaveLayer { bounds, opacity, z };
        self.ops.push((marker, None, Vec::new()));
        self.save_layer_stack.push(SaveLayerFrame {
            bounds,
            opacity,
            z,
            text_start: self.group_text_requests.len(),
        });
    }

    /// End the most recent save-layer group.
    pub fn end_save_layer(&mut self) {
        self.ops.push((DrawOp::EndSaveLayer, None, Vec::new()));
        if let Some(frame) = self.save_layer_stack.pop() {
            let text_end = self.group_text_requests.len();
            let group_texts: Vec<TextRequest> = self
                .group_text_requests
                .drain(frame.text_start..text_end)
                .collect();
            self.save_layer_groups.push(SaveLayerGroup {
                bounds: frame.bounds,
                opacity: frame.opacity,
                z: frame.z,
                text_requests: group_texts,
            });
        }
    }

    /// Completed save-layer groups, in paint order. Each group's ops
    /// are delimited by BeginSaveLayer/EndSaveLayer markers in `ops()`.
    /// The backend uses this to render groups offscreen.
    pub fn save_layer_groups(&self) -> &[SaveLayerGroup] {
        &self.save_layer_groups
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
                DrawOp::Quad(q) => {
                    let i = quad_idx;
                    quad_idx += 1;
                    if q.color[3] < 1.0 {
                        OpLocation::TransparentQuad { index: i }
                    } else {
                        OpLocation::Quad { index: i }
                    }
                }
                DrawOp::Image(_) => {
                    let i = image_idx;
                    image_idx += 1;
                    OpLocation::Image { index: i }
                }
                DrawOp::BeginSaveLayer { .. } | DrawOp::EndSaveLayer => OpLocation::SaveLayerMarker,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
        });

        assert_eq!(fb.image_count(), 1);
    }

    #[test]
    fn test_image_request_opacity() {
        let req = ImageRequest {
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            image_key: 1,
            transform: AffineTransform::identity().to_array(),
            opacity: 0.5,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
        });

        fb.push_clip(Bounds::new(0.0, 0.0, 100.0, 100.0));
        fb.add_image(ImageRequest {
            position: [10.0, 10.0],
            size: [30.0, 30.0],
            image_key: 2,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
        });
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [2.0, 2.0],
            image_key: 20,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
        });
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_rect(Bounds::from_xywh(0.0, 0.0, 1.0, 1.0), Color::RED, None, 0.0);
        fb.add_image(ImageRequest {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            image_key: 2,
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
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
            transform: AffineTransform::identity().to_array(),
            opacity: 1.0,
            z: 0.0,
        });
        fb.pop_rclip();

        assert_eq!(fb.ops()[0].2.len(), 1);
        assert_eq!(fb.ops()[0].2[0], (b, 12.0));
    }

    #[test]
    fn test_paint_index_assigns_decreasing_z() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 10.0, 10.0);

        fb.add_rect(b, Color::RED, None, 0.0);
        fb.add_text(
            "hello",
            Point::<Logical>::new(0.0, 0.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );
        fb.add_rect(b, Color::BLUE, None, 0.0);

        let rect0_z = match &fb.ops()[0].0 {
            DrawOp::Quad(q) => q.z,
            _ => unreachable!(),
        };
        let text_z = fb.text_requests()[0].z;
        let rect1_z = match &fb.ops()[1].0 {
            DrawOp::Quad(q) => q.z,
            _ => unreachable!(),
        };

        // z must strictly decrease in paint order (earlier = farther, later = closer).
        assert!(
            rect0_z > text_z,
            "first rect z {} should be > text z {}",
            rect0_z,
            text_z
        );
        assert!(
            text_z > rect1_z,
            "text z {} should be > second rect z {}",
            text_z,
            rect1_z
        );
    }

    #[test]
    fn test_z_values_in_valid_range() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 10.0, 10.0);

        fb.add_rect(b, Color::RED, None, 0.0);

        let z = match &fb.ops()[0].0 {
            DrawOp::Quad(q) => q.z,
            _ => unreachable!(),
        };

        // First paint should have z close to 1.0 (farthest), within (0, 1].
        assert!(z > 0.0 && z <= 1.0, "z {} should be in (0, 1]", z);
        assert!(
            (z - 1.0).abs() < 0.001,
            "first paint z {} should be ~1.0",
            z
        );
    }

    #[test]
    fn test_clear_resets_paint_index() {
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 10.0, 10.0);

        fb.add_rect(b, Color::RED, None, 0.0);
        fb.add_rect(b, Color::RED, None, 0.0);
        fb.clear();

        // After clear, first paint should again get z ~1.0.
        fb.add_rect(b, Color::RED, None, 0.0);
        let z = match &fb.ops()[0].0 {
            DrawOp::Quad(q) => q.z,
            _ => unreachable!(),
        };
        assert!(
            (z - 1.0).abs() < 0.001,
            "z after clear {} should be ~1.0",
            z
        );
    }

    #[test]
    fn test_text_occluded_by_later_geometry_z() {
        // Simulates the push-animation bug case:
        // outgoing-page text (paint index 0) vs incoming-page geometry (paint index 1).
        // Text z must be LARGER than geometry z so the GPU depth test
        // (LessEqual) rejects the text pixels behind the geometry.
        let mut fb = FrameBuilder::new();
        let b = Bounds::new(0.0, 0.0, 100.0, 100.0);

        // Outgoing page: text painted first (larger z = farther).
        fb.add_text(
            "outgoing",
            Point::<Logical>::new(0.0, 0.0),
            16.0,
            Color::BLACK,
            None,
            None,
        );

        // Incoming page: solid background painted second (smaller z = closer).
        fb.add_rect(b, Color::WHITE, None, 0.0);

        let text_z = fb.text_requests()[0].z;
        let geom_z = match &fb.ops()[0].0 {
            DrawOp::Quad(q) => q.z,
            _ => unreachable!(),
        };

        assert!(
            text_z > geom_z,
            "outgoing text z {} must be > incoming geometry z {} for depth occlusion",
            text_z,
            geom_z
        );
    }

    #[test]
    fn test_save_layer_markers_in_ops() {
        let mut fb = FrameBuilder::new();
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
            None,
            0.0,
        );
        fb.begin_save_layer(Bounds::from_xywh(10.0, 20.0, 200.0, 100.0), 0.85);
        fb.add_rect(
            Bounds::from_xywh(10.0, 20.0, 50.0, 50.0),
            Color::BLUE,
            None,
            0.0,
        );
        fb.end_save_layer();
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 10.0, 10.0),
            Color::GREEN,
            None,
            0.0,
        );

        let ops = fb.ops();
        assert!(matches!(ops[0].0, DrawOp::Quad(_)));
        assert!(matches!(
            &ops[1].0,
            DrawOp::BeginSaveLayer { opacity, .. } if (*opacity - 0.85).abs() < 1e-6
        ));
        assert!(matches!(ops[2].0, DrawOp::Quad(_)));
        assert!(matches!(ops[3].0, DrawOp::EndSaveLayer));
        assert!(matches!(ops[4].0, DrawOp::Quad(_)));
    }

    #[test]
    fn test_save_layer_markers_in_op_locations() {
        let mut fb = FrameBuilder::new();
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
            None,
            0.0,
        );
        fb.begin_save_layer(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0), 0.5);
        fb.add_rect(
            Bounds::from_xywh(0.0, 0.0, 50.0, 50.0),
            Color::BLUE,
            None,
            0.0,
        );
        fb.end_save_layer();

        let locations = fb.compute_op_locations();
        assert_eq!(locations.len(), 4);
        assert_eq!(locations[0].kind(), OpKind::Quad);
        assert_eq!(locations[1].kind(), OpKind::SaveLayerMarker);
        assert_eq!(locations[2].kind(), OpKind::Quad);
        assert_eq!(locations[3].kind(), OpKind::SaveLayerMarker);
    }
}
