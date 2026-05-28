use crate::core::{Logical, Point, Size};

pub const NO_CLIP_BOUNDS: [f32; 4] = [-1.0, -1.0, -1.0, -1.0];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub corner_radius: f32,
    // Clipping bounds (x, y, width, height). If width/height <= 0, no clipping.
    pub clip_bounds: [f32; 4],
    pub _padding: [f32; 2], // Maintain 16-byte alignment for safety
}

impl QuadInstance {
    /// Create a QuadInstance from logical coordinates
    pub fn from_logical(
        pos: Point<Logical>,
        size: Size<Logical>,
        color: crate::Color,
        border_color: crate::Color,
        border_width: f32,
        corner_radius: f32,
    ) -> Self {
        Self {
            position: pos.to_array(),
            size: size.to_array(),
            color: color.to_array(),
            border_color: border_color.to_array(),
            border_width,
            corner_radius,
            clip_bounds: NO_CLIP_BOUNDS, // No clipping by default
            _padding: [0.0; 2],
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<QuadInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                }, // position
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                }, // size
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                }, // color
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                }, // border_color
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                }, // border_width
                wgpu::VertexAttribute {
                    offset: 52,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32,
                }, // corner_radius
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                }, // clip_bounds
            ],
        }
    }
}
