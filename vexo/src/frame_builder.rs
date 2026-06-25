use crate::core::{AffineTransform, Color, Logical, Point, Stroke};
use crate::image_atlas::ImageKey;
use crate::quad_instance::QuadInstance;

#[derive(Clone)]
pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    /// Maximum width for text wrapping. If None, no wrapping.
    pub max_width: Option<f32>,
}

#[derive(Clone)]
pub struct ImageRequest {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub image_key: ImageKey,
    pub corner_radius: f32,
    pub transform: [f32; 6],
    pub opacity: f32,
}

pub type Bounds = crate::core::Bounds<Logical>;

/// A group of quads and text requests sharing the same clip region.
pub struct ClipGroup {
    /// The effective clip bounds for this group (logical coordinates).
    /// None means no clipping (full viewport).
    pub clip_bounds: Option<Bounds>,
    /// Quad instances in this group.
    pub quads: Vec<QuadInstance>,
    /// Text requests in this group.
    pub text_requests: Vec<TextRequest>,
    /// Image requests in this group.
    pub image_requests: Vec<ImageRequest>,
}

pub struct DrawRange {
    pub first_instance: u32,
    pub count: u32,
}

/// Flattened quad instances and per-group draw ranges for GPU upload.
pub struct FlattenedQuads {
    /// All quad instances in draw order.
    pub instances: Vec<QuadInstance>,
    /// For each clip group, which instances to draw.
    pub draw_ranges: Vec<DrawRange>,
}

pub struct FrameBuilder {
    clip_groups: Vec<ClipGroup>,
    /// Index of the last-used clip group (for O(1) lookup when clip hasn't changed).
    current_group_index: Option<usize>,

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
            clip_groups: Vec::new(),
            current_group_index: None,
            corner_radius_stack: Vec::new(),
            clip_stack: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: AffineTransform::identity(),
        }
    }

    pub fn clear(&mut self) {
        self.clip_groups.clear();
        self.current_group_index = None;
        self.corner_radius_stack.clear();
        self.clip_stack.clear();
        self.transform_stack.clear();
        self.current_transform = AffineTransform::identity();
    }

    pub fn quad_count(&self) -> usize {
        self.clip_groups.iter().map(|g| g.quads.len()).sum()
    }

    pub fn has_quads(&self) -> bool {
        self.clip_groups.iter().any(|g| !g.quads.is_empty())
    }

    pub fn clip_groups(&self) -> &[ClipGroup] {
        &self.clip_groups
    }

    /// Flatten all quad instances into a contiguous buffer with per-group draw ranges.
    pub fn flatten_quads(&self) -> FlattenedQuads {
        let mut instances = Vec::new();
        let mut draw_ranges = Vec::new();
        for group in &self.clip_groups {
            let first_instance = instances.len() as u32;
            instances.extend_from_slice(&group.quads);
            let count = group.quads.len() as u32;
            draw_ranges.push(DrawRange { first_instance, count });
        }
        FlattenedQuads {
            instances,
            draw_ranges,
        }
    }

    /// Take all text requests from all groups.
    pub fn take_text_requests(&mut self) -> Vec<TextRequest> {
        self.clip_groups
            .iter_mut()
            .flat_map(|g| std::mem::take(&mut g.text_requests))
            .collect()
    }

    pub fn text_count(&self) -> usize {
        self.clip_groups.iter().map(|g| g.text_requests.len()).sum()
    }

    /// Get all quad instances flattened across all clip groups (for testing/compat).
    pub fn quad_instances(&self) -> Vec<QuadInstance> {
        self.clip_groups.iter().flat_map(|g| &g.quads).copied().collect()
    }

    /// Get all text requests flattened across all clip groups (for testing/compat).
    pub fn text_requests(&self) -> Vec<TextRequest> {
        self.clip_groups.iter().flat_map(|g| &g.text_requests).cloned().collect()
    }

    /// Get or create the ClipGroup for the current effective clip.
    fn current_group(&mut self) -> &mut ClipGroup {
        let clip_key = self.current_clip();
        // Check if the last-used group still matches (common case: depth-first traversal)
        if let Some(idx) = self.current_group_index {
            if self.clip_groups[idx].clip_bounds == clip_key {
                return &mut self.clip_groups[idx];
            }
        }
        // Search for an existing group with the same clip key
        for (i, group) in self.clip_groups.iter().enumerate() {
            if group.clip_bounds == clip_key {
                self.current_group_index = Some(i);
                return &mut self.clip_groups[i];
            }
        }
        // Create a new group
        let idx = self.clip_groups.len();
        self.clip_groups.push(ClipGroup {
            clip_bounds: clip_key,
            quads: Vec::new(),
            text_requests: Vec::new(),
            image_requests: Vec::new(),
        });
        self.current_group_index = Some(idx);
        &mut self.clip_groups[idx]
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
        // Clip changed — next add_rect/add_text will find/create the right group
        self.current_group_index = None;
    }

    /// Pop the most recent clipping region from the stack.
    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
        self.current_group_index = None;
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

        self.current_group().quads.push(instance);
    }

    pub fn add_text(
        &mut self,
        content: impl Into<String>,
        position: Point<Logical>,
        size: f32,
        color: impl Into<Color>,
        max_width: Option<f32>,
    ) {
        let color: Color = color.into();

        self.current_group().text_requests.push(TextRequest {
            content: content.into(),
            position,
            size,
            color,
            max_width,
        });
    }

    pub fn add_image(&mut self, request: ImageRequest) {
        self.current_group().image_requests.push(request);
    }

    pub fn image_count(&self) -> usize {
        self.clip_groups.iter().map(|g| g.image_requests.len()).sum()
    }

    pub fn flatten_image_requests(&self) -> (Vec<ImageRequest>, Vec<DrawRange>) {
        let mut requests = Vec::new();
        let mut draw_ranges = Vec::new();
        for group in &self.clip_groups {
            let first_instance = requests.len() as u32;
            requests.extend_from_slice(&group.image_requests);
            let count = group.image_requests.len() as u32;
            draw_ranges.push(DrawRange { first_instance, count });
        }
        (requests, draw_ranges)
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

        let (requests, ranges) = fb.flatten_image_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(ranges.len(), 2);
    }
}
