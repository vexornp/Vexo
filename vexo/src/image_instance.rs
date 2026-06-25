use crate::core::AffineTransform;
use crate::image_atlas::AtlasRegion;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_origin: [f32; 2],
    pub uv_size: [f32; 2],
    pub corner_radius: f32,
    pub opacity: f32,
    pub transform: [f32; 6],
    pub _padding: [f32; 1],
}

impl ImageInstance {
    pub fn from_logical(
        pos: [f32; 2],
        size: [f32; 2],
        region: &AtlasRegion,
        atlas_size: [f32; 2],
        corner_radius: f32,
        transform: AffineTransform,
        opacity: f32,
    ) -> Self {
        Self {
            position: pos,
            size,
            uv_origin: [
                region.x as f32 / atlas_size[0],
                region.y as f32 / atlas_size[1],
            ],
            uv_size: [
                region.width as f32 / atlas_size[0],
                region.height as f32 / atlas_size[1],
            ],
            corner_radius,
            opacity,
            transform: transform.to_array(),
            _padding: [0.0],
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<ImageInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 24, shader_location: 4, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 36, shader_location: 9, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 40, shader_location: 6, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 48, shader_location: 7, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 56, shader_location: 8, format: wgpu::VertexFormat::Float32x2 },
            ],
        }
    }
}
