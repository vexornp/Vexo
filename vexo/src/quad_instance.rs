use crate::core::{AffineTransform, Logical, Point, Size};

pub const IDENTITY_TRANSFORM: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub corner_radius: f32,
    // 2D affine transform [a, b, c, d, e, f]. Identity by default.
    // | a  c  e |
    // | b  d  f |
    // | 0  0  1 |
    pub transform: [f32; 6],
    pub _padding: [f32; 4],
    pub shadow_color: [f32; 4],
    pub shadow_blur: f32,
    /// Depth value for GPU depth testing. Smaller = closer to camera (on top).
    /// Assigned by FrameBuilder in paint order.
    pub z: f32,
    pub _padding2: [f32; 2],
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
        z: f32,
    ) -> Self {
        Self {
            position: pos.to_array(),
            size: size.to_array(),
            color: color.to_array(),
            border_color: border_color.to_array(),
            border_width,
            corner_radius,
            transform: IDENTITY_TRANSFORM,
            _padding: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            z,
            _padding2: [0.0; 2],
        }
    }

    /// Create a QuadInstance with an explicit transform.
    pub fn with_transform(
        pos: Point<Logical>,
        size: Size<Logical>,
        color: crate::Color,
        border_color: crate::Color,
        border_width: f32,
        corner_radius: f32,
        transform: AffineTransform,
        z: f32,
    ) -> Self {
        Self {
            position: pos.to_array(),
            size: size.to_array(),
            color: color.to_array(),
            border_color: border_color.to_array(),
            border_width,
            corner_radius,
            transform: transform.to_array(),
            _padding: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            z,
            _padding2: [0.0; 2],
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
                    format: wgpu::VertexFormat::Float32x2,
                }, // transform [a, b]
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x2,
                }, // transform [c, d]
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x2,
                }, // transform [e, f]
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                }, // shadow_color
                wgpu::VertexAttribute {
                    offset: 112,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32,
                }, // shadow_blur
                wgpu::VertexAttribute {
                    offset: 116,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32,
                }, // z (depth)
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AffineTransform, Color, Logical, Point, Size};

    #[test]
    fn test_quad_instance_shadow_fields_default_zero() {
        let q = QuadInstance::from_logical(
            Point::new(0.0, 0.0),
            Size::new(10.0, 10.0),
            Color::RED,
            Color::BLACK,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(q.shadow_color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(q.shadow_blur, 0.0);
    }

    #[test]
    fn test_quad_instance_size_is_128_bytes() {
        assert_eq!(std::mem::size_of::<QuadInstance>(), 128);
    }

    #[test]
    fn test_quad_instance_with_transform_zero_shadow() {
        let q = QuadInstance::with_transform(
            Point::new(0.0, 0.0),
            Size::new(10.0, 10.0),
            Color::RED,
            Color::BLACK,
            0.0,
            0.0,
            AffineTransform::identity(),
            0.0,
        );
        assert_eq!(q.shadow_color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(q.shadow_blur, 0.0);
    }
}
