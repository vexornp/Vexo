use crate::core::{Color, Logical, Point, Size, Stroke};
use crate::quad_instance;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct TextRequest {
    pub content: String,
    pub position: Point<Logical>,
    pub size: f32,
    pub color: Color,
    /// Clipping bounds. If None, no clipping is applied.
    pub clip_bounds: Option<Bounds>,
}

pub type Bounds = crate::core::Bounds<Logical>;

pub struct UiBatcher {
    pub text_requests: Vec<TextRequest>,
    pub quad_instances: Vec<quad_instance::QuadInstance>,

    screen_size: Size<Logical>, // Logical size: pixel_size * scale_factor
    corner_radius_stack: Vec<f32>, // Stack for nested radius contexts
    clip_stack: Vec<Bounds>, // Stack for clipping regions
}

impl Default for UiBatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBatcher {
    pub fn new() -> Self {
        Self {
            text_requests: Vec::new(),
            quad_instances: Vec::new(),
            screen_size: Size::new(1.0, 1.0),
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

    /// Set logical screen size.
    pub fn set_screen_size(&mut self, size: Size<Logical>) {
        self.screen_size = size;
    }

    /// Get the logical screen size.
    pub fn screen_size(&self) -> Size<Logical> {
        self.screen_size
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

        // Get current clip bounds, or use negative values to indicate no clipping
        let clip_bounds = match self.current_clip() {
            Some(b) => b.to_array_xywh(),
            None => [-1.0, -1.0, -1.0, -1.0], // No clipping
        };

        self.quad_instances.push(quad_instance::QuadInstance {
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
        content: String,
        position: Point<Logical>,
        size: f32,
        color: impl Into<Color>,
    ) {
        let color: Color = color.into();

        // Get current clip bounds, or None to indicate no clipping
        let clip_bounds = self.current_clip();

        self.text_requests.push(TextRequest {
            content,
            position,
            size,
            color,
            clip_bounds,
        });
    }
}