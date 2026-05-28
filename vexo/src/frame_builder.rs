use crate::core::{Color, Logical, Point, Stroke};
use crate::quad_instance::{self, QuadInstance};

pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    /// Clipping bounds. If None, no clipping is applied.
    pub clip_bounds: Option<Bounds>,
}

pub type Bounds = crate::core::Bounds<Logical>;

pub struct FrameBuilder {
    text_requests: Vec<TextRequest>,
    quad_instances: Vec<QuadInstance>,

    corner_radius_stack: Vec<f32>,
    clip_stack: Vec<Bounds>,
}

impl Default for FrameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameBuilder {
    pub fn new() -> Self {
        Self {
            text_requests: Vec::new(),
            quad_instances: Vec::new(),
            corner_radius_stack: Vec::new(),
            clip_stack: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.text_requests.clear();
        self.quad_instances.clear();
        self.corner_radius_stack.clear();
        self.clip_stack.clear();
    }

    pub fn quad_count(&self) -> usize {
        self.quad_instances.len()
    }

    pub fn has_quads(&self) -> bool {
        !self.quad_instances.is_empty()
    }

    pub fn quad_instances(&self) -> &[QuadInstance] {
        &self.quad_instances
    }

    pub fn take_text_requests(&mut self) -> Vec<TextRequest> {
        std::mem::take(&mut self.text_requests)
    }

    pub fn text_count(&self) -> usize {
        self.text_requests.len()
    }

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

    /// Get the current clipping region from the stack.
    /// Returns None if no clip is set.
    pub fn current_clip(&self) -> Option<Bounds> {
        self.clip_stack.last().copied()
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

        let clip_bounds = self.current_clip().map_or(quad_instance::NO_CLIP_BOUNDS, |b| b.to_array_xywh());

        self.quad_instances.push(QuadInstance {
            position: [bounds.left, bounds.top],
            size: [bounds.width(), bounds.height()],
            color: fill.to_array(),
            border_color: border_color.to_array(),
            border_width,
            corner_radius: radius,
            clip_bounds,
            _padding: [0.0; 2],
        });
    }

    pub fn add_text(
        &mut self,
        content: impl Into<String>,
        position: Point<Logical>,
        size: f32,
        color: impl Into<Color>,
    ) {
        let color: Color = color.into();

        // Get current clip bounds, or None to indicate no clipping
        let clip_bounds = self.current_clip();

        self.text_requests.push(TextRequest {
            content: content.into(),
            position,
            size,
            color,
            clip_bounds,
        });
    }
}